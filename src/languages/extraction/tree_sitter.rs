//! Generic tree-sitter symbol extraction helpers

use crate::error::Result;
use crate::languages::plugin_trait::{Parameter, SourceLocation, Symbol, SymbolType};
use std::path::Path;
use tree_sitter::{Node, Parser, Tree};

/// Map a tree-sitter node kind to a symbol type using configured kind lists.
pub fn symbol_type_for_kind(
    kind: &str,
    function_kinds: &[&str],
    class_kinds: &[&str],
) -> Option<SymbolType> {
    if function_kinds.contains(&kind) {
        return Some(SymbolType::Function);
    }
    if class_kinds.contains(&kind) {
        return Some(SymbolType::Class);
    }
    match kind {
        "struct_item" | "struct_specifier" | "struct_type" | "struct_declaration" => {
            Some(SymbolType::Struct)
        }
        "enum_item" | "enum_specifier" | "enum_declaration" => Some(SymbolType::Enum),
        "interface_declaration" | "interface_type" | "trait_item" | "trait_declaration" => {
            Some(SymbolType::Interface)
        }
        "type_declaration" | "type_definition" | "class_declaration" | "class_specifier" => {
            Some(SymbolType::Class)
        }
        _ => None,
    }
}

/// Convert a tree-sitter node to a source location.
pub fn node_to_location(node: Node, file: &str) -> SourceLocation {
    SourceLocation {
        file: file.to_string(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_column: node.start_position().column,
        end_column: node.end_position().column,
    }
}

/// Extract symbol name from common tree-sitter child patterns.
pub fn extract_name_from_node(node: Node, source: &[u8]) -> Result<Option<String>> {
    let name_kinds = [
        "identifier",
        "type_identifier",
        "property_identifier",
        "field_identifier",
        "name",
    ];

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if name_kinds.contains(&child.kind()) {
            return Ok(Some(child.utf8_text(source)?.to_string()));
        }
    }

    // Nested name (e.g., function_declarator > identifier)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(name) = extract_name_from_node(child, source)? {
            return Ok(Some(name));
        }
    }

    Ok(None)
}

/// Generic parameter extraction from parameter_list / parameters nodes.
pub fn extract_parameters_generic(params_node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
    let mut parameters = Vec::new();
    let param_kinds = [
        "parameter",
        "parameter_declaration",
        "required_parameter",
        "optional_parameter",
        "formal_parameter",
    ];

    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        if param_kinds.contains(&child.kind()) || child.kind().contains("parameter") {
            let mut name = None;
            let mut param_type = None;
            let mut default_value = None;
            let mut inner = child.walk();
            for param_child in child.children(&mut inner) {
                match param_child.kind() {
                    "identifier" | "name" if name.is_none() => {
                        name = Some(param_child.utf8_text(source)?.to_string());
                    }
                    k if k.contains("type") || k == "type_identifier" => {
                        param_type = Some(param_child.utf8_text(source)?.to_string());
                    }
                    "default_value" | "assignment_expression" => {
                        default_value = Some(param_child.utf8_text(source)?.to_string());
                    }
                    _ => {}
                }
            }
            if let Some(name) = name {
                parameters.push(Parameter {
                    name,
                    param_type,
                    default_value,
                });
            }
        }
    }
    Ok(parameters)
}

/// Walk a tree and extract symbols matching configured node kinds.
pub fn extract_symbols_by_kinds(
    tree: &Tree,
    source: &[u8],
    file_path: &Path,
    function_kinds: &[&str],
    class_kinds: &[&str],
) -> Result<Vec<Symbol>> {
    let file = file_path.to_string_lossy().to_string();
    let mut symbols = Vec::new();
    walk_extract(
        tree.root_node(),
        source,
        &file,
        function_kinds,
        class_kinds,
        &mut symbols,
    )?;
    Ok(symbols)
}

fn walk_extract(
    node: Node,
    source: &[u8],
    file: &str,
    function_kinds: &[&str],
    class_kinds: &[&str],
    symbols: &mut Vec<Symbol>,
) -> Result<()> {
    let kind = node.kind();
    if let Some(symbol_type) = symbol_type_for_kind(kind, function_kinds, class_kinds) {
        if let Some(name) = extract_name_from_node(node, source)? {
            let mut parameters = Vec::new();
            if symbol_type == SymbolType::Function {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind().contains("parameter") {
                        parameters = extract_parameters_generic(child, source)?;
                        break;
                    }
                }
            }

            symbols.push(Symbol {
                name,
                symbol_type,
                qualified_name: None,
                location: node_to_location(node, file),
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
                metadata: serde_json::json!({ "extractor": "generic" }),
            });
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_extract(child, source, file, function_kinds, class_kinds, symbols)?;
    }
    Ok(())
}

/// Parse source with the given grammar and return the tree.
pub fn parse_source(
    source: &[u8],
    file_path: &Path,
    grammar: tree_sitter::Language,
) -> Result<Tree> {
    use crate::error::Error;
    let mut parser = Parser::new();
    parser
        .set_language(grammar)
        .map_err(|e| Error::PluginError(format!("Failed to set grammar: {e}")))?;
    parser.parse(source, None).ok_or_else(|| Error::ParseError {
        file: file_path.to_string_lossy().to_string().into(),
        line: 0,
        message: "Failed to parse source".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[cfg(feature = "lang-c")]
    #[test]
    fn test_generic_c_extraction() {
        let source = b"int add(int a, int b) { return a + b; }";
        let tree = parse_source(source, Path::new("test.c"), tree_sitter_c::language()).unwrap();
        let symbols = extract_symbols_by_kinds(
            &tree,
            source,
            Path::new("test.c"),
            &["function_definition"],
            &["struct_specifier"],
        )
        .unwrap();
        assert!(!symbols.is_empty());
    }
}
