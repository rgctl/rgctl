//! Symbol and relationship extraction

use crate::discovery::{DiscoveryConfig, FileDiscoverer};
use crate::graph_builder::GraphBuilder;
use crate::usage_detector::{ConfigUsage, ConfigUsageDetector};
use rgctl_error::{Error, Result};
use rgctl_graph::code_index::hash_code;
use rgctl_graph::content_store::hash_bytes;
use rgctl_graph::structural_sketch::{TokenBloom, build_token_bloom};
use rgctl_plugin_api::{ConfigKey, Relation, Symbol, SymbolType};
use rgctl_registry::LanguageRegistry;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Extracts symbols and relationships from a single file.
pub struct Extractor {
    registry: Arc<LanguageRegistry>,
}

/// Precomputed pass-1 work for one symbol (filled on extract workers).
#[derive(Debug, Clone, Default)]
pub struct SymbolPass1Prep {
    /// BLAKE3 hex digest of the symbol body when present.
    pub code_hash: Option<String>,
    /// Token bloom for functions (when sketched).
    pub token_bloom: Option<TokenBloom>,
}

/// Result of extracting a single file.
#[derive(Debug, Default, Clone)]
pub struct FileExtraction {
    /// Path to the source file
    pub path: PathBuf,
    /// BLAKE3 hex of `source` (computed once on the extract worker).
    pub file_hash: Option<String>,
    /// Extracted code symbols
    pub symbols: Vec<Symbol>,
    /// Parallel to [`Self::symbols`]: hash/bloom precomputed on the worker.
    pub symbol_preps: Vec<SymbolPass1Prep>,
    /// Extracted symbol relations
    pub relations: Vec<Relation>,
    /// Extracted configuration keys
    pub config_keys: Vec<ConfigKey>,
    /// Detected configuration usages in source
    pub config_usages: Vec<ConfigUsage>,
    /// Cached source bytes (avoids re-reading file during graph population)
    pub source: Vec<u8>,
    /// Out-of-line content blobs keyed by Blake3 hex (`body_ref`).
    pub content_blobs: HashMap<String, String>,
}

/// Lightweight remainder after pass-1 graph population (symbols committed; source dropped).
#[derive(Debug, Default)]
pub struct ExtractionTail {
    /// Source file path (used to sort pass-2 application order).
    pub path: PathBuf,
    /// Relations resolved in pass 2
    pub relations: Vec<Relation>,
    /// Config usages linked in pass 2
    pub config_usages: Vec<ConfigUsage>,
}

#[derive(Debug, Default, Clone, Copy)]
struct Pass1Profile {
    symbol_processing: Duration,
    config_key_processing: Duration,
}

#[derive(Debug, Default, Clone, Copy)]
struct Pass2Profile {
    relation_resolution: Duration,
    config_usage_resolution: Duration,
}

impl Extractor {
    /// Create a new extractor backed by the given registry.
    pub fn new(registry: Arc<LanguageRegistry>) -> Self {
        Self { registry }
    }

    /// Discover and extract all processable files under `root`.
    pub fn extract_repository(
        &self,
        root: &Path,
        discovery: &DiscoveryConfig,
    ) -> Result<Vec<FileExtraction>> {
        let discoverer = FileDiscoverer::with_config(Arc::clone(&self.registry), discovery.clone());
        let files = discoverer.discover(root)?;
        Ok(files
            .iter()
            .filter_map(|path| match self.extract_file(path) {
                Ok(extraction) => Some(extraction),
                Err(err) => {
                    tracing::warn!("Failed to extract {}: {}", path.display(), err);
                    None
                }
            })
            .collect())
    }

    /// Extract symbols, relations, and config references from one file.
    pub fn extract_file(&self, path: &Path) -> Result<FileExtraction> {
        self.extract_file_with_source(path, std::fs::read(path)?)
    }

    /// Same as [`Extractor::extract_file`], with the bytes supplied by the
    /// caller. `path` is still used for language detection and for the paths
    /// recorded on the returned symbols and relations, but it does not have to
    /// exist on disk.
    pub fn extract_file_with_source(&self, path: &Path, source: Vec<u8>) -> Result<FileExtraction> {
        let file_hash = Some(hash_bytes(&source));
        if let Ok(plugin) = self.registry.get_plugin_for_file(path) {
            let extracted = plugin.extract_all(path, &source)?;
            let config_usages = ConfigUsageDetector::detect(plugin.language_id(), &source, path);
            let symbol_preps = prepare_symbol_pass1(&source, &extracted.symbols);
            return Ok(FileExtraction {
                path: path.to_path_buf(),
                file_hash,
                symbols: extracted.symbols,
                symbol_preps,
                relations: extracted.relations,
                config_keys: Vec::new(),
                config_usages,
                source,
                content_blobs: extracted.content_blobs,
            });
        }

        // Manifests: Dependency extractors (section 3).
        if self.registry.is_manifest_file(path) {
            let (symbols, relations) = crate::manifests::extract_manifest(path, &source);
            let symbol_preps = prepare_symbol_pass1(&source, &symbols);
            return Ok(FileExtraction {
                path: path.to_path_buf(),
                file_hash,
                symbols,
                symbol_preps,
                relations,
                config_keys: Vec::new(),
                config_usages: Vec::new(),
                source,
                content_blobs: HashMap::new(),
            });
        }

        if let Ok(config_plugin) = self.registry.get_config_plugin_for_file(path) {
            let config_keys = config_plugin.extract_config_keys(path, &source)?;
            return Ok(FileExtraction {
                path: path.to_path_buf(),
                file_hash,
                symbols: Vec::new(),
                symbol_preps: Vec::new(),
                relations: Vec::new(),
                config_keys,
                config_usages: Vec::new(),
                source,
                content_blobs: HashMap::new(),
            });
        }

        Err(Error::UnsupportedLanguage(
            path.to_string_lossy().to_string(),
        ))
    }

    /// Pass 1 for one file: add symbols/config keys, then drop source from memory.
    pub fn populate_pass1(
        &self,
        extraction: &mut FileExtraction,
        builder: &mut GraphBuilder,
    ) -> Result<ExtractionTail> {
        let (tail, _profile) = self.populate_pass1_profiled(extraction, builder)?;
        Ok(tail)
    }

    fn populate_pass1_profiled(
        &self,
        extraction: &mut FileExtraction,
        builder: &mut GraphBuilder,
    ) -> Result<(ExtractionTail, Pass1Profile)> {
        use std::time::Instant;

        let mut profile = Pass1Profile::default();
        builder.begin_file_batch(&extraction.path);
        let file_id = builder.ensure_file_node_with_source(
            &extraction.path,
            (!extraction.source.is_empty()).then_some(extraction.source.as_slice()),
        );
        builder.merge_content_blobs(&extraction.content_blobs);

        if !extraction.symbols.is_empty() {
            let symbol_start = Instant::now();
            let use_preps = extraction.symbol_preps.len() == extraction.symbols.len();
            if use_preps {
                for (symbol, prep) in extraction.symbols.iter().zip(extraction.symbol_preps.iter())
                {
                    builder.add_symbol_with_prep(symbol, file_id, None, Some(prep));
                }
            } else {
                let source =
                    (!extraction.source.is_empty()).then_some(extraction.source.as_slice());
                let line_offsets = source.map(line_start_offsets);
                for symbol in &extraction.symbols {
                    let body = source.and_then(|bytes| {
                        let offsets = line_offsets.as_ref()?;
                        symbol_body_from_source(bytes, offsets, symbol)
                    });
                    builder.add_symbol_with_prep(symbol, file_id, body, None);
                }
            }
            profile.symbol_processing += symbol_start.elapsed();
        }

        if !extraction.config_keys.is_empty() {
            let config_key_start = Instant::now();
            for key in &extraction.config_keys {
                builder.add_config_key(key, file_id);
            }
            profile.config_key_processing += config_key_start.elapsed();
        }

        extraction.content_blobs.clear();
        extraction.source.clear();
        extraction.symbols.clear();
        extraction.symbol_preps.clear();
        extraction.config_keys.clear();
        builder.end_file_batch();

        Ok((
            ExtractionTail {
                path: extraction.path.clone(),
                relations: std::mem::take(&mut extraction.relations),
                config_usages: std::mem::take(&mut extraction.config_usages),
            },
            profile,
        ))
    }

    /// Pass 2: resolve relations and config usages (requires [`GraphBuilder::build_resolution_indexes`]).
    ///
    /// Tails are applied in stable path order so parallel extract completion order
    /// cannot change which relations see mid-pass stubs first (#125).
    pub fn populate_pass2(
        &self,
        tails: &[ExtractionTail],
        builder: &mut GraphBuilder,
    ) -> Result<()> {
        let _ = self.populate_pass2_profiled(tails, builder)?;
        Ok(())
    }

    fn populate_pass2_profiled(
        &self,
        tails: &[ExtractionTail],
        builder: &mut GraphBuilder,
    ) -> Result<Pass2Profile> {
        use std::time::Instant;

        let mut ordered: Vec<&ExtractionTail> = tails.iter().collect();
        ordered.sort_by(|a, b| a.path.cmp(&b.path));

        let mut profile = Pass2Profile::default();
        for tail in ordered {
            if !tail.relations.is_empty() {
                let relation_start = Instant::now();
                for relation in &tail.relations {
                    builder.add_relation(relation)?;
                }
                profile.relation_resolution += relation_start.elapsed();
            }

            if !tail.config_usages.is_empty() {
                let config_usage_start = Instant::now();
                for usage in &tail.config_usages {
                    builder.link_config_usage(
                        &usage.file,
                        usage.line,
                        &usage.key,
                        usage.usage_type,
                    );
                }
                profile.config_usage_resolution += config_usage_start.elapsed();
            }
        }
        Ok(profile)
    }

    /// Merge extracted files into a graph builder.
    pub fn populate_graph(
        &self,
        extractions: &[FileExtraction],
        builder: &mut GraphBuilder,
    ) -> Result<()> {
        use std::time::Instant;
        use tracing::info;

        let total_start = Instant::now();
        let file_count = extractions.len();
        let total_symbols: usize = extractions.iter().map(|e| e.symbols.len()).sum();
        let total_relations: usize = extractions.iter().map(|e| e.relations.len()).sum();
        let total_config_keys: usize = extractions.iter().map(|e| e.config_keys.len()).sum();
        let total_config_usages: usize = extractions.iter().map(|e| e.config_usages.len()).sum();

        info!(
            file_count,
            total_symbols,
            total_relations,
            total_config_keys,
            total_config_usages,
            "populate_graph starting"
        );

        let file_io_time = std::time::Duration::ZERO;
        let mut symbol_time = std::time::Duration::ZERO;
        let mut relation_time = std::time::Duration::ZERO;
        let mut config_key_time = std::time::Duration::ZERO;
        let mut config_usage_time = std::time::Duration::ZERO;

        let mut tails = Vec::with_capacity(file_count);
        for extraction in extractions {
            let mut owned = extraction.clone();
            let (tail, pass1_profile) = self.populate_pass1_profiled(&mut owned, builder)?;
            symbol_time += pass1_profile.symbol_processing;
            config_key_time += pass1_profile.config_key_processing;
            tails.push(tail);
        }

        builder.build_resolution_indexes();

        let pass2_profile = self.populate_pass2_profiled(&tails, builder)?;
        relation_time += pass2_profile.relation_resolution;
        config_usage_time += pass2_profile.config_usage_resolution;

        let total_elapsed = total_start.elapsed();
        info!(
            elapsed_total_secs = total_elapsed.as_secs_f64(),
            file_io_secs = file_io_time.as_secs_f64(),
            symbol_processing_secs = symbol_time.as_secs_f64(),
            relation_resolution_secs = relation_time.as_secs_f64(),
            config_key_secs = config_key_time.as_secs_f64(),
            config_usage_secs = config_usage_time.as_secs_f64(),
            "populate_graph complete"
        );

        builder.log_resolution_stats();

        Ok(())
    }
}

fn line_start_offsets(source: &[u8]) -> Vec<usize> {
    let mut offsets = Vec::with_capacity(source.len() / 32 + 1);
    offsets.push(0);
    for (i, &b) in source.iter().enumerate() {
        if b == b'\n' {
            offsets.push(i + 1);
        }
    }
    offsets
}

fn symbol_body_from_source<'a>(
    source: &'a [u8],
    line_offsets: &[usize],
    symbol: &Symbol,
) -> Option<&'a str> {
    let start = symbol.location.start_line.saturating_sub(1);
    let end_line = symbol.location.end_line.max(symbol.location.start_line);
    if start >= line_offsets.len() {
        return None;
    }
    let start_byte = line_offsets[start];
    let end_byte = if end_line < line_offsets.len() {
        line_offsets[end_line]
    } else {
        source.len()
    };
    let slice = source.get(start_byte..end_byte)?;
    let text = std::str::from_utf8(slice).ok()?.trim_end_matches('\n');
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn prepare_symbol_pass1(source: &[u8], symbols: &[Symbol]) -> Vec<SymbolPass1Prep> {
    if symbols.is_empty() {
        return Vec::new();
    }
    let offsets = line_start_offsets(source);
    let mut preps = Vec::with_capacity(symbols.len());
    for symbol in symbols {
        let body = symbol_body_from_source(source, &offsets, symbol);
        let code_hash = body.map(hash_code);
        let token_bloom = if matches!(symbol.symbol_type, SymbolType::Function) {
            Some(build_token_bloom(
                &symbol.name,
                symbol.qualified_name.as_deref(),
                symbol.signature.as_deref(),
                body,
            ))
        } else {
            None
        };
        preps.push(SymbolPass1Prep {
            code_hash,
            token_bloom,
        });
    }
    preps
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::Duration;
    use tempfile::TempDir;

    #[test]
    fn test_extract_rust_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("main.rs");
        fs::write(&path, "fn hello() {}\nfn world() {}\n").unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let result = extractor.extract_file(&path).unwrap();

        assert_eq!(result.symbols.len(), 2);
        assert!(result.symbols.iter().any(|s| s.name == "hello"));
    }

    #[test]
    fn test_extract_yaml_config() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("config.yaml");
        fs::write(&path, "server:\n  port: 8080\n").unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let result = extractor.extract_file(&path).unwrap();

        assert!(
            result
                .config_keys
                .iter()
                .any(|k| k.key_path == "server.port")
        );
    }

    #[test]
    fn ruby_include_extends_populates_graph() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../rgctl-tests/ecommerce-ruby/lib/order_dto.rb");
        if !path.is_file() {
            return;
        }
        let registry = std::sync::Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let extraction = extractor.extract_file(&path).expect("extract");
        assert!(
            extraction
                .relations
                .iter()
                .any(|r| r.relation_type == rgctl_plugin_api::RelationType::Extends)
        );
        let mut builder = GraphBuilder::new();
        extractor
            .populate_graph(&[extraction], &mut builder)
            .expect("populate");
        let (_, edges) = builder.into_graph();
        assert!(
            edges
                .iter()
                .any(|e| e.edge_type == rgctl_graph::schema::EdgeType::Extends),
            "expected Extends edge for include Trackable"
        );
    }

    #[test]
    fn test_populate_graph_ruby_ecommerce_calls() {
        use crate::discovery::DiscoveryConfig;
        use std::path::Path;

        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../rgctl-tests/ecommerce-ruby");
        if !root.is_dir() {
            return;
        }
        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let mut discovery = DiscoveryConfig::default();
        discovery.languages = Some(vec!["ruby".to_string()]);
        let extractions = extractor
            .extract_repository(&root, &discovery)
            .expect("extract");
        let call_rels = extractions
            .iter()
            .flat_map(|e| e.relations.iter())
            .filter(|r| r.relation_type == rgctl_plugin_api::RelationType::Calls)
            .count();
        assert!(call_rels > 0, "plugin should emit Calls relations");
        let extend_rels = extractions
            .iter()
            .flat_map(|e| e.relations.iter())
            .filter(|r| r.relation_type == rgctl_plugin_api::RelationType::Extends)
            .count();
        assert!(extend_rels > 0, "plugin should emit Extends relations");
        let mut builder = GraphBuilder::new();
        extractor
            .populate_graph(&extractions, &mut builder)
            .expect("populate");
        assert!(
            builder.edge_count() > call_rels,
            "expected graph edges including resolved Calls (edges={}, call_rels={})",
            builder.edge_count(),
            call_rels
        );

        let spill_dir = tempfile::TempDir::new().unwrap();
        let mut spill_builder = GraphBuilder::with_spill(spill_dir.path()).expect("spill");
        let extractions2 = extractor
            .extract_repository(&root, &discovery)
            .expect("extract2");
        let mut tails = Vec::new();
        for mut ex in extractions2 {
            tails.push(
                extractor
                    .populate_pass1(&mut ex, &mut spill_builder)
                    .expect("pass1"),
            );
        }
        spill_builder.build_resolution_indexes();
        extractor
            .populate_pass2(&tails, &mut spill_builder)
            .expect("pass2");
        assert!(
            spill_builder.edge_count() > call_rels + extend_rels,
            "spill path should retain Calls/Extends (edges={})",
            spill_builder.edge_count()
        );
    }

    #[test]
    fn test_populate_graph() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("lib.rs");
        fs::write(&path, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let extraction = extractor.extract_file(&path).unwrap();

        let mut builder = GraphBuilder::new();
        extractor
            .populate_graph(&[extraction], &mut builder)
            .unwrap();

        assert!(builder.node_count() >= 2);
        assert!(builder.edge_count() >= 2);
    }

    #[test]
    fn test_extract_markdown_headings_and_relations() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("docs/guide.md");
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, "# Checkout Flow\n\n## Cart\n\n[ADR](./adr.md)\n").unwrap();
        let adr = temp.path().join("docs/adr.md");
        fs::write(&adr, "# Payments\n").unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let extraction = extractor.extract_file(&path).unwrap();

        let expected_qn = format!("{}#checkout-flow", path.to_string_lossy());
        assert!(
            extraction.symbols.iter().any(|s| s.name == "Checkout Flow"
                && s.qualified_name.as_deref() == Some(expected_qn.as_str())),
            "heading symbol"
        );
        assert!(
            extraction
                .relations
                .iter()
                .any(|r| { r.relation_type == rgctl_plugin_api::RelationType::Defines }),
            "Defines relation"
        );
        assert!(
            extraction
                .relations
                .iter()
                .any(|r| { r.relation_type == rgctl_plugin_api::RelationType::References }),
            "References relation"
        );
    }

    #[test]
    fn extract_file_with_source_needs_no_file_on_disk() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("absent/lib.rs");
        assert!(!path.exists(), "path must not exist on disk");

        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let extraction = extractor
            .extract_file_with_source(
                &path,
                b"pub fn add(a: i32, b: i32) -> i32 { a + b }\n".to_vec(),
            )
            .unwrap();

        assert!(
            extraction.symbols.iter().any(|s| s.name == "add"),
            "symbol from caller-supplied bytes"
        );
        assert_eq!(extraction.path, path);
    }

    #[test]
    fn profiling_keeps_empty_buckets_zero() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("lib.rs");
        fs::write(&path, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let mut extraction = extractor.extract_file(&path).unwrap();
        let mut builder = GraphBuilder::new();

        let (tail, pass1) = extractor
            .populate_pass1_profiled(&mut extraction, &mut builder)
            .unwrap();
        assert_eq!(pass1.config_key_processing, Duration::ZERO);

        builder.build_resolution_indexes();
        let pass2 = extractor
            .populate_pass2_profiled(&[tail], &mut builder)
            .unwrap();
        assert_eq!(pass2.config_usage_resolution, Duration::ZERO);
    }

    #[test]
    fn maven_pom_emits_dependency_and_depends_on() {
        let temp = TempDir::new().unwrap();
        let pom = temp.path().join("pom.xml");
        fs::write(
            &pom,
            r#"<project>
  <dependencies>
    <dependency>
      <groupId>io.quarkus</groupId>
      <artifactId>quarkus-core</artifactId>
      <version>2.16.12.Final</version>
    </dependency>
  </dependencies>
</project>"#,
        )
        .unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let mut extraction = extractor.extract_file(&pom).unwrap();
        assert!(
            extraction
                .symbols
                .iter()
                .any(|s| s.name == "io.quarkus:quarkus-core"
                    && s.symbol_type == rgctl_plugin_api::SymbolType::Dependency)
        );

        let mut builder = GraphBuilder::new();
        let tail = extractor
            .populate_pass1(&mut extraction, &mut builder)
            .unwrap();
        builder.build_resolution_indexes();
        extractor.populate_pass2(&[tail], &mut builder).unwrap();

        let (nodes, edges) = builder.into_graph();
        assert!(
            nodes
                .iter()
                .any(|n| n.node_type == rgctl_graph::schema::NodeType::Dependency
                    && n.name == "io.quarkus:quarkus-core")
        );
        assert!(
            edges
                .iter()
                .any(|e| e.edge_type == rgctl_graph::schema::EdgeType::DependsOn)
        );
    }

    #[test]
    fn java_value_links_uses_config_to_properties() {
        let temp = TempDir::new().unwrap();
        let props = temp.path().join("application.properties");
        let java = temp.path().join("App.java");
        fs::write(&props, "app.jwt.secret=change-me\n").unwrap();
        fs::write(
            &java,
            "class App {\n  @Value(\"${app.jwt.secret}\")\n  String secret;\n}\n",
        )
        .unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(registry);
        let mut props_ex = extractor.extract_file(&props).unwrap();
        let mut java_ex = extractor.extract_file(&java).unwrap();
        assert!(
            java_ex
                .config_usages
                .iter()
                .any(|u| u.key == "app.jwt.secret")
        );

        let mut builder = GraphBuilder::new();
        let t1 = extractor
            .populate_pass1(&mut props_ex, &mut builder)
            .unwrap();
        let t2 = extractor
            .populate_pass1(&mut java_ex, &mut builder)
            .unwrap();
        builder.build_resolution_indexes();
        extractor.populate_pass2(&[t1, t2], &mut builder).unwrap();

        let (_nodes, edges) = builder.into_graph();
        assert!(
            edges
                .iter()
                .any(|e| e.edge_type == rgctl_graph::schema::EdgeType::UsesConfig),
            "expected UsesConfig from Java @Value to properties key"
        );
    }
}
