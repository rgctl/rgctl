//! Single-pass AST extraction for Elixir (`ScopeStack` + `KindDispatchTable`).
//!
//! tree-sitter-elixir represents almost everything as `call` nodes; runtime
//! dispatch inspects the leading identifier (`defmodule`, `def`, `if`, …).

use crate::plugin::Plugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{
    declaration_header_signature, node_to_location, record_call_site_spans,
    record_symbol_spans_from_source,
};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../elixir-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("access_call", HandlerMask::RELATION),
    ("after_block", HandlerMask::CFG_STMT),
    ("alias", HandlerMask::RELATION),
    ("anonymous_function", HandlerMask::SYMBOL),
    ("binary_operator", HandlerMask::CFG_STMT),
    (
        "call",
        HandlerMask::SYMBOL
            .union(HandlerMask::RELATION)
            .union(HandlerMask::CFG_STMT),
    ),
    ("catch_block", HandlerMask::CFG_STMT),
    ("else_block", HandlerMask::CFG_STMT),
    ("rescue_block", HandlerMask::CFG_STMT),
    ("stab_clause", HandlerMask::CFG_STMT),
];

const AST_MAX_DEPTH: usize = 2048;

const DEF_FORMS: &[&str] = &["def", "defp", "defmacro", "defmacrop"];
const IMPORT_FORMS: &[&str] = &["import", "alias", "require", "use"];
const CFG_FORMS: &[&str] = &[
    "if", "unless", "case", "cond", "for", "with", "receive", "try",
];

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn grammar() -> tree_sitter::Language {
    tree_sitter_elixir::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&grammar(), HANDLED_KINDS))
}

fn dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&grammar(), MANIFEST_JSON))
}

struct PendingCall<'a> {
    node: Node<'a>,
    from_symbol_idx: usize,
}

enum Visit<'a> {
    Pre(Node<'a>, usize),
    LeaveScope,
    LeaveGuard,
}

fn call_head<'a>(node: Node<'a>, source: &'a [u8]) -> Option<&'a str> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            return child.utf8_text(source).ok();
        }
        // Skip leading dots / unary for now.
        if child.is_named() {
            break;
        }
    }
    None
}

fn def_function_name(node: Node, source: &[u8]) -> Option<String> {
    let args = node.child_by_field_name("arguments").or_else(|| {
        let mut c = node.walk();
        node.children(&mut c).find(|ch| ch.kind() == "arguments")
    })?;
    let mut c = args.walk();
    for ch in args.children(&mut c) {
        match ch.kind() {
            "call" => {
                if let Some(head) = call_head(ch, source) {
                    return Some(head.to_string());
                }
            }
            "identifier" => {
                return ch.utf8_text(source).ok().map(str::to_string);
            }
            "alias" => {
                return ch.utf8_text(source).ok().map(str::to_string);
            }
            _ => {}
        }
    }
    None
}

fn module_name(node: Node, source: &[u8]) -> Option<String> {
    let args = {
        let mut c = node.walk();
        node.children(&mut c).find(|ch| ch.kind() == "arguments")
    }?;
    let mut c = args.walk();
    for ch in args.children(&mut c) {
        if matches!(ch.kind(), "alias" | "identifier" | "dot") {
            return ch.utf8_text(source).ok().map(|s| s.trim().to_string());
        }
    }
    None
}

fn import_target(node: Node, source: &[u8]) -> Option<String> {
    module_name(node, source).or_else(|| {
        node.utf8_text(source)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    })
}

fn elixir_call_target(node: Node, source: &[u8]) -> Option<(Option<String>, String)> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "identifier" {
            let h = child.utf8_text(source).ok()?.trim();
            if !h.is_empty() {
                return Some((None, h.to_string()));
            }
            return None;
        }
        if child.kind() == "dot" {
            let left = child
                .child_by_field_name("left")
                .or_else(|| child.named_child(0))
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let right = child
                .child_by_field_name("right")
                .or_else(|| child.named_child(1))
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::trim)
                .filter(|s| !s.is_empty())?;
            return Some((left, right.to_string()));
        }
        if child.is_named() {
            break;
        }
    }
    None
}

fn callback_function_name(node: Node, source: &[u8]) -> Option<String> {
    let args = node.child_by_field_name("arguments").or_else(|| {
        let mut c = node.walk();
        node.children(&mut c).find(|ch| ch.kind() == "arguments")
    })?;
    let mut stack = vec![args];
    while let Some(cur) = stack.pop() {
        if cur.kind() == "binary_operator"
            && let Some(left) = cur
                .child_by_field_name("left")
                .or_else(|| cur.named_child(0))
        {
            stack.push(left);
            continue;
        }
        if cur.kind() == "call"
            && let Some(h) = call_head(cur, source)
        {
            return Some(h.to_string());
        }
        if cur.kind() == "identifier" {
            return cur.utf8_text(source).ok().map(str::to_string);
        }
        let mut c = cur.walk();
        let children: Vec<_> = cur.children(&mut c).collect();
        for ch in children.into_iter().rev() {
            stack.push(ch);
        }
    }
    None
}

fn is_module_attribute_call(node: Node) -> bool {
    node.parent().is_some_and(|p| p.kind() == "unary_operator")
}

fn is_def_name_call(node: Node, source: &[u8]) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() != "arguments" {
        return false;
    }
    let Some(gp) = parent.parent() else {
        return false;
    };
    if gp.kind() != "call" {
        return false;
    }
    call_head(gp, source).is_some_and(|h| DEF_FORMS.contains(&h))
}

/// Single iterative DFS: symbols + deferred calls + complexity.
pub fn extract_all_single_pass(
    _plugin: &Plugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("elixir", grammar(), source).map_err(|e| Error::ParseError {
        file: file_path.to_path_buf(),
        line: 0,
        message: e,
    })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = dispatch();

    let mut ctx = SinglePassCtx::new(dispatch);
    let mut relations = Vec::new();
    let mut pending_calls: Vec<PendingCall<'_>> = Vec::new();
    let mut depth_warned = false;
    let mut anon_idx: u32 = 0;

    let mut stack = vec![Visit::Pre(root, 0)];

    while let Some(visit) = stack.pop() {
        match visit {
            Visit::LeaveScope => {
                if let Some(frame) = ctx.leave_scope() {
                    ctx.harvest_function_complexity(&frame);
                }
            }
            Visit::LeaveGuard => {
                ctx.scopes.leave_branch();
                let _ = ctx.leave_scope();
            }
            Visit::Pre(node, depth) => {
                if depth > AST_MAX_DEPTH {
                    if !depth_warned {
                        tracing::warn!(
                            file = %file_path.display(),
                            depth = AST_MAX_DEPTH,
                            "AST depth limit exceeded during single-pass extract; skipping deep branches"
                        );
                        depth_warned = true;
                    }
                    continue;
                }

                let mask = rgctl_plugin_api::dispatch_lookup_tracked(dispatch, node.kind_id());
                if mask.is_passive() {
                    for i in (0..node.child_count()).rev() {
                        if let Some(child) = node.child(i) {
                            stack.push(Visit::Pre(child, depth + 1));
                        }
                    }
                    continue;
                }
                let kind = node.kind();
                let mut pushed_scope = false;

                match kind {
                    "call" => {
                        let head = call_head(node, source).unwrap_or("");
                        if is_module_attribute_call(node) {
                            if matches!(head, "behaviour" | "behavior") {
                                if let (Some(enclosing_mod), Some(target)) = (
                                    ctx.scopes
                                        .current_module()
                                        .map(|m| m.qualified_prefix.clone()),
                                    import_target(node, source),
                                ) {
                                    relations.push(Relation {
                                        from: enclosing_mod,
                                        to: target.clone(),
                                        relation_type: RelationType::Implements,
                                        location: node_to_location(node, &file_path_str),
                                        metadata: serde_json::json!({
                                            "language": "elixir",
                                            "kind": "behaviour",
                                        }),
                                        to_qualified_hint: Some(target),
                                        to_type_hint: Some("interface".to_string()),
                                        ..Default::default()
                                    });
                                }
                            } else if matches!(head, "callback" | "macrocallback")
                                && let Some(name) = callback_function_name(node, source)
                            {
                                let qn = ctx.scopes.qualified_name(&name);
                                let attr_node = node.parent().unwrap_or(node);
                                let mut sym = Symbol {
                                    name,
                                    symbol_type: SymbolType::Function,
                                    qualified_name: Some(qn),
                                    location: node_to_location(attr_node, &file_path_str),
                                    signature: declaration_header_signature(attr_node, source),
                                    return_type: None,
                                    parameters: vec![],
                                    fields: vec![],
                                    modifiers: vec![],
                                    documentation: None,
                                    metadata: serde_json::json!({
                                        "language": "elixir",
                                        "kind": head,
                                    }),
                                    ..Default::default()
                                };
                                record_symbol_spans_from_source(&mut sym, attr_node, source);
                                ctx.symbols.push(sym);
                            }
                        } else if head == "defmodule" {
                            if let Some(name) = module_name(node, source) {
                                let qn = ctx.scopes.qualified_name(&name);
                                let mut sym = Symbol {
                                    name: name.clone(),
                                    symbol_type: SymbolType::Module,
                                    qualified_name: Some(qn.clone()),
                                    location: node_to_location(node, &file_path_str),
                                    signature: None,
                                    return_type: None,
                                    parameters: vec![],
                                    fields: vec![],
                                    modifiers: vec![],
                                    documentation: None,
                                    metadata: serde_json::json!({
                                        "language": "elixir",
                                        "kind": "defmodule",
                                    }),
                                    ..Default::default()
                                };
                                record_symbol_spans_from_source(&mut sym, node, source);
                                ctx.symbols.push(sym);
                                ctx.enter_scope(
                                    ScopeKind::Module,
                                    Some(name),
                                    qn,
                                    node.start_byte(),
                                    node.end_byte(),
                                    true,
                                );
                                pushed_scope = true;
                            }
                        } else if DEF_FORMS.contains(&head) {
                            if let Some(name) = def_function_name(node, source) {
                                let qn = ctx.scopes.qualified_name(&name);
                                let loc = ((node.end_position().row - node.start_position().row)
                                    + 1)
                                .max(1) as u32;
                                let mut sym = Symbol {
                                    name: name.clone(),
                                    symbol_type: SymbolType::Function,
                                    qualified_name: Some(qn.clone()),
                                    location: node_to_location(node, &file_path_str),
                                    signature: declaration_header_signature(node, source),
                                    return_type: None,
                                    parameters: vec![],
                                    fields: vec![],
                                    modifiers: vec![],
                                    documentation: None,
                                    metadata: serde_json::json!({
                                        "language": "elixir",
                                        "kind": head,
                                    }),
                                    ..Default::default()
                                };
                                record_symbol_spans_from_source(&mut sym, node, source);
                                ctx.symbols.push(sym);
                                ctx.enter_scope(
                                    ScopeKind::Function,
                                    Some(name),
                                    qn,
                                    node.start_byte(),
                                    node.end_byte(),
                                    true,
                                );
                                if let Some(f) = ctx.scopes.current_function_mut() {
                                    f.complexity.cyclomatic = 1;
                                    f.complexity.loc = loc;
                                }
                                pushed_scope = true;
                            }
                        } else if IMPORT_FORMS.contains(&head) {
                            if let Some(target) = import_target(node, source) {
                                if head == "use"
                                    && let Some(enclosing_mod) = ctx
                                        .scopes
                                        .current_module()
                                        .map(|m| m.qualified_prefix.clone())
                                {
                                    relations.push(Relation {
                                        from: enclosing_mod,
                                        to: target.clone(),
                                        relation_type: RelationType::Extends,
                                        location: node_to_location(node, &file_path_str),
                                        metadata: serde_json::json!({
                                            "language": "elixir",
                                            "kind": "use",
                                        }),
                                        to_qualified_hint: Some(target.clone()),
                                        to_type_hint: None,
                                        ..Default::default()
                                    });
                                }
                                let mut sym = Symbol {
                                    name: target.clone(),
                                    symbol_type: SymbolType::Import,
                                    qualified_name: Some(target),
                                    location: node_to_location(node, &file_path_str),
                                    signature: None,
                                    return_type: None,
                                    parameters: vec![],
                                    fields: vec![],
                                    modifiers: vec![],
                                    documentation: None,
                                    metadata: serde_json::json!({
                                        "language": "elixir",
                                        "kind": head,
                                    }),
                                    ..Default::default()
                                };
                                record_symbol_spans_from_source(&mut sym, node, source);
                                ctx.symbols.push(sym);
                            }
                        } else if CFG_FORMS.contains(&head) {
                            if ctx.scopes.current_function().is_some() {
                                let delta = 1 + ctx
                                    .scopes
                                    .current_function()
                                    .map(|f| f.complexity.cognitive_nesting)
                                    .unwrap_or(0);
                                ctx.scopes.note_branch(delta);
                                ctx.scopes.enter_branch();
                                let scope_depth = ctx.scopes.depth();
                                ctx.scopes.push(ScopeFrame::new(
                                    ScopeKind::GuardBlock,
                                    None,
                                    String::new(),
                                    node.start_byte(),
                                    node.end_byte(),
                                    scope_depth,
                                ));
                                stack.push(Visit::LeaveGuard);
                            }
                        } else if !is_def_name_call(node, source)
                            && ctx.scopes.current_function().is_some()
                            && elixir_call_target(node, source).is_some()
                        {
                            if let Some(frame) = ctx.scopes.current_function() {
                                if let Some(idx) = frame.symbol_idx {
                                    pending_calls.push(PendingCall {
                                        node,
                                        from_symbol_idx: idx,
                                    });
                                }
                            }
                        }
                    }
                    "anonymous_function" => {
                        anon_idx += 1;
                        let name = format!("<fn#{anon_idx}>");
                        let qn = ctx.scopes.qualified_name(&name);
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
                        let mut sym = Symbol {
                            name: name.clone(),
                            symbol_type: SymbolType::Function,
                            qualified_name: Some(qn.clone()),
                            location: node_to_location(node, &file_path_str),
                            signature: None,
                            return_type: None,
                            parameters: vec![],
                            fields: vec![],
                            modifiers: vec![],
                            documentation: None,
                            metadata: serde_json::json!({
                                "language": "elixir",
                                "kind": "anonymous_function",
                            }),
                            ..Default::default()
                        };
                        record_symbol_spans_from_source(&mut sym, node, source);
                        ctx.symbols.push(sym);
                        ctx.enter_scope(
                            ScopeKind::Lambda,
                            Some(name),
                            qn,
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        if let Some(f) = ctx.scopes.current_function_mut() {
                            f.complexity.cyclomatic = 1;
                            f.complexity.loc = loc;
                        }
                        pushed_scope = true;
                    }
                    "alias" => {
                        if let Some(target) = node
                            .utf8_text(source)
                            .ok()
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                        {
                            let from = ctx
                                .scopes
                                .current_function()
                                .map(|f| f.qualified_prefix.clone())
                                .or_else(|| {
                                    ctx.scopes
                                        .current_type()
                                        .map(|t| t.qualified_prefix.clone())
                                })
                                .unwrap_or_else(|| file_path_str.to_string());
                            relations.push(Relation {
                                from,
                                to: target.clone(),
                                relation_type: RelationType::References,
                                location: node_to_location(node, &file_path_str),
                                metadata: serde_json::json!({
                                    "language": "elixir",
                                    "kind": "alias",
                                }),
                                to_qualified_hint: Some(target),
                                to_type_hint: None,
                                ..Default::default()
                            });
                        }
                    }
                    "access_call" => {
                        if let Some(frame) = ctx.scopes.current_function() {
                            let from = frame.qualified_prefix.clone();
                            let target = node
                                .utf8_text(source)
                                .ok()
                                .map(|s| s.trim().to_string())
                                .filter(|s| !s.is_empty())
                                .unwrap_or_else(|| "access".to_string());
                            relations.push(Relation {
                                from,
                                to: target.clone(),
                                relation_type: RelationType::References,
                                location: node_to_location(node, &file_path_str),
                                metadata: serde_json::json!({
                                    "language": "elixir",
                                    "kind": "access_call",
                                }),
                                to_qualified_hint: Some(target),
                                to_type_hint: None,
                                ..Default::default()
                            });
                        }
                    }
                    "binary_operator" if ctx.scopes.current_function().is_some() => {
                        let op = node
                            .child_by_field_name("operator")
                            .and_then(|n| n.utf8_text(source).ok())
                            .unwrap_or("");
                        // Short-circuit / comparison ops contribute to cyclomatic complexity.
                        if matches!(
                            op,
                            "and" | "or" | "&&" | "||" | "andalso" | "orelse" | "in" | "not in"
                        ) {
                            let delta = 1 + ctx
                                .scopes
                                .current_function()
                                .map(|f| f.complexity.cognitive_nesting)
                                .unwrap_or(0);
                            ctx.scopes.note_branch(delta);
                        }
                    }
                    "stab_clause" | "catch_block" | "rescue_block" | "after_block"
                    | "else_block"
                        if ctx.scopes.current_function().is_some() =>
                    {
                        let delta = 1 + ctx
                            .scopes
                            .current_function()
                            .map(|f| f.complexity.cognitive_nesting)
                            .unwrap_or(0);
                        ctx.scopes.note_branch(delta);
                        ctx.scopes.enter_branch();
                        let scope_depth = ctx.scopes.depth();
                        ctx.scopes.push(ScopeFrame::new(
                            ScopeKind::GuardBlock,
                            None,
                            String::new(),
                            node.start_byte(),
                            node.end_byte(),
                            scope_depth,
                        ));
                        stack.push(Visit::LeaveGuard);
                    }
                    _ => {}
                }

                if pushed_scope {
                    stack.push(Visit::LeaveScope);
                }
                for i in (0..node.child_count()).rev() {
                    if let Some(child) = node.child(i) {
                        stack.push(Visit::Pre(child, depth + 1));
                    }
                }
            }
        }
    }

    for pending in pending_calls {
        let Some(from_fn) = ctx.symbols.get(pending.from_symbol_idx) else {
            continue;
        };
        let Some((recv_mod, callee)) = elixir_call_target(pending.node, source) else {
            continue;
        };
        let from = from_fn
            .qualified_name
            .clone()
            .unwrap_or_else(|| from_fn.name.clone());
        let enclosing_mod = from.rsplit_once('.').map(|(m, _)| m.to_string());
        let (to_type_hint, to_qualified_hint) = match recv_mod {
            Some(ref m) if m == "__MODULE__" => {
                let qh = enclosing_mod.as_ref().map(|em| format!("{em}.{callee}"));
                (enclosing_mod, qh)
            }
            Some(m)
                if m.chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_uppercase() || c == ':') =>
            {
                let clean = m.trim_start_matches(':').to_string();
                let qh = Some(format!("{clean}.{callee}"));
                (Some(clean), qh)
            }
            Some(_) => (None, Some(callee.clone())),
            None => {
                let qh = enclosing_mod
                    .as_ref()
                    .map(|em| format!("{em}.{callee}"))
                    .or_else(|| Some(callee.clone()));
                (enclosing_mod, qh)
            }
        };
        let mut rel = Relation {
            from,
            to: callee,
            relation_type: RelationType::Calls,
            location: node_to_location(pending.node, &file_path_str),
            metadata: serde_json::json!({ "language": "elixir" }),
            to_qualified_hint,
            to_type_hint,
            ..Default::default()
        };
        record_call_site_spans(&mut rel, pending.node);
        relations.push(rel);
    }

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn elixir_extracts_module_function_call_and_complexity() {
        let src = r#"
defmodule Foo do
  def bar(x) do
    if true do
      baz()
    end
  end
end
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("t.ex");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Module && s.name == "Foo"),
            "expected module Foo, got {:?}",
            result
                .symbols
                .iter()
                .map(|s| (&s.name, s.symbol_type))
                .collect::<Vec<_>>()
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Function && s.name == "bar"),
            "expected function bar, got {:?}",
            result.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            result
                .relations
                .iter()
                .any(|r| r.relation_type == RelationType::Calls && r.to.contains("baz")),
            "expected Calls to baz, got {:?}",
            result.relations
        );
        let bar = result
            .symbols
            .iter()
            .find(|s| s.name == "bar")
            .expect("bar");
        let metrics = complexity_from_metadata(bar).expect("rgctl_complexity");
        assert!(
            metrics.cyclomatic >= 2,
            "expected cyclomatic >= 2, got {}",
            metrics.cyclomatic
        );
    }

    #[test]
    fn elixir_extracts_behaviour_callback_use_and_remote_calls() {
        let src = r#"
defmodule OrderRepo do
  @callback save(integer()) :: integer()
end

defmodule BaseLedger do
  def save(x), do: x + 1
end

defmodule OrderLedger do
  @behaviour OrderRepo
  use BaseLedger

  def run(x) do
    BaseLedger.save(x)
  end
end
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("ledger.ex");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        assert!(
            result.symbols.iter().any(|s| {
                s.symbol_type == SymbolType::Function
                    && s.qualified_name.as_deref() == Some("OrderRepo.save")
            }),
            "expected @callback Function OrderRepo.save, got {:?}",
            result.symbols
        );
        assert!(
            result.relations.iter().any(|r| {
                r.relation_type == RelationType::Implements
                    && r.from == "OrderLedger"
                    && r.to == "OrderRepo"
            }),
            "expected OrderLedger Implements OrderRepo, got {:?}",
            result.relations
        );
        assert!(
            result.relations.iter().any(|r| {
                r.relation_type == RelationType::Extends
                    && r.from == "OrderLedger"
                    && r.to == "BaseLedger"
            }),
            "expected OrderLedger Extends BaseLedger via use, got {:?}",
            result.relations
        );
        assert!(
            result.relations.iter().any(|r| {
                r.relation_type == RelationType::Calls
                    && r.from == "OrderLedger.run"
                    && r.to_qualified_hint.as_deref() == Some("BaseLedger.save")
            }),
            "expected remote call OrderLedger.run -> BaseLedger.save, got {:?}",
            result.relations
        );
    }
}
