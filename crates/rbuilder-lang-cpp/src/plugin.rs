//! C++ language plugin using Tree-sitter.

use rbuilder_plugin_api::*;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// C++ language plugin.
pub struct CppPlugin {
    _parser: Parser,
}

impl CppPlugin {
    /// Create a new C++ plugin.
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set C++ grammar: {e}")))?;
        Ok(Self { _parser: parser })
    }

    fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set C++ grammar: {e}")))?;
        parser.parse(source, None).ok_or_else(|| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: "Failed to parse C++ source".to_string(),
        })
    }

    fn extract_function(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        scope: Option<&str>,
    ) -> Result<Option<Symbol>> {
        let Some(name) = function_name_from_node(node, source) else {
            return Ok(None);
        };

        let qualified_name = scope.map(|s| format!("{s}::{name}"));

        Ok(Some(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "cpp" }),
        }))
    }

    fn extract_class(
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
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "cpp" }),
        })
    }

    fn traverse(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbols: &mut Vec<Symbol>,
        scope: Option<&str>,
    ) -> Result<()> {
        let next_scope = match node.kind() {
            "namespace_definition" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
                .map(|name| {
                    scope
                        .map(|s| format!("{s}::{name}"))
                        .unwrap_or(name)
                }),
            "class_specifier" | "struct_specifier" => type_name(node, source),
            _ => None,
        };
        let active_scope = next_scope.as_deref().or(scope);

        match node.kind() {
            "function_definition" => {
                if let Some(sym) = self.extract_function(node, source, file_path, active_scope)? {
                    symbols.push(sym);
                }
            }
            "declaration" => {
                if let Some(sym) = self.extract_function(node, source, file_path, active_scope)? {
                    symbols.push(sym);
                }
            }
            "class_specifier" => {
                if type_name(node, source).is_some() {
                    symbols.push(self.extract_class(node, source, file_path, SymbolType::Class)?);
                }
            }
            "struct_specifier" => {
                if type_name(node, source).is_some() {
                    symbols.push(self.extract_class(node, source, file_path, SymbolType::Struct)?);
                }
            }
            "enum_specifier" => {
                if type_name(node, source).is_some() {
                    symbols.push(self.extract_class(node, source, file_path, SymbolType::Enum)?);
                }
            }
            "type_definition" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if matches!(child.kind(), "class_specifier" | "struct_specifier")
                        && type_name(child, source).is_some()
                    {
                        let st = if child.kind() == "class_specifier" {
                            SymbolType::Class
                        } else {
                            SymbolType::Struct
                        };
                        symbols.push(self.extract_class(child, source, file_path, st)?);
                    }
                }
            }
            "preproc_include" | "using_declaration" => {
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
                    metadata: serde_json::json!({ "language": "cpp" }),
                });
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.traverse(child, source, file_path, symbols, active_scope)?;
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
        if matches!(node.kind(), "class_specifier" | "struct_specifier") {
            let class_name = type_name(node, source).unwrap_or_default();
            if !class_name.is_empty() {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "base_class_clause" {
                        collect_base_relations(&class_name, child, source, file_path, relations);
                    }
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_inheritance(child, source, file_path, relations)?;
        }
        Ok(())
    }
}

impl LanguagePlugin for CppPlugin {
    fn language_id(&self) -> &str {
        "cpp"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["cpp", "cc", "cxx", "hpp", "hh", "hxx"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_cpp::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let tree = self.parse(file_path, source)?;
        let mut symbols = Vec::new();
        self.traverse(
            tree.root_node(),
            source,
            &file_path.to_string_lossy(),
            &mut symbols,
            None,
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
            CPP_CALL_KINDS,
            "cpp",
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

fn collect_base_relations(
    from: &str,
    base_clause: Node,
    source: &[u8],
    file_path: &Path,
    relations: &mut Vec<Relation>,
) {
    let mut cursor = base_clause.walk();
    let mut access_public = false;
    for child in base_clause.children(&mut cursor) {
        match child.kind() {
            "access_specifier" => {
                access_public = child.utf8_text(source).ok().is_some_and(|t| t.contains("public"));
            }
            "type_identifier" | "qualified_identifier" => {
                let base = child
                    .utf8_text(source)
                    .ok()
                    .map(str::to_string)
                    .or_else(|| {
                        child
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
                    });
                if let Some(base) = base {
                    let relation_type = if access_public {
                        RelationType::Extends
                    } else {
                        RelationType::Implements
                    };
                    relations.push(relation(from, &base, relation_type, child, file_path));
                }
            }
            _ => {}
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
        metadata: serde_json::json!({ "language": "cpp" }),
        to_qualified_hint: None,
        to_type_hint: None,
    }
}

fn function_name_from_node(node: Node, source: &[u8]) -> Option<String> {
    if matches!(node.kind(), "function_definition" | "declaration") {
        if let Some(decl) = node.child_by_field_name("declarator") {
            return name_from_declarator(decl, source);
        }
    }
    None
}

fn name_from_declarator(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "type_identifier" | "field_identifier" | "destructor_name" => {
            node.utf8_text(source).ok().map(str::to_string)
        }
        "qualified_identifier" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string)),
        "function_declarator" | "pointer_declarator" | "reference_declarator"
        | "parenthesized_declarator" | "array_declarator" => {
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

fn type_name(node: Node, source: &[u8]) -> Option<String> {
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
    fn test_extract_cpp_class_and_method() {
        let source = br#"
#include <string>

class UserService {
public:
    std::string authenticate(const std::string& token) {
        return token;
    }
};
"#;
        let plugin = CppPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("UserService.cpp"), source)
            .unwrap();
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "authenticate"));
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
void foo() {
    bar();
    baz(1);
}

void bar() {}
int baz(int x) { return x; }
"#;
        let plugin = CppPlugin::new().unwrap();
        let path = Path::new("example.cpp");
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
        let source = br#"class ServiceImpl : public BaseService, IService {};"#;
        let plugin = CppPlugin::new().unwrap();
        let path = Path::new("Service.cpp");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Extends)),
            "missing Extends: {relations:?}"
        );
    }
}
