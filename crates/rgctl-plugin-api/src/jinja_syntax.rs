//! Shared immutable syntax settings; parser implementation lives in the Jinja plugin.
use serde::{Deserialize, Serialize};

/// Statically known Jinja delimiters. Line syntax is disabled by default.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct JinjaSyntax {
    /// Opening expression delimiter.
    pub variable_start_string: String,
    /// Closing expression delimiter.
    pub variable_end_string: String,
    /// Opening statement delimiter.
    pub block_start_string: String,
    /// Closing statement delimiter.
    pub block_end_string: String,
    /// Opening comment delimiter.
    pub comment_start_string: String,
    /// Closing comment delimiter.
    pub comment_end_string: String,
    /// Optional prefix for statements beginning a line.
    pub line_statement_prefix: Option<String>,
    /// Optional prefix for comments beginning a line.
    pub line_comment_prefix: Option<String>,
}

impl Default for JinjaSyntax {
    fn default() -> Self {
        Self {
            variable_start_string: "{{".into(),
            variable_end_string: "}}".into(),
            block_start_string: "{%".into(),
            block_end_string: "%}".into(),
            comment_start_string: "{#".into(),
            comment_end_string: "#}".into(),
            line_statement_prefix: None,
            line_comment_prefix: None,
        }
    }
}

impl JinjaSyntax {
    /// Reject empty, multiline, or ambiguous opening delimiters.
    ///
    /// # Errors
    /// Returns an error for an invalid profile.
    pub fn validate(&self) -> std::result::Result<(), String> {
        let starts = [
            &self.variable_start_string,
            &self.block_start_string,
            &self.comment_start_string,
        ];
        for token in starts.iter().copied().chain([
            &self.variable_end_string,
            &self.block_end_string,
            &self.comment_end_string,
        ]) {
            if token.is_empty() || token.contains(['\r', '\n']) {
                return Err("template delimiters must be nonempty single-line strings".into());
            }
        }
        for (i, left) in starts.iter().enumerate() {
            for right in &starts[i + 1..] {
                if left.starts_with(right.as_str()) || right.starts_with(left.as_str()) {
                    return Err("template opening delimiters must not overlap".into());
                }
            }
        }
        for prefix in [&self.line_statement_prefix, &self.line_comment_prefix]
            .into_iter()
            .flatten()
        {
            if prefix.is_empty() || prefix.contains(['\r', '\n']) {
                return Err("line prefixes must be nonempty single-line strings".into());
            }
        }
        if self.line_statement_prefix.is_some()
            && self.line_statement_prefix == self.line_comment_prefix
        {
            return Err("statement and comment line prefixes must differ".into());
        }
        Ok(())
    }
}
