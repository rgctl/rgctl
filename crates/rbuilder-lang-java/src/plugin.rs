//! Java language plugin
//!
//! Task 3.2.3: Extract classes, interfaces, enums, and methods from Java source.

use rbuilder_plugin_api::*;
use rbuilder_plugin_api::{Error, Result};
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Java language plugin using Tree-sitter.
pub struct JavaPlugin {
    _parser: Parser,
}

impl JavaPlugin {
    /// Create a new Java plugin.
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Java grammar: {e}")))?;
        Ok(Self { _parser: parser })
    }

    fn extract_method(&self, node: Node, source: &[u8], file_path: &str) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut modifiers = Vec::new();
        let mut return_type = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" if name.is_none() => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "type_identifier"
                | "void_type"
                | "integral_type"
                | "floating_point_type"
                | "boolean_type" => {
                    return_type = Some(child.utf8_text(source)?.to_string());
                }
                "modifiers" => {
                    modifiers.push(child.utf8_text(source)?.to_string());
                }
                _ => {}
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Method missing name".to_string(),
        })?;

        // Try to find the containing class for qualified name
        let qualified_name = self.find_containing_class_name(node, source)
            .map(|class| format!("{}.{}", class, name));

        Ok(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name,
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
            return_type,
            parameters: vec![],
            fields: vec![],
            modifiers,
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
        })
    }

    fn extract_type(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbol_type: SymbolType,
    ) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut modifiers = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" if name.is_none() => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "modifiers" => {
                    modifiers.push(child.utf8_text(source)?.to_string());
                }
                _ => {}
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Type missing name".to_string(),
        })?;

        Ok(Symbol {
            name: name.clone(),
            symbol_type,
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
            modifiers,
            documentation: None,
            metadata: serde_json::json!({ "language": "java" }),
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
            "method_declaration" => symbols.push(self.extract_method(node, source, file_path)?),
            "class_declaration" => {
                symbols.push(self.extract_type(node, source, file_path, SymbolType::Class)?)
            }
            "interface_declaration" => {
                symbols.push(self.extract_type(node, source, file_path, SymbolType::Interface)?)
            }
            "enum_declaration" => {
                symbols.push(self.extract_type(node, source, file_path, SymbolType::Enum)?)
            }
            "import_declaration" => {
                let text = node.utf8_text(source)?.trim().to_string();
                symbols.push(Symbol {
                    name: text.clone(),
                    symbol_type: SymbolType::Import,
                    qualified_name: None,
                    location: SourceLocation {
                        file: file_path.to_string(),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        start_column: 0,
                        end_column: 0,
                    },
                    signature: None,
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "language": "java" }),
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

impl LanguagePlugin for JavaPlugin {
    fn language_id(&self) -> &str {
        "java"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["java"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_java::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Java grammar: {e}")))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| Error::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message: "Failed to parse Java source".to_string(),
            })?;

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
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_java::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Java grammar: {e}")))?;

        let tree = parser
            .parse(source, None)
            .ok_or_else(|| Error::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message: "Failed to parse Java source".to_string(),
            })?;

        let mut relations = Vec::new();

        // Extract method calls, inheritance, and implementations
        self.extract_calls(tree.root_node(), source, file_path, symbols, &mut relations)?;
        self.extract_inheritance(
            tree.root_node(),
            source,
            file_path,
            symbols,
            &mut relations,
        )?;

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

impl JavaPlugin {
    /// Extract method call relationships
    fn extract_calls(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let mut cursor = node.walk();

        // Find the containing method for any calls we find
        let containing_method = self.find_containing_method(node, source, symbols);

        // Look for method invocations
        if node.kind() == "method_invocation" {
            if let Some(from_method) = &containing_method {
                // Extract the method name being called
                if let Some(method_name_node) = node.child_by_field_name("name") {
                    let simple_name = method_name_node.utf8_text(source).unwrap_or("").to_string();

                    if !simple_name.is_empty() {
                        // Try to find the qualified name from symbols
                        // Look for any method with this simple name
                        let to_method = symbols
                            .iter()
                            .find(|s| s.name == simple_name && s.symbol_type == SymbolType::Function)
                            .and_then(|s| s.qualified_name.as_ref())
                            .cloned()
                            .unwrap_or(simple_name.clone());

                        relations.push(Relation {
                            from: from_method.clone(),
                            to: to_method,
                            relation_type: RelationType::Calls,
                            location: SourceLocation {
                                file: file_path.to_string_lossy().to_string(),
                                start_line: node.start_position().row + 1,
                                end_line: node.end_position().row + 1,
                                start_column: node.start_position().column,
                                end_column: node.end_position().column,
                            },
                            metadata: serde_json::json!({ "language": "java" }),
                        });
                    }
                }
            }
        }

        // Recurse into children
        for child in node.children(&mut cursor) {
            self.extract_calls(child, source, file_path, symbols, relations)?;
        }

        Ok(())
    }

    /// Extract inheritance relationships (implements and extends)
    fn extract_inheritance(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let mut cursor = node.walk();

        // Handle class declarations
        if node.kind() == "class_declaration" {
            let class_name = self.find_class_name(node, source)?;

            // Look for "extends" clause
            if let Some(superclass) = node.child_by_field_name("superclass") {
                // The superclass node contains "extends" keyword and type_identifier
                let mut sc_cursor = superclass.walk();
                for child in superclass.children(&mut sc_cursor) {
                    if child.kind() == "type_identifier" || child.kind() == "generic_type" {
                        let parent_class = child.utf8_text(source).unwrap_or("").to_string();
                        if !parent_class.is_empty() {
                            relations.push(Relation {
                                from: class_name.clone(),
                                to: parent_class,
                                relation_type: RelationType::Extends,
                                location: SourceLocation {
                                    file: file_path.to_string_lossy().to_string(),
                                    start_line: child.start_position().row + 1,
                                    end_line: child.end_position().row + 1,
                                    start_column: child.start_position().column,
                                    end_column: child.end_position().column,
                                },
                                metadata: serde_json::json!({ "language": "java" }),
                            });
                        }
                    }
                }
            }

            // Look for "implements" clause
            if let Some(interfaces) = node.child_by_field_name("interfaces") {
                let mut impl_cursor = interfaces.walk();
                for interface_node in interfaces.children(&mut impl_cursor) {
                    // Handle type_list which contains the actual type identifiers
                    if interface_node.kind() == "type_list" {
                        let mut type_cursor = interface_node.walk();
                        for type_node in interface_node.children(&mut type_cursor) {
                            if type_node.kind() == "type_identifier" || type_node.kind() == "generic_type" {
                                let interface_name = type_node.utf8_text(source).unwrap_or("").to_string();
                                if !interface_name.is_empty() {
                                    relations.push(Relation {
                                        from: class_name.clone(),
                                        to: interface_name,
                                        relation_type: RelationType::Implements,
                                        location: SourceLocation {
                                            file: file_path.to_string_lossy().to_string(),
                                            start_line: type_node.start_position().row + 1,
                                            end_line: type_node.end_position().row + 1,
                                            start_column: type_node.start_position().column,
                                            end_column: type_node.end_position().column,
                                        },
                                        metadata: serde_json::json!({ "language": "java" }),
                                    });
                                }
                            }
                        }
                    }
                    // Also handle direct type identifiers
                    else if interface_node.kind() == "type_identifier"
                        || interface_node.kind() == "generic_type"
                    {
                        let interface_name =
                            interface_node.utf8_text(source).unwrap_or("").to_string();
                        if !interface_name.is_empty() {
                            relations.push(Relation {
                                from: class_name.clone(),
                                to: interface_name,
                                relation_type: RelationType::Implements,
                                location: SourceLocation {
                                    file: file_path.to_string_lossy().to_string(),
                                    start_line: interface_node.start_position().row + 1,
                                    end_line: interface_node.end_position().row + 1,
                                    start_column: interface_node.start_position().column,
                                    end_column: interface_node.end_position().column,
                                },
                                metadata: serde_json::json!({ "language": "java" }),
                            });
                        }
                    }
                }
            }
        }

        // Recurse into children
        for child in node.children(&mut cursor) {
            self.extract_inheritance(child, source, file_path, symbols, relations)?;
        }

        Ok(())
    }

    /// Find the name of the class containing a given node
    fn find_containing_class_name(&self, node: Node, source: &[u8]) -> Option<String> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "class_declaration" || parent.kind() == "interface_declaration" {
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        return child.utf8_text(source).ok().map(|s| s.to_string());
                    }
                }
            }
            current = parent;
        }
        None
    }

    /// Find the fully qualified name of the method containing a given node
    fn find_containing_method(&self, node: Node, source: &[u8], _symbols: &[Symbol]) -> Option<String> {
        let mut current = node;
        let mut method_name = None;
        let mut class_name = None;

        // Find method name first
        while let Some(parent) = current.parent() {
            if parent.kind() == "method_declaration" && method_name.is_none() {
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        method_name = child.utf8_text(source).ok().map(|s| s.to_string());
                        break;
                    }
                }
            }
            if parent.kind() == "class_declaration" && class_name.is_none() {
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        class_name = child.utf8_text(source).ok().map(|s| s.to_string());
                        break;
                    }
                }
            }
            current = parent;
        }

        // Return qualified name if both found, otherwise just method name
        match (class_name, method_name) {
            (Some(class), Some(method)) => Some(format!("{}.{}", class, method)),
            (None, Some(method)) => Some(method),
            _ => None,
        }
    }

    /// Find the name of a class
    fn find_class_name(&self, node: Node, source: &[u8]) -> Result<String> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                return Ok(child.utf8_text(source)?.to_string());
            }
        }
        Err(Error::ParseError {
            file: "unknown".into(),
            line: node.start_position().row + 1,
            message: "Class missing name".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_extract_java_class_and_method() {
        let source = br#"
public class UserService {
    public String authenticate(String token) {
        return token;
    }
}
"#;
        let plugin = JavaPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("UserService.java"), source)
            .unwrap();
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "authenticate"));
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
public class Example {
    public void foo() {
        bar();
    }
    public void bar() {}
}
"#;
        let plugin = JavaPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("Example.java"), source)
            .unwrap();
        let relations = plugin
            .extract_relations(Path::new("Example.java"), source, &symbols)
            .unwrap();

        println!("Extracted {} relations", relations.len());
        for rel in &relations {
            println!("  {:?}: {} -> {}", rel.relation_type, rel.from, rel.to);
        }

        assert!(!relations.is_empty(), "Should extract at least one relation");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls)),
            "Should extract a Calls relation"
        );
    }

    #[test]
    fn test_extract_relations_implements() {
        let source = br#"public class ServiceImpl implements Service {}"#;
        let plugin = JavaPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("Service.java"), source)
            .unwrap();
        let relations = plugin
            .extract_relations(Path::new("Service.java"), source, &symbols)
            .unwrap();

        println!("Extracted {} relations", relations.len());
        for rel in &relations {
            println!("  {:?}: {} -> {}", rel.relation_type, rel.from, rel.to);
        }

        assert!(!relations.is_empty(), "Should extract at least one relation");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Implements)),
            "Should extract an Implements relation"
        );
    }

    #[test]
    fn test_extract_relations_extends() {
        let source = br#"public class DerivedClass extends BaseClass {}"#;
        let plugin = JavaPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("Base.java"), source)
            .unwrap();
        let relations = plugin
            .extract_relations(Path::new("Base.java"), source, &symbols)
            .unwrap();

        println!("Extracted {} relations", relations.len());
        for rel in &relations {
            println!("  {:?}: {} -> {}", rel.relation_type, rel.from, rel.to);
        }

        assert!(!relations.is_empty(), "Should extract at least one relation");
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Extends)),
            "Should extract an Extends relation"
        );
    }
}
