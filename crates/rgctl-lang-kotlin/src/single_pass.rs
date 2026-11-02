//! Single-pass AST extraction for Kotlin (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::KotlinPlugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../kotlin-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("annotated_expression", HandlerMask::AST_SKELETON),
    ("annotated_lambda", HandlerMask::AST_SKELETON),
    ("annotation", HandlerMask::RELATION),
    ("anonymous_function", HandlerMask::SYMBOL),
    ("anonymous_initializer", HandlerMask::SYMBOL),
    ("as_expression", HandlerMask::CFG_STMT),
    ("assignment", HandlerMask::AST_SKELETON),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("block", HandlerMask::CFG_STMT),
    ("call_expression", HandlerMask::RELATION),
    ("callable_reference", HandlerMask::RELATION),
    ("catch_block", HandlerMask::CFG_STMT),
    ("class_declaration", HandlerMask::SYMBOL),
    ("class_parameter", HandlerMask::SYMBOL),
    ("class_parameters", HandlerMask::AST_SKELETON),
    ("companion_object", HandlerMask::SYMBOL),
    ("constructor_delegation_call", HandlerMask::RELATION),
    ("constructor_invocation", HandlerMask::RELATION),
    ("delegation_specifier", HandlerMask::RELATION),
    ("delegation_specifiers", HandlerMask::RELATION),
    ("do_while_statement", HandlerMask::CFG_STMT),
    ("enum_entry", HandlerMask::SYMBOL),
    ("explicit_delegation", HandlerMask::RELATION),
    ("file_annotation", HandlerMask::RELATION),
    ("finally_block", HandlerMask::CFG_STMT),
    ("for_statement", HandlerMask::CFG_STMT),
    ("function_body", HandlerMask::CFG_STMT),
    ("function_declaration", HandlerMask::SYMBOL),
    ("function_value_parameters", HandlerMask::AST_SKELETON),
    ("getter", HandlerMask::SYMBOL),
    ("if_expression", HandlerMask::CFG_STMT),
    ("import", HandlerMask::RELATION),
    ("in_expression", HandlerMask::CFG_STMT),
    ("index_expression", HandlerMask::RELATION),
    ("infix_expression", HandlerMask::RELATION),
    ("is_expression", HandlerMask::CFG_STMT),
    ("labeled_expression", HandlerMask::CFG_STMT),
    ("lambda_literal", HandlerMask::SYMBOL),
    ("lambda_parameters", HandlerMask::AST_SKELETON),
    ("multi_variable_declaration", HandlerMask::AST_SKELETON),
    ("navigation_expression", HandlerMask::RELATION),
    ("object_declaration", HandlerMask::SYMBOL),
    ("object_literal", HandlerMask::SYMBOL),
    ("parameter", HandlerMask::AST_SKELETON),
    ("primary_constructor", HandlerMask::SYMBOL),
    ("property_declaration", HandlerMask::SYMBOL),
    ("property_delegate", HandlerMask::AST_SKELETON),
    ("range_expression", HandlerMask::CFG_STMT),
    ("return_expression", HandlerMask::CFG_STMT),
    ("secondary_constructor", HandlerMask::SYMBOL),
    ("setter", HandlerMask::SYMBOL),
    ("spread_expression", HandlerMask::AST_SKELETON),
    ("throw_expression", HandlerMask::CFG_STMT),
    ("try_expression", HandlerMask::CFG_STMT),
    ("type_alias", HandlerMask::SYMBOL),
    ("type_arguments", HandlerMask::AST_SKELETON),
    ("type_constraint", HandlerMask::AST_SKELETON),
    ("type_parameter", HandlerMask::AST_SKELETON),
    ("type_parameters", HandlerMask::AST_SKELETON),
    ("unary_expression", HandlerMask::AST_SKELETON),
    ("variable_declaration", HandlerMask::AST_SKELETON),
    ("when_entry", HandlerMask::CFG_STMT),
    ("when_expression", HandlerMask::CFG_STMT),
    ("while_statement", HandlerMask::CFG_STMT),
];

/// Call-like nodes flushed as `Calls` / `References` after scope walk.
const KOTLIN_RELATION_CALL_KINDS: &[&str] = &[
    "call_expression",
    "callable_reference",
    "constructor_delegation_call",
    "constructor_invocation",
    "index_expression",
    "infix_expression",
    "navigation_expression",
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn kotlin_grammar() -> tree_sitter::Language {
    tree_sitter_kotlin_ng::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn kotlin_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&kotlin_grammar(), HANDLED_KINDS))
}

fn kotlin_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&kotlin_grammar(), MANIFEST_JSON))
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

/// Single iterative DFS: symbols + deferred calls + complexity; then inheritance.
pub fn extract_all_single_pass(
    plugin: &KotlinPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree =
        parse_with_pool("kotlin", kotlin_grammar(), source).map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: e,
        })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let package = KotlinPlugin::package_name(root, source);
    let dispatch = kotlin_dispatch();

    let mut ctx = SinglePassCtx::new(dispatch);
    let mut relations = Vec::new();
    let mut pending_calls: Vec<PendingCall<'_>> = Vec::new();
    let mut lambda_idx: u32 = 0;
    let mut anon_fn_idx: u32 = 0;
    let mut object_lit_idx: u32 = 0;
    let mut init_idx: u32 = 0;

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
                    "class_declaration" | "object_declaration" | "companion_object" => {
                        let before = ctx.symbols.len();
                        plugin.emit_type_and_ctors(
                            node,
                            source,
                            &file_path_str,
                            package.as_deref(),
                            &mut ctx.symbols,
                        );
                        if ctx.symbols.len() > before {
                            let type_sym = &ctx.symbols[before];
                            let name = type_sym.name.clone();
                            let qn = type_sym
                                .qualified_name
                                .clone()
                                .unwrap_or_else(|| name.clone());
                            let scope_kind = match type_sym.symbol_type {
                                SymbolType::Interface => ScopeKind::Interface,
                                SymbolType::Enum => ScopeKind::Enum,
                                _ => ScopeKind::Class,
                            };
                            let depth_s = ctx.scopes.depth();
                            let mut frame = ScopeFrame::new(
                                scope_kind,
                                Some(name),
                                qn,
                                node.start_byte(),
                                node.end_byte(),
                                depth_s,
                            );
                            frame.symbol_idx = Some(before);
                            ctx.scopes.push(frame);
                            pushed_scope = true;
                        }
                    }
                    "object_literal" => {
                        object_lit_idx += 1;
                        if let Some(sym) = plugin.emit_object_literal(
                            node,
                            source,
                            &file_path_str,
                            package.as_deref(),
                            object_lit_idx,
                        ) {
                            let name = sym.name.clone();
                            let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                            ctx.symbols.push(sym);
                            let depth_s = ctx.scopes.depth();
                            let mut frame = ScopeFrame::new(
                                ScopeKind::Class,
                                Some(name),
                                qn,
                                node.start_byte(),
                                node.end_byte(),
                                depth_s,
                            );
                            frame.symbol_idx = Some(ctx.symbols.len() - 1);
                            ctx.scopes.push(frame);
                            pushed_scope = true;
                        }
                    }
                    "function_declaration" => {
                        if let Some(sym) =
                            plugin.emit_function(node, source, &file_path_str, package.as_deref())
                        {
                            let params = sym.parameters.len() as u32;
                            let loc = ((node.end_position().row - node.start_position().row) + 1)
                                .max(1) as u32;
                            let name = sym.name.clone();
                            let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
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
                                f.complexity.param_count = params;
                                f.complexity.loc = loc;
                            }
                            pushed_scope = true;
                        }
                    }
                    "lambda_literal" => {
                        lambda_idx += 1;
                        if let Some(sym) = plugin.emit_lambda_like(
                            node,
                            source,
                            &file_path_str,
                            package.as_deref(),
                            lambda_idx,
                            "lambda",
                        ) {
                            let name = sym.name.clone();
                            let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
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
                            }
                            pushed_scope = true;
                        }
                    }
                    "anonymous_function" => {
                        anon_fn_idx += 1;
                        if let Some(sym) = plugin.emit_lambda_like(
                            node,
                            source,
                            &file_path_str,
                            package.as_deref(),
                            anon_fn_idx,
                            "anon_fn",
                        ) {
                            let name = sym.name.clone();
                            let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
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
                            }
                            pushed_scope = true;
                        }
                    }
                    "anonymous_initializer" => {
                        init_idx += 1;
                        if let Some(sym) = plugin.emit_anonymous_initializer(
                            node,
                            source,
                            &file_path_str,
                            package.as_deref(),
                            init_idx,
                        ) {
                            let name = sym.name.clone();
                            let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
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
                            }
                            pushed_scope = true;
                        }
                    }
                    "primary_constructor" | "secondary_constructor" => {
                        // Ctors are emitted with the enclosing type; bind scope by span.
                        let start_line = node.start_position().row + 1;
                        if let Some(idx) = ctx.symbols.iter().position(|s| {
                            s.location.start_line == start_line
                                && s.metadata.get("is_constructor")
                                    == Some(&serde_json::json!(true))
                        }) {
                            let name = ctx.symbols[idx].name.clone();
                            let qn = ctx.symbols[idx]
                                .qualified_name
                                .clone()
                                .unwrap_or_else(|| name.clone());
                            let params = ctx.symbols[idx].parameters.len() as u32;
                            let loc = ((node.end_position().row - node.start_position().row) + 1)
                                .max(1) as u32;
                            let depth_s = ctx.scopes.depth();
                            let mut frame = ScopeFrame::new(
                                ScopeKind::Function,
                                Some(name),
                                qn,
                                node.start_byte(),
                                node.end_byte(),
                                depth_s,
                            );
                            frame.symbol_idx = Some(idx);
                            frame.complexity.cyclomatic = 1;
                            frame.complexity.param_count = params;
                            frame.complexity.loc = loc;
                            ctx.scopes.push(frame);
                            pushed_scope = true;
                        }
                    }
                    "import" => {
                        plugin.emit_import(node, source, &file_path_str, &mut ctx.symbols);
                    }
                    "annotation" | "file_annotation" => {
                        plugin.emit_annotation_relation(
                            node,
                            source,
                            file_path,
                            &ctx.symbols,
                            &mut relations,
                        );
                    }
                    "explicit_delegation" | "delegation_specifiers" => {
                        // Edges are also collected via delegation_specifier walk; keep visit active.
                    }
                    "type_alias" => {
                        // Grammar: `typealias` keyword, then `identifier` name, then aliased type.
                        let mut name = None;
                        let mut aliased = None;
                        let mut c = node.walk();
                        for ch in node.children(&mut c) {
                            match ch.kind() {
                                "identifier" | "simple_identifier" | "type_identifier"
                                    if name.is_none() =>
                                {
                                    name = ch.utf8_text(source).ok().map(|s| s.trim().to_string());
                                }
                                "user_type" | "nullable_type" | "function_type" => {
                                    aliased =
                                        ch.utf8_text(source).ok().map(|s| s.trim().to_string());
                                }
                                _ => {}
                            }
                        }
                        if let Some(name) = name {
                            ctx.symbols.push(Symbol {
                                name: name.clone(),
                                symbol_type: SymbolType::TypeAlias,
                                qualified_name: Some(
                                    package
                                        .as_ref()
                                        .map(|p| format!("{p}.{name}"))
                                        .unwrap_or(name),
                                ),
                                location: SourceLocation {
                                    file: file_path_str.to_string(),
                                    start_line: node.start_position().row + 1,
                                    end_line: node.end_position().row + 1,
                                    start_column: node.start_position().column,
                                    end_column: node.end_position().column,
                                },
                                signature: node
                                    .utf8_text(source)
                                    .ok()
                                    .map(|s| s.trim().to_string()),
                                return_type: aliased,
                                parameters: vec![],
                                fields: vec![],
                                modifiers: vec![],
                                documentation: None,
                                metadata: serde_json::json!({
                                    "language": "kotlin",
                                    "kind": "type_alias",
                                }),
                            });
                        }
                    }
                    _ => {}
                }

                if KOTLIN_RELATION_CALL_KINDS.contains(&kind)
                    && let Some(frame) = ctx.scopes.current_function()
                    && let Some(idx) = frame.symbol_idx
                {
                    pending_calls.push(PendingCall {
                        node,
                        from_symbol_idx: idx,
                    });
                }

                if ctx.scopes.current_function().is_some() {
                    match kind {
                        "if_expression" | "when_expression" | "while_statement"
                        | "for_statement" | "do_while_statement" | "catch_block" => {
                            let delta = 1 + ctx
                                .scopes
                                .current_function()
                                .map(|f| f.complexity.cognitive_nesting)
                                .unwrap_or(0);
                            ctx.scopes.note_branch(delta);
                            ctx.scopes.enter_branch();
                            let depth_s = ctx.scopes.depth();
                            ctx.scopes.push(ScopeFrame::new(
                                ScopeKind::GuardBlock,
                                None,
                                String::new(),
                                node.start_byte(),
                                node.end_byte(),
                                depth_s,
                            ));
                            stack.push(Visit::LeaveGuard);
                        }
                        "when_entry" | "is_expression" | "in_expression" | "as_expression"
                        | "range_expression" | "labeled_expression" => {
                            let delta = 1 + ctx
                                .scopes
                                .current_function()
                                .map(|f| f.complexity.cognitive_nesting)
                                .unwrap_or(0);
                            ctx.scopes.note_branch(delta);
                        }
                        "binary_expression" => {
                            if let Ok(text) = node.utf8_text(source) {
                                if text.contains("&&") || text.contains("||") {
                                    ctx.scopes.note_branch(1);
                                }
                            }
                        }
                        "return_expression" | "jump_expression"
                            if (kind == "return_expression"
                                || node
                                    .utf8_text(source)
                                    .ok()
                                    .is_some_and(|t| t.trim_start().starts_with("return"))) =>
                        {
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
        let Some(from_fn) = ctx.symbols.get(pending.from_symbol_idx) else {
            continue;
        };
        let kind = pending.node.kind();
        if kind == "call_expression" || kind == "constructor_invocation" {
            push_call_relation_from(
                pending.node,
                source,
                file_path,
                &ctx.symbols,
                from_fn,
                "kotlin",
                &mut relations,
            );
        } else {
            plugin.emit_reference_relation(
                pending.node,
                source,
                file_path,
                from_fn,
                &mut relations,
            );
        }
    }

    plugin.collect_inheritance_relations(root, source, file_path, &mut relations);

    rgctl_plugin_helpers::apply_framework_catalog(
        "kotlin",
        source,
        Some(root),
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}
