//! Symbol and relationship extraction

use crate::discovery::{DiscoveryConfig, FileDiscoverer};
use crate::graph_builder::GraphBuilder;
use crate::usage_detector::{ConfigUsage, ConfigUsageDetector};
use rbuilder_error::{Error, Result};
use rbuilder_plugin_api::{ConfigKey, Relation, Symbol};
use rbuilder_registry::LanguageRegistry;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Extracts symbols and relationships from a single file.
pub struct Extractor {
    registry: Arc<LanguageRegistry>,
}

/// Result of extracting a single file.
#[derive(Debug, Default)]
pub struct FileExtraction {
    /// Path to the source file
    pub path: PathBuf,
    /// Extracted code symbols
    pub symbols: Vec<Symbol>,
    /// Extracted symbol relations
    pub relations: Vec<Relation>,
    /// Extracted configuration keys
    pub config_keys: Vec<ConfigKey>,
    /// Detected configuration usages in source
    pub config_usages: Vec<ConfigUsage>,
    /// Cached source bytes (avoids re-reading file during graph population)
    pub source: Vec<u8>,
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
        let source = std::fs::read(path)?;
        let mut extraction = FileExtraction {
            path: path.to_path_buf(),
            source: source.clone(), // Cache source bytes
            ..Default::default()
        };

        if let Ok(plugin) = self.registry.get_plugin_for_file(path) {
            extraction.symbols = plugin.extract_symbols(path, &source)?;
            extraction.relations = plugin.extract_relations(path, &source, &extraction.symbols)?;
            extraction.config_usages =
                ConfigUsageDetector::detect(plugin.language_id(), &source, path);
            return Ok(extraction);
        }

        if let Ok(plugin) = self.registry.get_config_plugin_for_file(path) {
            extraction.config_keys = plugin.extract_config_keys(path, &source)?;
            return Ok(extraction);
        }

        Err(Error::UnsupportedLanguage(
            path.to_string_lossy().to_string(),
        ))
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

        // PASS 1: Add all symbols and config keys
        for extraction in extractions {
            let file_id = builder.ensure_file_node(&extraction.path);

            // Use cached source bytes (no file I/O!)
            let source = if extraction.source.is_empty() {
                None
            } else {
                Some(&extraction.source)
            };

            // Measure symbol processing
            let sym_start = Instant::now();
            for symbol in &extraction.symbols {
                let body = source.and_then(|bytes| symbol_body_from_source(bytes, symbol));
                if let Some(body) = body.as_deref() {
                    builder.add_symbol_with_body(symbol, file_id, Some(body));
                } else {
                    builder.add_symbol(symbol, file_id);
                }
            }
            symbol_time += sym_start.elapsed();

            // Optional: Calculate complexity (skip for now - can be done post-processing)
            // Uncomment to enable complexity calculation during graph building:
            /*
            if let (Some(bytes), Some(plugin)) = (source, plugin.as_ref()) {
                for symbol in &extraction.symbols {
                    if let Some(metrics) = plugin.calculate_complexity(symbol, bytes)? {
                        builder.add_complexity(symbol, &metrics);
                    }
                }
            }
            */

            // Measure config key processing
            let cfg_key_start = Instant::now();
            for key in &extraction.config_keys {
                builder.add_config_key(key, file_id);
            }
            config_key_time += cfg_key_start.elapsed();
        }

        // BUILD RESOLUTION INDEXES (converts O(n) scans to O(1) lookups)
        builder.build_resolution_indexes();

        // PASS 2: Process relations and config usages (now with fast lookups)
        for extraction in extractions {
            // Measure relation resolution
            let rel_start = Instant::now();
            for relation in &extraction.relations {
                builder.add_relation(relation)?;
            }
            relation_time += rel_start.elapsed();

            // Measure config usage linking
            let cfg_usage_start = Instant::now();
            for usage in &extraction.config_usages {
                builder.link_config_usage(&usage.file, usage.line, &usage.key, usage.usage_type);
            }
            config_usage_time += cfg_usage_start.elapsed();
        }

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

        // Log detailed resolution statistics
        builder.log_resolution_stats();

        Ok(())
    }
}

fn symbol_body_from_source(source: &[u8], symbol: &Symbol) -> Option<String> {
    let text = std::str::from_utf8(source).ok()?;
    let start = symbol.location.start_line.saturating_sub(1);
    let line_count = symbol
        .location
        .end_line
        .saturating_sub(symbol.location.start_line)
        .saturating_add(1)
        .max(1);
    let body: String = text
        .lines()
        .skip(start)
        .take(line_count)
        .collect::<Vec<_>>()
        .join("\n");
    if body.is_empty() {
        None
    } else {
        Some(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_extract_rust_file() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("main.rs");
        fs::write(&path, "fn hello() {}\nfn world() {}\n").unwrap();

        let registry = Arc::new(rbuilder_languages::default_registry());
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

        let registry = Arc::new(rbuilder_languages::default_registry());
        let extractor = Extractor::new(registry);
        let result = extractor.extract_file(&path).unwrap();

        assert!(result
            .config_keys
            .iter()
            .any(|k| k.key_path == "server.port"));
    }

    #[test]
    fn test_populate_graph() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("lib.rs");
        fs::write(&path, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

        let registry = Arc::new(rbuilder_languages::default_registry());
        let extractor = Extractor::new(registry);
        let extraction = extractor.extract_file(&path).unwrap();

        let mut builder = GraphBuilder::new();
        extractor
            .populate_graph(&[extraction], &mut builder)
            .unwrap();

        assert!(builder.node_count() >= 2);
        assert!(builder.edge_count() >= 2);
    }
}
