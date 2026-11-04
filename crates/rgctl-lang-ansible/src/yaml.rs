//! Borrowed YAML views and decoded-scalar source maps over one tree-sitter parse.

use rgctl_plugin_api::{Error, Result, SourceLocation, parse_with_pool};
use std::borrow::Cow;
use std::ops::Range;
use std::path::Path;
use tree_sitter::{Node, Tree};

pub(crate) struct Yaml<'source> {
    pub source: &'source [u8],
    pub tree: Tree,
    lines: Vec<usize>,
}

pub(crate) struct Scalar {
    pub value: String,
    pub literal: bool,
    offsets: Vec<usize>,
    lines: Vec<usize>,
}

impl Scalar {
    pub fn range(&self, range: Range<usize>) -> Range<usize> {
        let offset = |index: usize| {
            self.offsets
                .get(index)
                .copied()
                .unwrap_or_else(|| self.offsets.last().copied().unwrap_or(0))
        };
        offset(range.start)..offset(range.end)
    }

    pub fn location(&self, yaml: &Yaml<'_>, location: &SourceLocation) -> SourceLocation {
        let offset = |line: usize, column: usize| {
            self.lines
                .get(line.saturating_sub(1))
                .copied()
                .unwrap_or(self.value.len())
                .saturating_add(column)
                .min(self.value.len())
        };
        yaml.location(
            self.range(
                offset(location.start_line, location.start_column)
                    ..offset(location.end_line, location.end_column),
            ),
            location.file.clone(),
        )
    }
}

#[derive(Default)]
struct Decode {
    bytes: Vec<u8>,
    offsets: Vec<usize>,
}

impl Decode {
    fn literal(&mut self, source: &[u8], start: usize, end: usize) {
        self.bytes.extend_from_slice(&source[start..end]);
        self.offsets.extend(start..end);
    }

    fn character(&mut self, character: char, offset: usize) {
        let mut bytes = [0u8; 4];
        let bytes = character.encode_utf8(&mut bytes).as_bytes();
        self.bytes.extend_from_slice(bytes);
        self.offsets
            .resize(self.offsets.len() + bytes.len(), offset);
    }

    fn finish(mut self, end: usize) -> std::result::Result<Scalar, String> {
        self.offsets.push(end);
        let value = String::from_utf8(self.bytes).map_err(|error| error.to_string())?;
        let mut lines = vec![0];
        lines.extend(
            value
                .bytes()
                .enumerate()
                .filter_map(|(index, byte)| (byte == b'\n').then_some(index + 1)),
        );
        Ok(Scalar {
            value,
            literal: false,
            offsets: self.offsets,
            lines,
        })
    }
}

pub(crate) fn named(node: Node<'_>) -> impl DoubleEndedIterator<Item = Node<'_>> {
    (0..node.named_child_count()).filter_map(move |index| node.named_child(index))
}

pub(crate) fn value(mut node: Node<'_>) -> Node<'_> {
    while matches!(
        node.kind(),
        "stream" | "document" | "block_node" | "flow_node" | "block_sequence_item"
    ) {
        let next = named(node).find(|child| {
            !matches!(
                child.kind(),
                "comment"
                    | "anchor"
                    | "tag"
                    | "yaml_directive"
                    | "tag_directive"
                    | "reserved_directive"
            )
        });
        let Some(child) = next else { break };
        node = child;
    }
    node
}

pub(crate) fn sequence(node: Node<'_>) -> Vec<Node<'_>> {
    let node = value(node);
    if matches!(node.kind(), "block_sequence" | "flow_sequence") {
        named(node)
            .filter(|child| child.kind() != "comment")
            .map(value)
            .collect()
    } else {
        Vec::new()
    }
}

impl<'source> Yaml<'source> {
    pub fn parse(path: &Path, source: &'source [u8]) -> Result<Self> {
        std::str::from_utf8(source).map_err(|error| Error::ParseError {
            file: path.to_path_buf(),
            line: 1,
            message: error.to_string(),
        })?;
        let tree = parse_with_pool("ansible", crate::language(), source).map_err(|message| {
            Error::ParseError {
                file: path.to_path_buf(),
                line: 0,
                message,
            }
        })?;
        let mut lines = vec![0];
        lines.extend(
            source
                .iter()
                .enumerate()
                .filter_map(|(index, byte)| (*byte == b'\n').then_some(index + 1)),
        );
        Ok(Self {
            source,
            tree,
            lines,
        })
    }

    pub fn location(&self, range: Range<usize>, file: String) -> SourceLocation {
        let point = |offset: usize| {
            let line = self.lines.partition_point(|start| *start <= offset).max(1);
            let start = self.lines.get(line - 1).copied().unwrap_or(0);
            (line, offset.saturating_sub(start))
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

    pub fn node_location(&self, node: Node<'_>, file: &str) -> SourceLocation {
        self.location(node.byte_range(), file.to_owned())
    }

    pub fn key(&self, node: Node<'_>) -> std::result::Result<Cow<'source, str>, String> {
        let node = value(node);
        let bytes = &self.source[node.byte_range()];
        if !matches!(node.kind(), "single_quote_scalar" | "double_quote_scalar") {
            return std::str::from_utf8(bytes)
                .map(Cow::Borrowed)
                .map_err(|error| error.to_string());
        }
        if bytes.len() >= 2
            && !bytes[1..bytes.len() - 1]
                .iter()
                .any(|byte| matches!(byte, b'\\' | b'\'' | b'"' | b'\r' | b'\n'))
        {
            return std::str::from_utf8(&bytes[1..bytes.len() - 1])
                .map(Cow::Borrowed)
                .map_err(|error| error.to_string());
        }
        self.scalar(node).map(|scalar| Cow::Owned(scalar.value))
    }

    pub fn pairs<'tree>(&self, node: Node<'tree>) -> Vec<(Cow<'source, str>, Node<'tree>)> {
        let node = value(node);
        if !matches!(node.kind(), "block_mapping" | "flow_mapping") {
            return Vec::new();
        }
        let mut pairs = Vec::with_capacity(node.named_child_count());
        for pair in
            named(node).filter(|child| matches!(child.kind(), "block_mapping_pair" | "flow_pair"))
        {
            if let (Some(key), Some(value)) = (
                pair.child_by_field_name("key"),
                pair.child_by_field_name("value"),
            ) && let Ok(key) = self.key(key)
            {
                pairs.push((key, value));
            }
        }
        pairs
    }

    pub fn scalar(&self, node: Node<'_>) -> std::result::Result<Scalar, String> {
        let literal = self.tag(node) == Some("!unsafe");
        let node = value(node);
        let mut decoded = Decode::default();
        match node.kind() {
            "block_scalar" => self.block_scalar(node, &mut decoded)?,
            "single_quote_scalar" | "double_quote_scalar" => {
                let start = node.start_byte();
                let end = node.end_byte();
                if end < start + 2 {
                    return Err("unterminated YAML scalar".into());
                }
                self.flow_scalar(
                    start + 1,
                    end - 1,
                    node.kind() == "double_quote_scalar",
                    node.kind() == "single_quote_scalar",
                    &mut decoded,
                )?;
            }
            "plain_scalar" | "string_scalar" | "boolean_scalar" | "integer_scalar"
            | "float_scalar" | "null_scalar" | "timestamp_scalar" => {
                self.flow_scalar(
                    node.start_byte(),
                    node.end_byte(),
                    false,
                    false,
                    &mut decoded,
                )?;
            }
            "alias" => return Err("YAML aliases require a known value and are unresolved".into()),
            _ => return Err(format!("expected a scalar, found {}", node.kind())),
        }
        decoded.finish(node.end_byte()).map(|mut scalar| {
            scalar.literal = literal;
            scalar
        })
    }

    pub fn tag(&self, mut node: Node<'_>) -> Option<&'source str> {
        loop {
            if let Some(tag) = named(node).find(|child| child.kind() == "tag") {
                return std::str::from_utf8(&self.source[tag.byte_range()]).ok();
            }
            if !matches!(
                node.kind(),
                "flow_node" | "block_node" | "block_sequence_item"
            ) {
                return None;
            }
            node =
                named(node).find(|child| !matches!(child.kind(), "anchor" | "comment" | "tag"))?;
        }
    }

    fn flow_scalar(
        &self,
        mut pos: usize,
        end: usize,
        double: bool,
        single: bool,
        out: &mut Decode,
    ) -> std::result::Result<(), String> {
        let mut trim = out.bytes.len();
        while pos < end {
            let byte = self.source[pos];
            if single && byte == b'\'' && self.source.get(pos + 1) == Some(&b'\'') {
                out.character('\'', pos);
                pos += 2;
                trim = out.bytes.len();
                continue;
            }
            if double && byte == b'\\' {
                let escaped_at = pos;
                pos += 1;
                let escaped = self
                    .source
                    .get(pos)
                    .copied()
                    .ok_or("unterminated YAML escape")?;
                pos += 1;
                let character = match escaped {
                    b'0' => '\0',
                    b'a' => '\x07',
                    b'b' => '\x08',
                    b't' | b'\t' => '\t',
                    b'n' => '\n',
                    b'v' => '\x0b',
                    b'f' => '\x0c',
                    b'r' => '\r',
                    b'e' => '\x1b',
                    b' ' => ' ',
                    b'"' => '"',
                    b'/' => '/',
                    b'\\' => '\\',
                    b'N' => '\u{85}',
                    b'_' => '\u{a0}',
                    b'L' => '\u{2028}',
                    b'P' => '\u{2029}',
                    b'x' | b'u' | b'U' => {
                        let width = if escaped == b'x' {
                            2
                        } else if escaped == b'u' {
                            4
                        } else {
                            8
                        };
                        let digits = self
                            .source
                            .get(pos..pos + width)
                            .filter(|_| pos + width <= end)
                            .ok_or("short YAML Unicode escape")?;
                        let digits =
                            std::str::from_utf8(digits).map_err(|error| error.to_string())?;
                        let code =
                            u32::from_str_radix(digits, 16).map_err(|error| error.to_string())?;
                        pos += width;
                        char::from_u32(code).ok_or("invalid YAML Unicode scalar")?
                    }
                    b'\r' | b'\n' => {
                        if escaped == b'\r' && self.source.get(pos) == Some(&b'\n') {
                            pos += 1;
                        }
                        while pos < end && matches!(self.source[pos], b' ' | b'\t') {
                            pos += 1;
                        }
                        trim = out.bytes.len();
                        continue;
                    }
                    _ => return Err("unsupported YAML escape".into()),
                };
                out.character(character, escaped_at);
                trim = out.bytes.len();
                continue;
            }
            if matches!(byte, b'\r' | b'\n') {
                out.bytes.truncate(trim);
                out.offsets.truncate(trim);
                let newline = pos;
                let mut breaks = 0;
                while pos < end {
                    if self.source[pos] == b'\r' {
                        pos += 1;
                        if pos < end && self.source[pos] == b'\n' {
                            pos += 1;
                        }
                    } else if self.source[pos] == b'\n' {
                        pos += 1;
                    } else {
                        break;
                    }
                    breaks += 1;
                    while pos < end && matches!(self.source[pos], b' ' | b'\t') {
                        pos += 1;
                    }
                    if pos >= end || !matches!(self.source[pos], b'\r' | b'\n') {
                        break;
                    }
                }
                if breaks == 1 {
                    out.character(' ', newline);
                } else {
                    for _ in 1..breaks {
                        out.character('\n', newline);
                    }
                }
                trim = out.bytes.len();
                continue;
            }
            out.literal(self.source, pos, pos + 1);
            if !matches!(byte, b' ' | b'\t') {
                trim = out.bytes.len();
            }
            pos += 1;
        }
        Ok(())
    }

    fn block_scalar(&self, node: Node<'_>, out: &mut Decode) -> std::result::Result<(), String> {
        let start = node.start_byte();
        let end = node.end_byte();
        let Some(newline) = self.source[start..end]
            .iter()
            .position(|byte| *byte == b'\n')
            .map(|n| start + n)
        else {
            return Ok(());
        };
        let header = &self.source[start..newline];
        let folded = header.first() == Some(&b'>');
        let strip = header.contains(&b'-');
        let keep = header.contains(&b'+');
        let line_start = self
            .lines
            .get(node.start_position().row)
            .copied()
            .unwrap_or(start);
        let parent_indent = self.source[line_start..start]
            .iter()
            .take_while(|byte| **byte == b' ')
            .count();
        let explicit = header
            .iter()
            .find(|byte| matches!(**byte, b'1'..=b'9'))
            .map(|byte| parent_indent + usize::from(*byte - b'0'));
        let mut lines = Vec::new();
        let mut pos = newline + 1;
        while pos < end {
            let next = self.source[pos..end]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(end, |n| pos + n);
            let text_end = if next > pos && self.source[next - 1] == b'\r' {
                next - 1
            } else {
                next
            };
            let indent = self.source[pos..text_end]
                .iter()
                .take_while(|byte| **byte == b' ')
                .count();
            lines.push((pos, text_end, next, indent));
            pos = if next < end { next + 1 } else { end };
        }
        let indent = explicit
            .or_else(|| {
                lines
                    .iter()
                    .filter(|(start, end, _, indent)| end - start > *indent)
                    .map(|(_, _, _, indent)| *indent)
                    .min()
            })
            .unwrap_or(0);
        for (index, &(start, text_end, newline, spaces)) in lines.iter().enumerate() {
            let content = (start + spaces.min(indent)).min(text_end);
            out.literal(self.source, content, text_end);
            if newline >= end {
                continue;
            }
            let nonempty = text_end > content;
            let next = lines.get(index + 1);
            let separator = if !folded || next.is_none() {
                Some('\n')
            } else if let Some(&(next_start, next_end, _, next_spaces)) = next {
                let next_content = (next_start + next_spaces.min(indent)).min(next_end);
                let next_nonempty = next_end > next_content;
                if spaces > indent || next_spaces > indent {
                    Some('\n')
                } else if nonempty && next_nonempty {
                    Some(' ')
                } else if nonempty && !next_nonempty {
                    None
                } else {
                    Some('\n')
                }
            } else {
                Some('\n')
            };
            if let Some(separator) = separator {
                out.character(separator, newline);
            }
        }
        if !keep {
            let had_break = out.bytes.last() == Some(&b'\n');
            while out.bytes.last() == Some(&b'\n') {
                out.bytes.pop();
                out.offsets.pop();
            }
            if !strip && had_break {
                out.character('\n', end.saturating_sub(1));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn scalar_decoding_matches_yaml_for_supported_quote_and_block_styles() {
        for input in [
            "value: plain {{ name }}\n",
            "value: 'it''s {{ name }}'\n",
            "value: \"é {{ name }}\\n\\u0061\\x62\"\n",
            "value: \"first {{ name }}\n  second\"\n",
            "value: \"first\\\n  {{ name }}\"\n",
            "value: |\n  first {{ name }}\n  second\n",
            "value: >\n  first {{ name }}\n  second\n",
            "value: >-\n  first\n\n  {{ name }}\n",
            "value: |+\n  {{ name }}\n\n",
            "value: |2\n  {{ name }}\n",
            "value: >\n  paragraph\n    code {{ name }}\n  next\n",
        ] {
            let yaml = Yaml::parse(Path::new("task.yml"), input.as_bytes()).expect("YAML");
            assert!(!yaml.tree.root_node().has_error(), "{input}");
            let pairs = yaml.pairs(yaml.tree.root_node());
            let node = pairs.first().expect("value pair").1;
            let scalar = yaml.scalar(node).expect("scalar");
            let oracle: std::collections::BTreeMap<String, String> =
                serde_yaml::from_str(input).expect("oracle");
            assert_eq!(scalar.value, oracle["value"], "input: {input}");
            if let Some(name) = scalar.value.find("name") {
                let range = scalar.range(name..name + 4);
                assert_eq!(&input.as_bytes()[range], b"name", "source map: {input}");
            }
        }
    }
}
