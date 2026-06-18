//! Puppet module extraction plugin (Phase 18).

pub mod parser;

use crate::error::Result;
use crate::languages::plugin_trait::*;
use parser::PuppetParser;
use std::path::Path;

/// Puppet IaC plugin — modules, classes, defined types, resources.
pub struct PuppetPlugin {
    parser: PuppetParser,
}

impl PuppetPlugin {
    /// Create a new Puppet plugin instance.
    pub fn new() -> Result<Self> {
        Ok(Self {
            parser: PuppetParser::new(),
        })
    }

    fn parse_file(&self, file_path: &Path, source: &[u8]) -> (Vec<Symbol>, Vec<Relation>) {
        let file = file_path.to_string_lossy();
        if !PuppetParser::is_puppet_path(&file) {
            return (vec![], vec![]);
        }
        let text = std::str::from_utf8(source).unwrap_or("");
        self.parser.parse(&file, text)
    }
}

impl LanguagePlugin for PuppetPlugin {
    fn language_id(&self) -> &str {
        "puppet"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec![]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        Ok(self.parse_file(file_path, source).0)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        Ok(self.parse_file(file_path, source).1)
    }

    fn calculate_complexity(
        &self,
        _symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        Ok(None)
    }

    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities {
            extracts_functions: false,
            extracts_types: true,
            extracts_modules: true,
            extracts_relations: true,
            calculates_complexity: false,
            extracts_documentation: false,
            supports_incremental: false,
        }
    }
}
