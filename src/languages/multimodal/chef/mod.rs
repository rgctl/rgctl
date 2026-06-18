//! Chef cookbook extraction plugin (Phase 17).

pub mod parser;

use crate::error::Result;
use crate::languages::plugin_trait::*;
use parser::ChefParser;
use std::path::Path;

/// Chef IaC plugin — cookbooks, recipes, resources, attributes, templates.
pub struct ChefPlugin {
    parser: ChefParser,
}

impl ChefPlugin {
    /// Create a new Chef plugin instance.
    pub fn new() -> Result<Self> {
        Ok(Self {
            parser: ChefParser::new(),
        })
    }

    fn parse_file(&self, file_path: &Path, source: &[u8]) -> (Vec<Symbol>, Vec<Relation>) {
        let file = file_path.to_string_lossy();
        if !ChefParser::is_chef_path(&file) {
            return (vec![], vec![]);
        }
        let text = std::str::from_utf8(source).unwrap_or("");
        self.parser.parse(&file, text)
    }
}

impl LanguagePlugin for ChefPlugin {
    fn language_id(&self) -> &str {
        "chef"
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chef_plugin_recipe() {
        let plugin = ChefPlugin::new().unwrap();
        let source = br#"
package 'nginx' do
  action :install
end
"#;
        let path = Path::new("cookbooks/nginx/recipes/default.rb");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        assert!(symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::ChefRecipe));
    }
}
