//! Single-pass AST extraction for Lua (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::Plugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{
    extract_name_from_node, node_to_location, record_symbol_spans_from_source,
};
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
    source: &[u8],
    name: String,
    file_path: &str,
) -> bool {
    let loc = ((node.end_position().row - node.start_position().row) + 1).max(1) as u32;
    let qn = name.clone();
    let mut sym = Symbol {
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
    };
    record_symbol_spans_from_source(&mut sym, node, source);
    ctx.symbols
        .push(rgctl_plugin_helpers::callable_identity::symbol(
            sym,
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
    // Source-order ordinals distinguish separate lexical local bindings with
    // the same name. Cache by assignment/slot so nested tables reuse an owner.
    let mut local_table_owners = std::collections::HashMap::<(usize, usize), String>::new();
    let mut local_table_ordinals = std::collections::HashMap::<String, usize>::new();
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
                        let mut current = node;
                        let mut owner = None;
                        let mut path = Vec::new();
                        while let Some(parent) = current.parent() {
                            match parent.kind() {
                                "field" => {
                                    // Only a named table value has a stable member owner;
                                    // array entries and computed keys cannot inherit it.
                                    let Some(key) = parent.child_by_field_name("name") else {
                                        break;
                                    };
                                    if key.kind() != "identifier" {
                                        break;
                                    }
                                    if let Ok(name) = key.utf8_text(source) {
                                        path.push(name.to_owned());
                                    }
                                }
                                "table_constructor" | "parenthesized_expression" => {}
                                "expression_list" => {
                                    let Some(assignment) = parent
                                        .parent()
                                        .filter(|n| n.kind() == "assignment_statement")
                                    else {
                                        break;
                                    };
                                    let mut expressions = parent.walk();
                                    let position = parent
                                        .named_children(&mut expressions)
                                        .filter(|n| n.kind() != "comment")
                                        .position(|n| n.id() == current.id());
                                    let Some(position) = position else {
                                        break;
                                    };
                                    let mut children = assignment.walk();
                                    let lhs = assignment
                                        .named_children(&mut children)
                                        .find(|n| n.kind() == "variable_list");
                                    owner = lhs.and_then(|lhs| {
                                        let mut names = lhs.walk();
                                        lhs.children_by_field_name("name", &mut names)
                                            .nth(position)
                                            .and_then(|n| n.utf8_text(source).ok())
                                            .map(str::to_owned)
                                    });
                                    if assignment
                                        .parent()
                                        .is_some_and(|n| n.kind() == "variable_declaration")
                                        && let Some(local) = owner.as_mut()
                                    {
                                        let owner_key = (assignment.id(), position);
                                        if let Some(previous) = local_table_owners.get(&owner_key) {
                                            *local = previous.clone();
                                        } else {
                                            let base = if let Some(frame) =
                                                ctx.scopes.current_function()
                                            {
                                                format!("{}.{}", frame.qualified_prefix, local)
                                            } else {
                                                local.clone()
                                            };
                                            let ordinal = local_table_ordinals
                                                .entry(base.clone())
                                                .or_default();
                                            *local = if *ordinal == 0 {
                                                base
                                            } else {
                                                format!("{base}#local{}", *ordinal + 1)
                                            };
                                            *ordinal += 1;
                                            local_table_owners.insert(owner_key, local.clone());
                                        }
                                    }
                                    break;
                                }
                                // Do not attribute tables returned by a function or
                                // passed to a call to an unrelated outer assignment.
                                _ => break,
                            }
                            current = parent;
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
                                push_function_symbol(&mut ctx, node, source, name, &file_path_str);
                        }
                    }
                    "assignment_statement" => {
                        if let Some(name) = assignment_function_name(node, source) {
                            pushed_scope =
                                push_function_symbol(&mut ctx, node, source, name, &file_path_str);
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
    fn table_members_follow_their_matching_assignment_and_local_function_scope() {
        let source = "local a,b = {x=1}, {y=2, nested={z=3}}\nfunction first() local config={value=1} end\nfunction second() local config={value=2} end\n";
        let plugin = Plugin::new().unwrap();
        let result =
            extract_all_single_pass(&plugin, Path::new("fields.lua"), source.as_bytes()).unwrap();
        let fields: std::collections::BTreeSet<_> = result
            .symbols
            .iter()
            .filter(|symbol| symbol.metadata["kind"] == "field")
            .map(|symbol| symbol.qualified_name.as_deref().unwrap())
            .collect();
        assert_eq!(
            fields,
            std::collections::BTreeSet::from([
                "a.x",
                "b.y",
                "b.nested",
                "b.nested.z",
                "first.config.value",
                "second.config.value"
            ])
        );
        for field in result
            .symbols
            .iter()
            .filter(|symbol| symbol.metadata["kind"] == "field")
        {
            let start =
                field.start_byte.unwrap() as usize + field.name_rel_offset.unwrap() as usize;
            let end = start
                + field
                    .name_token_len
                    .map(usize::from)
                    .unwrap_or(field.name.len());
            assert_eq!(&source[start..end], field.name);
        }
    }

    #[test]
    fn separate_lexical_locals_and_unequal_assignments_do_not_alias_table_fields() {
        let source = "function f() do local config={value=1, nested={leaf=1}} end do local config={value=2} end end\nlocal a,b = {one=1}\nlocal c = {two=2}, {discarded=3}\n";
        let plugin = Plugin::new().unwrap();
        let result =
            extract_all_single_pass(&plugin, Path::new("fields.lua"), source.as_bytes()).unwrap();
        let fields: std::collections::BTreeSet<_> = result
            .symbols
            .iter()
            .filter(|symbol| symbol.metadata["kind"] == "field")
            .map(|symbol| symbol.qualified_name.as_deref().unwrap())
            .collect();
        assert_eq!(
            fields,
            std::collections::BTreeSet::from([
                "f.config.value",
                "f.config.nested",
                "f.config.nested.leaf",
                "f.config#local2.value",
                "a.one",
                "c.two"
            ])
        );
    }

    #[test]
    fn unrelated_returned_and_argument_tables_do_not_inherit_assignment_owners() {
        let source = "local a = factory({argument=1})\nlocal b = {make=function() return {returned=1} end}\nlocal c = {{array_member=1}}\n";
        let plugin = Plugin::new().unwrap();
        let result =
            extract_all_single_pass(&plugin, Path::new("fields.lua"), source.as_bytes()).unwrap();
        let fields: Vec<_> = result
            .symbols
            .iter()
            .filter(|symbol| symbol.metadata["kind"] == "field")
            .collect();
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].qualified_name.as_deref(), Some("b.make"));
    }

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
