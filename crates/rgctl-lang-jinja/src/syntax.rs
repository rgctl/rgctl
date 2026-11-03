//! Syntax normalization with an original-byte boundary map.

use rgctl_plugin_api::{Error, Result, SourceLocation, parse_with_pool};
use std::path::Path;

pub use rgctl_plugin_api::JinjaSyntax as SyntaxProfile;

/// A pooled parse plus normalized bytes and original source boundaries.
pub struct ParsedTemplate {
    /// The syntax tree. Locations in this tree address normalized bytes.
    pub tree: tree_sitter::Tree,
    /// Canonical parser input; literals have inactive opening braces masked.
    pub normalized: Vec<u8>,
    /// Effective syntax after applying a file header.
    pub syntax: SyntaxProfile,
    source_boundaries: Vec<usize>,
    line_offsets: Vec<usize>,
}

impl ParsedTemplate {
    /// Map a normalized byte boundary to an original source boundary.
    pub fn original_offset(&self, offset: usize) -> usize {
        self.source_boundaries
            .get(offset)
            .copied()
            .unwrap_or_else(|| self.source_boundaries.last().copied().unwrap_or(0))
    }

    /// Recover a node's span in the original source.
    pub fn original_range(&self, node: tree_sitter::Node<'_>) -> std::ops::Range<usize> {
        self.original_offset(node.start_byte())..self.original_offset(node.end_byte())
    }

    /// Convert a parsed node's span to a location in the original UTF-8 bytes.
    pub fn location(&self, node: tree_sitter::Node<'_>, path: &Path) -> SourceLocation {
        let range = self.original_range(node);
        self.location_for_range(range, path.to_string_lossy().into_owned())
    }

    /// Map an original byte range to a source location using cached line offsets.
    pub fn location_for_range(
        &self,
        range: std::ops::Range<usize>,
        file: String,
    ) -> SourceLocation {
        let point = |offset: usize| {
            let row = self.line_offsets.partition_point(|line| *line <= offset);
            let start = self
                .line_offsets
                .get(row.saturating_sub(1))
                .copied()
                .unwrap_or(0);
            (row.max(1), offset.saturating_sub(start))
        };
        let (start_line, start_column) = point(range.start);
        let (end_line, end_column) = point(range.end);
        SourceLocation {
            file,
            start_line,
            end_line,
            start_column,
            end_column,
        }
    }
}

#[derive(Default)]
struct MappedBytes {
    bytes: Vec<u8>,
    boundaries: Vec<usize>,
}

impl MappedBytes {
    fn token(&mut self, token: &[u8], original: usize) {
        self.bytes.extend_from_slice(token);
        self.boundaries
            .resize(self.boundaries.len() + token.len(), original);
    }

    fn copy(&mut self, source: &[u8], start: usize, end: usize, literal: bool) {
        for (index, byte) in source.iter().enumerate().take(end).skip(start) {
            // Inactive delimiters in literal content must not become canonical
            // expressions after normalization (e.g. {{ literal }} with [% %]).
            self.bytes.push(if literal && *byte == b'{' {
                b' '
            } else {
                *byte
            });
            self.boundaries.push(index);
        }
    }

    fn finish(
        mut self,
        source: &[u8],
        syntax: SyntaxProfile,
        path: &Path,
    ) -> Result<ParsedTemplate> {
        self.boundaries.push(source.len());
        let mut line_offsets = vec![0];
        line_offsets.extend(
            source
                .iter()
                .enumerate()
                .filter_map(|(index, byte)| (*byte == b'\n').then_some(index + 1)),
        );
        let tree = parse_with_pool("jinja", crate::language(), &self.bytes).map_err(|message| {
            Error::ParseError {
                file: path.to_path_buf(),
                line: 0,
                message,
            }
        })?;
        Ok(ParsedTemplate {
            tree,
            normalized: self.bytes,
            source_boundaries: self.boundaries,
            syntax,
            line_offsets,
        })
    }
}

fn syntax_error(path: &Path, message: impl Into<String>) -> Error {
    Error::ParseError {
        file: path.to_path_buf(),
        line: 1,
        message: message.into(),
    }
}

fn split_settings(header: &str) -> std::result::Result<Vec<&str>, String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quote = None;
    let mut escaped = false;
    for (index, byte) in header.bytes().enumerate() {
        if escaped {
            escaped = false;
        } else if quote.is_some() && byte == b'\\' {
            escaped = true;
        } else if quote == Some(byte) {
            quote = None;
        } else if quote.is_none() && matches!(byte, b'\'' | b'"') {
            quote = Some(byte);
        } else if quote.is_none() && byte == b',' {
            parts.push(&header[start..index]);
            start = index + 1;
        }
    }
    if quote.is_some() {
        return Err("unterminated quoted syntax setting".into());
    }
    parts.push(&header[start..]);
    Ok(parts)
}

fn apply_header(
    source: &[u8],
    defaults: &SyntaxProfile,
) -> std::result::Result<(SyntaxProfile, usize), String> {
    let start = usize::from(source.starts_with(b"\xef\xbb\xbf")) * 3;
    let end = source
        .iter()
        .position(|b| *b == b'\n')
        .unwrap_or(source.len());
    let first = std::str::from_utf8(&source[start..end]).map_err(|e| e.to_string())?;
    let Some(header) = first.strip_prefix("#jinja2:") else {
        return Ok((defaults.clone(), 0));
    };
    let mut syntax = defaults.clone();
    for entry in split_settings(header)? {
        if entry.trim().is_empty() {
            continue;
        }
        let Some((key, raw)) = entry.split_once(':') else {
            return Err("syntax settings require key: value entries".into());
        };
        let raw = raw.trim();
        let value = if raw.starts_with(['\'', '"']) {
            let quote = raw.as_bytes()[0];
            if raw.len() < 2 || raw.as_bytes().last() != Some(&quote) {
                return Err("invalid quoted syntax setting".into());
            }
            raw[1..raw.len() - 1].to_owned()
        } else {
            raw.to_owned()
        };
        match key.trim() {
            "variable_start_string" => syntax.variable_start_string = value,
            "variable_end_string" => syntax.variable_end_string = value,
            "block_start_string" => syntax.block_start_string = value,
            "block_end_string" => syntax.block_end_string = value,
            "comment_start_string" => syntax.comment_start_string = value,
            "comment_end_string" => syntax.comment_end_string = value,
            "line_statement_prefix" => syntax.line_statement_prefix = Some(value),
            "line_comment_prefix" => syntax.line_comment_prefix = Some(value),
            "trim_blocks" | "lstrip_blocks" | "keep_trailing_newline" => {
                if !matches!(raw, "true" | "false" | "True" | "False") {
                    return Err("whitespace settings must be literal booleans".into());
                }
            }
            key => return Err(format!("unsupported Jinja syntax setting: {key}")),
        }
    }
    Ok((syntax, end))
}

fn code_end(source: &[u8], mut pos: usize, end: &[u8], line: bool) -> usize {
    let mut quote = None;
    let mut escaped = false;
    let mut brackets = Vec::new();
    while pos < source.len() {
        let byte = source[pos];
        if quote.is_none() && brackets.is_empty() {
            if (!line && source[pos..].starts_with(end)) || (line && byte == b'\n') {
                break;
            }
        }
        if escaped {
            escaped = false;
        } else if quote.is_some() && byte == b'\\' {
            escaped = true;
        } else if quote == Some(byte) {
            quote = None;
        } else if quote.is_none() {
            match byte {
                b'\'' | b'"' => quote = Some(byte),
                b'(' | b'[' | b'{' => brackets.push(byte),
                b')' | b']' | b'}' => {
                    let expected = match byte {
                        b')' => b'(',
                        b']' => b'[',
                        _ => b'{',
                    };
                    if brackets.last() == Some(&expected) {
                        brackets.pop();
                    }
                }
                _ => {}
            }
        }
        pos += 1;
    }
    pos
}

fn tag_word(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes)
        .unwrap_or("")
        .trim_matches(|c: char| c.is_whitespace() || matches!(c, '-' | '+'))
}

/// Parse a complete template with explicit syntax and optional file header.
///
/// # Errors
/// Returns an error for invalid UTF-8, syntax configuration, or parser failure.
/// Syntax errors in the tree are retained for diagnostic extraction.
pub fn parse_template(
    path: &Path,
    source: &[u8],
    defaults: &SyntaxProfile,
) -> Result<ParsedTemplate> {
    normalize(path, source, defaults, true)
}

/// Parse an embedded Ansible scalar with standard syntax and no file header.
///
/// # Errors
/// Returns an error for invalid UTF-8 or parser setup failure.
pub fn parse_fragment(path: &Path, source: &[u8]) -> Result<ParsedTemplate> {
    normalize(path, source, &SyntaxProfile::default(), false)
}

fn normalize(
    path: &Path,
    source: &[u8],
    defaults: &SyntaxProfile,
    allow_header: bool,
) -> Result<ParsedTemplate> {
    std::str::from_utf8(source).map_err(|e| syntax_error(path, e.to_string()))?;
    let (syntax, header_end) = if allow_header {
        apply_header(source, defaults).map_err(|e| syntax_error(path, e))?
    } else {
        (defaults.clone(), 0)
    };
    syntax.validate().map_err(|e| syntax_error(path, e))?;
    let mut mapped = MappedBytes {
        bytes: Vec::with_capacity(source.len()),
        boundaries: Vec::with_capacity(source.len() + 1),
    };
    let mut pos = 0;
    let mut raw = false;
    while pos < source.len() {
        if pos < header_end {
            let byte = source[pos];
            mapped.token(if byte == b'\r' { b"\r" } else { b" " }, pos);
            pos += 1;
            continue;
        }
        if pos == 0 || source[pos - 1] == b'\n' {
            let prefix_pos = pos
                + source[pos..]
                    .iter()
                    .take_while(|b| matches!(**b, b' ' | b'\t'))
                    .count();
            let comment = syntax
                .line_comment_prefix
                .as_deref()
                .filter(|prefix| source[prefix_pos..].starts_with(prefix.as_bytes()));
            let statement = syntax
                .line_statement_prefix
                .as_deref()
                .filter(|prefix| source[prefix_pos..].starts_with(prefix.as_bytes()));
            if let Some(prefix) = comment.filter(|comment| {
                !raw && statement.is_none_or(|statement| comment.len() >= statement.len())
            }) {
                let end = source[prefix_pos..]
                    .iter()
                    .position(|b| *b == b'\n')
                    .map_or(source.len(), |n| prefix_pos + n);
                mapped.token(b"{#", pos);
                for index in prefix_pos + prefix.len()..end {
                    mapped.token(
                        if source[index] == b'#' {
                            b" "
                        } else {
                            &source[index..index + 1]
                        },
                        index,
                    );
                }
                mapped.token(b"#}", end);
                pos = end;
                continue;
            }
            if let Some(prefix) = statement {
                let body = prefix_pos + prefix.len();
                let end = code_end(source, body, b"", true);
                let word = tag_word(&source[body..end]).trim_end_matches(':').trim();
                if !raw || word == "endraw" {
                    mapped.token(b"{%", pos);
                    let trimmed_end = end
                        - source[body..end]
                            .iter()
                            .rev()
                            .take_while(|b| b.is_ascii_whitespace())
                            .count();
                    let body_end = if source[..trimmed_end].ends_with(b":") {
                        trimmed_end - 1
                    } else {
                        trimmed_end
                    };
                    mapped.copy(source, body, body_end, false);
                    mapped.copy(source, trimmed_end, end, false);
                    mapped.token(b"%}", end);
                    raw = word == "raw" || (raw && word != "endraw");
                    pos = end;
                    continue;
                }
            }
        }
        let mut handled = false;
        for (open, close, canonical_open, canonical_close, kind) in [
            (
                &syntax.variable_start_string,
                &syntax.variable_end_string,
                b"{{",
                b"}}",
                0,
            ),
            (
                &syntax.block_start_string,
                &syntax.block_end_string,
                b"{%",
                b"%}",
                1,
            ),
            (
                &syntax.comment_start_string,
                &syntax.comment_end_string,
                b"{#",
                b"#}",
                2,
            ),
        ] {
            if !source[pos..].starts_with(open.as_bytes()) || (raw && kind != 1) {
                continue;
            }
            let body = pos + open.len();
            let end = if kind == 2 {
                source[body..]
                    .windows(close.len())
                    .position(|w| w == close.as_bytes())
                    .map_or(source.len(), |n| body + n)
            } else {
                code_end(source, body, close.as_bytes(), false)
            };
            let word = tag_word(&source[body..end]);
            if raw && word != "endraw" {
                continue;
            }
            mapped.token(canonical_open, pos);
            if kind == 2 {
                for index in body..end {
                    mapped.token(
                        if source[index] == b'#' {
                            b" "
                        } else {
                            &source[index..index + 1]
                        },
                        index,
                    );
                }
            } else {
                mapped.copy(source, body, end, false);
            }
            if end < source.len() {
                mapped.token(canonical_close, end);
                pos = end + close.len();
            } else {
                pos = end;
            }
            if kind == 1 {
                raw = word == "raw" || (raw && word != "endraw");
            }
            handled = true;
            break;
        }
        if !handled {
            mapped.copy(source, pos, pos + 1, true);
            pos += 1;
        }
    }
    mapped.finish(source, syntax, path)
}

/// Parse a bare expression using the same pooled grammar as complete templates.
///
/// # Errors
/// Returns an error for invalid UTF-8 or parser setup failure.
pub fn parse_expression(path: &Path, source: &[u8]) -> Result<ParsedTemplate> {
    std::str::from_utf8(source).map_err(|e| syntax_error(path, e.to_string()))?;
    let mut mapped = MappedBytes::default();
    mapped.token(b"{{ ", 0);
    mapped.copy(source, 0, source.len(), false);
    mapped.token(b" }}", source.len());
    mapped.finish(source, SyntaxProfile::default(), path)
}
