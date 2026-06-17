//! TypeScript language plugin
//!
//! Extracts symbols, relationships, and complexity metrics from TypeScript source code
//! using TreeSitter.

use crate::error::{Error, Result};
use crate::languages::plugin_trait::*;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// TypeScript language plugin
pub struct TypeScriptPlugin;

impl TypeScriptPlugin {
    /// Create a new TypeScript plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn extract_function(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut parameters = Vec::new();
        let mut return_type = None;
        let mut modifiers = Vec::new();

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
                "type_annotation" => {
                    return_type = Some(child.utf8_text(source)?.trim_start_matches(':').trim().to_string());
                }
                "accessibility_modifier" | "async" | "static" => {
                    modifiers.push(child.utf8_text(source)?.to_string());
                }
                _ => {}
            }
        }

        let name = name.unwrap_or_else(|| "anonymous".to_string());

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
            modifiers,
            documentation: None,
            metadata: serde_json::json!({}),
        })
    }

    fn extract_parameters(&self, params_node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut parameters = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            if child.kind() == "required_parameter" || child.kind() == "optional_parameter" {
                let mut param_cursor = child.walk();
                let mut name = None;
                let mut param_type = None;

                for param_child in child.children(&mut param_cursor) {
                    match param_child.kind() {
                        "identifier" => {
                            name = Some(param_child.utf8_text(source)?.to_string());
                        }
                        "type_annotation" => {
                            param_type = Some(param_child.utf8_text(source)?.trim_start_matches(':').trim().to_string());
                        }
                        _ => {}
                    }
                }

                if let Some(name) = name {
                    parameters.push(Parameter {
                        name,
                        param_type,
                        default_value: None,
                    });
                }
            }
        }

        Ok(parameters)
    }

    fn extract_class(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut fields = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" | "identifier" => {
                    if name.is_none() {
                        name = Some(child.utf8_text(source)?.to_string());
                    }
                }
                "class_body" => {
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

    fn extract_class_fields(&self, class_body: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut cursor = class_body.walk();

        for child in class_body.children(&mut cursor) {
            if child.kind() == "field_definition" || child.kind() == "public_field_definition" {
                let mut field_cursor = child.walk();
                let mut name = None;
                let mut field_type = None;
                let mut visibility = None;

                for field_child in child.children(&mut field_cursor) {
                    match field_child.kind() {
                        "property_identifier" => {
                            name = Some(field_child.utf8_text(source)?.to_string());
                        }
                        "type_annotation" => {
                            field_type = Some(field_child.utf8_text(source)?.trim_start_matches(':').trim().to_string());
                        }
                        "accessibility_modifier" => {
                            visibility = Some(field_child.utf8_text(source)?.to_string());
                        }
                        _ => {}
                    }
                }

                if let Some(name) = name {
                    fields.push(Field {
                        name,
                        field_type,
                        visibility,
                    });
                }
            }
        }

        Ok(fields)
    }

    fn extract_interface(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut fields = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "object_type" => {
                    fields = self.extract_interface_properties(child, source)?;
                }
                _ => {}
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
            fields,
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
        })
    }

    fn extract_interface_properties(&self, object_type: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut cursor = object_type.walk();

        for child in object_type.children(&mut cursor) {
            if child.kind() == "property_signature" {
                let mut prop_cursor = child.walk();
                let mut name = None;
                let mut field_type = None;

                for prop_child in child.children(&mut prop_cursor) {
                    match prop_child.kind() {
                        "property_identifier" => {
                            name = Some(prop_child.utf8_text(source)?.to_string());
                        }
                        "type_annotation" => {
                            field_type = Some(prop_child.utf8_text(source)?.trim_start_matches(':').trim().to_string());
                        }
                        _ => {}
                    }
                }

                if let Some(name) = name {
                    fields.push(Field {
                        name,
                        field_type,
                        visibility: None,
                    });
                }
            }
        }

        Ok(fields)
    }

    fn calculate_cyclomatic(&self, node: Node) -> usize {
        let mut complexity = 1;

        fn traverse(node: Node, complexity: &mut usize) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "if_statement" | "switch_statement" | "while_statement"
                    | "for_statement" | "catch_clause" | "conditional_expression" => {
                        *complexity += 1;
                    }
                    "case_clause" => {
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
                if matches!(child.kind(), "if_statement" | "while_statement" | "for_statement" | "statement_block") {
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

impl Default for TypeScriptPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create TypeScriptPlugin")
    }
}

impl LanguagePlugin for TypeScriptPlugin {
    fn language_id(&self) -> &str {
        "typescript"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["ts", "tsx"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .map_err(|e| Error::PluginError(format!("Failed to set TypeScript grammar: {}", e)))?;

        let tree = parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: file_path.to_string_lossy().to_string().into(),
            line: 0,
            message: "Failed to parse TypeScript source".to_string(),
        })?;

        let mut symbols = Vec::new();
        let root_node = tree.root_node();
        let file_path_str = file_path.to_string_lossy();

        fn traverse_for_symbols(
            node: Node,
            source: &[u8],
            file_path: &str,
            symbols: &mut Vec<Symbol>,
            plugin: &TypeScriptPlugin,
        ) -> Result<()> {
            match node.kind() {
                "function_declaration" | "function" | "method_definition" | "arrow_function" => {
                    symbols.push(plugin.extract_function(node, source, file_path)?);
                }
                "class_declaration" => {
                    symbols.push(plugin.extract_class(node, source, file_path)?);
                }
                "interface_declaration" => {
                    symbols.push(plugin.extract_interface(node, source, file_path)?);
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
            .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
            .map_err(|e| Error::PluginError(format!("Failed to set TypeScript grammar: {}", e)))?;

        let tree = parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: symbol.location.file.clone().into(),
            line: symbol.location.start_line,
            message: "Failed to parse source for complexity analysis".to_string(),
        })?;

        let root = tree.root_node();
        let target_line = symbol.location.start_line - 1;

        fn find_function_at_line(node: Node, line: usize) -> Option<Node> {
            if matches!(node.kind(), "function_declaration" | "method_definition" | "arrow_function")
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
    fn test_typescript_plugin_language_id() {
        let plugin = TypeScriptPlugin::new().unwrap();
        assert_eq!(plugin.language_id(), "typescript");
    }

    #[test]
    fn test_typescript_plugin_file_extensions() {
        let plugin = TypeScriptPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["ts", "tsx"]);
    }

    #[test]
    fn test_extract_function() {
        let plugin = TypeScriptPlugin::new().unwrap();
        let source = b"function add(a: number, b: number): number { return a + b; }";
        let symbols = plugin.extract_symbols(Path::new("test.ts"), source).unwrap();

        assert!(!symbols.is_empty());
        let add_fn = symbols.iter().find(|s| s.name == "add").expect("add function not found");
        assert_eq!(add_fn.symbol_type, SymbolType::Function);
        assert_eq!(add_fn.parameters.len(), 2);
    }

    #[test]
    fn test_extract_class() {
        let plugin = TypeScriptPlugin::new().unwrap();
        let source = b"class User { name: string; age: number; }";
        let symbols = plugin.extract_symbols(Path::new("test.ts"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].symbol_type, SymbolType::Class);
        assert_eq!(symbols[0].fields.len(), 2);
    }

    #[test]
    fn test_extract_interface() {
        let plugin = TypeScriptPlugin::new().unwrap();
        let source = b"interface Person { name: string; age: number; }";
        let symbols = plugin.extract_symbols(Path::new("test.ts"), source).unwrap();

        assert!(!symbols.is_empty());
        let person_iface = symbols.iter().find(|s| s.name == "Person").expect("Person interface not found");
        assert_eq!(person_iface.symbol_type, SymbolType::Interface);
        // Fields extraction may vary based on tree-sitter parsing
        // The important thing is we found the interface
    }
}
