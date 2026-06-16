//! Symbol and relationship extraction

use crate::config::usage_detector::{ConfigUsage, ConfigUsageDetector};
use crate::discovery::{DiscoveryConfig, FileDiscoverer};
use crate::error::{Error, Result};
use crate::extraction::graph_builder::GraphBuilder;
use crate::languages::plugin_trait::{ConfigKey, Relation, Symbol};
use crate::languages::registry::LanguageRegistry;
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
}

impl Extractor {
    /// Create a new extractor backed by the given registry.
    pub fn new(registry: Arc<LanguageRegistry>) -> Self {
        Self { registry }
    }

    /// Discover and extract all processable files under `root`.
    pub fn extract_repository(&self, root: &Path, discovery: &DiscoveryConfig) -> Result<Vec<FileExtraction>> {
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
    pub fn populate_graph(&self, extractions: &[FileExtraction], builder: &mut GraphBuilder) -> Result<()> {
        for extraction in extractions {
            let file_id = builder.ensure_file_node(&extraction.path);

            for symbol in &extraction.symbols {
                builder.add_symbol(symbol, file_id);

                if let Ok(plugin) = self.registry.get_plugin_for_file(&extraction.path) {
                    let source = std::fs::read(&extraction.path)?;
                    if let Some(metrics) = plugin.calculate_complexity(symbol, &source)? {
                        builder.add_complexity(symbol, &metrics);
                    }
                }
            }

            for relation in &extraction.relations {
                builder.add_relation(relation)?;
            }

            for key in &extraction.config_keys {
                builder.add_config_key(key, file_id);
            }

            for usage in &extraction.config_usages {
                builder.link_config_usage(
                    &usage.file,
                    usage.line,
                    &usage.key,
                    usage.usage_type,
                );
            }
        }
        Ok(())
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

        let registry = Arc::new(LanguageRegistry::new());
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

        let registry = Arc::new(LanguageRegistry::new());
        let extractor = Extractor::new(registry);
        let result = extractor.extract_file(&path).unwrap();

        assert!(result.config_keys.iter().any(|k| k.key_path == "server.port"));
    }

    #[test]
    fn test_populate_graph() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("lib.rs");
        fs::write(&path, "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();

        let registry = Arc::new(LanguageRegistry::new());
        let extractor = Extractor::new(registry);
        let extraction = extractor.extract_file(&path).unwrap();

        let mut builder = GraphBuilder::new();
        extractor.populate_graph(&[extraction], &mut builder).unwrap();

        assert!(builder.node_count() >= 2);
        assert!(builder.edge_count() >= 2);
    }
}
