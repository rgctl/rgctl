//! Rust language plugin
//!
//! Extracts symbols, relationships, and complexity metrics from Rust source code
//! using TreeSitter.

use crate::error::{Error, Result};
use crate::languages::plugin_trait::*;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Rust language plugin
pub struct RustPlugin;

impl RustPlugin {
    /// Create a new Rust plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    /// Extract function node details
    fn extract_function(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut parameters = Vec::new();
        let mut return_type = None;
        let mut modifiers = Vec::new();
        let mut documentation = None;

        // Look for function identifier
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "parameters" => {
                    parameters = self.extract_parameters(child, source)?;
                }
                "visibility_modifier" => {
                    modifiers.push(child.utf8_text(source)?.to_string());
                }
                "function_modifiers" => {
                    let mods = child.utf8_text(source)?.to_string();
                    modifiers.extend(mods.split_whitespace().map(|s| s.to_string()));
                }
                _ if child.kind().contains("type") => {
                    return_type = Some(child.utf8_text(source)?.to_string());
                }
                _ => {}
            }
        }

        // Look for doc comments
        if let Some(prev_sibling) = node.prev_sibling() {
            if prev_sibling.kind() == "line_comment" {
                let comment = prev_sibling.utf8_text(source)?;
                if comment.starts_with("///") || comment.starts_with("//!") {
                    documentation = Some(comment.trim_start_matches("///").trim_start_matches("//!").trim().to_string());
                }
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
            qualified_name: None, // Will be set by module path resolution
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: Some(node.utf8_text(source)?.split('{').next().unwrap_or("").trim().to_string()),
            return_type,
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
            if child.kind() == "parameter" {
                let mut name = None;
                let mut param_type = None;
                let mut param_cursor = child.walk();

                for param_child in child.children(&mut param_cursor) {
                    match param_child.kind() {
                        "identifier" => {
                            name = Some(param_child.utf8_text(source)?.to_string());
                        }
                        _ if param_child.kind().contains("type") => {
                            param_type = Some(param_child.utf8_text(source)?.to_string());
                        }
                        _ => {}
                    }
                }

                if let Some(name) = name {
                    parameters.push(Parameter {
                        name,
                        param_type,
                        default_value: None, // Rust doesn't have default params
                    });
                }
            }
        }

        Ok(parameters)
    }

    /// Extract struct or enum definition
    fn extract_type_definition(&self, node: Node, source: &[u8], file_path: &str, is_enum: bool) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut fields = Vec::new();
        let mut modifiers = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "visibility_modifier" => {
                    modifiers.push(child.utf8_text(source)?.to_string());
                }
                "field_declaration_list" => {
                    fields = self.extract_fields(child, source)?;
                }
                _ => {}
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: format!("{} missing name", if is_enum { "Enum" } else { "Struct" }),
        })?;

        Ok(Symbol {
            name: name.clone(),
            symbol_type: if is_enum { SymbolType::Enum } else { SymbolType::Struct },
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
            modifiers,
            documentation: None,
            metadata: serde_json::json!({}),
        })
    }

    /// Extract struct fields
    fn extract_fields(&self, field_list_node: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut cursor = field_list_node.walk();

        for child in field_list_node.children(&mut cursor) {
            if child.kind() == "field_declaration" {
                let mut name = None;
                let mut field_type = None;
                let mut visibility = None;
                let mut field_cursor = child.walk();

                for field_child in child.children(&mut field_cursor) {
                    match field_child.kind() {
                        "field_identifier" => {
                            name = Some(field_child.utf8_text(source)?.to_string());
                        }
                        "visibility_modifier" => {
                            visibility = Some(field_child.utf8_text(source)?.to_string());
                        }
                        _ if field_child.kind().contains("type") => {
                            field_type = Some(field_child.utf8_text(source)?.to_string());
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

    /// Calculate cyclomatic complexity for a function node
    fn calculate_cyclomatic(&self, node: Node, _source: &[u8]) -> usize {
        let mut complexity = 1; // Base complexity

        fn traverse(node: Node, complexity: &mut usize) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "if_expression" | "match_expression" | "while_expression"
                    | "for_expression" | "loop_expression" => {
                        *complexity += 1;
                    }
                    "match_arm" => {
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

    /// Calculate cognitive complexity (weighted by nesting)
    fn calculate_cognitive(&self, node: Node, _source: &[u8]) -> usize {
        let mut cognitive = 0;

        fn traverse(node: Node, cognitive: &mut usize, nesting: usize) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "if_expression" | "match_expression" | "while_expression"
                    | "for_expression" | "loop_expression" => {
                        *cognitive += 1 + nesting; // +1 for construct, +nesting for depth
                        traverse(child, cognitive, nesting + 1);
                    }
                    "match_arm" => {
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

    /// Count lines of code for a node
    fn count_loc(&self, node: Node) -> usize {
        (node.end_position().row - node.start_position().row + 1).max(1)
    }

    /// Count nesting depth
    fn count_nesting_depth(&self, node: Node) -> usize {
        let mut max_depth = 0;

        fn traverse(node: Node, max_depth: &mut usize, current_depth: usize) {
            *max_depth = (*max_depth).max(current_depth);
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(child.kind(), "if_expression" | "match_expression" | "while_expression"
                    | "for_expression" | "loop_expression" | "block") {
                    traverse(child, max_depth, current_depth + 1);
                } else {
                    traverse(child, max_depth, current_depth);
                }
            }
        }

        traverse(node, &mut max_depth, 0);
        max_depth
    }

    /// Count return statements
    fn count_returns(&self, node: Node) -> usize {
        let mut count = 0;

        fn traverse(node: Node, count: &mut usize) {
            if node.kind() == "return_expression" {
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

impl Default for RustPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create RustPlugin")
    }
}

impl LanguagePlugin for RustPlugin {
    fn language_id(&self) -> &str {
        "rust"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["rs"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_rust::language())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_rust::language())
            .map_err(|e| Error::PluginError(format!("Failed to set Rust grammar: {}", e)))?;

        let tree = parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: file_path.to_string_lossy().to_string().into(),
            line: 0,
            message: "Failed to parse Rust source".to_string(),
        })?;

        let mut symbols = Vec::new();
        let root_node = tree.root_node();
        let file_path_str = file_path.to_string_lossy();

        fn traverse_for_symbols(
            node: Node,
            source: &[u8],
            file_path: &str,
            symbols: &mut Vec<Symbol>,
            plugin: &RustPlugin,
        ) -> Result<()> {
            match node.kind() {
                "function_item" => {
                    symbols.push(plugin.extract_function(node, source, file_path)?);
                }
                "struct_item" => {
                    symbols.push(plugin.extract_type_definition(node, source, file_path, false)?);
                }
                "enum_item" => {
                    symbols.push(plugin.extract_type_definition(node, source, file_path, true)?);
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
        // Placeholder - will be implemented after basic symbol extraction works
        Ok(vec![])
    }

    fn calculate_complexity(&self, symbol: &Symbol, source: &[u8]) -> Result<Option<ComplexityMetrics>> {
        if symbol.symbol_type != SymbolType::Function {
            return Ok(None);
        }

        // Re-parse to find the function node
        let mut parser = Parser::new();
        parser
            .set_language(tree_sitter_rust::language())
            .map_err(|e| Error::PluginError(format!("Failed to set Rust grammar: {}", e)))?;

        let tree = parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: symbol.location.file.clone().into(),
            line: symbol.location.start_line,
            message: "Failed to parse source for complexity analysis".to_string(),
        })?;

        // Find the function node by location
        let root = tree.root_node();
        let target_line = symbol.location.start_line - 1; // TreeSitter uses 0-indexed lines

        fn find_function_at_line(node: Node, line: usize) -> Option<Node> {
            if node.kind() == "function_item" && node.start_position().row == line {
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
                cyclomatic: self.calculate_cyclomatic(func_node, source),
                cognitive: self.calculate_cognitive(func_node, source),
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
    fn test_rust_plugin_language_id() {
        let plugin = RustPlugin::new().unwrap();
        assert_eq!(plugin.language_id(), "rust");
    }

    #[test]
    fn test_rust_plugin_file_extensions() {
        let plugin = RustPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["rs"]);
    }

    #[test]
    fn test_rust_plugin_can_handle() {
        let plugin = RustPlugin::new().unwrap();
        assert!(plugin.can_handle(Path::new("test.rs")));
        assert!(plugin.can_handle(Path::new("src/lib.rs")));
        assert!(!plugin.can_handle(Path::new("test.py")));
    }

    #[test]
    fn test_extract_simple_function() {
        let plugin = RustPlugin::new().unwrap();
        let source = b"fn add(a: i32, b: i32) -> i32 { a + b }";
        let symbols = plugin.extract_symbols(Path::new("test.rs"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "add");
        assert_eq!(symbols[0].symbol_type, SymbolType::Function);
        assert_eq!(symbols[0].parameters.len(), 2);
        assert_eq!(symbols[0].parameters[0].name, "a");
        assert_eq!(symbols[0].parameters[1].name, "b");
        assert!(symbols[0].return_type.is_some());
    }

    #[test]
    fn test_extract_function_with_modifiers() {
        let plugin = RustPlugin::new().unwrap();
        let source = b"pub async fn fetch_data() -> Result<String> { Ok(String::new()) }";
        let symbols = plugin.extract_symbols(Path::new("test.rs"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "fetch_data");
        assert!(symbols[0].modifiers.contains(&"pub".to_string()));
    }

    #[test]
    fn test_extract_struct() {
        let plugin = RustPlugin::new().unwrap();
        let source = b"pub struct User { pub name: String, age: u32 }";
        let symbols = plugin.extract_symbols(Path::new("test.rs"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].symbol_type, SymbolType::Struct);
        assert_eq!(symbols[0].fields.len(), 2);
        assert_eq!(symbols[0].fields[0].name, "name");
        assert_eq!(symbols[0].fields[1].name, "age");
    }

    #[test]
    fn test_extract_enum() {
        let plugin = RustPlugin::new().unwrap();
        let source = b"enum Status { Active, Inactive, Pending }";
        let symbols = plugin.extract_symbols(Path::new("test.rs"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Status");
        assert_eq!(symbols[0].symbol_type, SymbolType::Enum);
    }

    #[test]
    fn test_extract_multiple_symbols() {
        let plugin = RustPlugin::new().unwrap();
        let source = br#"
            fn helper() -> i32 { 42 }

            struct Config {
                port: u16,
            }

            fn main() {
                let c = Config { port: 8080 };
            }
        "#;
        let symbols = plugin.extract_symbols(Path::new("test.rs"), source).unwrap();

        assert_eq!(symbols.len(), 3); // helper, Config, main
        assert_eq!(symbols[0].name, "helper");
        assert_eq!(symbols[1].name, "Config");
        assert_eq!(symbols[2].name, "main");
    }

    #[test]
    fn test_calculate_complexity_simple() {
        let plugin = RustPlugin::new().unwrap();
        let source = b"fn simple() { println!(\"hello\"); }";
        let symbols = plugin.extract_symbols(Path::new("test.rs"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        let complexity = plugin.calculate_complexity(&symbols[0], source).unwrap();

        assert!(complexity.is_some());
        let metrics = complexity.unwrap();
        assert_eq!(metrics.cyclomatic, 1); // No branches
        assert_eq!(metrics.parameters, 0);
    }

    #[test]
    fn test_calculate_complexity_with_branches() {
        let plugin = RustPlugin::new().unwrap();
        let source = br#"
            fn check(x: i32) -> bool {
                if x > 0 {
                    if x < 100 {
                        return true;
                    }
                }
                false
            }
        "#;
        let symbols = plugin.extract_symbols(Path::new("test.rs"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        let complexity = plugin.calculate_complexity(&symbols[0], source).unwrap();

        assert!(complexity.is_some());
        let metrics = complexity.unwrap();
        assert_eq!(metrics.cyclomatic, 3); // Base + 2 if statements
        assert!(metrics.cognitive >= 2); // Nested conditions should have cognitive cost
        assert_eq!(metrics.parameters, 1);
        assert_eq!(metrics.returns, 1);
    }

    #[test]
    fn test_calculate_complexity_with_match() {
        let plugin = RustPlugin::new().unwrap();
        let source = br#"
            fn handle(x: Option<i32>) -> i32 {
                match x {
                    Some(v) => v,
                    None => 0,
                }
            }
        "#;
        let symbols = plugin.extract_symbols(Path::new("test.rs"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        let complexity = plugin.calculate_complexity(&symbols[0], source).unwrap();

        assert!(complexity.is_some());
        let metrics = complexity.unwrap();
        assert!(metrics.cyclomatic >= 2); // Match + arms
    }

    #[test]
    fn test_complexity_not_calculated_for_structs() {
        let plugin = RustPlugin::new().unwrap();
        let source = b"struct Data { value: i32 }";
        let symbols = plugin.extract_symbols(Path::new("test.rs"), source).unwrap();

        assert_eq!(symbols.len(), 1);
        let complexity = plugin.calculate_complexity(&symbols[0], source).unwrap();
        assert!(complexity.is_none()); // Structs don't have complexity
    }

    #[test]
    fn test_source_location_accuracy() {
        let plugin = RustPlugin::new().unwrap();
        let source = br#"
fn first() {}

fn second() {}
"#;
        let symbols = plugin.extract_symbols(Path::new("test.rs"), source).unwrap();

        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].location.start_line, 2); // first() on line 2
        assert_eq!(symbols[1].location.start_line, 4); // second() on line 4
    }
}
