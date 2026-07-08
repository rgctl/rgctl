//! JavaScript language plugin
//!
//! Extracts symbols, relationships, and complexity metrics from JavaScript source code
//! using TreeSitter.

use rbuilder_plugin_api::*;
use rbuilder_plugin_api::{Error, Result};
use rbuilder_semantic::type_inference::TypeInferencer;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// JavaScript language plugin
pub struct JavaScriptPlugin;

impl JavaScriptPlugin {
    /// Create a new JavaScript plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn extract_function(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut parameters = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "property_identifier" => {
                    if name.is_none() {
                        name = Some(child.utf8_text(source)?.to_string());
                    }
                }
                "formal_parameters" => {
                    parameters = self.extract_parameters(child, source)?;
                }
                _ => {}
            }
        }

        let name = name.unwrap_or_else(|| "anonymous".to_string());

        // Infer types for parameters
        let function_source = node.utf8_text(source).unwrap_or("");
        let inferencer = TypeInferencer::new();
        let inferred_types = inferencer.infer_javascript(function_source);

        // Update parameters with inferred types
        for param in &mut parameters {
            if param.param_type.is_none() {
                if let Some(inference) = inferred_types.get(&param.name) {
                    param.param_type = Some(format!("{:?}", inference.inferred));
                }
            }
        }

        Ok(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name: None,
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: Some(
                node.utf8_text(source)?
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            ),
            return_type: None,
            parameters,
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
        })
    }

    fn extract_parameters(&self, params_node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut parameters = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            if child.kind() == "identifier" {
                parameters.push(Parameter {
                    name: child.utf8_text(source)?.to_string(),
                    param_type: None,
                    default_value: None,
                });
            } else if child.kind() == "assignment_pattern" {
                let mut assign_cursor = child.walk();
                let mut name = None;
                let mut default = None;

                for assign_child in child.children(&mut assign_cursor) {
                    if assign_child.kind() == "identifier" {
                        name = Some(assign_child.utf8_text(source)?.to_string());
                    } else if name.is_some() {
                        default = Some(assign_child.utf8_text(source)?.to_string());
                    }
                }

                if let Some(name) = name {
                    parameters.push(Parameter {
                        name,
                        param_type: None,
                        default_value: default,
                    });
                }
            }
        }

        Ok(parameters)
    }

    fn extract_class(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;

        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                name = Some(child.utf8_text(source)?.to_string());
                break;
            }
        }

        let name = name.unwrap_or_else(|| "AnonymousClass".to_string());

        Ok(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Class,
            qualified_name: None,
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
        })
    }

    fn calculate_cyclomatic(&self, node: Node) -> usize {
        let mut complexity = 1;

        fn traverse(node: Node, complexity: &mut usize) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "if_statement" | "switch_statement" | "while_statement" | "for_statement"
                    | "catch_clause" | "ternary_expression" => {
                        *complexity += 1;
                    }
                    "case" => {
                        *complexity += 1;
                    }
                    _ => {}
                }
                traverse(child, complexity);
            }
        }

        traverse(node, &mut complexity);
        complexity
    }

    fn calculate_cognitive(&self, node: Node) -> usize {
        let mut cognitive = 0;

        fn traverse(node: Node, cognitive: &mut usize, nesting: usize) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "if_statement" | "while_statement" | "for_statement" => {
                        *cognitive += 1 + nesting;
                        traverse(child, cognitive, nesting + 1);
                    }
                    "switch_statement" | "catch_clause" => {
                        *cognitive += 1 + nesting;
                        traverse(child, cognitive, nesting);
                    }
                    _ => {
                        traverse(child, cognitive, nesting);
                    }
                }
            }
        }

        traverse(node, &mut cognitive, 0);
        cognitive
    }

    fn count_loc(&self, node: Node) -> usize {
        (node.end_position().row - node.start_position().row + 1).max(1)
    }

    fn count_nesting_depth(&self, node: Node) -> usize {
        let mut max_depth = 0;

        fn traverse(node: Node, max_depth: &mut usize, current_depth: usize) {
            *max_depth = (*max_depth).max(current_depth);
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(
                    child.kind(),
                    "if_statement" | "while_statement" | "for_statement" | "statement_block"
                ) {
                    traverse(child, max_depth, current_depth + 1);
                } else {
                    traverse(child, max_depth, current_depth);
                }
            }
        }

        traverse(node, &mut max_depth, 0);
        max_depth
    }

    fn count_returns(&self, node: Node) -> usize {
        let mut count = 0;

        fn traverse(node: Node, count: &mut usize) {
            if node.kind() == "return_statement" {
                *count += 1;
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                traverse(child, count);
            }
        }

        traverse(node, &mut count);
        count
    }
}

impl Default for JavaScriptPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create JavaScriptPlugin")
    }
}

impl LanguagePlugin for JavaScriptPlugin {
    fn language_id(&self) -> &str {
        "javascript"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["js", "jsx", "mjs", "cjs"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_javascript::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set JavaScript grammar: {}", e)))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| Error::ParseError {
                file: file_path.to_string_lossy().to_string().into(),
                line: 0,
                message: "Failed to parse JavaScript source".to_string(),
            })?;

        let mut symbols = Vec::new();
        let root_node = tree.root_node();
        let file_path_str = file_path.to_string_lossy();

        fn traverse_for_symbols(
            node: Node,
            source: &[u8],
            file_path: &str,
            symbols: &mut Vec<Symbol>,
            plugin: &JavaScriptPlugin,
        ) -> Result<()> {
            match node.kind() {
                "function_declaration" | "function" | "method_definition" | "arrow_function" => {
                    symbols.push(plugin.extract_function(node, source, file_path)?);
                }
                "class_declaration" => {
                    symbols.push(plugin.extract_class(node, source, file_path)?);
                }
                _ => {}
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                traverse_for_symbols(child, source, file_path, symbols, plugin)?;
            }

            Ok(())
        }

        traverse_for_symbols(root_node, source, &file_path_str, &mut symbols, self)?;
        Ok(symbols)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set JavaScript grammar: {e}")))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| Error::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message: "Failed to parse JavaScript source".to_string(),
            })?;

        let mut relations = Vec::new();
        walk_calls(
            tree.root_node(),
            source,
            file_path,
            symbols,
            JS_CALL_KINDS,
            "javascript",
            &mut relations,
        );
        Ok(relations)
    }

    fn calculate_complexity(
        &self,
        symbol: &Symbol,
        source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        if symbol.symbol_type != SymbolType::Function {
            return Ok(None);
        }

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_javascript::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set JavaScript grammar: {}", e)))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| Error::ParseError {
                file: symbol.location.file.clone().into(),
                line: symbol.location.start_line,
                message: "Failed to parse source for complexity analysis".to_string(),
            })?;

        let root = tree.root_node();
        let target_line = symbol.location.start_line - 1;

        fn find_function_at_line(node: Node, line: usize) -> Option<Node> {
            if matches!(
                node.kind(),
                "function_declaration" | "method_definition" | "arrow_function"
            ) && node.start_position().row == line
            {
                return Some(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = find_function_at_line(child, line) {
                    return Some(found);
                }
            }
            None
        }

        if let Some(func_node) = find_function_at_line(root, target_line) {
            Ok(Some(ComplexityMetrics {
                cyclomatic: self.calculate_cyclomatic(func_node),
                cognitive: self.calculate_cognitive(func_node),
                loc: self.count_loc(func_node),
                parameters: symbol.parameters.len(),
                nesting_depth: self.count_nesting_depth(func_node),
                returns: self.count_returns(func_node),
            }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_javascript_plugin_language_id() {
        let plugin = JavaScriptPlugin::new().unwrap();
        assert_eq!(plugin.language_id(), "javascript");
    }

    #[test]
    fn test_javascript_plugin_file_extensions() {
        let plugin = JavaScriptPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["js", "jsx", "mjs", "cjs"]);
    }

    #[test]
    fn test_extract_function() {
        let plugin = JavaScriptPlugin::new().unwrap();
        let source = b"function add(a, b) { return a + b; }";
        let symbols = plugin
            .extract_symbols(Path::new("test.js"), source)
            .unwrap();

        assert!(!symbols.is_empty());
        let add_fn = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("add function not found");
        assert_eq!(add_fn.symbol_type, SymbolType::Function);
        assert_eq!(add_fn.parameters.len(), 2);
    }

    #[test]
    fn test_extract_arrow_function() {
        let plugin = JavaScriptPlugin::new().unwrap();
        let source = b"const multiply = (x, y) => x * y;";
        let symbols = plugin
            .extract_symbols(Path::new("test.js"), source)
            .unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].symbol_type, SymbolType::Function);
    }

    #[test]
    fn test_extract_class() {
        let plugin = JavaScriptPlugin::new().unwrap();
        let source = b"class User { constructor(name) { this.name = name; } }";
        let symbols = plugin
            .extract_symbols(Path::new("test.js"), source)
            .unwrap();

        assert!(!symbols.is_empty());
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].symbol_type, SymbolType::Class);
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
function caller() {
    helper();
}

function helper() {}
"#;
        let plugin = JavaScriptPlugin::new().unwrap();
        let path = Path::new("test.js");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls) && r.to == "helper"),
            "expected Calls -> helper, got {relations:?}"
        );
    }
}
