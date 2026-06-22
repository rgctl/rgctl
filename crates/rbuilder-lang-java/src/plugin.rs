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
        _file_path: &Path,
        _source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        Ok(vec![])
    }

    fn calculate_complexity(
        &self,
        _symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        Ok(None)
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
}
