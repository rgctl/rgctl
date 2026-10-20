//! Append-only disk spill for extract → columnar compile without full `Vec` residency.
//!
//! Record layout (little-endian):
//! - **nodes.seg**: `id[16]` + `len[u64]` + `bincode(Node)`
//! - **edges.seg**: `from[16]` + `to[16]` + `edge_type[u8]` + `pad[7]` + `len[u64]` + `bincode(Edge)`
//!
//! Compile externally sorts by the same keys as [`crate::write_columnar_from_nodes_edges`]
//! and hashes the spilled bincode blobs for digest identity.

use crate::columnar_snapshot::{
    EdgeRow, StringPool, append_node_columnar_prehashed, write_columnar_assembled,
};
use crate::csr::edge_type_to_u8;
use crate::schema::{Edge, Node, NodeType};
use rgctl_error::{Error, Result};
use std::cmp::Ordering;
use rayon::prelude::*;
use std::thread;
use std::collections::{BinaryHeap, HashMap};
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
use uuid::Uuid;

/// Default run size for external merge-sort (~256 MiB of record payload).
///
/// Larger runs cut multi-way merge I/O on kernel-scale spills (nodes/edges
/// segs are hundreds of MiB). Peak RSS during sort grows by one run buffer.
pub const DEFAULT_SORT_RUN_BYTES: usize = 256 * 1024 * 1024;

/// Process-wide sort-run override (`0` = use [`DEFAULT_SORT_RUN_BYTES`]).
/// Set by discover `--with-limits` for constrained containers; leave unset on desktop.
static SORT_RUN_BYTES_OVERRIDE: AtomicUsize = AtomicUsize::new(0);

/// Cap external-sort run buffers for this process (discover `--with-limits`).
pub fn set_sort_run_bytes_override(bytes: Option<usize>) {
    SORT_RUN_BYTES_OVERRIDE.store(bytes.unwrap_or(0), AtomicOrdering::Relaxed);
}

fn effective_sort_run_bytes() -> usize {
    let o = SORT_RUN_BYTES_OVERRIDE.load(AtomicOrdering::Relaxed);
    if o == 0 {
        DEFAULT_SORT_RUN_BYTES
    } else {
        o
    }
}

const NODE_KEY_LEN: usize = 16;
const EDGE_KEY_LEN: usize = 16 + 16 + 8; // from + to + type/pad
/// Cap for one spilled bincode blob. Larger length prefixes are treated as corrupt
/// (ASCII hex / uninitialized bytes interpreted as `u64`).
const MAX_SPILL_BLOB_BYTES: u64 = 32 * 1024 * 1024;

/// Append-only spill writers for nodes and edges during extract.
pub struct SegmentedSpill {
    dir: PathBuf,
    nodes: BufWriter<File>,
    edges: BufWriter<File>,
    node_count: usize,
    edge_count: usize,
    /// Reused bincode buffer (avoids a fresh `Vec<u8>` per append).
    scratch: Vec<u8>,
}

/// Closed spill ready for external sort + columnar compile.
pub struct FinishedSpill {
    dir: PathBuf,
    node_count: usize,
    edge_count: usize,
}

impl SegmentedSpill {
    /// Create spill files under `dir` (created if missing).
    pub fn create(dir: impl AsRef<Path>) -> Result<Self> {
        let dir = dir.as_ref().to_path_buf();
        fs::create_dir_all(&dir)?;
        let nodes = BufWriter::with_capacity(8 * 1024 * 1024, File::create(dir.join("nodes.seg"))?);
        let edges = BufWriter::with_capacity(8 * 1024 * 1024, File::create(dir.join("edges.seg"))?);
        Ok(Self {
            dir,
            nodes,
            edges,
            node_count: 0,
            edge_count: 0,
            scratch: Vec::with_capacity(64 * 1024),
        })
    }

    /// Spill directory path.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Nodes appended so far.
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Edges appended so far.
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Append a node as length-prefixed bincode with UUID key prefix.
    pub fn append_node(&mut self, node: &Node) -> Result<()> {
        self.scratch.clear();
        bincode::serialize_into(&mut self.scratch, node)
            .map_err(|e| Error::SerdeError(format!("segmented spill node serialize: {e}")))?;
        self.nodes.write_all(node.id.as_bytes())?;
        self.nodes
            .write_all(&(self.scratch.len() as u64).to_le_bytes())?;
        self.nodes.write_all(&self.scratch)?;
        self.node_count += 1;
        Ok(())
    }

    /// Append an edge as length-prefixed bincode with sort key prefix.
    ///
    /// Serializes [`Edge::for_columnar_digest`] so digest bytes match topology-only
    /// columnar rows after rematerialize/compact.
    pub fn append_edge(&mut self, edge: &Edge) -> Result<()> {
        let canonical = edge.for_columnar_digest();
        self.scratch.clear();
        bincode::serialize_into(&mut self.scratch, &canonical)
            .map_err(|e| Error::SerdeError(format!("segmented spill edge serialize: {e}")))?;
        let mut key = [0u8; EDGE_KEY_LEN];
        key[..16].copy_from_slice(canonical.from.as_bytes());
        key[16..32].copy_from_slice(canonical.to.as_bytes());
        key[32] = edge_type_to_u8(canonical.edge_type);
        self.edges.write_all(&key)?;
        self.edges
            .write_all(&(self.scratch.len() as u64).to_le_bytes())?;
        self.edges.write_all(&self.scratch)?;
        self.edge_count += 1;
        Ok(())
    }

    /// Flush and close writers.
    pub fn finish(mut self) -> Result<FinishedSpill> {
        self.nodes.flush()?;
        self.edges.flush()?;
        // Drop writers so files are closed before sort reopens them.
        drop(self.nodes);
        drop(self.edges);
        Ok(FinishedSpill {
            dir: self.dir,
            node_count: self.node_count,
            edge_count: self.edge_count,
        })
    }
}

impl FinishedSpill {
    /// Spill directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Nodes appended so far.
    pub fn node_count(&self) -> usize {
        self.node_count
    }

    /// Edges appended so far.
    pub fn edge_count(&self) -> usize {
        self.edge_count
    }

    /// Remove the spill directory tree.
    pub fn cleanup(self) -> Result<()> {
        if self.dir.exists() {
            fs::remove_dir_all(&self.dir)?;
        }
        Ok(())
    }
}

/// Read externally-sorted nodes and edges from a finished spill (for delta compact).
pub fn materialize_sorted_graph(spill: &FinishedSpill) -> Result<(Vec<Node>, Vec<Edge>)> {
    let dir = &spill.dir;
    let nodes_unsorted = dir.join("nodes.seg");
    let edges_unsorted = dir.join("edges.seg");
    let nodes_sorted = dir.join("nodes.sorted.seg");
    let edges_sorted = dir.join("edges.sorted.seg");

    thread::scope(|scope| -> Result<()> {
        let node_err = scope.spawn(|| {
            external_sort_records(
                &nodes_unsorted,
                &nodes_sorted,
                NODE_KEY_LEN,
                effective_sort_run_bytes(),
                spill.node_count,
            )
        });
        let edge_err = scope.spawn(|| {
            external_sort_records(
                &edges_unsorted,
                &edges_sorted,
                EDGE_KEY_LEN,
                effective_sort_run_bytes(),
                spill.edge_count,
            )
        });
        node_err.join().map_err(|_| Error::GraphError("node sort panicked".into()))??;
        edge_err.join().map_err(|_| Error::GraphError("edge sort panicked".into()))??;
        Ok(())
    })?;

    let mut nodes = Vec::with_capacity(spill.node_count);
    {
        let mut reader = BufReader::with_capacity(8 * 1024 * 1024, File::open(&nodes_sorted)?);
        for i in 0..spill.node_count {
            let rec = read_one_record_at(&mut reader, NODE_KEY_LEN, "node", i, spill.node_count)?;
            let node: Node = bincode::deserialize(&rec.blob)
                .map_err(|e| Error::SerdeError(format!("segmented spill node deserialize: {e}")))?;
            nodes.push(node);
        }
    }

    let mut edges = Vec::with_capacity(spill.edge_count);
    {
        let mut reader = BufReader::with_capacity(8 * 1024 * 1024, File::open(&edges_sorted)?);
        for i in 0..spill.edge_count {
            let rec = read_one_record_at(&mut reader, EDGE_KEY_LEN, "edge", i, spill.edge_count)?;
            let edge: Edge = bincode::deserialize(&rec.blob)
                .map_err(|e| Error::SerdeError(format!("segmented spill edge deserialize: {e}")))?;
            edges.push(edge);
        }
    }

    Ok((nodes, edges))
}

/// Compile a columnar v2 snapshot from a finished spill (external sort + stream encode).
///
/// Digest matches [`crate::write_columnar_from_nodes_edges`] for the same node/edge set.
/// Removes the spill directory on success.
pub fn write_columnar_from_spill(spill: FinishedSpill, path: &Path) -> Result<String> {
    let dir = spill.dir.clone();
    let node_count = spill.node_count;
    let edge_count = spill.edge_count;

    let nodes_unsorted = dir.join("nodes.seg");
    let edges_unsorted = dir.join("edges.seg");
    let nodes_sorted = dir.join("nodes.sorted.seg");
    let edges_sorted = dir.join("edges.sorted.seg");

    thread::scope(|scope| -> Result<()> {
        let node_err = scope.spawn(|| {
            external_sort_records(
                &nodes_unsorted,
                &nodes_sorted,
                NODE_KEY_LEN,
                effective_sort_run_bytes(),
                node_count,
            )
        });
        let edge_err = scope.spawn(|| {
            external_sort_records(
                &edges_unsorted,
                &edges_sorted,
                EDGE_KEY_LEN,
                effective_sort_run_bytes(),
                edge_count,
            )
        });
        node_err.join().map_err(|_| Error::GraphError("node sort panicked".into()))??;
        edge_err.join().map_err(|_| Error::GraphError("edge sort panicked".into()))??;
        Ok(())
    })?;

    let mut hasher = blake3::Hasher::new();
    let mut strings = StringPool::new();
    let mut node_rows = Vec::with_capacity(node_count);
    let mut extensions_blob = Vec::new();
    let mut name_index: HashMap<String, Vec<Uuid>> = HashMap::new();
    let mut type_index: HashMap<NodeType, Vec<Uuid>> = HashMap::new();

    {
        let mut reader = BufReader::with_capacity(8 * 1024 * 1024, File::open(&nodes_sorted)?);
        let mut prev_key: Option<[u8; MAX_SPILL_KEY_LEN]> = None;
        let mut prev_key_len = 0usize;
        for i in 0..node_count {
            let rec = read_one_record_at(&mut reader, NODE_KEY_LEN, "node", i, node_count)?;
            if prev_key
                .as_ref()
                .is_some_and(|k| k[..prev_key_len] == rec.key[..rec.key_len])
            {
                continue;
            }
            prev_key = Some(rec.key);
            prev_key_len = rec.key_len;
            let node: Node = bincode::deserialize(&rec.blob)
                .map_err(|e| Error::SerdeError(format!("segmented spill node deserialize: {e}")))?;
            append_node_columnar_prehashed(
                &node,
                &rec.blob,
                &mut hasher,
                &mut strings,
                &mut extensions_blob,
                &mut name_index,
                &mut type_index,
                &mut node_rows,
                None,
            )?;
        }
    }

    let mut edge_rows = Vec::with_capacity(edge_count);
    {
        let mut reader = BufReader::with_capacity(8 * 1024 * 1024, File::open(&edges_sorted)?);
        let mut prev_key: Option<[u8; MAX_SPILL_KEY_LEN]> = None;
        let mut prev_key_len = 0usize;
        for i in 0..edge_count {
            let rec = read_one_record_at(&mut reader, EDGE_KEY_LEN, "edge", i, edge_count)?;
            if prev_key
                .as_ref()
                .is_some_and(|k| k[..prev_key_len] == rec.key[..rec.key_len])
            {
                continue;
            }
            prev_key = Some(rec.key);
            prev_key_len = rec.key_len;
            hasher.update(&rec.blob);
            let from = Uuid::from_bytes(rec.key[..16].try_into().unwrap());
            let to = Uuid::from_bytes(rec.key[16..32].try_into().unwrap());
            let edge_type = rec.key[32];
            edge_rows.push(EdgeRow {
                from: *from.as_bytes(),
                to: *to.as_bytes(),
                edge_type,
                _pad: [0; 7],
            });
        }
    }

    let content_digest = hasher.finalize().to_hex().to_string();
    write_columnar_assembled(
        path,
        &node_rows,
        &edge_rows,
        &strings,
        &extensions_blob,
        &name_index,
        &type_index,
        &content_digest,
    )?;

    spill.cleanup()?;
    Ok(content_digest)
}

fn external_sort_records(
    input: &Path,
    output: &Path,
    key_len: usize,
    run_bytes: usize,
    record_count: usize,
) -> Result<()> {
    if record_count == 0 {
        File::create(output)?;
        return Ok(());
    }

    let meta = fs::metadata(input)?;
    // Small enough to sort in one pass in RAM.
    if meta.len() as usize <= run_bytes || record_count < 10_000 {
        let mut records = read_all_records(input, key_len, record_count)?;
        records.par_sort_unstable_by(|a, b| a.key[..a.key_len].cmp(&b.key[..b.key_len]));
        write_records(output, &records)?;
        return Ok(());
    }

    let parent = output.parent().unwrap_or_else(|| Path::new("."));
    let run_prefix = output
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("run");
    let mut run_paths = Vec::new();
    let mut reader = BufReader::with_capacity(8 * 1024 * 1024, File::open(input)?);
    let mut remaining = record_count;
    let mut run_idx = 0usize;

    while remaining > 0 {
        let mut batch = Vec::new();
        let mut batch_bytes = 0usize;
        while remaining > 0 && (batch.is_empty() || batch_bytes < run_bytes) {
            let rec = read_one_record(&mut reader, key_len)?;
            batch_bytes += rec.key_len + 8 + rec.blob.len();
            batch.push(rec);
            remaining -= 1;
        }
        batch.par_sort_unstable_by(|a, b| a.key[..a.key_len].cmp(&b.key[..b.key_len]));
        let run_path = parent.join(format!("{run_prefix}-{run_idx}.seg"));
        write_records(&run_path, &batch)?;
        run_paths.push(run_path);
        run_idx += 1;
    }

    if run_paths.len() == 1 {
        fs::rename(&run_paths[0], output)?;
        return Ok(());
    }

    k_way_merge(&run_paths, output, key_len)?;
    for p in run_paths {
        let _ = fs::remove_file(p);
    }
    Ok(())
}

const MAX_SPILL_KEY_LEN: usize = EDGE_KEY_LEN;

#[derive(Debug)]
struct SpillRecord {
    key: [u8; MAX_SPILL_KEY_LEN],
    key_len: usize,
    blob: Vec<u8>,
}

fn read_one_record<R: Read>(reader: &mut R, key_len: usize) -> Result<SpillRecord> {
    read_record_or_eof(reader, key_len)?.ok_or_else(|| {
        Error::IoError(std::io::Error::new(
            ErrorKind::UnexpectedEof,
            "spill: unexpected end of records",
        ))
    })
}

fn read_one_record_at<R: Read>(
    reader: &mut R,
    key_len: usize,
    kind: &str,
    index: usize,
    total: usize,
) -> Result<SpillRecord> {
    read_record_or_eof(reader, key_len)?.ok_or_else(|| {
        Error::IoError(std::io::Error::new(
            ErrorKind::UnexpectedEof,
            format!("spill {kind} truncated at {index}/{total}"),
        ))
    })
}

/// Read one length-prefixed spill record. `Ok(None)` only at a clean record boundary.
fn read_record_or_eof<R: Read>(reader: &mut R, key_len: usize) -> Result<Option<SpillRecord>> {
    let mut key = [0u8; MAX_SPILL_KEY_LEN];
    let mut got = 0usize;
    while got < key_len {
        let n = reader.read(&mut key[got..key_len])?;
        if n == 0 {
            if got == 0 {
                return Ok(None);
            }
            return Err(Error::IoError(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                format!("spill record truncated after {got}/{key_len} key bytes"),
            )));
        }
        got += n;
    }

    let mut len_buf = [0u8; 8];
    reader.read_exact(&mut len_buf).map_err(|e| {
        if e.kind() == ErrorKind::UnexpectedEof {
            Error::IoError(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                "spill record truncated at length prefix",
            ))
        } else {
            Error::IoError(e)
        }
    })?;
    let len_u64 = u64::from_le_bytes(len_buf);
    if len_u64 > MAX_SPILL_BLOB_BYTES {
        return Err(Error::SerdeError(format!(
            "spill record length {len_u64} exceeds {MAX_SPILL_BLOB_BYTES} (corrupt length prefix)"
        )));
    }
    let len = len_u64 as usize;
    let mut blob = Vec::new();
    blob.try_reserve_exact(len).map_err(|_| {
        Error::SerdeError(format!("spill record length {len} too large to allocate"))
    })?;
    blob.resize(len, 0);
    reader.read_exact(&mut blob).map_err(|e| {
        if e.kind() == ErrorKind::UnexpectedEof {
            Error::IoError(std::io::Error::new(
                ErrorKind::UnexpectedEof,
                format!("spill record truncated: expected {len} blob bytes"),
            ))
        } else {
            Error::IoError(e)
        }
    })?;
    Ok(Some(SpillRecord {
        key,
        key_len,
        blob,
    }))
}

fn read_all_records(path: &Path, key_len: usize, count: usize) -> Result<Vec<SpillRecord>> {
    let mut reader = BufReader::with_capacity(8 * 1024 * 1024, File::open(path)?);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        out.push(read_one_record(&mut reader, key_len)?);
    }
    Ok(out)
}

fn write_records(path: &Path, records: &[SpillRecord]) -> Result<()> {
    let mut w = BufWriter::with_capacity(8 * 1024 * 1024, File::create(path)?);
    for rec in records {
        w.write_all(&rec.key[..rec.key_len])?;
        w.write_all(&(rec.blob.len() as u64).to_le_bytes())?;
        w.write_all(&rec.blob)?;
    }
    w.flush()?;
    Ok(())
}

#[derive(Eq)]
struct HeapEntry {
    key: [u8; MAX_SPILL_KEY_LEN],
    key_len: usize,
    blob: Vec<u8>,
    run_idx: usize,
}

impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key[..self.key_len] == other.key[..other.key_len] && self.run_idx == other.run_idx
    }
}

impl Ord for HeapEntry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse for min-heap via BinaryHeap
        match other.key[..other.key_len].cmp(&self.key[..self.key_len]) {
            std::cmp::Ordering::Equal => other.run_idx.cmp(&self.run_idx),
            o => o,
        }
    }
}

impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn k_way_merge(run_paths: &[PathBuf], output: &Path, key_len: usize) -> Result<()> {
    let mut readers: Vec<BufReader<File>> = run_paths
        .iter()
        .map(|p| Ok(BufReader::with_capacity(1024 * 1024, File::open(p)?)))
        .collect::<Result<Vec<_>>>()?;

    let mut heap = BinaryHeap::new();
    for (i, reader) in readers.iter_mut().enumerate() {
        match read_record_or_eof(reader, key_len)? {
            Some(rec) => heap.push(HeapEntry {
                key: rec.key,
                key_len: rec.key_len,
                blob: rec.blob,
                run_idx: i,
            }),
            None => {}
        }
    }

    let mut out = BufWriter::with_capacity(8 * 1024 * 1024, File::create(output)?);
    while let Some(entry) = heap.pop() {
        out.write_all(&entry.key[..entry.key_len])?;
        out.write_all(&(entry.blob.len() as u64).to_le_bytes())?;
        out.write_all(&entry.blob)?;
        let i = entry.run_idx;
        if let Some(rec) = read_record_or_eof(&mut readers[i], key_len)? {
            heap.push(HeapEntry {
                key: rec.key,
                key_len: rec.key_len,
                blob: rec.blob,
                run_idx: i,
            });
        }
    }
    out.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::EdgeType;
    use crate::write_columnar_from_nodes_edges;
    use tempfile::TempDir;

    #[test]
    fn spill_compile_digest_matches_vec_path() {
        let a = Node::new(NodeType::Function, "a");
        let b = Node::new(NodeType::Function, "b");
        let a_id = a.id;
        let b_id = b.id;
        let e1 = Edge::new(a_id, b_id, EdgeType::Calls);
        let e2 = Edge::new(b_id, a_id, EdgeType::Calls);

        let tmp = TempDir::new().unwrap();
        let mut spill = SegmentedSpill::create(tmp.path().join("spill")).unwrap();
        // Append out of order to exercise sort.
        spill.append_node(&b).unwrap();
        spill.append_node(&a).unwrap();
        spill.append_edge(&e2).unwrap();
        spill.append_edge(&e1).unwrap();
        let finished = spill.finish().unwrap();

        let path_spill = tmp.path().join("from_spill.bin");
        let path_vecs = tmp.path().join("from_vecs.bin");
        let d_spill = write_columnar_from_spill(finished, &path_spill).unwrap();
        let d_vecs = write_columnar_from_nodes_edges(vec![a, b], vec![e1, e2], &path_vecs).unwrap();
        assert_eq!(d_spill, d_vecs);
    }

    #[test]
    fn garbage_ascii_length_prefix_is_error_not_abort() {
        // 4051096950100079460 == little-endian ASCII "d3be6c88" (hex digest fragment).
        let mut bytes = vec![0u8; NODE_KEY_LEN];
        bytes.extend_from_slice(b"d3be6c88");
        let err = read_one_record(&mut bytes.as_slice(), NODE_KEY_LEN).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("corrupt length prefix") || msg.contains("exceeds"),
            "{msg}"
        );
    }

    #[test]
    fn truncated_spill_reports_record_index() {
        let mut bytes = vec![0u8; NODE_KEY_LEN];
        bytes.extend_from_slice(&8u64.to_le_bytes());
        bytes.extend_from_slice(&[1, 2, 3]); // short blob
        let err = read_one_record_at(&mut bytes.as_slice(), NODE_KEY_LEN, "node", 0, 1).unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("truncated"), "{msg}");
    }
}
