//! Tree-sitter language plugin driven by `languages.toml`

use crate::error::Result;
use crate::languages::extraction::tree_sitter::{extract_symbols_by_kinds, parse_source};
use crate::languages::extraction::ComplexityCalculator;
use crate::languages::generic::config::{get_language_config, LanguageConfig};
use crate::languages::plugin_trait::*;
use std::path::Path;
use tree_sitter::{Node, Tree};

type GrammarFn = fn() -> tree_sitter::Language;

/// Generic tree-sitter plugin configured via `languages.toml`.
pub struct TreeSitterLanguagePlugin {
    config: &'static LanguageConfig,
    grammar: GrammarFn,
}

impl TreeSitterLanguagePlugin {
    /// Create a plugin for the given language ID and grammar loader.
    pub fn new(language_id: &str, grammar: GrammarFn) -> Result<Self> {
        let config = get_language_config(language_id).ok_or_else(|| {
            crate::error::Error::PluginError(format!("Unknown tree-sitter language: {language_id}"))
        })?;
        Ok(Self { config, grammar })
    }

    fn parse(&self, source: &[u8], file_path: &Path) -> Result<Tree> {
        parse_source(source, file_path, (self.grammar)())
    }

    fn find_node_at_line<'a>(&self, node: Node<'a>, line: usize, kinds: &[&str]) -> Option<Node<'a>> {
        if kinds.contains(&node.kind()) && node.start_position().row == line {
            return Some(node);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if let Some(found) = self.find_node_at_line(child, line, kinds) {
                return Some(found);
            }
        }
        None
    }
}

impl LanguagePlugin for TreeSitterLanguagePlugin {
    fn language_id(&self) -> &str {
        self.config.id
    }

    fn file_extensions(&self) -> Vec<&str> {
        self.config.extensions.to_vec()
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some((self.grammar)())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let tree = self.parse(source, file_path)?;
        extract_symbols_by_kinds(
            &tree,
            source,
            file_path,
            self.config.function_kinds,
            self.config.class_kinds,
        )
    }

    fn extract_relations(
        &self,
        _file_path: &Path,
        _source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        Ok(vec![])
    }

    fn calculate_complexity(&self, symbol: &Symbol, source: &[u8]) -> Result<Option<ComplexityMetrics>> {
        if !self.config.enable_complexity || symbol.symbol_type != SymbolType::Function {
            return Ok(None);
        }

        let tree = self.parse(source, Path::new(&symbol.location.file))?;
        let target_line = symbol.location.start_line.saturating_sub(1);
        let kinds: Vec<&str> = self.config.function_kinds.to_vec();

        if let Some(func_node) = self.find_node_at_line(tree.root_node(), target_line, &kinds) {
            let branch_kinds = [
                "if_statement",
                "if_expression",
                "for_statement",
                "while_statement",
                "switch_statement",
                "match_expression",
                "case_statement",
                "elif_clause",
                "else_clause",
            ];
            Ok(Some(ComplexityMetrics {
                cyclomatic: ComplexityCalculator::cyclomatic(func_node, &branch_kinds),
                cognitive: ComplexityCalculator::cognitive(func_node, &branch_kinds),
                loc: ComplexityCalculator::loc(func_node),
                parameters: symbol.parameters.len(),
                nesting_depth: ComplexityCalculator::nesting_depth(
                    func_node,
                    &["block", "statement_block", "compound_statement"],
                ),
                returns: ComplexityCalculator::return_count(func_node, "return_statement"),
            }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "lang-c")]
    use super::*;
    #[cfg(feature = "lang-c")]
    use std::path::Path;

    #[cfg(feature = "lang-c")]
    #[test]
    fn test_tree_sitter_c_plugin() {
        let plugin =
            TreeSitterLanguagePlugin::new("c", || tree_sitter_c::LANGUAGE.into()).unwrap();
        let source = b"int add(int a, int b) { return a + b; }";
        let symbols = plugin.extract_symbols(Path::new("test.c"), source).unwrap();
        assert!(!symbols.is_empty());
    }
}
