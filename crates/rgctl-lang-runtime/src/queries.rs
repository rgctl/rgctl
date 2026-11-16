//! Declarative `tags.scm` query execution for Tier-2 (`handler = "queries"`) languages.
//!
//! Follows the [tree-sitter code-navigation](https://tree-sitter.github.io/tree-sitter/code-navigation)
//! capture vocabulary (`@definition.*`, `@reference.*`, `@name`, `@doc`, `@import.path`).

use rgctl_ast_coverage::{check_tags_scm, extract_tag_capture_names};
use rgctl_plugin_api::Result;
use rgctl_plugin_api::{
    Error, ExtractAllResult, Relation, RelationType, SourceLocation, Symbol, SymbolType,
};
use rgctl_plugin_helpers::{node_to_location, record_call_site_spans, record_symbol_spans};
use std::path::Path;
use tree_sitter::{Language, Node, Query, QueryCursor, StreamingIterator, Tree};

/// Role of a tags.scm capture after stripping the leading `@`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureRole {
    /// Identifier / name node paired with a definition or reference.
    Name,
    /// Optional documentation comment capture.
    Doc,
    /// Local-scope bookkeeping (`@local.*`) — ignored for graph emission.
    Local,
    /// Emits a [`Symbol`].
    Definition(SymbolType),
    /// Emits a `Calls` relation.
    ReferenceCall,
    /// Emits a `References` / type-use relation.
    ReferenceClass,
    /// Emits an `Implements` relation.
    ReferenceImplementation,
    /// Emits an `Import` symbol from a path capture.
    ImportPath,
}

/// Map a capture name (without `@`) to its graph role.
pub fn capture_role(name: &str) -> Option<CaptureRole> {
    if name == "name" {
        return Some(CaptureRole::Name);
    }
    if name == "doc" {
        return Some(CaptureRole::Doc);
    }
    if name.starts_with("local.") {
        return Some(CaptureRole::Local);
    }
    match name {
        "definition.function" => Some(CaptureRole::Definition(SymbolType::Function)),
        "definition.method" => Some(CaptureRole::Definition(SymbolType::Method)),
        "definition.class" => Some(CaptureRole::Definition(SymbolType::Class)),
        "definition.interface" => Some(CaptureRole::Definition(SymbolType::Interface)),
        "definition.module" | "definition.namespace" => {
            Some(CaptureRole::Definition(SymbolType::Module))
        }
        "definition.type" => Some(CaptureRole::Definition(SymbolType::TypeAlias)),
        "definition.macro" => Some(CaptureRole::Definition(SymbolType::Macro)),
        "definition.field" | "definition.enummember" | "definition.constant" => {
            Some(CaptureRole::Definition(SymbolType::Variable))
        }
        "definition.enum" => Some(CaptureRole::Definition(SymbolType::Enum)),
        "definition.import" => Some(CaptureRole::Definition(SymbolType::Import)),
        "reference.call" => Some(CaptureRole::ReferenceCall),
        "reference.class" => Some(CaptureRole::ReferenceClass),
        "reference.implementation" => Some(CaptureRole::ReferenceImplementation),
        "import.path" => Some(CaptureRole::ImportPath),
        _ => None,
    }
}

/// Collect unique `@capture` names from a `tags.scm` source string.
pub fn extract_capture_names(tags_scm: &str) -> Vec<String> {
    extract_tag_capture_names(tags_scm)
}

/// Validate that every `@capture` in `tags_scm` is in the supported vocabulary.
///
/// Returns human-readable issue strings (empty = ok). Delegates to
/// [`rgctl_ast_coverage::check_tags_scm`].
pub fn validate_tags_captures(language_id: &str, tags_scm: &str) -> Vec<String> {
    check_tags_scm(language_id, tags_scm)
        .into_iter()
        .map(|i| format!("{}: {}", i.language, i.message))
        .collect()
}

/// Compile `tags_scm` against `language`, returning capture-name validation + query errors.
pub fn compile_tags_query(language_id: &str, language: &Language, tags_scm: &str) -> Result<Query> {
    let capture_issues = validate_tags_captures(language_id, tags_scm);
    if !capture_issues.is_empty() {
        return Err(Error::PluginError(capture_issues.join("; ")));
    }
    // Ensure every capture has a role mapping (defense in depth vs allow-list drift).
    for name in extract_capture_names(tags_scm) {
        if name.starts_with('_') {
            continue;
        }
        if capture_role(&name).is_none() {
            return Err(Error::PluginError(format!(
                "{language_id}: capture '@{name}' is allow-listed but has no Symbol/Relation mapping"
            )));
        }
    }
    Query::new(language, tags_scm)
        .map_err(|e| Error::PluginError(format!("{language_id}: failed to compile tags.scm: {e}")))
}

/// Index of capture ids → roles for a compiled [`Query`].
#[derive(Debug, Clone)]
pub struct CaptureIndex {
    roles: Vec<Option<CaptureRole>>,
}

impl CaptureIndex {
    /// Build from a compiled query's capture names.
    pub fn from_query(query: &Query) -> Result<Self> {
        let mut roles = Vec::with_capacity(query.capture_names().len());
        for name in query.capture_names() {
            if name.starts_with('_') {
                roles.push(None);
                continue;
            }
            match capture_role(name) {
                Some(role) => roles.push(Some(role)),
                None => {
                    return Err(Error::PluginError(format!(
                        "unknown tags.scm capture '@{name}'"
                    )));
                }
            }
        }
        Ok(Self { roles })
    }

    fn role(&self, capture_index: u32) -> Option<CaptureRole> {
        self.roles.get(capture_index as usize).copied().flatten()
    }
}

#[derive(Debug)]
struct PendingDef<'a> {
    name: String,
    symbol_type: SymbolType,
    location: SourceLocation,
    documentation: Option<String>,
    span_node: Node<'a>,
    name_node: Option<Node<'a>>,
}

fn is_definition_span_kind(kind: &str) -> bool {
    !kind.ends_with("_declarator")
        && !kind.ends_with("_name")
        && (kind.ends_with("_definition")
            || kind.ends_with("_declaration")
            || kind.ends_with("_defn")
            || kind.ends_with("_decl")
            || kind.ends_with("_def")
            || matches!(
                kind,
                "method"
                    | "singleton_method"
                    | "binding"
                    | "message"
                    | "service"
                    | "rpc"
                    | "function"
                    | "subroutine"
                    | "list_lit"
                    | "list"
                    | "class"
                    | "data_type"
                    | "newtype"
                    | "type_alias"
                    | "module_attribute"
                    | "rule"
                    | "element"
                    | "script_element"
                    | "rule_set"
                    | "keyframes_statement"
                    | "function_statement"
                    | "class_statement"
                    | "table"
                    | "pair"
                    | "defProc"
                    | "declProc"
                    | "declType"
                    | "declClass"
                    | "declIntf"
                    | "unit"
                    | "library"
                    | "subprogram_body"
            ))
}

/// Prefer the immediate enclosing `*_definition` / `*_declaration` node when `@definition.*`
/// is captured on a header/declarator child (e.g. `function_declarator`, `rpc_name`,
/// Fortran `function_statement`/`module_statement`, or Pascal `declProc` inside `defProc`),
/// without walking past a `function_definition` up to its enclosing `contract_declaration`/`class_declaration`.
fn definition_span_node(node: Node<'_>) -> Node<'_> {
    // Fortran `function_statement` / `subroutine_statement` / `module_statement` are 1-line
    // headers inside `function` / `subroutine` / `module` (whereas PowerShell `function_statement`
    // is the full function node). Pascal `declProc` is a 1-line header inside `defProc`.
    if matches!(
        node.kind(),
        "function_statement" | "subroutine_statement" | "module_statement" | "declProc"
    ) && let Some(parent) = node.parent()
        && matches!(
            parent.kind(),
            "function" | "subroutine" | "module" | "defProc"
        )
    {
        return parent;
    }
    if is_definition_span_kind(node.kind()) {
        return node;
    }
    let mut cur = node;
    for _ in 0..8 {
        let Some(parent) = cur.parent() else {
            break;
        };
        let kind = parent.kind();
        if is_definition_span_kind(kind) {
            return parent;
        }
        // Stop at translation-unit / source_file / program / class-body boundaries.
        if matches!(
            kind,
            "translation_unit"
                | "source_file"
                | "program"
                | "module"
                | "block"
                | "contract_body"
                | "class_body"
                | "declaration_list"
        ) {
            break;
        }
        cur = parent;
    }
    node
}

/// True when a `@reference.call` node is actually the function/macro signature header
/// inside a definition (e.g. Julia `function add(a, b)` where `signature` wraps a `call_expression`,
/// or Scheme/Racket `(define (add a b) ...)` where the second child of `define` is `(add a b)`).
fn is_inside_declaration_header(node: Node<'_>, source: &[u8]) -> bool {
    if node.kind() == "list"
        && let Some(parent) = node.parent()
        && parent.kind() == "list"
        && parent.named_child(1).map(|c| c.id()) == Some(node.id())
        && let Some(head) = parent.named_child(0)
        && let Ok(kw) = head.utf8_text(source)
        && matches!(kw.trim(), "define" | "define-syntax" | "defmacro" | "defun")
    {
        return true;
    }
    let mut cur = node;
    for _ in 0..4 {
        let Some(parent) = cur.parent() else {
            break;
        };
        if matches!(
            parent.kind(),
            "signature" | "function_declarator" | "method_declarator" | "function_declaration_left"
        ) {
            return true;
        }
        cur = parent;
    }
    false
}

/// Expand `span` up to a tight assignment/binding parent when `@name` sits just outside
/// `@definition.*` (e.g. `helper <- function(x) { ... }` in R or `name = expr` bindings).
fn enclose_name_in_span<'a>(span: Node<'a>, name_node: Option<Node<'a>>) -> Node<'a> {
    let Some(nn) = name_node else {
        return span;
    };
    if span.start_byte() <= nn.start_byte() && nn.end_byte() <= span.end_byte() {
        return span;
    }
    let mut cur = span;
    for _ in 0..4 {
        let Some(parent) = cur.parent() else {
            break;
        };
        if matches!(
            parent.kind(),
            "translation_unit"
                | "source_file"
                | "program"
                | "module"
                | "block"
                | "contract_body"
                | "class_body"
                | "declaration_list"
        ) {
            break;
        }
        if parent.start_byte() <= nn.start_byte() && nn.end_byte() <= parent.end_byte() {
            return parent;
        }
        cur = parent;
    }
    span
}

#[derive(Debug)]
struct PendingRef<'a> {
    role: CaptureRole,
    name: String,
    location: SourceLocation,
    ref_node: Node<'a>,
    name_node: Option<Node<'a>>,
}

/// Execute a precompiled tags query and emit a unified [`ExtractAllResult`].
pub fn extract_with_query(
    language_id: &str,
    file_path: &Path,
    source: &[u8],
    tree: &Tree,
    query: &Query,
    capture_index: &CaptureIndex,
) -> Result<ExtractAllResult> {
    let file = file_path.to_string_lossy().to_string();
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), source);

    let mut defs = Vec::new();
    let mut refs = Vec::new();

    while let Some(m) = matches.next() {
        let mut name: Option<String> = None;
        let mut name_capture_node: Option<Node> = None;
        let mut doc: Option<String> = None;
        let mut def_role: Option<SymbolType> = None;
        let mut def_node: Option<Node> = None;
        let mut ref_role: Option<CaptureRole> = None;
        let mut ref_node: Option<Node> = None;
        let mut import_path: Option<(String, Node)> = None;

        for capture in m.captures {
            let Some(role) = capture_index.role(capture.index) else {
                continue;
            };
            let node = capture.node;
            match role {
                CaptureRole::Name => {
                    if let Ok(text) = node.utf8_text(source) {
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            name = Some(trimmed.to_string());
                            name_capture_node = Some(node);
                        }
                    }
                }
                CaptureRole::Doc => {
                    if let Ok(text) = node.utf8_text(source) {
                        doc = Some(text.trim().to_string());
                    }
                }
                CaptureRole::Local => {}
                CaptureRole::Definition(st) => {
                    def_role = Some(st);
                    def_node = Some(node);
                }
                CaptureRole::ImportPath => {
                    if let Ok(text) = node.utf8_text(source) {
                        let cleaned = text.trim().trim_matches('"').trim_matches('\'').to_string();
                        if !cleaned.is_empty() {
                            import_path = Some((cleaned, node));
                        }
                    }
                }
                CaptureRole::ReferenceCall
                | CaptureRole::ReferenceClass
                | CaptureRole::ReferenceImplementation => {
                    ref_role = Some(role);
                    ref_node = Some(node);
                }
            }
        }

        if let Some((path_name, node)) = import_path {
            defs.push(PendingDef {
                name: path_name,
                symbol_type: SymbolType::Import,
                location: node_to_location(node, &file),
                documentation: None,
                span_node: node,
                name_node: None,
            });
        }

        if let (Some(st), Some(node)) = (def_role, def_node) {
            let def_name = if ref_role.is_some() {
                name.clone()
            } else {
                name.take()
            };
            let resolved_name = def_name.or_else(|| {
                node.utf8_text(source)
                    .ok()
                    .map(|s| s.lines().next().unwrap_or("").trim().to_string())
                    .filter(|s| !s.is_empty())
            });
            if let Some(n) = resolved_name {
                let base_span = definition_span_node(node);
                let span = enclose_name_in_span(base_span, name_capture_node);
                let valid_name_node = name_capture_node.filter(|nn| {
                    span.start_byte() <= nn.start_byte()
                        && nn.end_byte() <= span.end_byte()
                        && nn.end_byte() > nn.start_byte()
                });
                defs.push(PendingDef {
                    name: n,
                    symbol_type: st,
                    location: node_to_location(span, &file),
                    documentation: doc.take(),
                    span_node: span,
                    name_node: valid_name_node,
                });
            }
        }

        if let (Some(role), Some(node)) = (ref_role, ref_node) {
            if role == CaptureRole::ReferenceCall && is_inside_declaration_header(node, source) {
                continue;
            }
            let resolved_name = name.or_else(|| {
                node.utf8_text(source)
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
            });
            if let Some(n) = resolved_name {
                refs.push(PendingRef {
                    role,
                    name: n,
                    location: node_to_location(node, &file),
                    ref_node: node,
                    name_node: name_capture_node,
                });
            }
        }
    }

    let default_meta = serde_json::json!({
        "extractor": "queries",
        "language": language_id,
    });

    let mut symbols: Vec<Symbol> = defs
        .into_iter()
        .map(|d| {
            let mut sym = Symbol {
                name: d.name,
                symbol_type: d.symbol_type,
                qualified_name: None,
                location: d.location,
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: d.documentation,
                metadata: default_meta.clone(),
                ..Default::default()
            };
            record_symbol_spans(&mut sym, d.span_node, d.name_node);
            sym
        })
        .collect();

    // Stable order: definitions by start line/byte then name (deterministic across workers).
    symbols.sort_by(|a, b| {
        a.location
            .start_line
            .cmp(&b.location.start_line)
            .then(a.location.start_column.cmp(&b.location.start_column))
            .then(a.start_byte.cmp(&b.start_byte))
            .then(b.end_byte.cmp(&a.end_byte))
            .then(a.name.cmp(&b.name))
    });
    symbols.dedup_by(|a, b| {
        a.name == b.name
            && a.symbol_type == b.symbol_type
            && a.location.start_line == b.location.start_line
            && a.location.start_column == b.location.start_column
    });

    // Populate `qualified_name` (`Container::member`) from the innermost enclosing
    // type/module span so methods/fields in multiple classes/contracts in one file do not collide.
    struct ContainerSpan {
        name: String,
        start_line: usize,
        end_line: usize,
        start_byte: Option<u32>,
        end_byte: Option<u32>,
    }
    let containers: Vec<ContainerSpan> = symbols
        .iter()
        .filter(|s| {
            matches!(
                s.symbol_type,
                SymbolType::Class
                    | SymbolType::Interface
                    | SymbolType::Struct
                    | SymbolType::Enum
                    | SymbolType::Module
            )
        })
        .map(|s| ContainerSpan {
            name: s.name.clone(),
            start_line: s.location.start_line,
            end_line: s.location.end_line,
            start_byte: s.start_byte,
            end_byte: s.end_byte,
        })
        .collect();
    if !containers.is_empty() {
        for sym in &mut symbols {
            if sym.symbol_type == SymbolType::Import {
                continue;
            }
            let s_start = sym.location.start_line;
            let s_end = sym.location.end_line;
            let end_idx = containers.partition_point(|c| c.start_line <= s_start);
            if let Some(container) = containers[..end_idx]
                .iter()
                .filter(|c| {
                    if c.name == sym.name {
                        return false;
                    }
                    if let (Some(cs), Some(ce), Some(ss), Some(se)) =
                        (c.start_byte, c.end_byte, sym.start_byte, sym.end_byte)
                    {
                        cs <= ss && se <= ce && (cs < ss || se < ce)
                    } else {
                        c.end_line >= s_end && (c.start_line < s_start || c.end_line > s_end)
                    }
                })
                .min_by_key(|c| {
                    c.end_byte
                        .zip(c.start_byte)
                        .map(|(e, s)| e.saturating_sub(s) as usize)
                        .unwrap_or_else(|| c.end_line.saturating_sub(c.start_line))
                })
            {
                sym.qualified_name = Some(format!("{}::{}", container.name, sym.name));
            }
        }
    }

    let mut fn_name_counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    let function_symbols: Vec<&Symbol> = symbols
        .iter()
        .filter(|s| matches!(s.symbol_type, SymbolType::Function | SymbolType::Method))
        .collect();
    for f in &function_symbols {
        *fn_name_counts.entry(f.name.as_str()).or_insert(0) += 1;
    }
    let file_fallback = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(language_id);

    let mut relations = Vec::with_capacity(refs.len());
    for r in refs {
        let relation_type = match r.role {
            CaptureRole::ReferenceCall => RelationType::Calls,
            CaptureRole::ReferenceClass => RelationType::References,
            CaptureRole::ReferenceImplementation => RelationType::Implements,
            _ => continue,
        };

        let line = r.location.start_line;
        let ref_sb = r.ref_node.start_byte() as u32;
        let ref_eb = r.ref_node.end_byte() as u32;
        let end_idx = function_symbols.partition_point(|s| s.location.start_line <= line);
        let caller_sym = function_symbols[..end_idx]
            .iter()
            .copied()
            .filter(|s| {
                if let (Some(sb), Some(eb)) = (s.start_byte, s.end_byte) {
                    sb <= ref_sb && ref_eb <= eb
                } else {
                    line <= s.location.end_line
                }
            })
            .min_by_key(|s| {
                s.end_byte
                    .zip(s.start_byte)
                    .map(|(eb, sb)| eb.saturating_sub(sb) as usize)
                    .unwrap_or_else(|| s.location.end_line.saturating_sub(s.location.start_line))
            });
        let from = caller_sym
            .map(|s| {
                if fn_name_counts.get(s.name.as_str()).copied().unwrap_or(0) > 1 {
                    s.qualified_name.clone().unwrap_or_else(|| s.name.clone())
                } else {
                    s.name.clone()
                }
            })
            .unwrap_or_else(|| file_fallback.to_string());

        // When calling an intra-file / intra-container function whose symbol has a
        // `Container::member` qualified_name, supply `to_qualified_hint` so multi-file
        // repositories with common helper names resolve to the lexical target.
        let to_qualified_hint = if relation_type == RelationType::Calls {
            let caller_prefix = caller_sym
                .and_then(|s| s.qualified_name.as_deref())
                .and_then(|qn| qn.rsplit_once("::").map(|(p, _)| p));
            let same_container_match = caller_prefix.and_then(|prefix| {
                let expected_qn = format!("{prefix}::{}", r.name);
                function_symbols
                    .iter()
                    .find(|s| s.qualified_name.as_deref() == Some(expected_qn.as_str()))
                    .and_then(|s| s.qualified_name.clone())
            });
            same_container_match.or_else(|| {
                let mut matches_iter = function_symbols
                    .iter()
                    .filter(|s| s.name == r.name && s.qualified_name.is_some());
                let first = matches_iter.next()?;
                if matches_iter.all(|other| other.qualified_name == first.qualified_name) {
                    first.qualified_name.clone()
                } else {
                    None
                }
            })
        } else {
            None
        };

        let mut rel = Relation {
            from,
            to: r.name,
            relation_type,
            location: r.location,
            metadata: default_meta.clone(),
            to_qualified_hint,
            to_type_hint: None,
            ..Default::default()
        };
        record_call_site_spans(&mut rel, r.name_node.unwrap_or(r.ref_node));
        relations.push(rel);
    }

    relations.sort_by(|a, b| {
        a.location
            .start_line
            .cmp(&b.location.start_line)
            .then(a.location.start_column.cmp(&b.location.start_column))
            .then(a.from.cmp(&b.from))
            .then(a.to.cmp(&b.to))
    });

    Ok(ExtractAllResult::from_parts(symbols, relations))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_capture_names_dedupes() {
        let scm = r#"
(function_definition name: (identifier) @name) @definition.function
(call function: (identifier) @name) @reference.call
(call function: (identifier) @name) @reference.call
"#;
        let names = extract_capture_names(scm);
        assert!(names.contains(&"name".to_string()));
        assert!(names.contains(&"definition.function".to_string()));
        assert!(names.contains(&"reference.call".to_string()));
        assert_eq!(names.len(), 3);
    }

    #[test]
    fn validate_rejects_unknown_capture() {
        let issues = validate_tags_captures("zig", "(x) @definition.widget");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].contains("definition.widget"));
    }

    #[test]
    fn capture_role_maps_standard_vocabulary() {
        assert_eq!(
            capture_role("definition.function"),
            Some(CaptureRole::Definition(SymbolType::Function))
        );
        assert_eq!(
            capture_role("reference.call"),
            Some(CaptureRole::ReferenceCall)
        );
        assert_eq!(capture_role("import.path"), Some(CaptureRole::ImportPath));
        assert_eq!(capture_role("local.scope"), Some(CaptureRole::Local));
        assert!(capture_role("definition.widget").is_none());
    }
}
