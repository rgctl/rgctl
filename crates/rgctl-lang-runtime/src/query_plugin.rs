//! Tier-2 language plugin driven by a precompiled `tags.scm` query.

use crate::config::LanguageConfig;
use crate::queries::{
    CaptureIndex, capture_role, compile_tags_query, extract_capture_names, extract_with_query,
    validate_tags_captures,
};
use crate::regex_extract::{extract_regex_symbols, merge_symbols};
use rgctl_plugin_api::Result;
use rgctl_plugin_api::*;
use rgctl_plugin_helpers::ComplexityCalculator;
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::{Node, Query, Tree};

type GrammarFn = fn() -> tree_sitter::Language;

struct CompiledTagsQuery {
    query: Query,
    capture_index: CaptureIndex,
}

/// Generic tags.scm query plugin (`handler = "queries"`).
///
/// Validates the capture vocabulary eagerly at construction and compiles the
/// embedded `tags.scm` automaton lazily on first file extraction, reusing it
/// across all worker threads thereafter.
pub struct QueryLanguagePlugin {
    config: &'static LanguageConfig,
    grammar: GrammarFn,
    tags_scm: &'static str,
    compiled: OnceLock<std::result::Result<CompiledTagsQuery, String>>,
}

impl QueryLanguagePlugin {
    /// Create a plugin from a static config that includes `tags_scm`.
    pub fn from_config(config: &'static LanguageConfig, grammar: GrammarFn) -> Result<Self> {
        let tags = config.tags_scm.ok_or_else(|| {
            Error::PluginError(format!(
                "language '{}': handler=queries requires LanguageConfig.tags_scm",
                config.id
            ))
        })?;
        let capture_issues = validate_tags_captures(config.id, tags);
        if !capture_issues.is_empty() {
            return Err(Error::PluginError(capture_issues.join("; ")));
        }
        for name in extract_capture_names(tags) {
            if name.starts_with('_') {
                continue;
            }
            if capture_role(&name).is_none() {
                return Err(Error::PluginError(format!(
                    "{}: capture '@{name}' is allow-listed but has no Symbol/Relation mapping",
                    config.id
                )));
            }
        }
        Ok(Self {
            config,
            grammar,
            tags_scm: tags,
            compiled: OnceLock::new(),
        })
    }

    fn compiled_query(&self) -> Result<&CompiledTagsQuery> {
        let res = self.compiled.get_or_init(|| {
            let language = (self.grammar)();
            let query = compile_tags_query(self.config.id, &language, self.tags_scm)
                .map_err(|e| e.to_string())?;
            let capture_index = CaptureIndex::from_query(&query).map_err(|e| e.to_string())?;
            Ok(CompiledTagsQuery {
                query,
                capture_index,
            })
        });
        match res {
            Ok(compiled) => Ok(compiled),
            Err(msg) => Err(Error::PluginError(msg.clone())),
        }
    }

    fn parse(&self, source: &[u8], file_path: &Path) -> Result<Tree> {
        parse_with_pool(self.config.id, (self.grammar)(), source).map_err(|message| {
            Error::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message,
            }
        })
    }

    fn extract_all_inner(&self, file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
        let compiled = self.compiled_query()?;
        let tree = self.parse(source, file_path)?;
        let mut result = extract_with_query(
            self.config.id,
            file_path,
            source,
            &tree,
            &compiled.query,
            &compiled.capture_index,
        )?;
        if let Some(patterns) = self.config.regex_patterns {
            let supplemental = extract_regex_symbols(file_path, source, patterns, "queries+regex")?;
            merge_symbols(&mut result.symbols, supplemental);
        }
        Ok(result)
    }

    fn find_node_at_line<'a>(
        &self,
        node: Node<'a>,
        line: usize,
        kinds: &[&str],
    ) -> Option<Node<'a>> {
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

impl LanguagePlugin for QueryLanguagePlugin {
    fn language_id(&self) -> &str {
        self.config.id
    }

    fn file_extensions(&self) -> Vec<&str> {
        self.config.extensions.to_vec()
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some((self.grammar)())
    }

    fn cache_fingerprint(&self) -> Option<&[u8]> {
        self.config.tags_scm.map(str::as_bytes)
    }

    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities {
            extracts_functions: true,
            extracts_types: true,
            extracts_modules: true,
            extracts_relations: true,
            calculates_complexity: self.config.enable_complexity,
            extracts_documentation: true,
            supports_incremental: false,
        }
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        Ok(self.extract_all_inner(file_path, source)?.symbols)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        Ok(self.extract_all_inner(file_path, source)?.relations)
    }

    fn extract_all(&self, file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
        self.extract_all_inner(file_path, source)
    }

    fn calculate_complexity(
        &self,
        symbol: &Symbol,
        source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        if !self.config.enable_complexity
            || !matches!(
                symbol.symbol_type,
                SymbolType::Function | SymbolType::Method
            )
        {
            return Ok(None);
        }

        let tree = self.parse(source, Path::new(&symbol.location.file))?;
        let target_line = symbol.location.start_line.saturating_sub(1);
        let kinds: Vec<&str> = self.config.function_kinds.to_vec();
        if kinds.is_empty() {
            return Ok(None);
        }

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
    use super::*;
    use crate::config::test_configs::C_QUERIES;
    use std::path::Path;

    #[test]
    fn query_plugin_extracts_c_functions_and_calls() {
        let plugin =
            QueryLanguagePlugin::from_config(&C_QUERIES, || tree_sitter_c::LANGUAGE.into())
                .expect("compile C tags.scm");
        let source = br#"
int helper(int x) { return x + 1; }
int add(int a, int b) {
    int h = helper(a);
    return h + b;
}
"#;
        let result = plugin
            .extract_all(Path::new("math.c"), source)
            .expect("extract_all");
        assert!(
            result.symbols.iter().any(|s| s.name == "add"),
            "expected function add, got {:?}",
            result.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(result.symbols.iter().any(|s| s.name == "helper"));
        assert!(
            result.relations.iter().any(|r| {
                r.relation_type == RelationType::Calls && r.to == "helper" && r.from == "add"
            }),
            "expected Calls add->helper, got {:?}",
            result.relations
        );
    }

    #[test]
    fn query_plugin_rejects_bad_captures() {
        use crate::config::LanguageConfig;
        static BAD: LanguageConfig = LanguageConfig {
            id: "bad",
            extensions: &["c"],
            function_kinds: &["function_definition"],
            class_kinds: &[],
            enable_complexity: false,
            enable_type_inference: false,
            regex_patterns: None,
            tags_scm: Some("(function_definition) @definition.widget"),
        };
        let err = QueryLanguagePlugin::from_config(&BAD, || tree_sitter_c::LANGUAGE.into());
        assert!(err.is_err());
        let msg = err.err().unwrap().to_string();
        assert!(msg.contains("definition.widget"), "{msg}");
    }
}
