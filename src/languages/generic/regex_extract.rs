//! Shared regex-based symbol extraction for regex and hybrid tree-sitter plugins.

use crate::error::Result;
use crate::languages::generic::config::RegexPatternConfig;
use crate::languages::plugin_trait::{SourceLocation, Symbol};
use regex::Regex;
use std::path::Path;

/// Extract symbols from source using configured regex patterns (line-oriented).
pub fn extract_regex_symbols(
    file_path: &Path,
    source: &[u8],
    patterns: &[RegexPatternConfig],
    extractor: &str,
) -> Result<Vec<Symbol>> {
    let file = file_path.to_string_lossy().to_string();
    let text = String::from_utf8_lossy(source);
    let compiled: Vec<(Regex, _)> = patterns
        .iter()
        .map(|p| {
            Ok((
                Regex::new(p.pattern).map_err(|e| {
                    crate::error::Error::PluginError(format!("Invalid regex: {e}"))
                })?,
                p.symbol_type,
            ))
        })
        .collect::<Result<Vec<_>>>()?;

    let mut symbols = Vec::new();
    for (line_no, line) in text.lines().enumerate() {
        for (re, symbol_type) in &compiled {
            if let Some(cap) = re.captures(line) {
                symbols.push(Symbol {
                    name: cap[1].to_string(),
                    symbol_type: *symbol_type,
                    qualified_name: None,
                    location: SourceLocation {
                        file: file.clone(),
                        start_line: line_no + 1,
                        end_line: line_no + 1,
                        start_column: 0,
                        end_column: 0,
                    },
                    signature: Some(line.trim().to_string()),
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "extractor": extractor }),
                });
            }
        }
    }
    Ok(symbols)
}

/// Merge supplemental symbols into `base`, skipping duplicates at the same line/name.
pub fn merge_symbols(base: &mut Vec<Symbol>, supplemental: Vec<Symbol>) {
    for sym in supplemental {
        let duplicate = base.iter().any(|existing| {
            existing.name == sym.name
                && existing.location.start_line == sym.location.start_line
                && existing.symbol_type == sym.symbol_type
        });
        if !duplicate {
            base.push(sym);
        }
    }
}
