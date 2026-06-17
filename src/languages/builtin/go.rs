//! Go language plugin
//!
//! Extracts symbols, relationships, and complexity metrics from Go source code
//! using TreeSitter.

use crate::error::{Error, Result};
use crate::languages::plugin_trait::*;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Go language plugin
pub struct GoPlugin;

impl GoPlugin {
    /// Create a new Go plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn extract_function(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut parameters = Vec::new();
        let mut return_type = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    if name.is_none() {
                        name = Some(child.utf8_text(source)?.to_string());
                    }
                }
                "parameter_list" => {
                    parameters = self.extract_parameters(child, source)?;
                }
                "type_identifier" => {
                    return_type = Some(child.utf8_text(source)?.to_string());
                }
                _ => {}
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Function missing name".to_string(),
        })?;

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
            return_type,
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
            if child.kind() == "parameter_declaration" {
                let mut param_cursor = child.walk();
                let mut name = None;
                let mut param_type = None;

                for param_child in child.children(&mut param_cursor) {
                    match param_child.kind() {
                        "identifier" => {
                            name = Some(param_child.utf8_text(source)?.to_string());
                        }
                        "type_identifier" | "pointer_type" | "slice_type" => {
                            param_type = Some(param_child.utf8_text(source)?.to_string());
                        }
                        _ => {}
                    }
                }

                if let Some(name) = name {
                    parameters.push(Parameter {
                        name,
                        param_type,
                        default_value: None, // Go doesn't have default parameters
                    });
                }
            }
        }

        Ok(parameters)
    }

    fn extract_struct(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut fields = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "struct_type" => {
                    fields = self.extract_struct_fields(child, source)?;
                }
                _ => {}
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Struct missing name".to_string(),
        })?;

        Ok(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Struct,
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

    fn extract_struct_fields(&self, struct_type_node: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut cursor = struct_type_node.walk();

        for child in struct_type_node.children(&mut cursor) {
            if child.kind() == "field_declaration_list" {
                let mut field_cursor = child.walk();
                for field_child in child.children(&mut field_cursor) {
                    if field_child.kind() == "field_declaration" {
                        let mut decl_cursor = field_child.walk();
                        let mut name = None;
                        let mut field_type = None;

                        for decl_child in field_child.children(&mut decl_cursor) {
                            match decl_child.kind() {
                                "field_identifier" => {
                                    name = Some(decl_child.utf8_text(source)?.to_string());
                                }
                                "type_identifier" | "pointer_type" | "slice_type" => {
                                    field_type = Some(decl_child.utf8_text(source)?.to_string());
                                }
                                _ => {}
                            }
                        }

                        if let Some(name) = name {
                            fields.push(Field {
                                name,
                                field_type,
                                visibility: None, // Go uses capitalization for visibility
                            });
                        }
                    }
                }
            }
        }

        Ok(fields)
    }

    fn extract_interface(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;

        for child in node.children(&mut cursor) {
            if child.kind() == "type_identifier" {
                name = Some(child.utf8_text(source)?.to_string());
                break;
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Interface missing name".to_string(),
        })?;

        Ok(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Interface,
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
                    "if_statement" | "for_statement" | "switch_statement"
                    | "expression_case" | "default_case" => {
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
                    "if_statement" | "for_statement" => {
                        *cognitive += 1 + nesting;
                        traverse(child, cognitive, nesting + 1);
                    }
                    "switch_statement" => {
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
                if matches!(child.kind(), "if_statement" | "for_statement" | "block") {
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

impl Default for GoPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create GoPlugin")
    }
}

impl LanguagePlugin for GoPlugin {
    fn language_id(&self) -> &str {
        "go"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["go"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_go::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Go grammar: {}", e)))?;

        let tree = parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: file_path.to_string_lossy().to_string().into(),
            line: 0,
            message: "Failed to parse Go source".to_string(),
        })?;

        let mut symbols = Vec::new();
        let root_node = tree.root_node();
        let file_path_str = file_path.to_string_lossy();

        fn traverse_for_symbols(
            node: Node,
            source: &[u8],
            file_path: &str,
            symbols: &mut Vec<Symbol>,
            plugin: &GoPlugin,
        ) -> Result<()> {
            match node.kind() {
                "function_declaration" | "method_declaration" => {
                    symbols.push(plugin.extract_function(node, source, file_path)?);
                }
                "type_declaration" => {
                    // Check if it's a struct or interface
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "type_spec" {
                            let mut spec_cursor = child.walk();
                            for spec_child in child.children(&mut spec_cursor) {
                                match spec_child.kind() {
                                    "struct_type" => {
                                        symbols.push(plugin.extract_struct(child, source, file_path)?);
                                        break;
                                    }
                                    "interface_type" => {
                                        symbols.push(plugin.extract_interface(child, source, file_path)?);
                                        break;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
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
            .set_language(&tree_sitter_go::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Go grammar: {}", e)))?;

        let tree = parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: symbol.location.file.clone().into(),
            line: symbol.location.start_line,
            message: "Failed to parse source for complexity analysis".to_string(),
        })?;

        let root = tree.root_node();
        let target_line = symbol.location.start_line - 1;

        fn find_function_at_line(node: Node, line: usize) -> Option<Node> {
            if matches!(node.kind(), "function_declaration" | "method_declaration")
                && node.start_position().row == line {
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
    fn test_go_plugin_language_id() {
        let plugin = GoPlugin::new().unwrap();
        assert_eq!(plugin.language_id(), "go");
    }

    #[test]
    fn test_go_plugin_file_extensions() {
        let plugin = GoPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["go"]);
    }

    #[test]
    fn test_extract_function() {
        let plugin = GoPlugin::new().unwrap();
        let source = b"func Add(a int, b int) int { return a + b }";
        let symbols = plugin.extract_symbols(Path::new("test.go"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Add");
        assert_eq!(symbols[0].symbol_type, SymbolType::Function);
        assert_eq!(symbols[0].parameters.len(), 2);
    }

    #[test]
    fn test_extract_struct() {
        let plugin = GoPlugin::new().unwrap();
        let source = b"type User struct { Name string; Age int }";
        let symbols = plugin.extract_symbols(Path::new("test.go"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].symbol_type, SymbolType::Struct);
        assert_eq!(symbols[0].fields.len(), 2);
    }

    #[test]
    fn test_extract_interface() {
        let plugin = GoPlugin::new().unwrap();
        let source = b"type Reader interface { Read(p []byte) (n int, err error) }";
        let symbols = plugin.extract_symbols(Path::new("test.go"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Reader");
        assert_eq!(symbols[0].symbol_type, SymbolType::Interface);
    }
}
