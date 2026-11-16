//! Single-pass AST extraction for SQL (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::Plugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{
    extract_name_from_node, node_to_location, record_call_site_spans,
    record_symbol_spans_from_source,
};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../sql-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
/// Must stay in sync with non-Literal active labels in `sql-ast-coverage.json`.
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    // Symbol
    ("alter_schema", HandlerMask::SYMBOL),
    ("alter_table", HandlerMask::SYMBOL),
    ("alter_type", HandlerMask::SYMBOL),
    ("alter_view", HandlerMask::SYMBOL),
    ("column_definition", HandlerMask::SYMBOL),
    ("create_database", HandlerMask::SYMBOL),
    ("create_extension", HandlerMask::SYMBOL),
    ("create_function", HandlerMask::SYMBOL),
    ("create_index", HandlerMask::SYMBOL),
    ("create_materialized_view", HandlerMask::SYMBOL),
    ("create_role", HandlerMask::SYMBOL),
    ("create_schema", HandlerMask::SYMBOL),
    ("create_sequence", HandlerMask::SYMBOL),
    ("create_table", HandlerMask::SYMBOL),
    ("create_trigger", HandlerMask::SYMBOL),
    ("create_type", HandlerMask::SYMBOL),
    ("create_view", HandlerMask::SYMBOL),
    ("drop_function", HandlerMask::SYMBOL),
    ("drop_index", HandlerMask::SYMBOL),
    ("drop_schema", HandlerMask::SYMBOL),
    ("drop_table", HandlerMask::SYMBOL),
    ("drop_type", HandlerMask::SYMBOL),
    ("drop_view", HandlerMask::SYMBOL),
    ("function_declaration", HandlerMask::SYMBOL),
    // Relation
    ("cross_join", HandlerMask::RELATION),
    ("from", HandlerMask::RELATION),
    ("invocation", HandlerMask::RELATION),
    ("join", HandlerMask::RELATION),
    ("lateral_cross_join", HandlerMask::RELATION),
    ("lateral_join", HandlerMask::RELATION),
    // CfgStatement
    ("between_expression", HandlerMask::CFG_STMT),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("case", HandlerMask::CFG_STMT),
    ("filter_expression", HandlerMask::CFG_STMT),
    ("unary_expression", HandlerMask::CFG_STMT),
    ("when_clause", HandlerMask::CFG_STMT),
    // AstSkeleton
    ("add_column", HandlerMask::AST_SKELETON),
    ("add_constraint", HandlerMask::AST_SKELETON),
    ("alter_column", HandlerMask::AST_SKELETON),
    ("array_size_definition", HandlerMask::AST_SKELETON),
    ("comment_statement", HandlerMask::AST_SKELETON),
    ("cte", HandlerMask::AST_SKELETON),
    ("delete", HandlerMask::AST_SKELETON),
    ("drop_column", HandlerMask::AST_SKELETON),
    ("drop_constraint", HandlerMask::AST_SKELETON),
    ("frame_definition", HandlerMask::AST_SKELETON),
    ("group_by", HandlerMask::AST_SKELETON),
    ("insert", HandlerMask::AST_SKELETON),
    ("limit", HandlerMask::AST_SKELETON),
    ("offset", HandlerMask::AST_SKELETON),
    ("order_by", HandlerMask::AST_SKELETON),
    ("reset_statement", HandlerMask::AST_SKELETON),
    ("select", HandlerMask::AST_SKELETON),
    ("select_expression", HandlerMask::AST_SKELETON),
    ("set_operation", HandlerMask::AST_SKELETON),
    ("set_statement", HandlerMask::AST_SKELETON),
    ("statement", HandlerMask::AST_SKELETON),
    ("transaction", HandlerMask::AST_SKELETON),
    ("update", HandlerMask::AST_SKELETON),
    ("where", HandlerMask::AST_SKELETON),
    ("window_clause", HandlerMask::AST_SKELETON),
    ("window_function", HandlerMask::AST_SKELETON),
];

const AST_MAX_DEPTH: usize = 2048;

/// Control-flow hosts that contribute cyclomatic / cognitive complexity.
const BRANCH_KINDS: &[&str] = &[
    "between_expression",
    "binary_expression",
    "case",
    "filter_expression",
    "unary_expression",
    "when_clause",
];

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn grammar() -> tree_sitter::Language {
    tree_sitter_sql::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&grammar(), HANDLED_KINDS))
}

fn dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&grammar(), MANIFEST_JSON))
}

enum Visit<'a> {
    Pre(Node<'a>, usize),
    LeaveScope,
    LeaveGuard,
}

fn object_name(node: Node, source: &[u8]) -> Option<String> {
    if let Some(name) = node.child_by_field_name("name") {
        if let Ok(t) = name.utf8_text(source) {
            let t = t.trim();
            if !t.is_empty() {
                return Some(t.to_string());
            }
        }
        if let Some(n) = extract_name_from_node(name, source).ok().flatten() {
            return Some(n);
        }
    }
    extract_name_from_node(node, source).ok().flatten()
}

fn push_named_symbol(
    ctx: &mut SinglePassCtx,
    node: Node,
    source: &[u8],
    name: String,
    symbol_type: SymbolType,
    file_path: &str,
    kind_label: &str,
) {
    let mut sym = Symbol {
        name: name.clone(),
        symbol_type,
        qualified_name: Some(name),
        location: node_to_location(node, file_path),
        signature: None,
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({
            "language": "sql",
            "kind": kind_label,
        }),
        ..Default::default()
    };
    record_symbol_spans_from_source(&mut sym, node, source);
    ctx.symbols
        .push(rgctl_plugin_helpers::callable_identity::symbol(
            sym,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Sql,
        ));
}

fn symbol_type_for_ddl(kind: &str) -> SymbolType {
    match kind {
        "create_function" | "function_declaration" | "create_trigger" | "drop_function" => {
            SymbolType::Function
        }
        "column_definition" => SymbolType::Variable,
        "create_schema" | "alter_schema" | "drop_schema" | "create_database" => SymbolType::Module,
        _ => SymbolType::Table,
    }
}

/// Soft Calls from `from` / `join` relation targets (table references).
fn push_table_ref_call(
    node: Node,
    source: &[u8],
    file_path: &Path,
    from_fn: &Symbol,
    relations: &mut Vec<Relation>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "relation" || child.kind() == "object_reference" {
            if let Some(name) = object_name(child, source)
                .or_else(|| extract_name_from_node(child, source).ok().flatten())
            {
                let from = from_fn
                    .qualified_name
                    .clone()
                    .unwrap_or_else(|| from_fn.name.clone());
                let mut rel = Relation {
                    from,
                    to: name,
                    relation_type: RelationType::Calls,
                    location: node_to_location(child, &file_path.to_string_lossy()),
                    metadata: serde_json::json!({
                        "language": "sql",
                        "soft": true,
                        "kind": node.kind(),
                    }),
                    to_qualified_hint: None,
                    to_type_hint: None,
                    ..Default::default()
                };
                record_call_site_spans(&mut rel, child);
                relations.push(rel);
            }
        }
        // Nested object_reference under relation
        let mut inner = child.walk();
        for gc in child.children(&mut inner) {
            if gc.kind() == "object_reference" {
                if let Some(name) = object_name(gc, source) {
                    let from = from_fn
                        .qualified_name
                        .clone()
                        .unwrap_or_else(|| from_fn.name.clone());
                    let mut rel = Relation {
                        from,
                        to: name,
                        relation_type: RelationType::Calls,
                        location: node_to_location(gc, &file_path.to_string_lossy()),
                        metadata: serde_json::json!({
                            "language": "sql",
                            "soft": true,
                            "kind": node.kind(),
                        }),
                        to_qualified_hint: None,
                        to_type_hint: None,
                        ..Default::default()
                    };
                    record_call_site_spans(&mut rel, gc);
                    relations.push(rel);
                }
            }
        }
    }
}

fn push_invocation_call(
    node: Node,
    source: &[u8],
    file_path: &Path,
    from_fn: &Symbol,
    relations: &mut Vec<Relation>,
) {
    let name =
        object_name(node, source).or_else(|| extract_name_from_node(node, source).ok().flatten());
    let Some(name) = name else {
        return;
    };
    let from = from_fn
        .qualified_name
        .clone()
        .unwrap_or_else(|| from_fn.name.clone());
    let mut rel = Relation {
        from,
        to: name,
        relation_type: RelationType::Calls,
        location: node_to_location(node, &file_path.to_string_lossy()),
        metadata: serde_json::json!({
            "language": "sql",
            "soft": true,
            "kind": "invocation",
        }),
        to_qualified_hint: None,
        to_type_hint: None,
        ..Default::default()
    };
    record_call_site_spans(&mut rel, node);
    relations.push(rel);
}

/// Single iterative DFS: table/function/view symbols + soft from/join + case complexity.
pub fn extract_all_single_pass(
    _plugin: &Plugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("sql", grammar(), source).map_err(|e| Error::ParseError {
        file: file_path.to_path_buf(),
        line: 0,
        message: e,
    })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = dispatch();

    let mut ctx = SinglePassCtx::new(dispatch);
    let mut relations = Vec::new();
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
                    "create_table"
                    | "create_view"
                    | "create_materialized_view"
                    | "create_index"
                    | "create_type"
                    | "create_schema"
                    | "create_database"
                    | "create_role"
                    | "create_sequence"
                    | "create_extension"
                    | "create_trigger"
                    | "alter_table"
                    | "alter_view"
                    | "alter_type"
                    | "alter_schema"
                    | "drop_table"
                    | "drop_view"
                    | "drop_function"
                    | "drop_index"
                    | "drop_type"
                    | "drop_schema"
                    | "column_definition"
                    | "function_declaration" => {
                        if let Some(name) = object_name(node, source) {
                            push_named_symbol(
                                &mut ctx,
                                node,
                                source,
                                name,
                                symbol_type_for_ddl(kind),
                                &file_path_str,
                                kind,
                            );
                        }
                    }
                    "create_function" => {
                        if let Some(name) = object_name(node, source) {
                            let loc = ((node.end_position().row - node.start_position().row) + 1)
                                .max(1) as u32;
                            let qn = name.clone();
                            push_named_symbol(
                                &mut ctx,
                                node,
                                source,
                                name.clone(),
                                SymbolType::Function,
                                &file_path_str,
                                "create_function",
                            );
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
                    }
                    "from" | "join" | "cross_join" | "lateral_join" | "lateral_cross_join" => {
                        if let Some(frame) = ctx.scopes.current_function() {
                            if let Some(idx) = frame.symbol_idx {
                                if let Some(from_fn) = ctx.symbols.get(idx) {
                                    let from_sym = from_fn.clone();
                                    push_table_ref_call(
                                        node,
                                        source,
                                        file_path,
                                        &from_sym,
                                        &mut relations,
                                    );
                                }
                            }
                        }
                    }
                    "invocation" => {
                        if let Some(frame) = ctx.scopes.current_function() {
                            if let Some(idx) = frame.symbol_idx {
                                if let Some(from_fn) = ctx.symbols.get(idx) {
                                    let from_sym = from_fn.clone();
                                    push_invocation_call(
                                        node,
                                        source,
                                        file_path,
                                        &from_sym,
                                        &mut relations,
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                }

                if ctx.scopes.current_function().is_some() && BRANCH_KINDS.contains(&kind) {
                    let delta = 1 + ctx
                        .scopes
                        .current_function()
                        .map(|f| f.complexity.cognitive_nesting)
                        .unwrap_or(0);
                    ctx.scopes.note_branch(delta);
                    if kind == "case" || kind == "when_clause" {
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

    rgctl_plugin_helpers::callable_identity::finish(
        rgctl_plugin_helpers::callable_identity::Policy::Sql,
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
    fn sql_extracts_table_and_function() {
        let src = r#"
CREATE TABLE users (id INT);
CREATE FUNCTION foo() RETURNS INT LANGUAGE sql AS $$ SELECT 1; $$;
CREATE VIEW v AS SELECT * FROM users;
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("t.sql");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Table && s.name == "users"),
            "expected table users, got {:?}",
            result
                .symbols
                .iter()
                .map(|s| (&s.name, &s.symbol_type))
                .collect::<Vec<_>>()
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Function && s.name == "foo"),
            "expected function foo"
        );
        assert!(
            result.symbols.iter().any(|s| s.name == "v"),
            "expected view v"
        );
    }
}
