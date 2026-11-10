//! Single-pass AST extraction for Swift (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::SwiftPlugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{extract_name_from_node, node_to_location};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../swift-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
///
/// Note: tree-sitter-swift folds `struct`/`enum`/`extension` into `class_declaration`
/// (keyword child). Return/break/continue/throw share `control_transfer_statement`.
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("additive_expression", HandlerMask::CFG_STMT),
    ("as_expression", HandlerMask::CFG_STMT),
    ("associatedtype_declaration", HandlerMask::SYMBOL),
    ("await_expression", HandlerMask::CFG_STMT),
    ("call_expression", HandlerMask::RELATION),
    ("catch_block", HandlerMask::CFG_STMT),
    ("class_declaration", HandlerMask::SYMBOL),
    ("comparison_expression", HandlerMask::CFG_STMT),
    ("conjunction_expression", HandlerMask::CFG_STMT),
    ("control_transfer_statement", HandlerMask::CFG_STMT),
    ("deinit_declaration", HandlerMask::SYMBOL),
    ("disjunction_expression", HandlerMask::CFG_STMT),
    ("do_statement", HandlerMask::CFG_STMT),
    ("enum_entry", HandlerMask::SYMBOL),
    ("equality_expression", HandlerMask::CFG_STMT),
    ("for_statement", HandlerMask::CFG_STMT),
    ("function_body", HandlerMask::CFG_STMT),
    ("function_declaration", HandlerMask::SYMBOL),
    ("guard_statement", HandlerMask::CFG_STMT),
    ("if_statement", HandlerMask::CFG_STMT),
    ("import_declaration", HandlerMask::RELATION),
    ("init_declaration", HandlerMask::SYMBOL),
    ("lambda_literal", HandlerMask::SYMBOL),
    ("navigation_expression", HandlerMask::RELATION),
    ("property_declaration", HandlerMask::SYMBOL),
    ("protocol_declaration", HandlerMask::SYMBOL),
    ("protocol_function_declaration", HandlerMask::SYMBOL),
    ("protocol_property_declaration", HandlerMask::SYMBOL),
    ("range_expression", HandlerMask::CFG_STMT),
    ("repeat_while_statement", HandlerMask::CFG_STMT),
    ("subscript_declaration", HandlerMask::SYMBOL),
    ("switch_statement", HandlerMask::CFG_STMT),
    ("ternary_expression", HandlerMask::CFG_STMT),
    ("try_expression", HandlerMask::CFG_STMT),
    ("typealias_declaration", HandlerMask::SYMBOL),
    ("while_statement", HandlerMask::CFG_STMT),
    ("inheritance_specifier", HandlerMask::RELATION),
];

const SWIFT_CALL_KINDS: &[&str] = &["call_expression"];
const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn swift_grammar() -> tree_sitter::Language {
    tree_sitter_swift::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn swift_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&swift_grammar(), HANDLED_KINDS))
}

fn swift_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&swift_grammar(), MANIFEST_JSON))
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

fn class_decl_symbol_type(node: Node) -> (SymbolType, &'static str) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "struct" => return (SymbolType::Struct, "struct"),
            "enum" => return (SymbolType::Enum, "enum"),
            "extension" => return (SymbolType::Class, "extension"),
            "class" => return (SymbolType::Class, "class"),
            "actor" => return (SymbolType::Class, "actor"),
            _ => {}
        }
    }
    (SymbolType::Class, "class")
}

/// Callee name for a `call_expression` — member segment, not the receiver.
fn swift_call_callee(node: Node, source: &[u8]) -> Option<String> {
    let nav = node.child_by_field_name("function").or_else(|| {
        let mut c = node.walk();
        node.children(&mut c).find(|ch| {
            matches!(
                ch.kind(),
                "navigation_expression" | "member_expression" | "prefix_expression"
            )
        })
    });

    if let Some(nav) = nav {
        // Prefer `navigation_suffix` / member segment directly (avoid LIFO DFS that
        // overwrites with the receiver, and skip `value_arguments` of chained calls).
        if let Some(suffix) = nav.child_by_field_name("suffix").or_else(|| {
            let mut c = nav.walk();
            nav.children(&mut c).find(|ch| {
                matches!(
                    ch.kind(),
                    "navigation_suffix" | "suffix" | "member_expression"
                )
            })
        }) {
            if let Some(id) = identifier_in(suffix, source) {
                return Some(id);
            }
        }
        // Bare call `baz()` — function child is the identifier itself.
        if matches!(
            nav.kind(),
            "simple_identifier" | "identifier" | "type_identifier"
        ) {
            return nav.utf8_text(source).ok().map(str::to_string);
        }
        // Walk children in reverse, skipping argument lists.
        let mut last = None;
        let mut stack = vec![nav];
        while let Some(n) = stack.pop() {
            if matches!(
                n.kind(),
                "value_arguments" | "call_suffix" | "lambda_literal" | "tuple_expression"
            ) {
                continue;
            }
            if matches!(
                n.kind(),
                "simple_identifier" | "identifier" | "type_identifier"
            ) {
                last = n.utf8_text(source).ok().map(str::to_string);
            }
            let mut c = n.walk();
            let children: Vec<_> = n.children(&mut c).collect();
            for ch in children.into_iter().rev() {
                stack.push(ch);
            }
        }
        if last.is_some() {
            return last;
        }
    }
    identifier_in(node, source)
}

fn identifier_in(node: Node, source: &[u8]) -> Option<String> {
    if matches!(
        node.kind(),
        "simple_identifier" | "identifier" | "type_identifier"
    ) {
        return node.utf8_text(source).ok().map(str::to_string);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "simple_identifier" | "identifier" | "type_identifier"
        ) {
            return child.utf8_text(source).ok().map(str::to_string);
        }
    }
    None
}

fn type_decl_name(node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "type_identifier" | "simple_identifier" | "identifier" => {
                return child.utf8_text(source).ok().map(str::to_string);
            }
            "user_type" => {
                let mut uc = child.walk();
                for u in child.children(&mut uc) {
                    if u.kind() == "type_identifier" {
                        return u.utf8_text(source).ok().map(str::to_string);
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Property / binding name from `pattern` (not the type annotation).
fn property_decl_name(node: Node, source: &[u8]) -> Option<String> {
    if let Some(pattern) = node.child_by_field_name("pattern").or_else(|| {
        let mut c = node.walk();
        node.children(&mut c).find(|ch| {
            matches!(
                ch.kind(),
                "pattern" | "bound_identifier" | "value_binding_pattern" | "wildcard_pattern"
            )
        })
    }) {
        if let Some(id) = identifier_in(pattern, source) {
            return Some(id);
        }
        // Recurse one level for `bound_identifier` / nested patterns.
        let mut c = pattern.walk();
        for ch in pattern.children(&mut c) {
            if let Some(id) = identifier_in(ch, source) {
                return Some(id);
            }
        }
    }
    // Fallback: first bound_identifier / simple_identifier, skipping type nodes.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "type_annotation" | "user_type" | "type_identifier" | "modifiers"
        ) {
            continue;
        }
        if let Some(id) = identifier_in(child, source) {
            return Some(id);
        }
        let mut ic = child.walk();
        for ch in child.children(&mut ic) {
            if matches!(ch.kind(), "type_identifier" | "user_type") {
                continue;
            }
            if let Some(id) = identifier_in(ch, source) {
                return Some(id);
            }
        }
    }
    None
}

fn push_named_symbol(
    ctx: &mut SinglePassCtx,
    node: Node,
    source: &[u8],
    file_path: &str,
    symbol_type: SymbolType,
    kind_meta: &str,
    enter: Option<ScopeKind>,
) -> bool {
    let Some(name) = type_decl_name(node, source)
        .or_else(|| extract_name_from_node(node, source).ok().flatten())
    else {
        return false;
    };
    let owner = ctx
        .scopes
        .current_type()
        .map(|t| t.qualified_prefix.clone());
    let qn = match owner.as_deref() {
        Some(o) => format!("{o}.{name}"),
        None => name.clone(),
    };
    let loc = ((node.end_position().row - node.start_position().row) + 1).max(1) as u32;
    ctx.symbols
        .push(rgctl_plugin_helpers::callable_identity::symbol(
            Symbol {
                name: name.clone(),
                symbol_type,
                qualified_name: Some(qn.clone()),
                location: node_to_location(node, file_path),
                signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: serde_json::json!({
                    "language": "swift",
                    "kind": kind_meta,
                }),
                ..Default::default()
            },
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Swift,
        ));
    if let Some(scope_kind) = enter {
        let is_fn = scope_kind.is_function_like();
        ctx.enter_scope(
            scope_kind,
            Some(name),
            qn,
            node.start_byte(),
            node.end_byte(),
            true,
        );
        if is_fn {
            if let Some(f) = ctx.scopes.current_function_mut() {
                f.complexity.cyclomatic = 1;
                f.complexity.loc = loc;
            }
        }
        return true;
    }
    false
}

/// Single iterative DFS: symbols + deferred calls + complexity.
pub fn extract_all_single_pass(
    _plugin: &SwiftPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree =
        parse_with_pool("swift", swift_grammar(), source).map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: e,
        })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = swift_dispatch();

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
                    "inheritance_specifier" => {
                        if let (Some(owner), Some(parent)) = (
                            ctx.scopes.current_type(),
                            node.child_by_field_name("inherits_from"),
                        ) {
                            let parent = parent.utf8_text(source)?.to_string();
                            relations.push(Relation {
                                from: owner.qualified_prefix.clone(),
                                to: rgctl_plugin_helpers::simple_type_name(&parent),
                                relation_type: RelationType::Extends,
                                location: node_to_location(node, &file_path_str),
                                metadata: serde_json::json!({"language":"swift"}),
                                to_qualified_hint: Some(parent),
                                to_type_hint: None,
                            });
                        }
                    }
                    "class_declaration" => {
                        let (sym_ty, meta) = class_decl_symbol_type(node);
                        let scope = match sym_ty {
                            SymbolType::Enum => ScopeKind::Enum,
                            SymbolType::Interface => ScopeKind::Interface,
                            _ => ScopeKind::Class,
                        };
                        pushed_scope = push_named_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            sym_ty,
                            meta,
                            Some(scope),
                        );
                    }
                    "protocol_declaration" => {
                        pushed_scope = push_named_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Interface,
                            "protocol",
                            Some(ScopeKind::Interface),
                        );
                    }
                    "function_declaration"
                    | "init_declaration"
                    | "deinit_declaration"
                    | "subscript_declaration" => {
                        let (name, meta) = match kind {
                            "init_declaration" => ("init".to_string(), "init"),
                            "deinit_declaration" => ("deinit".to_string(), "deinit"),
                            "subscript_declaration" => ("subscript".to_string(), "subscript"),
                            _ => {
                                // Prefer direct simple_identifier; avoid body recursion.
                                let n = {
                                    let mut c = node.walk();
                                    node.children(&mut c)
                                        .find(|ch| {
                                            matches!(ch.kind(), "simple_identifier" | "identifier")
                                        })
                                        .and_then(|ch| {
                                            ch.utf8_text(source).ok().map(str::to_string)
                                        })
                                        .or_else(|| {
                                            extract_name_from_node(node, source).ok().flatten()
                                        })
                                        .unwrap_or_else(|| "func".to_string())
                                };
                                (n, "function")
                            }
                        };
                        let owner = ctx
                            .scopes
                            .current_type()
                            .map(|t| t.qualified_prefix.clone());
                        let qn = match owner.as_deref() {
                            Some(o) => format!("{o}.{name}"),
                            None => name.clone(),
                        };
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
                        ctx.symbols
                            .push(rgctl_plugin_helpers::callable_identity::symbol(
                                Symbol {
                                    name: name.clone(),
                                    symbol_type: SymbolType::Function,
                                    qualified_name: Some(qn.clone()),
                                    location: node_to_location(node, &file_path_str),
                                    signature: node
                                        .utf8_text(source)
                                        .ok()
                                        .map(|s| s.trim().to_string()),
                                    return_type: None,
                                    parameters: vec![],
                                    fields: vec![],
                                    modifiers: vec![],
                                    documentation: None,
                                    metadata: serde_json::json!({
                                        "language": "swift",
                                        "kind": meta,
                                    }),
                                    ..Default::default()
                                },
                                node,
                                source,
                                rgctl_plugin_helpers::callable_identity::Policy::Swift,
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
                        pushed_scope = true;
                    }
                    "property_declaration" => {
                        if let Some(name) = property_decl_name(node, source) {
                            let owner = ctx
                                .scopes
                                .current_type()
                                .map(|t| t.qualified_prefix.clone());
                            let qn = match owner.as_deref() {
                                Some(o) => format!("{o}.{name}"),
                                None => name.clone(),
                            };
                            ctx.symbols.push(Symbol {
                                name,
                                symbol_type: SymbolType::Variable,
                                qualified_name: Some(qn),
                                location: node_to_location(node, &file_path_str),
                                signature: node
                                    .utf8_text(source)
                                    .ok()
                                    .map(|s| s.trim().to_string()),
                                return_type: None,
                                parameters: vec![],
                                fields: vec![],
                                modifiers: vec![],
                                documentation: None,
                                metadata: serde_json::json!({
                                    "language": "swift",
                                    "kind": "property",
                                }),
                                ..Default::default()
                            });
                        }
                    }
                    "typealias_declaration" | "associatedtype_declaration" => {
                        let meta = if kind == "associatedtype_declaration" {
                            "associatedtype"
                        } else {
                            "typealias"
                        };
                        let _ = push_named_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::TypeAlias,
                            meta,
                            None,
                        );
                    }
                    "enum_entry" => {
                        if let Some(name) = type_decl_name(node, source)
                            .or_else(|| extract_name_from_node(node, source).ok().flatten())
                        {
                            let owner = ctx
                                .scopes
                                .current_type()
                                .map(|t| t.qualified_prefix.clone());
                            let qn = match owner.as_deref() {
                                Some(o) => format!("{o}.{name}"),
                                None => name.clone(),
                            };
                            ctx.symbols.push(Symbol {
                                name,
                                symbol_type: SymbolType::Enum,
                                qualified_name: Some(qn),
                                location: node_to_location(node, &file_path_str),
                                signature: node
                                    .utf8_text(source)
                                    .ok()
                                    .map(|s| s.trim().to_string()),
                                return_type: None,
                                parameters: vec![],
                                fields: vec![],
                                modifiers: vec![],
                                documentation: None,
                                metadata: serde_json::json!({
                                    "language": "swift",
                                    "kind": "enum_entry",
                                }),
                                ..Default::default()
                            });
                        }
                    }
                    "protocol_function_declaration" => {
                        let name = {
                            let mut c = node.walk();
                            node.children(&mut c)
                                .find(|ch| matches!(ch.kind(), "simple_identifier" | "identifier"))
                                .and_then(|ch| ch.utf8_text(source).ok().map(str::to_string))
                                .or_else(|| extract_name_from_node(node, source).ok().flatten())
                                .unwrap_or_else(|| "func".to_string())
                        };
                        let owner = ctx
                            .scopes
                            .current_type()
                            .map(|t| t.qualified_prefix.clone());
                        let qn = match owner.as_deref() {
                            Some(o) => format!("{o}.{name}"),
                            None => name.clone(),
                        };
                        ctx.symbols
                            .push(rgctl_plugin_helpers::callable_identity::symbol(
                                Symbol {
                                    name,
                                    symbol_type: SymbolType::Function,
                                    qualified_name: Some(qn),
                                    location: node_to_location(node, &file_path_str),
                                    signature: node
                                        .utf8_text(source)
                                        .ok()
                                        .map(|s| s.trim().to_string()),
                                    return_type: None,
                                    parameters: vec![],
                                    fields: vec![],
                                    modifiers: vec![],
                                    documentation: None,
                                    metadata: serde_json::json!({
                                        "language": "swift",
                                        "kind": "protocol_function",
                                    }),
                                    ..Default::default()
                                },
                                node,
                                source,
                                rgctl_plugin_helpers::callable_identity::Policy::Swift,
                            ));
                    }
                    "protocol_property_declaration" => {
                        if let Some(name) = property_decl_name(node, source)
                            .or_else(|| type_decl_name(node, source))
                            .or_else(|| extract_name_from_node(node, source).ok().flatten())
                        {
                            let owner = ctx
                                .scopes
                                .current_type()
                                .map(|t| t.qualified_prefix.clone());
                            let qn = match owner.as_deref() {
                                Some(o) => format!("{o}.{name}"),
                                None => name.clone(),
                            };
                            ctx.symbols.push(Symbol {
                                name,
                                symbol_type: SymbolType::Variable,
                                qualified_name: Some(qn),
                                location: node_to_location(node, &file_path_str),
                                signature: node
                                    .utf8_text(source)
                                    .ok()
                                    .map(|s| s.trim().to_string()),
                                return_type: None,
                                parameters: vec![],
                                fields: vec![],
                                modifiers: vec![],
                                documentation: None,
                                metadata: serde_json::json!({
                                    "language": "swift",
                                    "kind": "protocol_property",
                                }),
                                ..Default::default()
                            });
                        }
                    }
                    "lambda_literal" => {
                        let owner = ctx
                            .scopes
                            .current_function()
                            .map(|f| f.qualified_prefix.clone())
                            .or_else(|| {
                                ctx.scopes
                                    .current_type()
                                    .map(|t| t.qualified_prefix.clone())
                            });
                        let idx = ctx.symbols.len();
                        let name = format!("$closure{idx}");
                        let qn = match owner.as_deref() {
                            Some(o) => format!("{o}.{name}"),
                            None => name.clone(),
                        };
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
                        ctx.symbols
                            .push(rgctl_plugin_helpers::callable_identity::symbol(
                                Symbol {
                                    name: name.clone(),
                                    symbol_type: SymbolType::Function,
                                    qualified_name: Some(qn.clone()),
                                    location: node_to_location(node, &file_path_str),
                                    signature: node
                                        .utf8_text(source)
                                        .ok()
                                        .map(|s| s.trim().to_string()),
                                    return_type: None,
                                    parameters: vec![],
                                    fields: vec![],
                                    modifiers: vec![],
                                    documentation: None,
                                    metadata: serde_json::json!({
                                        "language": "swift",
                                        "kind": "lambda",
                                    }),
                                    ..Default::default()
                                },
                                node,
                                source,
                                rgctl_plugin_helpers::callable_identity::Policy::Swift,
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
                        pushed_scope = true;
                    }
                    "import_declaration" => {
                        if let Some(name) = extract_name_from_node(node, source).ok().flatten() {
                            ctx.symbols.push(Symbol {
                                name: name.clone(),
                                symbol_type: SymbolType::Import,
                                qualified_name: Some(name),
                                location: node_to_location(node, &file_path_str),
                                signature: None,
                                return_type: None,
                                parameters: vec![],
                                fields: vec![],
                                modifiers: vec![],
                                documentation: None,
                                metadata: serde_json::json!({
                                    "language": "swift",
                                    "kind": "import",
                                }),
                                ..Default::default()
                            });
                        }
                    }
                    _ => {}
                }

                if SWIFT_CALL_KINDS.contains(&kind) {
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
                        "if_statement"
                        | "guard_statement"
                        | "switch_statement"
                        | "for_statement"
                        | "while_statement"
                        | "repeat_while_statement"
                        | "do_statement"
                        | "catch_block"
                        | "ternary_expression"
                        | "conjunction_expression"
                        | "disjunction_expression"
                        | "comparison_expression"
                        | "equality_expression"
                        | "try_expression"
                        | "await_expression"
                        | "as_expression"
                        | "range_expression"
                        | "additive_expression" => {
                            let delta = 1 + ctx
                                .scopes
                                .current_function()
                                .map(|f| f.complexity.cognitive_nesting)
                                .unwrap_or(0);
                            ctx.scopes.note_branch(delta);
                            // Only enter nesting for statement-level CFG, not every expr.
                            if matches!(
                                kind,
                                "if_statement"
                                    | "guard_statement"
                                    | "switch_statement"
                                    | "for_statement"
                                    | "while_statement"
                                    | "repeat_while_statement"
                                    | "do_statement"
                                    | "catch_block"
                            ) {
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
                        "control_transfer_statement" => {
                            let text = node.utf8_text(source).unwrap_or("").trim_start();
                            if text.starts_with("return") || text.starts_with("throw") {
                                ctx.scopes.note_return();
                            }
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
        let Some(callee) = swift_call_callee(pending.node, source) else {
            // Fallback for grammars that expose a `function` field.
            push_call_relation_from(
                pending.node,
                source,
                file_path,
                &ctx.symbols,
                from_fn,
                "swift",
                &mut relations,
            );
            continue;
        };
        let from = from_fn
            .qualified_name
            .clone()
            .unwrap_or_else(|| from_fn.name.clone());
        let same_file: Vec<_> = ctx
            .symbols
            .iter()
            .filter(|s| {
                s.name == callee
                    && s.symbol_type == SymbolType::Function
                    && s.location.file == file_path_str
            })
            .collect();
        let to = match same_file.as_slice() {
            [only] => only
                .qualified_name
                .clone()
                .unwrap_or_else(|| callee.clone()),
            _ => callee.clone(),
        };
        relations.push(Relation {
            from,
            to,
            relation_type: RelationType::Calls,
            location: node_to_location(pending.node, &file_path_str),
            metadata: serde_json::json!({ "language": "swift" }),
            to_qualified_hint: Some(callee),
            to_type_hint: None,
            ..Default::default()
        });
        rgctl_plugin_helpers::callable_identity::call(
            pending.node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Swift,
            ctx.symbols.get(pending.from_symbol_idx),
            &mut relations[identity_relation_start..],
        );
    }

    rgctl_plugin_helpers::callable_identity::finish(
        rgctl_plugin_helpers::callable_identity::Policy::Swift,
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
    fn swift_extracts_class_function_call_and_complexity() {
        let src = r#"
class Foo {
  func bar() {
    if true { baz() }
    return
  }
}
"#;
        let plugin = SwiftPlugin::new().unwrap();
        let path = PathBuf::from("t.swift");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Class && s.name == "Foo"),
            "expected class Foo, got {:?}",
            result.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Function && s.name == "bar"),
            "expected function bar"
        );
        assert!(
            result
                .relations
                .iter()
                .any(|r| r.relation_type == RelationType::Calls),
            "expected Calls relation, got {:?}",
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
    fn swift_member_call_uses_callee_not_receiver() {
        let src = r#"
class Foo {
  func bar() {
    self.baz()
    client.fetch()
  }
  func baz() {}
}
"#;
        let plugin = SwiftPlugin::new().unwrap();
        let path = PathBuf::from("t.swift");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        let callees: Vec<_> = result
            .relations
            .iter()
            .filter(|r| r.relation_type == RelationType::Calls)
            .map(|r| r.to_qualified_hint.as_deref().unwrap_or(r.to.as_str()))
            .collect();
        assert!(
            callees.iter().any(|c| *c == "baz"),
            "expected callee baz, got {callees:?}"
        );
        assert!(
            callees.iter().any(|c| *c == "fetch"),
            "expected callee fetch, got {callees:?}"
        );
        assert!(
            !callees.iter().any(|c| *c == "self" || *c == "client"),
            "receiver must not be callee, got {callees:?}"
        );
    }

    #[test]
    fn swift_typed_property_uses_binding_name_not_type() {
        let src = r#"
class Foo {
  var count: Int = 0
  var label: String = ""
}
"#;
        let plugin = SwiftPlugin::new().unwrap();
        let path = PathBuf::from("t.swift");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        let props: Vec<_> = result
            .symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Variable)
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            props.contains(&"count") && props.contains(&"label"),
            "expected property names count/label, got {props:?}"
        );
        assert!(
            !props.iter().any(|p| *p == "Int" || *p == "String"),
            "must not extract type annotation as name, got {props:?}"
        );
    }
}
