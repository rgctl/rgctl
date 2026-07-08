//! C# language plugin using Tree-sitter.

use rbuilder_plugin_api::*;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// C# language plugin.
pub struct CSharpPlugin {
    _parser: Parser,
}

impl CSharpPlugin {
    /// Create a new C# plugin.
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set C# grammar: {e}")))?;
        Ok(Self { _parser: parser })
    }

    fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set C# grammar: {e}")))?;
        parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: "Failed to parse C# source".to_string(),
        })
    }

    fn extract_method(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let name = node
            .child_by_field_name("name")
            .or_else(|| find_child_kind(node, "identifier"))
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string)
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Method missing name".to_string(),
            })?;

        let qualified_name = self
            .find_containing_type_name(node, source)
            .map(|ty| format!("{ty}.{name}"));

        let return_type = method_return_type(node, source);
        let modifiers = modifier_texts(node, source);

        Ok(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type,
            parameters: vec![],
            fields: vec![],
            modifiers,
            documentation: None,
            metadata: serde_json::json!({ "language": "csharp" }),
        })
    }

    fn extract_type(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbol_type: SymbolType,
    ) -> Result<Symbol> {
        let name = type_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Type missing name".to_string(),
        })?;

        Ok(Symbol {
            name: name.clone(),
            symbol_type,
            qualified_name: None,
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: modifier_texts(node, source),
            documentation: None,
            metadata: serde_json::json!({ "language": "csharp" }),
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
            "method_declaration" | "local_function_statement" | "constructor_declaration" => {
                symbols.push(self.extract_method(node, source, file_path)?);
            }
            "class_declaration" | "struct_declaration" => {
                symbols.push(self.extract_type(node, source, file_path, SymbolType::Class)?);
            }
            "interface_declaration" => {
                symbols.push(self.extract_type(node, source, file_path, SymbolType::Interface)?);
            }
            "enum_declaration" => {
                symbols.push(self.extract_type(node, source, file_path, SymbolType::Enum)?);
            }
            "using_directive" => {
                let text = node.utf8_text(source)?.trim().to_string();
                symbols.push(Symbol {
                    name: text.clone(),
                    symbol_type: SymbolType::Import,
                    qualified_name: None,
                    location: source_location(node, file_path),
                    signature: None,
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "language": "csharp" }),
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

    fn extract_inheritance(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        match node.kind() {
            "class_declaration" => {
                let class_name = type_name(node, source).unwrap_or_default();
                if class_name.is_empty() {
                    return Ok(());
                }
                if let Some(base_list) = node.child_by_field_name("bases") {
                    collect_base_list_relations(
                        &class_name,
                        base_list,
                        source,
                        file_path,
                        true,
                        relations,
                    );
                } else if let Some(base_list) = find_child_kind(node, "base_list") {
                    collect_base_list_relations(
                        &class_name,
                        base_list,
                        source,
                        file_path,
                        true,
                        relations,
                    );
                }
            }
            "interface_declaration" => {
                let name = type_name(node, source).unwrap_or_default();
                if name.is_empty() {
                    return Ok(());
                }
                if let Some(base_list) = node.child_by_field_name("bases") {
                    collect_base_list_relations(
                        &name, base_list, source, file_path, false, relations,
                    );
                } else if let Some(base_list) = find_child_kind(node, "base_list") {
                    collect_base_list_relations(
                        &name, base_list, source, file_path, false, relations,
                    );
                }
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_inheritance(child, source, file_path, relations)?;
        }
        Ok(())
    }

    fn find_containing_type_name(&self, node: Node, source: &[u8]) -> Option<String> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if matches!(
                parent.kind(),
                "class_declaration" | "struct_declaration" | "interface_declaration"
            ) {
                return type_name(parent, source);
            }
            current = parent;
        }
        None
    }
}

impl LanguagePlugin for CSharpPlugin {
    fn language_id(&self) -> &str {
        "csharp"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["cs"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_c_sharp::LANGUAGE.into())
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
            CSHARP_CALL_KINDS,
            "csharp",
            &mut relations,
        );
        self.extract_inheritance(tree.root_node(), source, file_path, &mut relations)?;
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

fn collect_base_list_relations(
    from: &str,
    base_list: Node,
    source: &[u8],
    file_path: &Path,
    class_style: bool,
    relations: &mut Vec<Relation>,
) {
    let bases: Vec<String> = base_list
        .children(&mut base_list.walk())
        .filter(|c| c.is_named() && c.kind() == "identifier")
        .filter_map(|c| c.utf8_text(source).ok().map(str::to_string))
        .collect();

    if bases.is_empty() {
        return;
    }

    if class_style {
        if let Some(base) = bases.first() {
            relations.push(relation(
                from,
                base,
                RelationType::Extends,
                base_list,
                file_path,
            ));
        }
        for iface in bases.iter().skip(1) {
            relations.push(relation(
                from,
                iface,
                RelationType::Implements,
                base_list,
                file_path,
            ));
        }
    } else {
        for iface in &bases {
            relations.push(relation(
                from,
                iface,
                RelationType::Implements,
                base_list,
                file_path,
            ));
        }
    }
}

fn relation(from: &str, to: &str, relation_type: RelationType, node: Node, file_path: &Path) -> Relation {
    Relation {
        from: from.to_string(),
        to: to.to_string(),
        relation_type,
        location: SourceLocation {
            file: file_path.to_string_lossy().to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_column: node.start_position().column,
            end_column: node.end_position().column,
        },
        metadata: serde_json::json!({ "language": "csharp" }),
        to_qualified_hint: None,
        to_type_hint: None,
    }
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

fn modifier_texts(node: Node, source: &[u8]) -> Vec<String> {
    node.children(&mut node.walk())
        .filter(|c| c.kind() == "modifier")
        .filter_map(|c| c.utf8_text(source).ok().map(str::to_string))
        .collect()
}

fn type_name(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .or_else(|| find_child_kind(node, "identifier"))
        .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
}

fn method_return_type(node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "predefined_type" | "identifier" | "generic_name" | "nullable_type"
        ) {
            return child.utf8_text(source).ok().map(str::to_string);
        }
    }
    None
}

fn find_child_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = find_child_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_extract_csharp_class_and_method() {
        let source = br#"
using System;

public class UserService {
    public string Authenticate(string token) {
        return token;
    }
}
"#;
        let plugin = CSharpPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("UserService.cs"), source)
            .unwrap();
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "Authenticate"));
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
public class Example {
    public void Foo() {
        Bar();
    }
    public void Bar() {}
}
"#;
        let plugin = CSharpPlugin::new().unwrap();
        let path = Path::new("Example.cs");
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
    fn test_extract_relations_inheritance() {
        let source = br#"public class ServiceImpl : BaseService, IService {}"#;
        let plugin = CSharpPlugin::new().unwrap();
        let path = Path::new("Service.cs");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Extends)),
            "missing Extends: {relations:?}"
        );
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Implements)),
            "missing Implements: {relations:?}"
        );
    }
}
