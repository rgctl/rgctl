//! SQL DDL extraction plugin (CREATE TABLE, foreign keys).

use crate::error::Result;
use crate::languages::plugin_trait::*;
use regex::Regex;
use std::path::Path;
use std::sync::LazyLock;

static TABLE_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r##"(?i)CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?(?:`|\[|")?(\w+)"##).unwrap()
});
static COLUMN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r##"^\s*(?:`|\[|")?(\w+)(?:`|\]|")?\s+([A-Za-z0-9_(),\s]+)"##).unwrap()
});
static FK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r##"(?i)REFERENCES\s+(?:`|\[|")?(\w+)(?:`|\]|")?\s*\(\s*(\w+)\s*\)"##).unwrap()
});

/// SQL DDL language plugin.
pub struct SqlPlugin;

impl SqlPlugin {
/// Create a new SQL plugin instance.
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn loc(file: &str, line: usize) -> SourceLocation {
        SourceLocation {
            file: file.to_string(),
            start_line: line,
            end_line: line,
            start_column: 0,
            end_column: 0,
        }
    }
}

impl LanguagePlugin for SqlPlugin {
    fn language_id(&self) -> &str {
        "sql"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["sql"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let file = file_path.to_string_lossy();
        let text = std::str::from_utf8(source).unwrap_or("");
        let mut symbols = Vec::new();

        for (line_no, line) in text.lines().enumerate() {
            if let Some(cap) = TABLE_RE.captures(line) {
                let name = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                symbols.push(Symbol {
                    name: name.clone(),
                    symbol_type: SymbolType::Table,
                    qualified_name: None,
                    location: Self::loc(&file, line_no + 1),
                    signature: Some(line.trim().to_string()),
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "extractor": "sql" }),
                });
            }
        }

        // Attach columns to the most recent table in the same CREATE block.
        let mut current_table: Option<String> = None;
        for (line_no, line) in text.lines().enumerate() {
            if let Some(cap) = TABLE_RE.captures(line) {
                current_table = cap.get(1).map(|m| m.as_str().to_string());
                continue;
            }
            if line.contains(')') && !line.contains('(')
                && (line.trim() == ");" || line.trim().ends_with(");"))
            {
                current_table = None;
            }
            if let Some(table) = &current_table {
                if let Some(cap) = COLUMN_RE.captures(line) {
                    let col = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                    let col_type = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("").to_string();
                    if let Some(sym) = symbols.iter_mut().find(|s| s.name == *table) {
                        sym.fields.push(Field {
                            name: col,
                            field_type: Some(col_type),
                            visibility: None,
                        });
                    }
                    let _ = line_no;
                }
            }
        }

        Ok(symbols)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let file = file_path.to_string_lossy();
        let text = std::str::from_utf8(source).unwrap_or("");
        let mut relations = Vec::new();
        let mut current_table: Option<String> = None;

        for (line_no, line) in text.lines().enumerate() {
            if let Some(cap) = TABLE_RE.captures(line) {
                current_table = cap.get(1).map(|m| m.as_str().to_string());
            }
            if let (Some(from_table), Some(cap)) = (&current_table, FK_RE.captures(line)) {
                let to_table = cap.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
                if symbols.iter().any(|s| s.name == *from_table)
                    && symbols.iter().any(|s| s.name == to_table)
                {
                    relations.push(Relation {
                        from: from_table.clone(),
                        to: to_table,
                        relation_type: RelationType::References,
                        location: Self::loc(&file, line_no + 1),
                        metadata: serde_json::json!({}),
                    });
                }
            }
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sql_ddl_extraction() {
        let plugin = SqlPlugin::new().unwrap();
        let source = br#"
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) NOT NULL
);
CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    user_id INTEGER REFERENCES users(id)
);
"#;
        let symbols = plugin
            .extract_symbols(Path::new("schema.sql"), source)
            .unwrap();
        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "users");
        assert_eq!(symbols[0].symbol_type, SymbolType::Table);
        assert!(!symbols[0].fields.is_empty());
    }

    #[test]
    fn test_sql_foreign_key_relations() {
        let plugin = SqlPlugin::new().unwrap();
        let source = br#"
CREATE TABLE users (id INTEGER PRIMARY KEY);
CREATE TABLE posts (user_id INTEGER REFERENCES users(id));
"#;
        let symbols = plugin
            .extract_symbols(Path::new("schema.sql"), source)
            .unwrap();
        let relations = plugin
            .extract_relations(Path::new("schema.sql"), source, &symbols)
            .unwrap();
        assert_eq!(relations.len(), 1);
        assert_eq!(relations[0].to, "users");
    }
}
