//! Single-pass AST extraction for Lua (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::Plugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{extract_name_from_node, node_to_location};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../lua-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
/// Keep in sync with non-Literal active handlers in `lua-ast-coverage.json`.
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("assignment_statement", HandlerMask::SYMBOL),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("block", HandlerMask::CFG_STMT),
    ("bracket_index_expression", HandlerMask::AST_SKELETON),
    ("break_statement", HandlerMask::CFG_STMT),
    ("do_statement", HandlerMask::CFG_STMT),
    ("dot_index_expression", HandlerMask::AST_SKELETON),
    ("else_statement", HandlerMask::CFG_STMT),
    ("elseif_statement", HandlerMask::CFG_STMT),
    ("empty_statement", HandlerMask::CFG_STMT),
    ("for_generic_clause", HandlerMask::CFG_STMT),
    ("for_numeric_clause", HandlerMask::CFG_STMT),
    ("for_statement", HandlerMask::CFG_STMT),
    ("function_call", HandlerMask::RELATION),
    ("function_declaration", HandlerMask::SYMBOL),
    ("function_definition", HandlerMask::SYMBOL),
    ("goto_statement", HandlerMask::CFG_STMT),
    ("if_statement", HandlerMask::CFG_STMT),
    ("implicit_variable_declaration", HandlerMask::AST_SKELETON),
    ("label_statement", HandlerMask::CFG_STMT),
    ("method_index_expression", HandlerMask::AST_SKELETON),
    ("parameters", HandlerMask::AST_SKELETON),
    ("repeat_statement", HandlerMask::CFG_STMT),
    ("return_statement", HandlerMask::CFG_STMT),
    ("table_constructor", HandlerMask::SYMBOL),
    ("unary_expression", HandlerMask::CFG_STMT),
    ("variable_declaration", HandlerMask::AST_SKELETON),
    ("while_statement", HandlerMask::CFG_STMT),
];

const LUA_CALL_KINDS: &[&str] = &["function_call"];
const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn grammar() -> tree_sitter::Language {
    tree_sitter_lua::LANGUAGE.into()
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

fn push_function_symbol(
    ctx: &mut SinglePassCtx,
    node: Node,
    name: String,
    file_path: &str,
    source: &[u8],
) -> bool {
    let loc = ((node.end_position().row - node.start_position().row) + 1).max(1) as u32;
    let qn = name.clone();
    ctx.symbols
        .push(rgctl_plugin_helpers::callable_identity::symbol(
            Symbol {
                name: name.clone(),
                symbol_type: SymbolType::Function,
                qualified_name: Some(qn.clone()),
                location: node_to_location(node, file_path),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: serde_json::json!({ "language": "lua" }),
                ..Default::default()
            },
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Lua,
        ));
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
    true
}

/// `function foo()` / `function M.foo()` / `function M:bar()` name field.
fn lua_function_decl_name(node: Node, source: &[u8]) -> Option<String> {
    let name_node = node.child_by_field_name("name").or_else(|| {
        let mut c = node.walk();
        node.children(&mut c).find(|ch| {
            matches!(
                ch.kind(),
                "identifier" | "dot_index_expression" | "method_index_expression" | "attribute"
            )
        })
    })?;
    match name_node.kind() {
        "dot_index_expression" | "method_index_expression" | "attribute" => {
            // Full text preserves `M.foo` / `M:bar` (not just the receiver `M`).
            name_node
                .utf8_text(source)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        }
        _ => name_node
            .utf8_text(source)
            .ok()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| extract_name_from_node(name_node, source).ok().flatten()),
    }
}

fn assignment_function_name(node: Node, source: &[u8]) -> Option<String> {
    let mut has_fn_def = false;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "expression_list" {
            let mut ec = child.walk();
            for expr in child.children(&mut ec) {
                if expr.kind() == "function_definition" {
                    has_fn_def = true;
                    break;
                }
            }
        }
    }
    if !has_fn_def {
        return None;
    }
    // Prefer full LHS text (e.g. `M.m`) when present.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_list" {
            if let Ok(text) = child.utf8_text(source) {
                let t = text.trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
            return extract_name_from_node(child, source).ok().flatten();
        }
    }
    None
}

/// Single iterative DFS: symbols + deferred calls + complexity.
pub fn extract_all_single_pass(
    _plugin: &Plugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("lua", grammar(), source).map_err(|e| Error::ParseError {
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
                    "table_constructor" => {
                        let mut ancestor = node.parent();
                        let mut owner = None;
                        let mut path = Vec::new();
                        while let Some(parent) = ancestor {
                            if parent.kind() == "field"
                                && let Some(key) = parent.child_by_field_name("name")
                                && key.kind() == "identifier"
                            {
                                if let Ok(name) = key.utf8_text(source) {
                                    path.push(name.to_owned());
                                }
                            }
                            if parent.kind() == "assignment_statement" {
                                let mut c = parent.walk();
                                owner = parent
                                    .named_children(&mut c)
                                    .find(|n| n.kind() == "variable_list")
                                    .and_then(|n| n.named_child(0))
                                    .and_then(|n| n.utf8_text(source).ok())
                                    .map(str::to_owned);
                                break;
                            }
                            ancestor = parent.parent();
                        }
                        if let Some(mut owner) = owner {
                            for component in path.iter().rev() {
                                owner.push('.');
                                owner.push_str(component);
                            }
                            let mut cursor = node.walk();
                            for member in node
                                .named_children(&mut cursor)
                                .filter(|n| n.kind() == "field")
                            {
                                if let Some(key) = member.child_by_field_name("name")
                                    && key.kind() == "identifier"
                                    && let Ok(name) = key.utf8_text(source)
                                {
                                    let mut symbol = Symbol {
                                        name: name.into(),
                                        symbol_type: SymbolType::Variable,
                                        qualified_name: Some(format!("{owner}.{name}")),
                                        location: node_to_location(member, &file_path_str),
                                        metadata: serde_json::json!({"language":"lua","kind":"field"}),
                                        ..Default::default()
                                    };
                                    rgctl_plugin_helpers::spans::field_symbol_spans(
                                        &mut symbol,
                                        member,
                                        source,
                                    );
                                    ctx.symbols.push(symbol);
                                }
                            }
                        }
                    }
                    "function_declaration" => {
                        if let Some(name) = lua_function_decl_name(node, source) {
                            pushed_scope =
                                push_function_symbol(&mut ctx, node, name, &file_path_str, source);
                        }
                    }
                    "assignment_statement" => {
                        if let Some(name) = assignment_function_name(node, source) {
                            pushed_scope =
                                push_function_symbol(&mut ctx, node, name, &file_path_str, source);
                        }
                    }
                    "function_definition" => {
                        // Named only via `function_declaration` / assignment; skip bare anon.
                    }
                    _ => {}
                }

                if LUA_CALL_KINDS.contains(&kind) {
                    if let Some(frame) = ctx.scopes.current_function() {
                        if let Some(idx) = frame.symbol_idx {
                            pending_calls.push(PendingCall {
                                node,
                                from_symbol_idx: idx,
                            });
                        }
                    }
                }

                if ctx.scopes.current_function().is_some() {
                    match kind {
                        "if_statement" | "while_statement" | "for_statement"
                        | "repeat_statement" | "do_statement" => {
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
                        "elseif_statement" | "else_statement" => {
                            // Cyclomatic +1 without nesting (matches CFG branch fan-out).
                            if let Some(f) = ctx.scopes.current_function_mut() {
                                f.complexity.cyclomatic = f.complexity.cyclomatic.saturating_add(1);
                            }
                        }
                        "return_statement" => {
                            ctx.scopes.note_return();
                        }
                        _ => {}
                    }
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
        let identity_relation_start = relations.len();
        let Some(from_fn) = ctx.symbols.get(pending.from_symbol_idx) else {
            continue;
        };
        push_call_relation_from(
            pending.node,
            source,
            file_path,
            &ctx.symbols,
            from_fn,
            "lua",
            &mut relations,
        );
        rgctl_plugin_helpers::callable_identity::call(
            pending.node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Lua,
            ctx.symbols.get(pending.from_symbol_idx),
            &mut relations[identity_relation_start..],
        );
    }

    rgctl_plugin_helpers::callable_identity::finish(
        rgctl_plugin_helpers::callable_identity::Policy::Lua,
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn lua_extracts_function_call_and_complexity() {
        let src = r#"
function foo(x)
  if x then
    bar()
    return 1
  end
end
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("t.lua");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Function && s.name == "foo"),
            "expected function foo, got {:?}",
            result.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            result
                .relations
                .iter()
                .any(|r| r.relation_type == RelationType::Calls),
            "expected Calls relation, got {:?}",
            result.relations
        );
        let foo = result
            .symbols
            .iter()
            .find(|s| s.name == "foo")
            .expect("foo");
        let metrics = complexity_from_metadata(foo).expect("rgctl_complexity");
        assert!(
            metrics.cyclomatic >= 2,
            "expected cyclomatic >= 2, got {}",
            metrics.cyclomatic
        );
    }

    #[test]
    fn lua_table_and_method_function_names() {
        let src = r#"
local M = {}
function M.foo(x)
  return x
end
function M:bar(y)
  return y
end
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("t.lua");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        let names: Vec<_> = result
            .symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            names.iter().any(|n| *n == "M.foo" || n.ends_with(".foo")),
            "expected M.foo, got {names:?}"
        );
        assert!(
            names.iter().any(|n| *n == "M:bar" || n.ends_with(":bar")),
            "expected M:bar, got {names:?}"
        );
        assert!(
            !names.iter().all(|n| *n == "M"),
            "must not collapse all methods to receiver M, got {names:?}"
        );
    }
}
