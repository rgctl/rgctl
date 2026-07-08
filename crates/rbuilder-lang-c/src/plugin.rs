//! C language plugin using Tree-sitter.

use rbuilder_plugin_api::*;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// C language plugin.
pub struct CPlugin {
    _parser: Parser,
}

impl CPlugin {
    /// Create a new C plugin.
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set C grammar: {e}")))?;
        Ok(Self { _parser: parser })
    }

    fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set C grammar: {e}")))?;
        parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: "Failed to parse C source".to_string(),
        })
    }

    fn extract_function(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Option<Symbol>> {
        let Some(name) = function_name_from_node(node, source) else {
            return Ok(None);
        };

        Ok(Some(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name: None,
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "c" }),
        }))
    }

    fn extract_struct(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let name = struct_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Struct missing name".to_string(),
        })?;

        Ok(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Struct,
            qualified_name: None,
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "c" }),
        })
    }

    fn extract_enum(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Enum missing name".to_string(),
            })?;

        Ok(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Enum,
            qualified_name: None,
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "c" }),
        })
    }

    fn traverse(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        match node.kind() {
            "function_definition" => {
                if let Some(sym) = self.extract_function(node, source, file_path)? {
                    symbols.push(sym);
                }
            }
            "declaration" => {
                if let Some(sym) = self.extract_function(node, source, file_path)? {
                    symbols.push(sym);
                }
            }
            "struct_specifier" => {
                if struct_name(node, source).is_some() {
                    symbols.push(self.extract_struct(node, source, file_path)?);
                }
            }
            "enum_specifier" => {
                if node.child_by_field_name("name").is_some() {
                    symbols.push(self.extract_enum(node, source, file_path)?);
                }
            }
            "type_definition" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "struct_specifier" && struct_name(child, source).is_some() {
                        symbols.push(self.extract_struct(child, source, file_path)?);
                    } else if child.kind() == "enum_specifier"
                        && child.child_by_field_name("name").is_some()
                    {
                        symbols.push(self.extract_enum(child, source, file_path)?);
                    }
                }
            }
            "preprocessor_include" => {
                let text = node.utf8_text(source)?.trim().to_string();
                symbols.push(Symbol {
                    name: text,
                    symbol_type: SymbolType::Import,
                    qualified_name: None,
                    location: source_location(node, file_path),
                    signature: None,
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "language": "c" }),
                });
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse(child, source, file_path, symbols)?;
        }
        Ok(())
    }
}

impl LanguagePlugin for CPlugin {
    fn language_id(&self) -> &str {
        "c"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["c", "h"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_c::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let tree = self.parse(file_path, source)?;
        let mut symbols = Vec::new();
        self.traverse(
            tree.root_node(),
            source,
            &file_path.to_string_lossy(),
            &mut symbols,
        )?;
        Ok(symbols)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let tree = self.parse(file_path, source)?;
        let mut relations = Vec::new();
        walk_calls(
            tree.root_node(),
            source,
            file_path,
            symbols,
            C_CALL_KINDS,
            "c",
            &mut relations,
        );
        Ok(relations)
    }

    fn calculate_complexity(
        &self,
        _symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        Ok(None)
    }
}

fn function_name_from_node(node: Node, source: &[u8]) -> Option<String> {
    if node.kind() == "function_definition" {
        if let Some(decl) = node.child_by_field_name("declarator") {
            return name_from_declarator(decl, source);
        }
    }
    if node.kind() == "declaration" {
        if let Some(decl) = node.child_by_field_name("declarator") {
            return name_from_declarator(decl, source);
        }
    }
    None
}

fn name_from_declarator(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "type_identifier" => node.utf8_text(source).ok().map(str::to_string),
        "function_declarator" | "pointer_declarator" | "parenthesized_declarator" => {
            if let Some(inner) = node.child_by_field_name("declarator") {
                return name_from_declarator(inner, source);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    if let Some(name) = name_from_declarator(child, source) {
                        return Some(name);
                    }
                }
            }
            None
        }
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.is_named() {
                    if let Some(name) = name_from_declarator(child, source) {
                        return Some(name);
                    }
                }
            }
            None
        }
    }
}

fn struct_name(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
}

fn source_location(node: Node, file_path: &str) -> SourceLocation {
    SourceLocation {
        file: file_path.to_string(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_column: node.start_position().column,
        end_column: node.end_position().column,
    }
}

fn first_line(node: Node, source: &[u8]) -> String {
    node.utf8_text(source)
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_extract_c_function_and_struct() {
        let source = br#"
#include <stdio.h>

struct Cart {
    int user_id;
};

int add(int a, int b) {
    return a + b;
}
"#;
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("cart.c"), source)
            .unwrap();
        assert!(symbols.iter().any(|s| s.name == "add"));
        assert!(symbols.iter().any(|s| s.name == "Cart"));
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
void foo(void) {
    bar();
    baz(1);
}

void bar(void) {}
int baz(int x) { return x; }
"#;
        let plugin = CPlugin::new().unwrap();
        let path = Path::new("example.c");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls)),
            "expected Calls relations, got {relations:?}"
        );
    }

    #[test]
    fn test_extract_function_prototype() {
        let source = br#"int checkout(int user_id);"#;
        let plugin = CPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("order.h"), source)
            .unwrap();
        assert!(symbols.iter().any(|s| s.name == "checkout"));
    }
}
