//! Markdown language plugin (tree-sitter-md).

use rgctl_plugin_api::{
    AstHandlerTable, ComplexityMetrics, LanguageCapabilities, LanguagePlugin, Result, Symbol,
};
use std::path::Path;
use tree_sitter::Language;

/// Markdown documentation / context graph plugin.
pub struct MarkdownPlugin;

impl MarkdownPlugin {
    /// Create a new markdown plugin.
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl LanguagePlugin for MarkdownPlugin {
    fn language_id(&self) -> &str {
        "markdown"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["md", "mdx"]
    }

    fn grammar(&self) -> Option<Language> {
        Some(tree_sitter_md::LANGUAGE.into())
    }

    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities {
            extracts_functions: false,
            extracts_types: false,
            extracts_modules: true,
            extracts_relations: true,
            calculates_complexity: false,
            extracts_documentation: true,
            supports_incremental: false,
        }
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        Ok(self.extract_all(file_path, source)?.symbols)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<rgctl_plugin_api::Relation>> {
        Ok(self.extract_all(file_path, source)?.relations)
    }

    fn extract_all(
        &self,
        file_path: &Path,
        source: &[u8],
    ) -> Result<rgctl_plugin_api::ExtractAllResult> {
        crate::single_pass::extract_all_single_pass(file_path, source)
    }

    fn calculate_complexity(
        &self,
        _symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        Ok(None)
    }

    fn declared_ast_handlers(&self) -> Option<&AstHandlerTable> {
        Some(crate::single_pass::ast_handlers())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn extracts_headings_with_slug_qualified_names() {
        let plugin = MarkdownPlugin::new().expect("new");
        let path = Path::new("docs/guide.md");
        let source = "# Checkout Flow\n\n## Payment flow\n";
        let symbols = plugin
            .extract_symbols(path, source.as_bytes())
            .expect("extract");
        let headings: Vec<_> = symbols
            .iter()
            .filter(|s| s.metadata.get("kind") == Some(&serde_json::json!("heading")))
            .collect();
        assert_eq!(headings.len(), 2);
        assert_eq!(headings[0].name, "Checkout Flow");
        assert_eq!(
            headings[0].qualified_name.as_deref(),
            Some("docs/guide.md#checkout-flow")
        );
        assert_eq!(
            headings[1].qualified_name.as_deref(),
            Some("docs/guide.md#payment-flow")
        );
    }

    #[test]
    fn grammar_is_some() {
        let plugin = MarkdownPlugin::new().expect("new");
        assert!(plugin.grammar().is_some());
    }

    #[test]
    fn extract_all_matches_split_extract_paths() {
        let plugin = MarkdownPlugin::new().expect("new");
        let path = Path::new("docs/guide.md");
        let source = "# Checkout Flow\n\n[ADR](./adr.md)\n";
        let all = plugin.extract_all(path, source.as_bytes()).expect("all");
        let sym_only = plugin
            .extract_symbols(path, source.as_bytes())
            .expect("sym");
        let rel_only = plugin
            .extract_relations(path, source.as_bytes(), &sym_only)
            .expect("rel");
        assert_eq!(all.symbols.len(), sym_only.len());
        assert_eq!(all.relations.len(), rel_only.len());
    }

    #[test]
    fn capabilities_and_extensions() {
        let plugin = MarkdownPlugin::new().expect("new");
        let caps = plugin.capabilities();
        assert!(caps.extracts_modules);
        assert!(caps.extracts_relations);
        assert!(!caps.calculates_complexity);
        assert_eq!(plugin.file_extensions(), vec!["md", "mdx"]);
    }
}
