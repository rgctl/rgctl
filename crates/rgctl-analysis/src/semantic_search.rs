//! Opt-in semantic search over function symbols using binary-quantized embeddings.
//!
//! Embeddings are stored separately from [`AnalysisResults`] in `.rgctl/semantic_index.bin`
//! so the default discover path stays lean. Default semantic embedder is compiled
//! vocab (`vocab-accumulate-v1` FNV or `v2` distilled); code-daemon ONNX is opt-in (`--embedder code-daemon`).

use crate::semantic_embedder::{OnnxReloadOptions, SemanticEmbedder, embedder_for_index};
use crate::semantic_extract::{extract_body_tokens_for_node, extract_body_tokens_from_slice};
use rayon::prelude::*;
use rgctl_error::Result;
use rgctl_graph::backend::MemoryBackend;
use rgctl_graph::content_store::ContentStore;
use rgctl_graph::schema::{Node, NodeType};
use serde::{Deserialize, Serialize};
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Current on-disk schema version for [`SemanticIndex`].
pub const SEMANTIC_INDEX_SCHEMA_VERSION: u32 = 2;

/// Default filename under `.rgctl/`.
pub const SEMANTIC_INDEX_FILE: &str = "semantic_index.bin";

/// Default float dimensions before sign quantization (128 bytes per vector).
pub const DEFAULT_EMBEDDING_DIMENSIONS: usize = 256;

/// Identifier for the built-in deterministic hash embedder.
pub const SIGN_HASH_MODEL_ID: &str = "sign-hash-v1";

/// Suffix on [`SemanticIndex::model_id`] when function bodies were embedded.
pub const EMBED_BODIES_MODEL_SUFFIX: &str = "+bodies";

/// Persist embedder id plus optional `--embed-bodies` marker (forces rebuild on toggle).
pub fn persist_semantic_model_id(embedder_id: &str, embed_bodies: bool) -> String {
    if embed_bodies {
        format!("{embedder_id}{EMBED_BODIES_MODEL_SUFFIX}")
    } else {
        embedder_id.to_string()
    }
}

/// Strip [`EMBED_BODIES_MODEL_SUFFIX`] so query reload can resolve the embedder.
pub fn embedder_model_id(stored: &str) -> &str {
    stored
        .strip_suffix(EMBED_BODIES_MODEL_SUFFIX)
        .unwrap_or(stored)
}

/// One indexed function symbol and its metadata for query display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticEntry {
    /// Graph node UUID.
    pub node_id: Uuid,
    /// Short symbol name.
    pub name: String,
    /// Fully qualified name when known.
    pub qualified_name: Option<String>,
    /// Source file path when known.
    pub file_path: Option<String>,
    /// BLAKE3 body hash at index time (incremental reuse).
    #[serde(default)]
    pub code_hash: Option<String>,
    /// GQL node type label (`Function`, `Module`, …).
    #[serde(default)]
    pub node_type: Option<String>,
    /// Doc `kind` property when indexed (`heading`, `code_block`, …).
    #[serde(default)]
    pub kind: Option<String>,
    /// Framework / domain vocab tokens stamped at ingest (`semantic_tags` property).
    #[serde(default)]
    pub semantic_tags: Option<String>,
}

/// Bit-packed semantic index over function nodes only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SemanticIndex {
    /// Format version for forward-compatible load.
    pub schema_version: u32,
    /// Embedder identifier (e.g. `sign-hash-v1`).
    pub model_id: String,
    /// Float dimensions before quantization.
    pub dimensions: usize,
    /// Graph snapshot digest when indexed (optional invalidation).
    pub graph_digest: Option<String>,
    /// ONNX model path when `model_id` starts with `onnx:` or `code-daemon:v1`.
    #[serde(default)]
    pub model_path: Option<String>,
    /// SentencePiece path for ONNX embedders (optional; sibling auto-detect at index time).
    #[serde(default)]
    pub tokenizer_path: Option<String>,
    /// Row order matches contiguous slices of [`Self::binary_embeddings`].
    pub entries: Vec<SemanticEntry>,
    /// Flat bit-packed rows: `entries.len() * packed_bytes(dimensions)`.
    pub binary_embeddings: Vec<u8>,
}

impl SemanticIndex {
    /// Default path under a repository root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".rgctl").join(SEMANTIC_INDEX_FILE)
    }

    /// Bytes per quantized vector row.
    pub fn bytes_per_vector(&self) -> usize {
        packed_bytes(self.dimensions)
    }

    /// Number of indexed functions.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no functions were indexed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Binary embedding for row `index`.
    pub fn embedding_row(&self, index: usize) -> Option<&[u8]> {
        let stride = self.bytes_per_vector();
        let start = index.checked_mul(stride)?;
        self.binary_embeddings.get(start..start + stride)
    }

    /// Save index to disk (bincode, same pattern as [`crate::results::AnalysisResults`]).
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::io::BufWriter::new(std::fs::File::create(path)?);
        bincode::serialize_into(&mut file, self).map_err(serde_err)?;
        std::io::Write::flush(&mut file)?;
        Ok(())
    }

    /// Load index from disk.
    pub fn load(path: &Path) -> Result<Self> {
        let file = std::io::BufReader::new(std::fs::File::open(path)?);
        bincode::deserialize_from(file).map_err(serde_err)
    }

    /// Load when present; `Ok(None)` if the file is missing.
    pub fn open_if_exists(repo_root: &Path) -> Result<Option<Self>> {
        let path = Self::default_path(repo_root);
        if !path.is_file() {
            return Ok(None);
        }
        Self::load(&path).map(Some)
    }
}

/// Stats from an index build (full or incremental).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SemanticBuildStats {
    /// Total entries considered for the build.
    pub total: usize,
    /// Entries reused from an existing incremental index.
    pub reused: usize,
    /// Entries freshly embedded in this build.
    pub embedded: usize,
    /// Stale entries removed during incremental update.
    pub removed: usize,
}

/// Which graph nodes to embed in the semantic index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SemanticIndexScope {
    /// Function symbols only (default).
    #[default]
    Functions,
    /// Documentation headings (`:Module` with `kind=heading`).
    Docs,
    /// Functions and documentation sections.
    All,
}

/// Options controlling semantic index construction.
#[derive(Debug, Clone)]
pub struct SemanticBuildOptions {
    /// Embedding dimensionality.
    pub dimensions: usize,
    /// Optional graph content digest for incremental invalidation.
    pub graph_digest: Option<String>,
    /// Reuse embeddings from `existing` when true.
    pub incremental: bool,
    /// Prior index to reuse when `incremental` is set.
    pub existing: Option<SemanticIndex>,
    /// Persisted ONNX model path metadata.
    pub model_path: Option<String>,
    /// Persisted tokenizer path metadata.
    pub tokenizer_path: Option<String>,
    /// Repository root for content-store lookup (docs) and optional body slicing.
    pub repo_root: Option<PathBuf>,
    /// When true, re-read function source and append body identifier tokens.
    /// Off by default — declaration metadata is enough; query fusion uses discover token-blooms.
    pub embed_bodies: bool,
    /// Optional index-time call-graph diffusion (before sign quantization).
    pub diffuse: Option<crate::semantic_diffuse::DiffuseConfig>,
    /// Node kinds to index.
    pub scope: SemanticIndexScope,
}

impl SemanticBuildOptions {
    /// Defaults for a fresh full build.
    pub fn fresh(dimensions: usize, graph_digest: Option<String>) -> Self {
        Self {
            dimensions,
            graph_digest,
            incremental: false,
            existing: None,
            model_path: None,
            tokenizer_path: None,
            repo_root: None,
            embed_bodies: false,
            diffuse: None,
            scope: SemanticIndexScope::Functions,
        }
    }
}

/// Build a semantic index from all `Function` nodes using the given embedder.
pub fn build_index(
    backend: &MemoryBackend,
    embedder: &dyn SemanticEmbedder,
    options: SemanticBuildOptions,
) -> Result<(SemanticIndex, SemanticBuildStats)> {
    let row_bytes = packed_bytes(options.dimensions);
    let stored_model_id = persist_semantic_model_id(embedder.model_id(), options.embed_bodies);
    let mut entries = Vec::new();
    let mut binary_embeddings = Vec::new();
    let mut stats = SemanticBuildStats::default();

    let mut reuse_by_id: HashMap<Uuid, (SemanticEntry, Vec<u8>)> = HashMap::new();
    let digest_matches_existing = options.existing.as_ref().is_some_and(|existing| {
        existing.graph_digest == options.graph_digest
            && existing.dimensions == options.dimensions
            && existing.model_id == stored_model_id
    });
    if options.incremental
        && let Some(existing) = &options.existing
    {
        if existing.dimensions != options.dimensions || existing.model_id != stored_model_id {
            // Dimension/model/embed-bodies mismatch — full rebuild.
        } else {
            let stride = existing.bytes_per_vector();
            for (row, entry) in existing.entries.iter().enumerate() {
                if let Some(slice) = existing
                    .binary_embeddings
                    .get(row * stride..row * stride + stride)
                {
                    reuse_by_id.insert(entry.node_id, (entry.clone(), slice.to_vec()));
                }
            }
        }
    }

    let repo_root = options.repo_root.as_deref();
    let body_root = if options.embed_bodies {
        repo_root
    } else {
        None
    };
    let content_store = options
        .repo_root
        .as_ref()
        .and_then(|root| ContentStore::load(ContentStore::default_path(root)).ok());
    let candidates =
        collect_index_candidates(backend, options.scope, body_root, content_store.as_ref())?;

    let diffuse = options.diffuse.filter(|cfg| cfg.is_active());

    // Pure incremental hit: every indexed node reuses bits and digests match.
    // When diffusion is requested, never take the pure-reuse shortcut — bits must be
    // recomputed through the dense→diffuse→quantize path.
    let mut pure_reuse = options.incremental && digest_matches_existing && diffuse.is_none();
    if pure_reuse {
        for (fresh_entry, _) in &candidates {
            match reuse_by_id.get(&fresh_entry.node_id) {
                Some((old_entry, _)) if old_entry.code_hash == fresh_entry.code_hash => {}
                _ => {
                    pure_reuse = false;
                    break;
                }
            }
        }
        if candidates.len()
            != options
                .existing
                .as_ref()
                .map(|e| e.entries.len())
                .unwrap_or(0)
        {
            pure_reuse = false;
        }
    }

    if let Some(config) = diffuse.filter(|_| !pure_reuse) {
        // Dense path: embed all → CallGraph-aligned diffuse → quantize.
        let call_graph = crate::callgraph::CallGraph::from_backend(backend)?;
        let n_fn = call_graph.function_count();
        let dims = options.dimensions;
        let mut dense = vec![0.0f32; n_fn * dims];
        let mut entry_by_uuid: HashMap<Uuid, (SemanticEntry, Vec<f32>)> = HashMap::new();

        let mut seen = HashSet::new();
        for (fresh_entry, _) in &candidates {
            seen.insert(fresh_entry.node_id);
            stats.total += 1;
            stats.embedded += 1;
        }

        embed_jobs_chunked(embedder, &candidates, |entry, floats| {
            if let Some(&idx) = call_graph.id_to_index.get(&entry.node_id) {
                let start = idx as usize * dims;
                let copy = dims.min(floats.len());
                dense[start..start + copy].copy_from_slice(&floats[..copy]);
            }
            entry_by_uuid.insert(entry.node_id, (entry.clone(), floats));
            Ok(())
        })?;

        crate::semantic_diffuse::diffuse_call_topology(&call_graph, &mut dense, dims, config);

        for (idx, uuid) in call_graph.index_to_id.iter().enumerate() {
            if let Some((entry, _)) = entry_by_uuid.remove(uuid) {
                let start = idx * dims;
                entries.push(entry);
                binary_embeddings.extend_from_slice(&quantize_binary(&dense[start..start + dims]));
            }
        }
        // Functions not present in CallGraph — quantize local dense without diffusion.
        for (_id, (entry, floats)) in entry_by_uuid {
            entries.push(entry);
            binary_embeddings.extend_from_slice(&quantize_binary(&floats));
        }

        if options.incremental
            && let Some(existing) = &options.existing
        {
            stats.removed = existing
                .entries
                .iter()
                .filter(|entry| !seen.contains(&entry.node_id))
                .count();
        }
    } else {
        let mut seen = HashSet::new();
        let mut embed_jobs: Vec<(SemanticEntry, String)> = Vec::new();
        for (fresh_entry, text) in candidates {
            seen.insert(fresh_entry.node_id);
            stats.total += 1;

            if options.incremental
                && let Some((old_entry, old_bits)) = reuse_by_id.get(&fresh_entry.node_id)
                && old_entry.code_hash == fresh_entry.code_hash
            {
                entries.push(old_entry.clone());
                binary_embeddings.extend_from_slice(old_bits);
                stats.reused += 1;
                continue;
            }
            embed_jobs.push((fresh_entry, text));
        }

        embed_jobs_chunked(embedder, &embed_jobs, |entry, floats| {
            entries.push(entry.clone());
            binary_embeddings.extend_from_slice(&quantize_binary(&floats));
            stats.embedded += 1;
            Ok(())
        })?;

        if options.incremental
            && let Some(existing) = &options.existing
        {
            stats.removed = existing
                .entries
                .iter()
                .filter(|entry| !seen.contains(&entry.node_id))
                .count();
        }
    }

    debug_assert_eq!(binary_embeddings.len(), entries.len() * row_bytes);

    let index = SemanticIndex {
        schema_version: SEMANTIC_INDEX_SCHEMA_VERSION,
        model_id: stored_model_id,
        dimensions: options.dimensions,
        graph_digest: options.graph_digest,
        model_path: options.model_path,
        tokenizer_path: options.tokenizer_path,
        entries,
        binary_embeddings,
    };

    Ok((index, stats))
}

/// Build a semantic index from all `Function` nodes (sign-hash, non-incremental).
pub fn build_from_backend(
    backend: &MemoryBackend,
    dimensions: usize,
    graph_digest: Option<String>,
) -> Result<SemanticIndex> {
    let embedder = crate::semantic_embedder::SignHashEmbedder::new(dimensions);
    let (index, _stats) = build_index(
        backend,
        &embedder,
        SemanticBuildOptions::fresh(dimensions, graph_digest),
    )?;
    Ok(index)
}

fn semantic_entry_from_node(node: &Node) -> SemanticEntry {
    SemanticEntry {
        node_id: node.id,
        name: node.name.to_string(),
        qualified_name: node.qualified_name.as_ref().map(|s| s.to_string()),
        file_path: node.file_path.as_ref().map(|s| s.to_string()),
        code_hash: node
            .code_hash
            .as_ref()
            .map(|s| s.to_string())
            .or_else(|| node.get_property("body_hash").map(|s| s.to_string())),
        node_type: Some(format!("{:?}", node.node_type)),
        kind: node.get_property("kind").map(|s| s.to_string()),
        semantic_tags: node.get_property("semantic_tags").map(|s| s.to_string()),
    }
}

/// Functions always; framework-tagged Class/Interface/Struct for controller/stereotype search.
fn is_indexed_code_node(node: &Node) -> bool {
    if node.node_type == NodeType::Function {
        return true;
    }
    if matches!(
        node.node_type,
        NodeType::Class | NodeType::Interface | NodeType::Struct
    ) {
        return node.get_property("semantic_tags").is_some()
            || node.get_property("framework_tags").is_some();
    }
    false
}

struct PendingCandidate {
    entry: SemanticEntry,
    parts: Vec<String>,
    body_slice: Option<(String, usize, usize)>,
    body_ref: Option<String>,
}

impl PendingCandidate {
    fn needs_io(&self) -> bool {
        self.body_slice.is_some() || self.body_ref.is_some()
    }
}

fn is_doc_section(node: &Node) -> bool {
    if node.node_type != NodeType::Module {
        return false;
    }
    matches!(node.get_property("kind"), Some("heading" | "code_block"))
}

fn function_decl_parts(node: &Node) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    if let Some(qn) = &node.qualified_name {
        parts.push(qn.to_string());
    } else {
        parts.push(node.name.to_string());
    }
    if let Some(sig) = node.signature_text() {
        parts.push(sig.to_string());
    }
    if let Some(ret) = node.return_type_text() {
        parts.push(format!("returns {ret}"));
    }
    if let Some(doc) = node.get_property("documentation") {
        parts.push(doc.to_string());
    }
    if let Some(tags) = node.get_property("semantic_tags") {
        parts.push(tags.to_string());
    }
    if let Some(ftags) = node.get_property("framework_tags") {
        parts.push(ftags.to_string());
    }
    parts
}

fn pending_from_node(
    node: &Node,
    scope: SemanticIndexScope,
    embed_bodies: bool,
) -> Option<PendingCandidate> {
    let include_fn = matches!(
        scope,
        SemanticIndexScope::Functions | SemanticIndexScope::All
    );
    let include_docs = matches!(scope, SemanticIndexScope::Docs | SemanticIndexScope::All);
    if include_fn && is_indexed_code_node(node) {
        let body_slice = if embed_bodies && node.node_type == NodeType::Function {
            match (node.file_path.as_deref(), node.start_line, node.end_line) {
                (Some(path), Some(start), Some(end)) => Some((path.to_string(), start, end)),
                _ => None,
            }
        } else {
            None
        };
        return Some(PendingCandidate {
            entry: semantic_entry_from_node(node),
            parts: function_decl_parts(node),
            body_slice,
            body_ref: None,
        });
    }
    if include_docs && is_doc_section(node) {
        let mut parts: Vec<String> = Vec::new();
        if let Some(qn) = &node.qualified_name {
            parts.push(qn.to_string());
        }
        parts.push(node.name.to_string());
        let mut body_ref = None;
        if let Some(text) = node.get_property("body_text") {
            parts.push(text.to_string());
        } else if let Some(ref_key) = node.get_property("body_ref") {
            body_ref = Some(ref_key.to_string());
        }
        return Some(PendingCandidate {
            entry: semantic_entry_from_node(node),
            parts,
            body_slice: None,
            body_ref,
        });
    }
    None
}

fn collect_index_candidates(
    backend: &MemoryBackend,
    scope: SemanticIndexScope,
    body_root: Option<&Path>,
    content_store: Option<&ContentStore>,
) -> Result<Vec<(SemanticEntry, String)>> {
    let parallel_io =
        body_root.is_some() || matches!(scope, SemanticIndexScope::Docs | SemanticIndexScope::All);
    if !parallel_io {
        let mut candidates = Vec::new();
        backend.for_each_node(|node| {
            if let Some(text) = embed_text_for_scope(node, scope, None, None) {
                candidates.push((semantic_entry_from_node(node), text));
            }
        })?;
        return Ok(candidates);
    }

    let mut pending = Vec::new();
    backend.for_each_node(|node| {
        if let Some(item) = pending_from_node(node, scope, body_root.is_some()) {
            pending.push(item);
        }
    })?;
    if pending.iter().any(PendingCandidate::needs_io) {
        pending.par_iter_mut().for_each(|item| {
            if let (Some(root), Some((path, start, end))) = (body_root, item.body_slice.take())
                && let Ok(tokens) = extract_body_tokens_from_slice(root, &path, start, end)
            {
                let mut token_list: Vec<String> = tokens.into_iter().collect();
                token_list.sort_unstable();
                item.parts.extend(token_list);
            }
            if let Some(key) = item.body_ref.take()
                && let Some(store) = content_store
                && let Some(body) = store.get_str(&key)
            {
                item.parts.push(body.to_string());
            }
        });
    }
    Ok(pending
        .into_iter()
        .map(|item| (item.entry, item.parts.join(" ")))
        .collect())
}

fn embed_jobs_chunked(
    embedder: &dyn SemanticEmbedder,
    jobs: &[(SemanticEntry, String)],
    mut emit: impl FnMut(&SemanticEntry, Vec<f32>) -> Result<()>,
) -> Result<()> {
    if jobs.is_empty() {
        return Ok(());
    }
    let chunk_size = embedder.preferred_batch_size().max(1);
    for chunk in jobs.chunks(chunk_size) {
        let texts: Vec<&str> = chunk.iter().map(|(_, text)| text.as_str()).collect();
        let rows = embedder.embed_batch(&texts)?;
        if rows.len() != chunk.len() {
            return Err(rgctl_error::Error::ConfigError(format!(
                "embed_batch returned {} rows for {} texts",
                rows.len(),
                chunk.len()
            )));
        }
        for ((entry, _), floats) in chunk.iter().zip(rows) {
            emit(entry, floats)?;
        }
    }
    Ok(())
}

/// Collect embeddable text for a function node (declaration metadata only).
pub fn embed_text_for_node(node: &Node) -> Option<String> {
    embed_text_for_function(node, None)
}

/// Collect embeddable text for a function (or framework-tagged type) node.
pub fn embed_text_for_function(node: &Node, repo_root: Option<&Path>) -> Option<String> {
    if !is_indexed_code_node(node) {
        return None;
    }

    let mut parts: Vec<String> = Vec::new();
    if let Some(qn) = &node.qualified_name {
        parts.push(qn.to_string());
    } else {
        parts.push(node.name.to_string());
    }
    if let Some(sig) = node.signature_text() {
        parts.push(sig.to_string());
    }
    if let Some(ret) = node.return_type_text() {
        parts.push(format!("returns {ret}"));
    }
    if let Some(doc) = node.get_property("documentation") {
        parts.push(doc.to_string());
    }
    if let Some(tags) = node.get_property("semantic_tags") {
        parts.push(tags.to_string());
    }
    if let Some(ftags) = node.get_property("framework_tags") {
        parts.push(ftags.to_string());
    }

    if let Some(root) = repo_root
        && node.node_type == NodeType::Function
        && let Ok(body_tokens) = extract_body_tokens_for_node(root, node)
    {
        let mut token_list: Vec<String> = body_tokens.into_iter().collect();
        token_list.sort_unstable();
        parts.extend(token_list);
    }

    Some(parts.join(" "))
}

/// Collect embeddable text for a documentation section (`:Module` heading / code block).
pub fn embed_text_for_doc_node(
    node: &Node,
    content_store: Option<&ContentStore>,
) -> Option<String> {
    if node.node_type != NodeType::Module {
        return None;
    }
    let kind = node.get_property("kind")?;
    if kind != "heading" && kind != "code_block" {
        return None;
    }
    let mut parts: Vec<String> = Vec::new();
    if let Some(qn) = &node.qualified_name {
        parts.push(qn.to_string());
    }
    parts.push(node.name.to_string());
    if let Some(text) = node.get_property("body_text") {
        parts.push(text.to_string());
    } else if let Some(ref_key) = node.get_property("body_ref")
        && let Some(store) = content_store
        && let Some(body) = store.get_str(ref_key)
    {
        parts.push(body.to_string());
    }
    Some(parts.join(" "))
}

/// Resolve embeddable text for the configured semantic index scope.
pub fn embed_text_for_scope(
    node: &Node,
    scope: SemanticIndexScope,
    repo_root: Option<&Path>,
    content_store: Option<&ContentStore>,
) -> Option<String> {
    match scope {
        SemanticIndexScope::Functions => embed_text_for_function(node, repo_root),
        SemanticIndexScope::Docs => embed_text_for_doc_node(node, content_store),
        SemanticIndexScope::All => embed_text_for_function(node, repo_root)
            .or_else(|| embed_text_for_doc_node(node, content_store)),
    }
}

/// Deterministic sign-hash embedding (bag-of-tokens → sparse signed vector).
pub fn sign_hash_embed(text: &str, dimensions: usize) -> Vec<f32> {
    let mut vec = vec![0f32; dimensions];
    let tokens = crate::semantic_vocab::normalize_semantic_query_tokens(text);
    for token in tokens {
        let primary = fnv1a(token.as_bytes());
        let secondary = fnv1a(&[token.as_bytes(), b"#2"].concat());
        let sign = if primary & 1 == 0 { 1.0 } else { -1.0 };
        vec[primary as usize % dimensions] += sign;
        vec[secondary as usize % dimensions] += sign * 0.5;
    }
    vec
}

/// Sign-quantize a float vector into little-endian bit-packed bytes.
pub fn quantize_binary(floats: &[f32]) -> Vec<u8> {
    let mut out = vec![0u8; packed_bytes(floats.len())];
    for (i, value) in floats.iter().enumerate() {
        if *value >= 0.0 {
            out[i / 8] |= 1 << (i % 8);
        }
    }
    out
}

/// Hamming distance between two equal-length bit-packed vectors.
///
/// Processes 64-bit words so LLVM can lower XOR/`popcnt` efficiently.
pub fn hamming_distance(a: &[u8], b: &[u8]) -> u32 {
    debug_assert_eq!(
        a.len(),
        b.len(),
        "Hamming distance requires equal-length vectors"
    );

    let word_bytes = a.len() - (a.len() % 8);
    let mut total = 0u32;

    for (chunk_a, chunk_b) in a[..word_bytes]
        .as_chunks::<8>()
        .0
        .iter()
        .zip(b[..word_bytes].as_chunks::<8>().0.iter())
    {
        let word_a = u64::from_ne_bytes(*chunk_a);
        let word_b = u64::from_ne_bytes(*chunk_b);
        total += (word_a ^ word_b).count_ones();
    }

    for i in word_bytes..a.len() {
        total += (a[i] ^ b[i]).count_ones();
    }

    total
}

/// Return up to `k` nearest rows by Hamming distance (smallest first).
///
/// Parallel chunk scan with per-thread heaps, then merge. Cost is O(n log k).
pub fn hamming_top_k(index: &SemanticIndex, query: &[u8], k: usize) -> Vec<(usize, u32)> {
    if k == 0 || index.is_empty() {
        return Vec::new();
    }

    let stride = index.bytes_per_vector();
    debug_assert_eq!(index.binary_embeddings.len(), index.len() * stride);

    let merged = index
        .binary_embeddings
        .par_chunks(stride)
        .enumerate()
        .fold(
            || BinaryHeap::<(u32, usize)>::with_capacity(k.saturating_add(1)),
            |mut heap, (row, chunk)| {
                let dist = hamming_distance(query, chunk);
                if heap.len() < k {
                    heap.push((dist, row));
                } else if let Some(&(worst, _)) = heap.peek()
                    && dist < worst
                {
                    heap.pop();
                    heap.push((dist, row));
                }
                heap
            },
        )
        .reduce(BinaryHeap::new, |mut left, right| {
            for item in right {
                if left.len() < k {
                    left.push(item);
                } else if let Some(&(worst, _)) = left.peek()
                    && item.0 < worst
                {
                    left.pop();
                    left.push(item);
                }
            }
            left
        });

    let mut hits: Vec<(usize, u32)> = merged.into_iter().map(|(d, i)| (i, d)).collect();
    hits.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
    hits
}

/// Embed query text and search the index (sign-hash only; prefer [`query_index_with_embedder`]).
pub fn query_index(index: &SemanticIndex, text: &str, k: usize) -> Vec<SemanticHit> {
    query_index_with_embedder(index, text, k, &OnnxReloadOptions::default()).unwrap_or_default()
}

/// Embed query text and search using the embedder matching the index.
pub fn query_index_with_embedder(
    index: &SemanticIndex,
    text: &str,
    k: usize,
    reload: &OnnxReloadOptions,
) -> Result<Vec<SemanticHit>> {
    let embedder = embedder_for_index(index, reload)?;
    let query_bits = embedder.embed_binary(text)?;
    Ok(hamming_top_k(index, &query_bits, k)
        .into_iter()
        .filter_map(|(row, distance)| {
            let entry = index.entries.get(row)?;
            Some(SemanticHit {
                row,
                distance,
                entry: entry.clone(),
                fused_score: None,
            })
        })
        .collect())
}

/// One community-level semantic hit (pooled member embeddings).
#[derive(Debug, Clone, PartialEq)]
pub struct CommunitySemanticHit {
    /// Community id.
    pub community_id: usize,
    /// Human-readable label.
    pub label: String,
    /// Members contributing to the centroid.
    pub member_count: usize,
    /// Hamming distance to the query (lower is better).
    pub distance: u32,
    /// Similarity score in (0, 1].
    pub score: f64,
}

/// Search communities by pooling member function embeddings (majority-bit centroid).
pub fn query_communities(
    index: &SemanticIndex,
    analysis: &crate::results::AnalysisResults,
    labels: &std::collections::HashMap<usize, String>,
    text: &str,
    k: usize,
    reload: &OnnxReloadOptions,
) -> Result<Vec<CommunitySemanticHit>> {
    let Some(_table) = analysis.community.as_ref() else {
        return Ok(Vec::new());
    };
    let embedder = embedder_for_index(index, reload)?;
    let query_bits = embedder.embed_binary(text)?;
    let stride = index.bytes_per_vector();
    if stride == 0 || query_bits.len() != stride {
        return Ok(Vec::new());
    }

    let mut by_community: HashMap<usize, Vec<usize>> = HashMap::new();
    for (row, entry) in index.entries.iter().enumerate() {
        if let Some(cid) = analysis.get_community(entry.node_id) {
            by_community.entry(cid).or_default().push(row);
        }
    }

    let mut hits: Vec<CommunitySemanticHit> = Vec::new();
    for (cid, rows) in by_community {
        if rows.is_empty() {
            continue;
        }
        let mut bit_counts = vec![0i32; stride * 8];
        let mut used = 0usize;
        for row in &rows {
            let Some(bits) = index.embedding_row(*row) else {
                continue;
            };
            used += 1;
            for (byte_i, byte) in bits.iter().enumerate() {
                for bit in 0..8 {
                    let idx = byte_i * 8 + bit;
                    if idx >= bit_counts.len() {
                        break;
                    }
                    if (byte >> bit) & 1 == 1 {
                        bit_counts[idx] += 1;
                    } else {
                        bit_counts[idx] -= 1;
                    }
                }
            }
        }
        if used == 0 {
            continue;
        }
        let mut centroid = vec![0u8; stride];
        for (idx, count) in bit_counts.iter().enumerate() {
            if *count >= 0 {
                centroid[idx / 8] |= 1 << (idx % 8);
            }
        }
        let distance = hamming_distance(&query_bits, &centroid);
        let dims = index.dimensions.max(1);
        let score = 1.0 - (distance as f64 / dims as f64);
        hits.push(CommunitySemanticHit {
            community_id: cid,
            label: labels
                .get(&cid)
                .cloned()
                .unwrap_or_else(|| format!("Community {cid}")),
            member_count: rows.len(),
            distance,
            score,
        });
    }

    hits.sort_by(|a, b| {
        a.distance.cmp(&b.distance).then_with(|| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    });
    hits.truncate(k);
    Ok(hits)
}

/// One query hit with Hamming distance.
#[derive(Debug, Clone, PartialEq)]
pub struct SemanticHit {
    /// Row index in the index tables.
    pub row: usize,
    /// Hamming distance to the query (lower is better).
    pub distance: u32,
    /// Indexed function metadata.
    pub entry: SemanticEntry,
    /// Late-fusion score when two-stage ranking is enabled (higher is better).
    pub fused_score: Option<f64>,
}

fn packed_bytes(dimensions: usize) -> usize {
    dimensions.div_ceil(8)
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn serde_err(err: impl std::fmt::Display) -> rgctl_error::Error {
    rgctl_error::Error::SerdeError(format!("semantic index: {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_graph::backend::GraphBackend;
    use rgctl_graph::schema::Node;
    use std::collections::HashSet;

    #[test]
    fn quantize_round_trip_sign() {
        let floats = vec![1.0, -2.0, 0.0, 3.0];
        let bits = quantize_binary(&floats);
        assert_eq!(bits, vec![0b00001101]);
    }

    #[test]
    fn hamming_distance_counts_bit_flips() {
        assert_eq!(hamming_distance(&[0b1111_0000], &[0b1010_0000]), 2);
    }

    #[test]
    fn hamming_distance_matches_byte_wise_reference() {
        fn byte_wise(a: &[u8], b: &[u8]) -> u32 {
            a.iter()
                .zip(b.iter())
                .map(|(left, right)| (left ^ right).count_ones())
                .sum()
        }

        let a: Vec<u8> = (0..37).map(|i| (i * 17) as u8).collect();
        let b: Vec<u8> = (0..37).map(|i| (i * 31) as u8).collect();
        assert_eq!(hamming_distance(&a, &b), byte_wise(&a, &b));

        let packed = vec![0u8; packed_bytes(1024)];
        let mut flipped = packed.clone();
        flipped[0] = 0xFF;
        flipped[31] = 0x0F;
        assert_eq!(
            hamming_distance(&packed, &flipped),
            byte_wise(&packed, &flipped)
        );
    }

    #[test]
    fn hamming_top_k_returns_smallest_distances() {
        let index = SemanticIndex {
            schema_version: SEMANTIC_INDEX_SCHEMA_VERSION,
            model_id: SIGN_HASH_MODEL_ID.into(),
            dimensions: 8,
            graph_digest: None,
            model_path: None,
            tokenizer_path: None,
            entries: (0..4)
                .map(|i| SemanticEntry {
                    node_id: Uuid::new_v4(),
                    name: format!("f{i}"),
                    qualified_name: None,
                    file_path: None,
                    code_hash: None,
                    node_type: None,
                    kind: None,
                    semantic_tags: None,
                })
                .collect(),
            binary_embeddings: vec![
                0b0000_0000, // dist 0
                0b0000_0001, // dist 1
                0b0000_0011, // dist 2
                0b1111_1111, // dist 8
            ],
        };
        let query = vec![0b0000_0000];
        let top = hamming_top_k(&index, &query, 2);
        assert_eq!(top, vec![(0, 0), (1, 1)]);
    }

    #[test]
    fn sign_hash_embed_is_deterministic() {
        let a = sign_hash_embed("authenticate user token", 64);
        let b = sign_hash_embed("authenticate user token", 64);
        assert_eq!(a, b);
        assert_ne!(a, sign_hash_embed("revoke user token", 64));
    }

    #[test]
    fn embed_text_for_function_includes_signature() {
        let node = Node::new(NodeType::Function, "run")
            .with_qualified_name("auth::run")
            .with_signature("async fn run(token: &str) -> bool");
        let text = embed_text_for_node(&node).unwrap();
        assert!(text.contains("auth::run"));
        assert!(text.contains("async fn run"));
    }

    #[test]
    fn embed_text_skips_markup_heading_modules() {
        let heading = Node::new(NodeType::Module, "Checkout Flow")
            .with_property("kind".to_string(), "heading".to_string())
            .with_file_path("docs/guide.md");
        assert!(embed_text_for_function(&heading, None).is_none());
        assert!(embed_text_for_node(&heading).is_none());
    }

    #[test]
    fn build_and_query_from_backend() {
        let mut backend = MemoryBackend::new();
        let n1 = Node::new(NodeType::Function, "authenticate")
            .with_qualified_name("auth::authenticate")
            .with_signature("fn authenticate(token: &str) -> bool");
        let n2 = Node::new(NodeType::Function, "revoke")
            .with_qualified_name("auth::revoke")
            .with_signature("fn revoke(token: &str)");
        let class = Node::new(NodeType::Class, "AuthService");
        backend.insert_node(n1.clone()).unwrap();
        backend.insert_node(n2.clone()).unwrap();
        backend.insert_node(class).unwrap();

        let index = build_from_backend(&backend, 128, None).unwrap();
        assert_eq!(index.len(), 2);

        // Prefer the distinctive name token — filler/synonym normalization in
        // `sign_hash_embed` can flatten weak overlaps like shared `token` args.
        let hits = query_index(&index, "authenticate", 2);
        assert_eq!(hits.len(), 2);
        let auth_hit = hits
            .iter()
            .find(|h| h.entry.node_id == n1.id)
            .expect("authenticate should be retrieved");
        let revoke_hit = hits
            .iter()
            .find(|h| h.entry.node_id == n2.id)
            .expect("revoke should be retrieved");
        assert!(
            auth_hit.distance <= revoke_hit.distance,
            "authenticate distance {} should be <= revoke {}",
            auth_hit.distance,
            revoke_hit.distance
        );
    }

    #[test]
    fn persist_model_id_marks_embed_bodies() {
        assert_eq!(
            persist_semantic_model_id(SIGN_HASH_MODEL_ID, false),
            SIGN_HASH_MODEL_ID
        );
        let stored = persist_semantic_model_id(SIGN_HASH_MODEL_ID, true);
        assert_eq!(
            stored,
            format!("{SIGN_HASH_MODEL_ID}{EMBED_BODIES_MODEL_SUFFIX}")
        );
        assert_eq!(embedder_model_id(&stored), SIGN_HASH_MODEL_ID);
    }

    #[test]
    fn save_load_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(SEMANTIC_INDEX_FILE);
        let index = SemanticIndex {
            schema_version: SEMANTIC_INDEX_SCHEMA_VERSION,
            model_id: SIGN_HASH_MODEL_ID.into(),
            dimensions: 16,
            graph_digest: Some("abc".into()),
            model_path: None,
            tokenizer_path: None,
            entries: vec![SemanticEntry {
                node_id: Uuid::new_v4(),
                name: "main".into(),
                qualified_name: None,
                file_path: Some("src/main.rs".into()),
                code_hash: Some("abc".into()),
                node_type: None,
                kind: None,
                semantic_tags: None,
            }],
            binary_embeddings: vec![0b1010_1010, 0b0101_0101],
        };
        index.save(&path).unwrap();
        let loaded = SemanticIndex::load(&path).unwrap();
        assert_eq!(loaded, index);
    }

    #[test]
    fn incremental_reuses_unchanged_code_hash() {
        let mut backend = MemoryBackend::new();
        let n1 = Node::new(NodeType::Function, "authenticate")
            .with_code_hash("h1")
            .with_signature("fn authenticate()");
        let n2 = Node::new(NodeType::Function, "revoke")
            .with_code_hash("h2")
            .with_signature("fn revoke()");
        backend.insert_node(n1).unwrap();
        backend.insert_node(n2.clone()).unwrap();

        let embedder = crate::semantic_embedder::SignHashEmbedder::new(64);
        let (index, stats) =
            build_index(&backend, &embedder, SemanticBuildOptions::fresh(64, None)).unwrap();
        assert_eq!(stats.embedded, 2);

        let (index2, stats2) = build_index(
            &backend,
            &embedder,
            SemanticBuildOptions {
                dimensions: 64,
                graph_digest: None,
                incremental: true,
                existing: Some(index),
                model_path: None,
                tokenizer_path: None,
                repo_root: None,
                embed_bodies: false,
                diffuse: None,
                scope: SemanticIndexScope::Functions,
            },
        )
        .unwrap();
        assert_eq!(stats2.reused, 2);
        assert_eq!(stats2.embedded, 0);
        assert_eq!(index2.len(), 2);

        // Change one function body hash — only that row re-embeds.
        let mut n2_updated = n2;
        n2_updated.code_hash = Some("h2-v2".into());
        n2_updated.signature = Some("fn revoke(token: &str)".into());
        backend.insert_node(n2_updated).unwrap();
        let (index3, stats3) = build_index(
            &backend,
            &embedder,
            SemanticBuildOptions {
                dimensions: 64,
                graph_digest: None,
                incremental: true,
                existing: Some(index2),
                model_path: None,
                tokenizer_path: None,
                repo_root: None,
                embed_bodies: false,
                diffuse: None,
                scope: SemanticIndexScope::Functions,
            },
        )
        .unwrap();
        assert_eq!(stats3.reused, 1);
        assert_eq!(stats3.embedded, 1);
        assert_eq!(index3.len(), 2);
    }

    #[test]
    fn body_tokens_improve_retrieval_for_implementation_vocabulary() {
        let dir = tempfile::tempdir().unwrap();
        let rel = "src/net.rs";
        let abs = dir.path().join(rel);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(
            &abs,
            "fn cryptic_a() {}\n\nfn cryptic_b() {\n    let port = ntohs(raw);\n}\n",
        )
        .unwrap();

        let mut backend = MemoryBackend::new();
        let opaque = Node::new(NodeType::Function, "cryptic_a")
            .with_file_path(rel)
            .with_location(1, 1);
        let helper = Node::new(NodeType::Function, "cryptic_b")
            .with_file_path(rel)
            .with_location(3, 5)
            .with_code_hash("body-v1");
        backend.insert_node(opaque.clone()).unwrap();
        backend.insert_node(helper.clone()).unwrap();

        let index_no_body = build_index(
            &backend,
            &crate::semantic_embedder::SignHashEmbedder::new(128),
            SemanticBuildOptions {
                dimensions: 128,
                graph_digest: None,
                incremental: false,
                existing: None,
                model_path: None,
                tokenizer_path: None,
                repo_root: None,
                embed_bodies: false,
                diffuse: None,
                scope: SemanticIndexScope::Functions,
            },
        )
        .unwrap()
        .0;

        let index_with_body = build_index(
            &backend,
            &crate::semantic_embedder::SignHashEmbedder::new(128),
            SemanticBuildOptions {
                dimensions: 128,
                graph_digest: None,
                incremental: false,
                existing: None,
                model_path: None,
                tokenizer_path: None,
                repo_root: Some(dir.path().to_path_buf()),
                embed_bodies: true,
                diffuse: None,
                scope: SemanticIndexScope::Functions,
            },
        )
        .unwrap()
        .0;

        let hits_no_body = query_index(&index_no_body, "ntohs packet port", 2);
        let hits_with_body = query_index(&index_with_body, "ntohs packet port", 2);

        let dist_no_body = hits_no_body
            .iter()
            .find(|hit| hit.entry.node_id == helper.id)
            .map(|hit| hit.distance)
            .expect("helper indexed");
        let dist_with_body = hits_with_body
            .iter()
            .find(|hit| hit.entry.node_id == helper.id)
            .map(|hit| hit.distance)
            .expect("helper indexed");

        assert!(dist_with_body < dist_no_body);
        assert_eq!(hits_with_body[0].entry.node_id, helper.id);
    }

    #[test]
    fn repo_root_without_embed_bodies_does_not_scan_source() {
        let dir = tempfile::tempdir().unwrap();
        let rel = "src/net.rs";
        let abs = dir.path().join(rel);
        std::fs::create_dir_all(abs.parent().unwrap()).unwrap();
        std::fs::write(&abs, "fn cryptic_b() {\n    let port = ntohs(raw);\n}\n").unwrap();

        let mut backend = MemoryBackend::new();
        let helper = Node::new(NodeType::Function, "cryptic_b")
            .with_file_path(rel)
            .with_location(1, 3);
        backend.insert_node(helper).unwrap();

        let embedder = crate::semantic_embedder::SignHashEmbedder::new(128);
        let no_root = build_index(&backend, &embedder, SemanticBuildOptions::fresh(128, None))
            .unwrap()
            .0;
        let with_root = build_index(
            &backend,
            &embedder,
            SemanticBuildOptions {
                dimensions: 128,
                graph_digest: None,
                incremental: false,
                existing: None,
                model_path: None,
                tokenizer_path: None,
                repo_root: Some(dir.path().to_path_buf()),
                embed_bodies: false,
                diffuse: None,
                scope: SemanticIndexScope::Functions,
            },
        )
        .unwrap()
        .0;
        assert_eq!(
            no_root.binary_embeddings, with_root.binary_embeddings,
            "repo_root without embed_bodies must not change vectors"
        );
    }

    #[test]
    fn toggling_embed_bodies_invalidates_incremental_reuse() {
        let mut backend = MemoryBackend::new();
        backend
            .insert_node(Node::new(NodeType::Function, "authenticate").with_code_hash("h1"))
            .unwrap();

        let embedder = crate::semantic_embedder::SignHashEmbedder::new(64);
        let (index, stats) =
            build_index(&backend, &embedder, SemanticBuildOptions::fresh(64, None)).unwrap();
        assert_eq!(stats.embedded, 1);
        assert_eq!(index.model_id, SIGN_HASH_MODEL_ID);

        let (index_bodies, stats_bodies) = build_index(
            &backend,
            &embedder,
            SemanticBuildOptions {
                incremental: true,
                existing: Some(index),
                embed_bodies: true,
                ..SemanticBuildOptions::fresh(64, None)
            },
        )
        .unwrap();
        assert_eq!(stats_bodies.reused, 0);
        assert_eq!(stats_bodies.embedded, 1);
        assert_eq!(
            index_bodies.model_id,
            persist_semantic_model_id(SIGN_HASH_MODEL_ID, true)
        );

        let (_, stats_again) = build_index(
            &backend,
            &embedder,
            SemanticBuildOptions {
                incremental: true,
                existing: Some(index_bodies),
                embed_bodies: true,
                ..SemanticBuildOptions::fresh(64, None)
            },
        )
        .unwrap();
        assert_eq!(stats_again.reused, 1);
        assert_eq!(stats_again.embedded, 0);
    }

    #[test]
    fn hamming_top_k_covers_all_rows_when_k_large() {
        let dims = 8;
        let rows = 5usize;
        let mut bits = Vec::new();
        for i in 0..rows {
            bits.push(i as u8);
        }
        let index = SemanticIndex {
            schema_version: SEMANTIC_INDEX_SCHEMA_VERSION,
            model_id: SIGN_HASH_MODEL_ID.into(),
            dimensions: dims,
            graph_digest: None,
            model_path: None,
            tokenizer_path: None,
            entries: (0..rows)
                .map(|i| SemanticEntry {
                    node_id: Uuid::new_v4(),
                    name: format!("f{i}"),
                    qualified_name: None,
                    file_path: None,
                    code_hash: None,
                    node_type: None,
                    kind: None,
                    semantic_tags: None,
                })
                .collect(),
            binary_embeddings: bits,
        };
        let hits = hamming_top_k(&index, &[0u8], rows + 10);
        let rows_seen: HashSet<_> = hits.iter().map(|(r, _)| *r).collect();
        assert_eq!(rows_seen.len(), rows);
    }

    #[test]
    fn hash_embed_batch_preserves_per_text_vectors() {
        let emb = crate::semantic_embedder::SignHashEmbedder::new(32);
        let texts = ["checkout cart", "publish event", "process order"];
        let refs: Vec<&str> = texts.to_vec();
        let batch = emb.embed_batch(&refs).unwrap();
        for (text, row) in texts.iter().zip(batch) {
            assert_eq!(row, emb.embed(text).unwrap());
        }
    }

    #[test]
    fn embed_batch_preserves_order_under_parallelism() {
        let emb = crate::semantic_embedder::SignHashEmbedder::new(64);
        let texts: Vec<String> = (0..32)
            .map(|i| format!("symbol{i} unique{i} token"))
            .collect();
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        let batch = emb.embed_batch(&refs).unwrap();
        assert_eq!(batch.len(), texts.len());
        for (text, row) in texts.iter().zip(&batch) {
            assert_eq!(row, &emb.embed(text).unwrap());
        }
    }

    #[test]
    fn diffuse_index_embeds_call_neighbors() {
        use rgctl_graph::schema::{Edge, EdgeType};
        let mut backend = MemoryBackend::new();
        let caller = Node::new(NodeType::Function, "caller").with_signature("fn caller()");
        let callee = Node::new(NodeType::Function, "callee").with_signature("fn callee()");
        let caller_id = caller.id;
        let callee_id = callee.id;
        backend.insert_node(caller).unwrap();
        backend.insert_node(callee).unwrap();
        backend
            .insert_edge(Edge::new(caller_id, callee_id, EdgeType::Calls))
            .unwrap();

        let embedder = crate::semantic_embedder::SignHashEmbedder::new(64);
        let (index, stats) = build_index(
            &backend,
            &embedder,
            SemanticBuildOptions {
                diffuse: Some(crate::semantic_diffuse::DiffuseConfig {
                    alpha: 0.25,
                    iterations: 1,
                    mode: crate::semantic_diffuse::DiffuseNeighborMode::Callees,
                }),
                ..SemanticBuildOptions::fresh(64, None)
            },
        )
        .unwrap();
        assert_eq!(stats.embedded, 2);
        assert_eq!(index.len(), 2);
        assert!(index.entries.iter().any(|entry| entry.name == "caller"));
        assert!(index.entries.iter().any(|entry| entry.name == "callee"));
    }

    #[test]
    fn framework_tagged_nodes_rank_natural_language_queries() {
        use crate::semantic_fusion::{SemanticFusionConfig, query_index_with_fusion};
        use crate::semantic_vocab::VocabAccumulateEmbedder;

        let mut backend = MemoryBackend::new();
        // Keep QNs disjoint so name tokens do not leak across roles
        // (e.g. `OrderController.fetch_by_id` would pollute "controllers").
        let controller = Node::new(NodeType::Class, "OrderController")
            .with_qualified_name("com.example.web.OrderController")
            .with_property(
                "semantic_tags".into(),
                "controller controllers rest api spring".into(),
            )
            .with_property("framework_tags".into(), "role:controller".into());
        let endpoint = Node::new(NodeType::Function, "fetch_by_id")
            .with_qualified_name("com.example.api.fetch_by_id")
            .with_property(
                "semantic_tags".into(),
                "endpoint endpoints route routes http rest api handler spring".into(),
            )
            .with_property("framework_tags".into(), "role:endpoint".into());
        let reflection = Node::new(NodeType::Class, "DynamicLoader")
            .with_qualified_name("com.example.reflect.DynamicLoader")
            .with_property(
                "semantic_tags".into(),
                "reflection reflect dynamic introspection invoke jvm".into(),
            )
            .with_property("framework_tags".into(), "role:reflection".into());
        let unrelated = Node::new(NodeType::Function, "calculate_tax")
            .with_qualified_name("com.example.tax.calculate_tax")
            .with_signature("fn calculate_tax(amount: f64) -> f64");
        let ctrl_id = controller.id;
        let ep_id = endpoint.id;
        let refl_id = reflection.id;
        backend.insert_node(controller).unwrap();
        backend.insert_node(endpoint).unwrap();
        backend.insert_node(reflection).unwrap();
        backend.insert_node(unrelated).unwrap();

        let dims = 64;
        let embedder = VocabAccumulateEmbedder::new(dims).unwrap();
        let (index, _stats) =
            build_index(&backend, &embedder, SemanticBuildOptions::fresh(dims, None)).unwrap();
        assert!(
            index.len() >= 3,
            "tagged classes + endpoint should be indexed, got {}",
            index.len()
        );

        let fusion = SemanticFusionConfig {
            enabled: true,
            keyword_and: false,
            w_semantic: 0.5,
            w_name: 0.3,
            w_sketch: 0.2,
            w_blast: 0.0,
            w_centrality: 0.0,
            ..Default::default()
        };
        let reload = OnnxReloadOptions::default();

        let hits = query_index_with_fusion(
            &index,
            "hey give me all my end points",
            4,
            &reload,
            &fusion,
            None,
            Some(&backend),
            None,
        )
        .unwrap();
        assert_eq!(hits[0].entry.node_id, ep_id, "endpoints → fetch_by_id");

        let hits = query_index_with_fusion(
            &index,
            "give me all my controllers",
            4,
            &reload,
            &fusion,
            None,
            Some(&backend),
            None,
        )
        .unwrap();
        assert_eq!(
            hits[0].entry.node_id, ctrl_id,
            "controllers → OrderController"
        );

        let hits = query_index_with_fusion(
            &index,
            "give me all my reflection classes",
            4,
            &reload,
            &fusion,
            None,
            Some(&backend),
            None,
        )
        .unwrap();
        assert_eq!(hits[0].entry.node_id, refl_id, "reflection → DynamicLoader");
    }
}
