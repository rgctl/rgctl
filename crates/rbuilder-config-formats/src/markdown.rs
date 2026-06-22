//! Markdown documentation parser
//!
//! Task 1.3.5: Extract headings and code blocks from Markdown files.

use pulldown_cmark::{Event, Parser, Tag};
use rbuilder_plugin_api::Result;
use rbuilder_plugin_api::*;
use std::path::Path;

/// Markdown documentation plugin
pub struct MarkdownPlugin;

impl MarkdownPlugin {
    /// Create a new markdown plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl LanguagePlugin for MarkdownPlugin {
    fn language_id(&self) -> &str {
        "markdown"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["md", "mdx"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities {
            extracts_functions: false,
            extracts_types: false,
            extracts_modules: true,
            extracts_relations: false,
            calculates_complexity: false,
            extracts_documentation: true,
            supports_incremental: false,
        }
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let file = file_path.to_string_lossy().to_string();
        let text = String::from_utf8_lossy(source);
        let parser = Parser::new(&text);

        let mut symbols = Vec::new();
        let mut line = 1usize;
        let mut code_index = 0usize;
        let mut pending_heading = false;

        for event in parser {
            match event {
                Event::Start(Tag::Heading { .. }) => {
                    pending_heading = true;
                }
                Event::Text(content) if pending_heading => {
                    symbols.push(Symbol {
                        name: content.to_string(),
                        symbol_type: SymbolType::Module,
                        qualified_name: None,
                        location: SourceLocation {
                            file: file.clone(),
                            start_line: line,
                            end_line: line,
                            start_column: 0,
                            end_column: 0,
                        },
                        signature: None,
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "kind": "heading" }),
                    });
                    pending_heading = false;
                }
                Event::Start(Tag::CodeBlock(kind)) => {
                    let lang = match kind {
                        pulldown_cmark::CodeBlockKind::Fenced(lang) => lang.to_string(),
                        pulldown_cmark::CodeBlockKind::Indented => String::new(),
                    };
                    symbols.push(Symbol {
                        name: format!("code_block_{code_index}"),
                        symbol_type: SymbolType::Module,
                        qualified_name: None,
                        location: SourceLocation {
                            file: file.clone(),
                            start_line: line,
                            end_line: line,
                            start_column: 0,
                            end_column: 0,
                        },
                        signature: None,
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "kind": "code_block", "language": lang }),
                    });
                    code_index += 1;
                }
                Event::SoftBreak | Event::HardBreak => line += 1,
                _ => {}
            }
        }

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
    fn test_markdown_heading_extraction() {
        let md = r#"# API Documentation

## Authentication

### JWT Tokens
"#;
        let plugin = MarkdownPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("README.md"), md.as_bytes())
            .unwrap();

        assert_eq!(symbols.len(), 3);
        assert_eq!(symbols[0].name, "API Documentation");
    }
}
