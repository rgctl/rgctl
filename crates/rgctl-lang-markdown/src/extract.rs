//! Symbol and relation extraction from a parsed markdown forest.

use crate::parse::ParsedMarkdown;
use crate::slug::{slugify, unique_slug};
use rgctl_plugin_api::{
    ExtractAllResult, Relation, RelationType, SourceLocation, Symbol, SymbolType,
};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use tree_sitter::Node;

/// Inline `body_text` on node properties; larger payloads use `code_hash` + code index.
pub const INLINE_BODY_MAX_BYTES: usize = 32_768;

#[derive(Clone)]
struct HeadingRecord {
    heading_start_byte: usize,
    heading_end_byte: usize,
    symbol_index: usize,
}

/// Extract symbols and relations from a parsed markdown file.
pub fn extract(
    parsed: &ParsedMarkdown,
    file_path: &Path,
    source: &[u8],
) -> rgctl_plugin_api::Result<ExtractAllResult> {
    let file = file_path.to_string_lossy().to_string();
    let mut ctx = ExtractCtx {
        file: file.clone(),
        file_path,
        source,
        parsed,
        symbols: Vec::new(),
        relations: Vec::new(),
        slug_counts: HashMap::new(),
        code_index: 0,
        link_index: 0,
        heading_stack: Vec::new(),
        link_defs: HashMap::new(),
        heading_records: Vec::new(),
        content_blobs: HashMap::new(),
    };

    collect_link_definitions(parsed.block.root_node(), source, &mut ctx.link_defs);
    walk_block(parsed.block.root_node(), &mut ctx)?;
    finalize_section_bodies(&mut ctx);
    Ok(ExtractAllResult {
        symbols: ctx.symbols,
        relations: ctx.relations,
        content_blobs: ctx.content_blobs,
        source_references: Vec::new(),
    })
}

struct ExtractCtx<'a> {
    file: String,
    file_path: &'a Path,
    source: &'a [u8],
    parsed: &'a ParsedMarkdown,
    symbols: Vec<Symbol>,
    relations: Vec<Relation>,
    slug_counts: HashMap<String, usize>,
    code_index: usize,
    link_index: usize,
    heading_stack: Vec<(u8, String)>,
    link_defs: HashMap<String, String>,
    heading_records: Vec<HeadingRecord>,
    content_blobs: HashMap<String, String>,
}

impl ExtractCtx<'_> {
    fn current_heading_qn(&self) -> Option<&str> {
        self.heading_stack.last().map(|(_, qn)| qn.as_str())
    }

    fn relation_from(&self) -> String {
        self.current_heading_qn()
            .map(str::to_string)
            .unwrap_or_else(|| self.file.clone())
    }
}

fn walk_block(node: Node<'_>, ctx: &mut ExtractCtx<'_>) -> rgctl_plugin_api::Result<()> {
    match node.kind() {
        "atx_heading" | "setext_heading" => extract_heading(node, ctx)?,
        "fenced_code_block" | "indented_code_block" => extract_code_block(node, ctx)?,
        "minus_metadata" => extract_yaml_frontmatter(node, ctx)?,
        "plus_metadata" => extract_toml_frontmatter(node, ctx)?,
        "inline" | "pipe_table_cell" => extract_inline_container(node, ctx)?,
        "link_reference_definition" => {}
        "image" | "wiki_link" => {}
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                walk_block(child, ctx)?;
            }
        }
    }
    Ok(())
}

fn extract_heading(node: Node<'_>, ctx: &mut ExtractCtx<'_>) -> rgctl_plugin_api::Result<()> {
    let level = heading_level(node);
    let text = heading_text(node, ctx.source);
    let base = slugify(&text);
    let slug = unique_slug(&base, &mut ctx.slug_counts);
    let qn = format!("{}#{slug}", ctx.file);

    while ctx
        .heading_stack
        .last()
        .is_some_and(|(stack_level, _)| *stack_level >= level)
    {
        ctx.heading_stack.pop();
    }

    if let Some((_, parent_qn)) = ctx.heading_stack.last() {
        push_defines(ctx, parent_qn.clone(), qn.clone(), node);
    }

    let symbol_index = ctx.symbols.len();
    ctx.symbols.push(symbol(
        text,
        SymbolType::Module,
        Some(qn.clone()),
        location(node, &ctx.file),
        json!({ "kind": "heading", "level": level.to_string(), "slug": slug }),
    ));
    ctx.heading_records.push(HeadingRecord {
        heading_start_byte: node.start_byte(),
        heading_end_byte: node.end_byte(),
        symbol_index,
    });
    ctx.heading_stack.push((level, qn.clone()));

    visit_inline_descendants(node, ctx)?;
    Ok(())
}

fn extract_code_block(node: Node<'_>, ctx: &mut ExtractCtx<'_>) -> rgctl_plugin_api::Result<()> {
    let lang = info_string(node, ctx.source);
    let name = format!("code_block_{}", ctx.code_index);
    let qn = format!("{}#{name}", ctx.file);
    ctx.code_index += 1;

    let raw_body = code_block_body(node, ctx.source);
    let body = raw_body.trim();
    let mut meta = serde_json::Map::from_iter([
        ("kind".to_string(), json!("code_block")),
        ("language".to_string(), json!(lang)),
    ]);
    attach_body_metadata(&mut meta, body, &mut ctx.content_blobs);

    ctx.symbols.push(symbol(
        name,
        SymbolType::Module,
        Some(qn.clone()),
        location(node, &ctx.file),
        serde_json::Value::Object(meta),
    ));

    if let Some(heading_qn) = ctx.current_heading_qn() {
        push_defines(ctx, heading_qn.to_string(), qn, node);
    }
    Ok(())
}

fn extract_yaml_frontmatter(
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
) -> rgctl_plugin_api::Result<()> {
    let text = node.utf8_text(ctx.source).unwrap_or("").trim();
    let stripped = text
        .trim_start_matches("---")
        .trim_end_matches("---")
        .trim();
    if stripped.is_empty() {
        return Ok(());
    }
    match serde_yaml::from_str::<serde_yaml::Value>(stripped) {
        Ok(value) => flatten_yaml("", &value, node, ctx),
        Err(_) => Ok(()),
    }
}

fn extract_toml_frontmatter(
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
) -> rgctl_plugin_api::Result<()> {
    let text = node.utf8_text(ctx.source).unwrap_or("").trim();
    let stripped = text
        .trim_start_matches("+++")
        .trim_end_matches("+++")
        .trim();
    if stripped.is_empty() {
        return Ok(());
    }
    match stripped.parse::<toml::Value>() {
        Ok(value) => flatten_toml("", &value, node, ctx),
        Err(_) => Ok(()),
    }
}

fn flatten_yaml(
    prefix: &str,
    value: &serde_yaml::Value,
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
) -> rgctl_plugin_api::Result<()> {
    match value {
        serde_yaml::Value::Mapping(map) => {
            for (k, v) in map {
                let Some(key) = k.as_str() else { continue };
                let path = if prefix.is_empty() {
                    key.to_string()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_yaml(&path, v, node, ctx)?;
            }
            Ok(())
        }
        _ if !prefix.is_empty() => {
            push_frontmatter_key(prefix, value, node, ctx);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn flatten_toml(
    prefix: &str,
    value: &toml::Value,
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
) -> rgctl_plugin_api::Result<()> {
    match value {
        toml::Value::Table(map) => {
            for (key, v) in map {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_toml(&path, v, node, ctx)?;
            }
            Ok(())
        }
        _ if !prefix.is_empty() => {
            push_frontmatter_key_toml(prefix, value, node, ctx);
            Ok(())
        }
        _ => Ok(()),
    }
}

fn push_frontmatter_key(
    key: &str,
    value: &serde_yaml::Value,
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
) {
    let qn = format!("{}#fm.{key}", ctx.file);
    let value_str = scalar_to_string_yaml(value);
    let mut meta = serde_json::Map::from_iter([
        ("kind".to_string(), json!("frontmatter")),
        ("value".to_string(), json!(value_str)),
    ]);
    if !value_str.is_empty() {
        attach_body_metadata(&mut meta, &value_str, &mut ctx.content_blobs);
    }
    ctx.symbols.push(symbol(
        key.to_string(),
        SymbolType::Variable,
        Some(qn),
        location(node, &ctx.file),
        serde_json::Value::Object(meta),
    ));
}

fn push_frontmatter_key_toml(
    key: &str,
    value: &toml::Value,
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
) {
    let qn = format!("{}#fm.{key}", ctx.file);
    let value_str = scalar_to_string_toml(value);
    let mut meta = serde_json::Map::from_iter([
        ("kind".to_string(), json!("frontmatter")),
        ("value".to_string(), json!(value_str)),
    ]);
    if !value_str.is_empty() {
        attach_body_metadata(&mut meta, &value_str, &mut ctx.content_blobs);
    }
    ctx.symbols.push(symbol(
        key.to_string(),
        SymbolType::Variable,
        Some(qn),
        location(node, &ctx.file),
        serde_json::Value::Object(meta),
    ));
}

fn extract_inline_container(
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
) -> rgctl_plugin_api::Result<()> {
    let Some(inline_tree) = ctx.parsed.inline_tree(node) else {
        return Ok(());
    };
    walk_inline(inline_tree.root_node(), ctx, node)
}

fn visit_inline_descendants(
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
) -> rgctl_plugin_api::Result<()> {
    if node.kind() == "inline" || node.kind() == "pipe_table_cell" {
        return extract_inline_container(node, ctx);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_inline_descendants(child, ctx)?;
    }
    Ok(())
}

fn walk_inline(
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
    block_node: Node<'_>,
) -> rgctl_plugin_api::Result<()> {
    match node.kind() {
        "inline_link" => extract_inline_link(node, ctx, block_node, None)?,
        "full_reference_link" | "collapsed_reference_link" | "shortcut_link" => {
            if let Some(dest) = resolve_reference(node, ctx) {
                extract_inline_link(node, ctx, block_node, Some(dest))?;
            }
        }
        "uri_autolink" => {
            let url = node.utf8_text(ctx.source).unwrap_or("").trim().to_string();
            if !url.is_empty() && !is_external_url(&url) {
                emit_link(node, ctx, block_node, url.clone(), url)?;
            }
        }
        "image" | "wiki_link" | "email_autolink" => {}
        _ => {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                walk_inline(child, ctx, block_node)?;
            }
        }
    }
    Ok(())
}

fn extract_inline_link(
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
    block_node: Node<'_>,
    dest_override: Option<String>,
) -> rgctl_plugin_api::Result<()> {
    let text = link_text(node, ctx.source);
    let dest = dest_override.unwrap_or_else(|| link_destination(node, ctx.source));
    emit_link(node, ctx, block_node, text, dest)
}

fn emit_link(
    node: Node<'_>,
    ctx: &mut ExtractCtx<'_>,
    _block_node: Node<'_>,
    text: String,
    dest: String,
) -> rgctl_plugin_api::Result<()> {
    if dest.is_empty() {
        return Ok(());
    }
    let name = if text.is_empty() { dest.clone() } else { text };
    let qn = format!("{}#link_{}", ctx.file, ctx.link_index);
    ctx.link_index += 1;
    ctx.symbols.push(symbol(
        name,
        SymbolType::Import,
        Some(qn),
        location(node, &ctx.file),
        json!({ "kind": "markdown_link", "url": dest }),
    ));

    if is_external_url(&dest) {
        return Ok(());
    }

    let from = ctx.relation_from();
    if let Some(to) = resolve_internal_target(ctx.file_path, &ctx.file, &dest) {
        let has_fragment = dest.contains('#');
        let to_type = if has_fragment { "module" } else { "file" };
        ctx.relations.push(Relation {
            from,
            to: to.clone(),
            relation_type: RelationType::References,
            location: location(node, &ctx.file),
            metadata: json!({ "kind": "markdown_link" }),
            to_qualified_hint: Some(to),
            to_type_hint: Some(to_type.to_string()),
        });
    }
    Ok(())
}

fn resolve_internal_target(file_path: &Path, file: &str, dest: &str) -> Option<String> {
    let dest = dest.trim();
    if dest.is_empty() {
        return None;
    }
    let (path_part, frag) = match dest.split_once('#') {
        Some(("", frag)) => (None, Some(frag)),
        Some((path, frag)) => (Some(path), Some(frag)),
        None => (Some(dest), None),
    };

    let resolved_file = if let Some(rel) = path_part {
        join_href(file_path, rel)
    } else {
        file.to_string()
    };

    match frag {
        Some(f) if !f.is_empty() => Some(format!("{resolved_file}#{f}")),
        _ => Some(resolved_file),
    }
}

fn join_href(file_path: &Path, rel: &str) -> String {
    let rel = rel.split('?').next().unwrap_or(rel);
    let base = file_path.parent().unwrap_or(Path::new("."));
    let mut out = if rel.starts_with('/') {
        PathBuf::from(rel)
    } else {
        base.to_path_buf()
    };
    if !rel.starts_with('/') {
        for c in Path::new(rel).components() {
            match c {
                Component::Prefix(p) => out = PathBuf::from(p.as_os_str()),
                Component::RootDir => out = PathBuf::from(std::path::MAIN_SEPARATOR_STR),
                Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                Component::Normal(s) => out.push(s),
            }
        }
    }
    out.to_string_lossy().to_string()
}

fn is_external_url(dest: &str) -> bool {
    let lower = dest.to_ascii_lowercase();
    lower.contains("://") || lower.starts_with("mailto:")
}

fn collect_link_definitions(node: Node<'_>, source: &[u8], defs: &mut HashMap<String, String>) {
    if node.kind() == "link_reference_definition" {
        let label = child_text_by_kind(node, source, "link_label");
        let dest = child_text_by_kind(node, source, "link_destination");
        if !label.is_empty() && !dest.is_empty() {
            defs.insert(normalize_label(&label), strip_destination(&dest));
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_link_definitions(child, source, defs);
    }
}

fn resolve_reference(node: Node<'_>, ctx: &ExtractCtx<'_>) -> Option<String> {
    let label = child_text_by_kind(node, ctx.source, "link_label");
    let key = if label.is_empty() {
        normalize_label(&link_text(node, ctx.source))
    } else {
        normalize_label(&label)
    };
    ctx.link_defs.get(&key).cloned()
}

fn normalize_label(label: &str) -> String {
    label
        .trim()
        .trim_matches('[')
        .trim_matches(']')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn heading_level(node: Node<'_>) -> u8 {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "atx_h1_marker" | "setext_h1_underline" => return 1,
            "atx_h2_marker" | "setext_h2_underline" => return 2,
            "atx_h3_marker" => return 3,
            "atx_h4_marker" => return 4,
            "atx_h5_marker" => return 5,
            "atx_h6_marker" => return 6,
            _ => {}
        }
    }
    1
}

fn heading_text(node: Node<'_>, source: &[u8]) -> String {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        if matches!(child.kind(), "inline" | "heading_content" | "paragraph") {
            return collapse_ws(child.utf8_text(source).unwrap_or(""));
        }
    }
    collapse_ws(node.utf8_text(source).unwrap_or(""))
}

fn info_string(node: Node<'_>, source: &[u8]) -> String {
    child_text_by_kind(node, source, "info_string")
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

fn code_block_body(node: Node<'_>, source: &[u8]) -> String {
    if let Some(content) = find_child_kind(node, "code_fence_content") {
        return content
            .utf8_text(source)
            .unwrap_or("")
            .trim_end_matches('\n')
            .to_string();
    }
    let raw = node.utf8_text(source).unwrap_or("");
    raw.lines()
        .map(|line| line.strip_prefix("    ").unwrap_or(line).trim_end())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

fn find_child_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
        let mut child_cursor = child.walk();
        for grandchild in child.children(&mut child_cursor) {
            if grandchild.kind() == kind {
                return Some(grandchild);
            }
        }
    }
    None
}

fn link_text(node: Node<'_>, source: &[u8]) -> String {
    collapse_ws(&child_text_by_kind(node, source, "link_text"))
}

fn link_destination(node: Node<'_>, source: &[u8]) -> String {
    strip_destination(&child_text_by_kind(node, source, "link_destination"))
}

fn strip_destination(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim()
        .to_string()
}

fn child_text_by_kind(node: Node<'_>, source: &[u8], kind: &str) -> String {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return child.utf8_text(source).unwrap_or("").to_string();
        }
    }
    String::new()
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn location(node: Node<'_>, file: &str) -> SourceLocation {
    SourceLocation {
        file: file.to_string(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_column: node.start_position().column,
        end_column: node.end_position().column,
    }
}

fn symbol(
    name: String,
    symbol_type: SymbolType,
    qualified_name: Option<String>,
    location: SourceLocation,
    metadata: serde_json::Value,
) -> Symbol {
    Symbol {
        name,
        symbol_type,
        qualified_name,
        location,
        signature: None,
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata,
    }
}

fn push_defines(ctx: &mut ExtractCtx<'_>, from: String, to: String, node: Node<'_>) {
    ctx.relations.push(Relation {
        from,
        to: to.clone(),
        relation_type: RelationType::Defines,
        location: location(node, &ctx.file),
        metadata: json!({ "kind": "heading_nesting" }),
        to_qualified_hint: Some(to),
        to_type_hint: Some("module".to_string()),
    });
}

fn hash_body(body: &str) -> String {
    blake3::hash(body.as_bytes()).to_hex().to_string()
}

fn attach_body_metadata(
    meta: &mut serde_json::Map<String, serde_json::Value>,
    body: &str,
    content_blobs: &mut HashMap<String, String>,
) {
    if body.is_empty() {
        return;
    }
    let hash = hash_body(body);
    meta.insert("body_hash".to_string(), json!(hash));
    if body.len() <= INLINE_BODY_MAX_BYTES {
        meta.insert("body_text".to_string(), json!(body));
    } else {
        meta.insert("body_truncated".to_string(), json!("true"));
        meta.insert("body_ref".to_string(), json!(hash));
        content_blobs.insert(hash, body.to_string());
    }
}

fn byte_offset_to_line(source: &[u8], byte_offset: usize) -> usize {
    let clamped = byte_offset.min(source.len());
    1 + source[..clamped].iter().filter(|&&b| b == b'\n').count()
}

fn finalize_section_bodies(ctx: &mut ExtractCtx<'_>) {
    let records = ctx.heading_records.clone();
    for (i, rec) in records.iter().enumerate() {
        let body_start = rec.heading_end_byte;
        let body_end = records
            .get(i + 1)
            .map(|r| r.heading_start_byte)
            .unwrap_or(ctx.source.len());
        if body_start >= body_end {
            continue;
        }
        let body = String::from_utf8_lossy(&ctx.source[body_start..body_end])
            .trim()
            .to_string();
        if body.is_empty() {
            continue;
        }
        let symbol = &mut ctx.symbols[rec.symbol_index];
        if let Some(obj) = symbol.metadata.as_object_mut() {
            attach_body_metadata(obj, &body, &mut ctx.content_blobs);
        }
        symbol.location.end_line = byte_offset_to_line(ctx.source, body_end);
    }
}

fn scalar_to_string_yaml(value: &serde_yaml::Value) -> String {
    match value {
        serde_yaml::Value::String(s) => s.clone(),
        serde_yaml::Value::Number(n) => n.to_string(),
        serde_yaml::Value::Bool(b) => b.to_string(),
        serde_yaml::Value::Null => String::new(),
        other => serde_yaml::to_string(other)
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
    }
}

fn scalar_to_string_toml(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) => f.to_string(),
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(dt) => dt.to_string(),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn extract_guide(source: &str) -> ExtractAllResult {
        let path = Path::new("docs/guide.md");
        let parsed = crate::parse::parse_markdown(source.as_bytes(), path).expect("parse");
        extract(&parsed, path, source.as_bytes()).expect("extract")
    }

    #[test]
    fn nested_headings_define_contains() {
        let source = "# Parent\n\n## Child\n";
        let relations = extract_guide(source).relations;
        assert!(relations.iter().any(|r| {
            r.relation_type == RelationType::Defines
                && r.from == "docs/guide.md#parent"
                && r.to == "docs/guide.md#child"
        }));
    }

    #[test]
    fn heading_section_body_text_and_span() {
        let source = "# Parent\n\nSection prose here.\n\n## Child\n\nChild body.\n";
        let symbols = extract_guide(source).symbols;
        let parent = symbols
            .iter()
            .find(|s| s.qualified_name.as_deref() == Some("docs/guide.md#parent"))
            .expect("parent heading");
        assert_eq!(
            parent.metadata.get("body_text").and_then(|v| v.as_str()),
            Some("Section prose here.")
        );
        assert!(parent.metadata.get("body_hash").is_some());
        assert!(parent.location.end_line > parent.location.start_line);

        let child = symbols
            .iter()
            .find(|s| s.qualified_name.as_deref() == Some("docs/guide.md#child"))
            .expect("child heading");
        assert_eq!(
            child.metadata.get("body_text").and_then(|v| v.as_str()),
            Some("Child body.")
        );
    }

    #[test]
    fn fenced_code_block_stores_body_text() {
        let source = "```java\nclass X {}\n```\n";
        let symbols = extract_guide(source).symbols;
        let block = symbols
            .iter()
            .find(|s| s.metadata.get("kind") == Some(&serde_json::json!("code_block")))
            .expect("code block");
        assert_eq!(
            block.metadata.get("body_text").and_then(|v| v.as_str()),
            Some("class X {}")
        );
        assert!(block.metadata.get("body_hash").is_some());
    }

    #[test]
    fn frontmatter_scalar_has_value_and_hash() {
        let path = Path::new("AGENTS.md");
        let source = "---\ntitle: Hello World\n---\n# Guide\n";
        let parsed = crate::parse::parse_markdown(source.as_bytes(), path).expect("parse");
        let symbols = extract(&parsed, path, source.as_bytes())
            .expect("extract")
            .symbols;
        let title = symbols
            .iter()
            .find(|s| s.name == "title")
            .expect("title key");
        assert_eq!(
            title.metadata.get("value").and_then(|v| v.as_str()),
            Some("Hello World")
        );
        assert_eq!(
            title.metadata.get("body_text").and_then(|v| v.as_str()),
            Some("Hello World")
        );
        assert!(title.metadata.get("body_hash").is_some());
    }

    #[test]
    fn duplicate_slugs_get_suffixes() {
        let source = "# Overview\n\n# Overview\n";
        let symbols = extract_guide(source).symbols;
        let slugs: Vec<_> = symbols
            .iter()
            .filter(|s| s.metadata.get("kind") == Some(&serde_json::json!("heading")))
            .map(|s| s.qualified_name.as_deref().unwrap_or(""))
            .collect();
        assert!(slugs.contains(&"docs/guide.md#overview"));
        assert!(slugs.contains(&"docs/guide.md#overview-2"));
    }

    #[test]
    fn file_link_targets_file_with_hint() {
        let source = "[ADR](./adr.md)\n";
        let relations = extract_guide(source).relations;
        let rel = relations
            .iter()
            .find(|r| r.relation_type == RelationType::References)
            .expect("references");
        assert_eq!(rel.to, "docs/adr.md");
        assert_eq!(rel.to_type_hint.as_deref(), Some("file"));
        assert_eq!(rel.from, "docs/guide.md");
    }

    #[test]
    fn heading_fragment_link_uses_module_hint() {
        let source = "# Checkout Flow\n\n[Payments](./adr.md#payments)\n";
        let relations = extract_guide(source).relations;
        let rel = relations
            .iter()
            .find(|r| r.relation_type == RelationType::References)
            .expect("references");
        assert_eq!(rel.to, "docs/adr.md#payments");
        assert_eq!(rel.to_type_hint.as_deref(), Some("module"));
        assert_eq!(rel.from, "docs/guide.md#checkout-flow");
    }

    #[test]
    fn literal_fragment_not_slugified() {
        let source = "# Checkout Flow\n\n[Flow](./adr.md#checkout-flow)\n";
        let relations = extract_guide(source).relations;
        let rel = relations
            .iter()
            .find(|r| r.relation_type == RelationType::References)
            .expect("references");
        assert_eq!(rel.to, "docs/adr.md#checkout-flow");
    }

    #[test]
    fn java_file_link_resolves_relative_path() {
        let source = "[API](../src/CheckoutService.java)\n";
        let relations = extract_guide(source).relations;
        let rel = relations
            .iter()
            .find(|r| r.relation_type == RelationType::References)
            .expect("references");
        assert!(rel.to.ends_with("CheckoutService.java"));
        assert_eq!(rel.to_type_hint.as_deref(), Some("file"));
    }

    #[test]
    fn external_url_has_no_reference_edge() {
        let source = "[Site](https://example.com)\n";
        let relations = extract_guide(source).relations;
        assert!(relations.is_empty());
    }

    #[test]
    fn fenced_code_block_with_info_string() {
        let source = "```java\nclass X {}\n```\n";
        let symbols = extract_guide(source).symbols;
        let block = symbols
            .iter()
            .find(|s| s.metadata.get("kind") == Some(&serde_json::json!("code_block")))
            .expect("code block");
        assert_eq!(
            block.metadata.get("language").and_then(|v| v.as_str()),
            Some("java")
        );
        assert_eq!(
            block.metadata.get("body_text").and_then(|v| v.as_str()),
            Some("class X {}")
        );
    }

    #[test]
    fn yaml_frontmatter_flattens_nested_keys() {
        let path = Path::new("AGENTS.md");
        let source = "---\nmetadata:\n  author: bot\n---\n# Guide\n";
        let parsed = crate::parse::parse_markdown(source.as_bytes(), path).expect("parse");
        let symbols = extract(&parsed, path, source.as_bytes())
            .expect("extract")
            .symbols;
        assert!(symbols.iter().any(|s| {
            s.name == "metadata.author"
                && s.symbol_type == SymbolType::Variable
                && s.qualified_name.as_deref() == Some("AGENTS.md#fm.metadata.author")
        }));
    }

    #[test]
    fn images_and_wiki_links_emit_no_symbols() {
        let source = "![alt](img.png)\n[[Wiki]]\n";
        let symbols = extract_guide(source).symbols;
        assert!(symbols.is_empty());
    }

    #[test]
    fn angle_bracket_link_preserves_spaced_fragment() {
        let source = "# Checkout Flow\n\n[Wrong](<./adr.md#Checkout Flow>)\n";
        let relations = extract_guide(source).relations;
        let rel = relations
            .iter()
            .find(|r| r.relation_type == RelationType::References)
            .expect("references");
        assert_eq!(rel.to, "docs/adr.md#Checkout Flow");
        assert_eq!(rel.to_type_hint.as_deref(), Some("module"));
    }

    #[test]
    fn external_url_creates_link_symbol_without_reference_edge() {
        let source = "[Stripe](https://stripe.com/docs)\n";
        let out = extract_guide(source);
        let symbols = out.symbols;
        let relations = out.relations;
        assert_eq!(relations.len(), 0);
        let link = symbols
            .iter()
            .find(|s| s.metadata.get("kind") == Some(&serde_json::json!("markdown_link")))
            .expect("link symbol");
        assert_eq!(
            link.metadata.get("url").and_then(|v| v.as_str()),
            Some("https://stripe.com/docs")
        );
    }

    #[test]
    fn indented_code_block_extracted() {
        let source = "    fn main() {}\n";
        let symbols = extract_guide(source).symbols;
        let block = symbols
            .iter()
            .find(|s| s.metadata.get("kind") == Some(&serde_json::json!("code_block")))
            .expect("indented code block");
        assert_eq!(block.name, "code_block_0");
    }

    #[test]
    fn shortcut_reference_link_resolves() {
        let source = r#"
[Pay][pay]

[pay]: ./adr.md#payments
"#;
        let relations = extract_guide(source).relations;
        let rel = relations
            .iter()
            .find(|r| r.relation_type == RelationType::References)
            .expect("references");
        assert_eq!(rel.to, "docs/adr.md#payments");
    }

    #[test]
    fn link_under_child_heading_uses_child_qualified_name() {
        let source = "# Checkout Flow\n\n## Cart\n\n[Overview](./adr.md#overview)\n";
        let relations = extract_guide(source).relations;
        let rel = relations
            .iter()
            .find(|r| {
                r.relation_type == RelationType::References && r.to.ends_with("adr.md#overview")
            })
            .expect("overview link");
        assert_eq!(rel.from, "docs/guide.md#cart");
    }

    #[test]
    fn unquoted_spaced_fragment_does_not_resolve_full_title() {
        let source = "# Checkout Flow\n\n[Wrong](./adr.md#Checkout Flow)\n";
        let relations = extract_guide(source).relations;
        assert!(
            !relations
                .iter()
                .any(|r| r.to == "docs/adr.md#Checkout Flow"),
            "unquoted spaced fragment must not resolve to full visible title"
        );
    }

    #[test]
    fn same_file_fragment_link() {
        let source = "# Overview\n\n[Jump](#overview)\n";
        let relations = extract_guide(source).relations;
        let rel = relations
            .iter()
            .find(|r| r.relation_type == RelationType::References)
            .expect("references");
        assert_eq!(rel.to, "docs/guide.md#overview");
        assert_eq!(rel.from, "docs/guide.md#overview");
    }

    #[test]
    fn toml_frontmatter_flattens_keys() {
        let path = Path::new("config.md");
        let source = "+++\ntitle = \"Demo\"\n+++\n# Title\n";
        let parsed = crate::parse::parse_markdown(source.as_bytes(), path).expect("parse");
        let symbols = extract(&parsed, path, source.as_bytes())
            .expect("extract")
            .symbols;
        assert!(symbols.iter().any(|s| {
            s.name == "title"
                && s.symbol_type == SymbolType::Variable
                && s.qualified_name.as_deref() == Some("config.md#fm.title")
        }));
    }

    #[test]
    fn reference_style_links_resolve() {
        let source = r#"
[Payments][pay]

[pay]: ./adr.md#payments
"#;
        let relations = extract_guide(source).relations;
        let rel = relations
            .iter()
            .find(|r| r.relation_type == RelationType::References)
            .expect("references");
        assert_eq!(rel.to, "docs/adr.md#payments");
        assert_eq!(rel.to_type_hint.as_deref(), Some("module"));
    }

    #[test]
    fn collapsed_reference_link_resolves() {
        let source = r#"
[ADR][]

[ADR]: ./adr.md
"#;
        let relations = extract_guide(source).relations;
        let rel = relations
            .iter()
            .find(|r| r.relation_type == RelationType::References)
            .expect("references");
        assert_eq!(rel.to, "docs/adr.md");
        assert_eq!(rel.to_type_hint.as_deref(), Some("file"));
    }
}
