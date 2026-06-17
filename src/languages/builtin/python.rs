//! Python language plugin
//!
//! Extracts symbols, relationships, and complexity metrics from Python source code
//! using TreeSitter.

use crate::error::{Error, Result};
use crate::languages::plugin_trait::*;
use crate::semantic::type_inference::TypeInferencer;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Python language plugin
pub struct PythonPlugin {
    _parser: Parser,
}

impl PythonPlugin {
    /// Create a new Python plugin
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Python grammar: {}", e)))?;
        Ok(Self { _parser: parser })
    }

    /// Extract function definition
    fn extract_function(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut parameters = Vec::new();
        let mut modifiers = Vec::new();
        let mut documentation = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "parameters" => {
                    parameters = self.extract_parameters(child, source)?;
                }
                "block" => {
                    // Check for docstring
                    let mut block_cursor = child.walk();
                    for block_child in child.children(&mut block_cursor) {
                        if block_child.kind() == "expression_statement" {
                            let mut expr_cursor = block_child.walk();
                            for expr_child in block_child.children(&mut expr_cursor) {
                                if expr_child.kind() == "string" {
                                    let doc = expr_child.utf8_text(source)?;
                                    documentation = Some(doc.trim_matches(|c| c == '"' || c == '\'').to_string());
                                    break;
                                }
                            }
                            break;
                        }
                    }
                }
                "decorator" => {
                    modifiers.push(child.utf8_text(source)?.to_string());
                }
                _ => {}
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Function missing name".to_string(),
        })?;

        // Infer types for parameters without explicit type hints
        let function_source = node.utf8_text(source).unwrap_or("");
        let inferencer = TypeInferencer::new();
        let inferred_types = inferencer.infer_python(function_source);

        // Update parameters with inferred types
        for param in &mut parameters {
            if param.param_type.is_none() {
                if let Some(inference) = inferred_types.get(&param.name) {
                    // Store inferred type with confidence as metadata
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
            signature: Some(node.utf8_text(source)?.lines().next().unwrap_or("").trim().to_string()),
            return_type: None, // Python has optional type hints
            parameters,
            fields: vec![],
            modifiers,
            documentation,
            metadata: serde_json::json!({}),
        })
    }

    /// Extract function parameters
    fn extract_parameters(&self, params_node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut parameters = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            if child.kind() == "identifier" {
                parameters.push(Parameter {
                    name: child.utf8_text(source)?.to_string(),
                    param_type: None, // Python has optional type hints
                    default_value: None,
                });
            } else if child.kind() == "default_parameter" {
                let mut param_cursor = child.walk();
                let mut name = None;
                let mut default = None;

                for param_child in child.children(&mut param_cursor) {
                    if param_child.kind() == "identifier" {
                        name = Some(param_child.utf8_text(source)?.to_string());
                    } else if name.is_some() && param_child.kind() != "=" {
                        default = Some(param_child.utf8_text(source)?.to_string());
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

    /// Extract class definition
    fn extract_class(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut fields = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "block" => {
                    fields = self.extract_class_fields(child, source)?;
                }
                _ => {}
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Class missing name".to_string(),
        })?;

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
            fields,
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
        })
    }

    /// Extract class fields from assignments
    fn extract_class_fields(&self, block_node: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut cursor = block_node.walk();

        for child in block_node.children(&mut cursor) {
            if child.kind() == "expression_statement" {
                let mut expr_cursor = child.walk();
                for expr_child in child.children(&mut expr_cursor) {
                    if expr_child.kind() == "assignment" {
                        let mut assign_cursor = expr_child.walk();
                        for assign_child in expr_child.children(&mut assign_cursor) {
                            if assign_child.kind() == "identifier" {
                                fields.push(Field {
                                    name: assign_child.utf8_text(source)?.to_string(),
                                    field_type: None,
                                    visibility: None,
                                });
                                break;
                            }
                        }
                    }
                }
            }
        }

        Ok(fields)
    }

    /// Calculate cyclomatic complexity
    fn calculate_cyclomatic(&self, node: Node) -> usize {
        let mut complexity = 1;

        fn traverse(node: Node, complexity: &mut usize) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "if_statement" | "elif_clause" | "while_statement"
                    | "for_statement" | "except_clause" => {
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

    /// Calculate cognitive complexity
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
                    "elif_clause" | "except_clause" => {
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
                if matches!(child.kind(), "if_statement" | "while_statement" | "for_statement" | "block") {
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

impl Default for PythonPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create PythonPlugin")
    }
}

impl LanguagePlugin for PythonPlugin {
    fn language_id(&self) -> &str {
        "python"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["py", "pyw"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_python::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Python grammar: {}", e)))?;

        let tree = parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: file_path.to_string_lossy().to_string().into(),
            line: 0,
            message: "Failed to parse Python source".to_string(),
        })?;

        let mut symbols = Vec::new();
        let root_node = tree.root_node();
        let file_path_str = file_path.to_string_lossy();

        fn traverse_for_symbols(
            node: Node,
            source: &[u8],
            file_path: &str,
            symbols: &mut Vec<Symbol>,
            plugin: &PythonPlugin,
        ) -> Result<()> {
            match node.kind() {
                "function_definition" => {
                    symbols.push(plugin.extract_function(node, source, file_path)?);
                }
                "class_definition" => {
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
        _file_path: &Path,
        _source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        Ok(vec![])
    }

    fn calculate_complexity(&self, symbol: &Symbol, source: &[u8]) -> Result<Option<ComplexityMetrics>> {
        if symbol.symbol_type != SymbolType::Function {
            return Ok(None);
        }

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Python grammar: {}", e)))?;

        let tree = parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: symbol.location.file.clone().into(),
            line: symbol.location.start_line,
            message: "Failed to parse source for complexity analysis".to_string(),
        })?;

        let root = tree.root_node();
        let target_line = symbol.location.start_line - 1;

        fn find_function_at_line(node: Node, line: usize) -> Option<Node> {
            if node.kind() == "function_definition" && node.start_position().row == line {
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
    fn test_python_plugin_language_id() {
        let plugin = PythonPlugin::new().unwrap();
        assert_eq!(plugin.language_id(), "python");
    }

    #[test]
    fn test_python_plugin_file_extensions() {
        let plugin = PythonPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["py", "pyw"]);
    }

    #[test]
    fn test_extract_simple_function() {
        let plugin = PythonPlugin::new().unwrap();
        let source = b"def add(a, b):\n    return a + b";
        let symbols = plugin.extract_symbols(Path::new("test.py"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "add");
        assert_eq!(symbols[0].symbol_type, SymbolType::Function);
        assert_eq!(symbols[0].parameters.len(), 2);
    }

    #[test]
    fn test_extract_function_with_defaults() {
        let plugin = PythonPlugin::new().unwrap();
        let source = b"def greet(name, greeting='Hello'):\n    return f'{greeting} {name}'";
        let symbols = plugin.extract_symbols(Path::new("test.py"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].parameters.len(), 2);
        assert_eq!(symbols[0].parameters[1].default_value, Some("'Hello'".to_string()));
    }

    #[test]
    fn test_extract_class() {
        let plugin = PythonPlugin::new().unwrap();
        let source = b"class User:\n    def __init__(self):\n        self.name = 'test'";
        let symbols = plugin.extract_symbols(Path::new("test.py"), source).unwrap();

        assert_eq!(symbols.len(), 2); // Class + __init__ method
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].symbol_type, SymbolType::Class);
    }

    #[test]
    fn test_calculate_complexity() {
        let plugin = PythonPlugin::new().unwrap();
        let source = b"def check(x):\n    if x > 0:\n        if x < 100:\n            return True\n    return False";
        let symbols = plugin.extract_symbols(Path::new("test.py"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        let complexity = plugin.calculate_complexity(&symbols[0], source).unwrap();
        assert!(complexity.is_some());
        let metrics = complexity.unwrap();
        assert_eq!(metrics.cyclomatic, 3);
    }

    #[test]
    fn test_type_inference() {
        let plugin = PythonPlugin::new().unwrap();
        let source = b"def process(name, count):\n    return name.upper() + str(count + 1)";
        let symbols = plugin.extract_symbols(Path::new("test.py"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].parameters.len(), 2);

        // name should be inferred as String (uses .upper())
        let name_param = &symbols[0].parameters[0];
        assert_eq!(name_param.name, "name");
        assert!(name_param.param_type.is_some());
        assert!(name_param.param_type.as_ref().unwrap().contains("String"));

        // count should be inferred as Numeric (uses +)
        let count_param = &symbols[0].parameters[1];
        assert_eq!(count_param.name, "count");
        assert!(count_param.param_type.is_some());
        assert!(count_param.param_type.as_ref().unwrap().contains("Numeric"));
    }
}
