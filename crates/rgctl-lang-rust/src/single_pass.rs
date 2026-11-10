//! Single-pass AST extraction for Rust (`ScopeStack` + `KindDispatchTable`).

use crate::extract_depth::{
    AST_MAX_DEPTH, extract_associated_type, extract_enum_variant_symbol, extract_extern_crate,
    extract_macro_definition, extract_mod_symbol, extract_trait_symbols, extract_type_item,
    extract_use_symbols, module_prefix_from_path, visit_depth_relation_node,
};
use crate::plugin::RustPlugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../rust-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("array_expression", HandlerMask::AST_SKELETON),
    ("assignment_expression", HandlerMask::AST_SKELETON),
    ("associated_type", HandlerMask::SYMBOL),
    ("async_block", HandlerMask::CFG_STMT),
    ("attribute", HandlerMask::RELATION),
    ("attribute_item", HandlerMask::RELATION),
    ("await_expression", HandlerMask::CFG_STMT),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("block", HandlerMask::CFG_STMT),
    ("break_expression", HandlerMask::CFG_STMT),
    ("call_expression", HandlerMask::RELATION),
    ("captured_pattern", HandlerMask::AST_SKELETON),
    ("closure_expression", HandlerMask::CFG_STMT),
    ("closure_parameters", HandlerMask::AST_SKELETON),
    ("compound_assignment_expr", HandlerMask::AST_SKELETON),
    ("const_item", HandlerMask::SYMBOL),
    ("continue_expression", HandlerMask::CFG_STMT),
    ("else_clause", HandlerMask::CFG_STMT),
    ("empty_statement", HandlerMask::CFG_STMT),
    ("enum_item", HandlerMask::SYMBOL),
    ("enum_variant", HandlerMask::SYMBOL),
    ("extern_crate_declaration", HandlerMask::SYMBOL),
    ("field_declaration", HandlerMask::SYMBOL),
    ("field_expression", HandlerMask::RELATION),
    ("field_pattern", HandlerMask::AST_SKELETON),
    ("for_expression", HandlerMask::CFG_STMT),
    ("function_item", HandlerMask::SYMBOL),
    ("function_signature_item", HandlerMask::SYMBOL),
    ("generic_function", HandlerMask::RELATION),
    ("generic_pattern", HandlerMask::AST_SKELETON),
    ("if_expression", HandlerMask::CFG_STMT),
    ("impl_item", HandlerMask::SYMBOL),
    ("index_expression", HandlerMask::AST_SKELETON),
    ("inner_attribute_item", HandlerMask::RELATION),
    ("let_chain", HandlerMask::CFG_STMT),
    ("let_condition", HandlerMask::CFG_STMT),
    ("let_declaration", HandlerMask::CFG_STMT),
    ("loop_expression", HandlerMask::CFG_STMT),
    ("macro_definition", HandlerMask::SYMBOL),
    ("macro_invocation", HandlerMask::RELATION),
    ("match_arm", HandlerMask::CFG_STMT),
    ("match_block", HandlerMask::CFG_STMT),
    ("match_expression", HandlerMask::CFG_STMT),
    ("match_pattern", HandlerMask::AST_SKELETON),
    ("mod_item", HandlerMask::SYMBOL),
    ("mut_pattern", HandlerMask::AST_SKELETON),
    ("or_pattern", HandlerMask::AST_SKELETON),
    ("range_expression", HandlerMask::CFG_STMT),
    ("range_pattern", HandlerMask::AST_SKELETON),
    ("ref_pattern", HandlerMask::AST_SKELETON),
    ("reference_expression", HandlerMask::AST_SKELETON),
    ("reference_pattern", HandlerMask::AST_SKELETON),
    ("remaining_field_pattern", HandlerMask::AST_SKELETON),
    ("return_expression", HandlerMask::CFG_STMT),
    ("scoped_identifier", HandlerMask::RELATION),
    ("slice_pattern", HandlerMask::AST_SKELETON),
    ("static_item", HandlerMask::SYMBOL),
    ("struct_expression", HandlerMask::RELATION),
    ("struct_item", HandlerMask::SYMBOL),
    ("struct_pattern", HandlerMask::AST_SKELETON),
    ("trait_item", HandlerMask::SYMBOL),
    ("try_expression", HandlerMask::CFG_STMT),
    ("tuple_expression", HandlerMask::AST_SKELETON),
    ("tuple_pattern", HandlerMask::AST_SKELETON),
    ("tuple_struct_pattern", HandlerMask::AST_SKELETON),
    ("type_cast_expression", HandlerMask::AST_SKELETON),
    ("type_item", HandlerMask::SYMBOL),
    ("unary_expression", HandlerMask::CFG_STMT),
    ("union_item", HandlerMask::SYMBOL),
    ("unit_expression", HandlerMask::AST_SKELETON),
    ("unsafe_block", HandlerMask::CFG_STMT),
    ("use_as_clause", HandlerMask::RELATION),
    ("use_declaration", HandlerMask::RELATION),
    ("where_clause", HandlerMask::AST_SKELETON),
    ("while_expression", HandlerMask::CFG_STMT),
    ("yield_expression", HandlerMask::CFG_STMT),
];

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn rust_grammar() -> tree_sitter::Language {
    tree_sitter_rust::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn rust_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&rust_grammar(), HANDLED_KINDS))
}

fn rust_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&rust_grammar(), MANIFEST_JSON))
}

/// Pending call site: flush after the full symbol list exists (forward refs).
struct PendingCall<'a> {
    node: Node<'a>,
    from_symbol_idx: usize,
}

enum Visit<'a> {
    Pre(Node<'a>, usize),
    /// Pop one scope frame pushed on the matching Pre.
    LeaveScope,
    /// Pop a control-flow GuardBlock and decrement cognitive nesting.
    LeaveGuard,
}

/// Single iterative DFS: symbols + depth relations + deferred calls + complexity.
pub fn extract_all_single_pass(
    plugin: &RustPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("rust", rust_grammar(), source).map_err(|e| Error::ParseError {
        file: file_path.to_path_buf(),
        line: 0,
        message: e,
    })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let module_prefix = module_prefix_from_path(file_path);
    let dispatch = rust_dispatch();

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

                // --- Symbols + scope enter ---
                let mut pushed_scope = false;
                match kind {
                    "function_item" => {
                        let impl_ty = ctx.scopes.current_type().and_then(|f| {
                            if f.kind == ScopeKind::Impl {
                                f.name.clone()
                            } else {
                                None
                            }
                        });
                        let sym = plugin.extract_function(
                            node,
                            source,
                            &file_path_str,
                            impl_ty.as_deref(),
                        )?;
                        let params = sym.parameters.len() as u32;
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
                        let name = sym.name.clone();
                        let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                        let scope_kind = if sym.symbol_type == SymbolType::Method {
                            ScopeKind::Method
                        } else {
                            ScopeKind::Function
                        };
                        ctx.symbols.push(sym);
                        ctx.enter_scope(
                            scope_kind,
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
                    "struct_item" => {
                        let sym =
                            plugin.extract_type_definition(node, source, &file_path_str, false)?;
                        let name = sym.name.clone();
                        let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                        ctx.symbols.push(sym);
                        ctx.enter_scope(
                            ScopeKind::Struct,
                            Some(name),
                            qn,
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        pushed_scope = true;
                    }
                    "enum_item" => {
                        let sym =
                            plugin.extract_type_definition(node, source, &file_path_str, true)?;
                        let name = sym.name.clone();
                        let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                        ctx.symbols.push(sym);
                        ctx.enter_scope(
                            ScopeKind::Enum,
                            Some(name),
                            qn,
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        pushed_scope = true;
                    }
                    "use_declaration" => {
                        ctx.symbols
                            .extend(extract_use_symbols(node, source, &file_path_str));
                    }
                    "mod_item" => {
                        if let Some(m) = extract_mod_symbol(node, source, &file_path_str) {
                            let name = m.name.clone();
                            let qn = m.qualified_name.clone().unwrap_or_else(|| name.clone());
                            ctx.symbols.push(m);
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
                    }
                    "trait_item" => {
                        let before = ctx.symbols.len();
                        ctx.symbols.extend(extract_trait_symbols(
                            node,
                            source,
                            &file_path_str,
                            module_prefix.as_deref(),
                        ));
                        if ctx.symbols.len() > before {
                            let trait_sym = &ctx.symbols[before];
                            let name = trait_sym.name.clone();
                            let qn = trait_sym
                                .qualified_name
                                .clone()
                                .unwrap_or_else(|| name.clone());
                            // Bind scope to the trait symbol (index `before`), not last method.
                            let depth = ctx.scopes.depth();
                            let mut frame = ScopeFrame::new(
                                ScopeKind::Trait,
                                Some(name),
                                qn,
                                node.start_byte(),
                                node.end_byte(),
                                depth,
                            );
                            frame.symbol_idx = Some(before);
                            ctx.scopes.push(frame);
                            pushed_scope = true;
                        }
                    }
                    "impl_item" => {
                        let ty = node
                            .child_by_field_name("type")
                            .and_then(|n| {
                                n.utf8_text(source)
                                    .ok()
                                    .map(|s| s.rsplit("::").next().unwrap_or(s).trim().to_string())
                            })
                            .or_else(|| {
                                let mut c = node.walk();
                                node.children(&mut c)
                                    .find(|ch| ch.kind() == "type_identifier")
                                    .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
                            });
                        let name = ty.clone();
                        let qn = ty.unwrap_or_default();
                        ctx.enter_scope(
                            ScopeKind::Impl,
                            name,
                            qn,
                            node.start_byte(),
                            node.end_byte(),
                            false,
                        );
                        pushed_scope = true;
                    }
                    "type_item" => {
                        if let Some(t) = extract_type_item(node, source, &file_path_str) {
                            ctx.symbols.push(t);
                        }
                    }
                    "macro_definition" => {
                        if let Some(m) = extract_macro_definition(node, source, &file_path_str) {
                            ctx.symbols.push(m);
                        }
                    }
                    "extern_crate_declaration" => {
                        if let Some(c) = extract_extern_crate(node, source, &file_path_str) {
                            ctx.symbols.push(c);
                        }
                    }
                    "associated_type" => {
                        // Trait-associated types are emitted inside `extract_trait_symbols`.
                        let under_trait = ctx
                            .scopes
                            .current_type()
                            .is_some_and(|f| f.kind == ScopeKind::Trait);
                        if !under_trait {
                            let owner = ctx
                                .scopes
                                .current_type()
                                .map(|f| f.qualified_prefix.clone());
                            if let Some(at) = extract_associated_type(
                                node,
                                source,
                                &file_path_str,
                                owner.as_deref(),
                            ) {
                                ctx.symbols.push(at);
                            }
                        }
                    }
                    "enum_variant" => {
                        if let Some(frame) = ctx.scopes.current_type()
                            && frame.kind == ScopeKind::Enum
                        {
                            let enum_name = frame.name.clone().unwrap_or_default();
                            let enum_qn = frame.qualified_prefix.clone();
                            if let Some(v) = extract_enum_variant_symbol(
                                node,
                                source,
                                &file_path_str,
                                &enum_name,
                                &enum_qn,
                            ) {
                                ctx.symbols.push(v);
                            }
                        }
                    }
                    _ => {}
                }

                // --- Calls (deferred) ---
                if RUST_CALL_KINDS.contains(&kind)
                    && let Some(frame) = ctx.scopes.current_function()
                    && let Some(idx) = frame.symbol_idx
                {
                    pending_calls.push(PendingCall {
                        node,
                        from_symbol_idx: idx,
                    });
                }

                // --- Depth relations (scope-based from_fn) ---
                let from_fn = containing_function_from_scope(&ctx.scopes, &ctx.symbols)
                    .map(|s| s.qualified_name.clone().unwrap_or_else(|| s.name.clone()));
                visit_depth_relation_node(
                    node,
                    source,
                    &file_path_str,
                    &ctx.symbols,
                    from_fn.as_deref(),
                    &mut relations,
                );

                // --- Complexity on control-flow / returns ---
                if ctx.scopes.current_function().is_some() {
                    match kind {
                        "if_expression" | "match_expression" | "while_expression"
                        | "for_expression" | "loop_expression" => {
                            let delta = 1 + ctx
                                .scopes
                                .current_function()
                                .map(|f| f.complexity.cognitive_nesting)
                                .unwrap_or(0);
                            ctx.scopes.note_branch(delta);
                            ctx.scopes.enter_branch();
                            let depth = ctx.scopes.depth();
                            ctx.scopes.push(ScopeFrame::new(
                                ScopeKind::GuardBlock,
                                None,
                                String::new(),
                                node.start_byte(),
                                node.end_byte(),
                                depth,
                            ));
                            stack.push(Visit::LeaveGuard);
                        }
                        "match_arm" => {
                            let delta = 1 + ctx
                                .scopes
                                .current_function()
                                .map(|f| f.complexity.cognitive_nesting)
                                .unwrap_or(0);
                            ctx.scopes.note_branch(delta);
                        }
                        "return_expression" => {
                            ctx.scopes.note_return();
                        }
                        _ => {}
                    }
                }

                // Children + leave marker (post-order leave after children).
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

    // Flush calls with the complete symbol list (forward-reference safe).
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
            "rust",
            &mut relations,
        );
        rgctl_plugin_helpers::callable_identity::call(
            pending.node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Rust,
            ctx.symbols.get(pending.from_symbol_idx),
            &mut relations[identity_relation_start..],
        );
    }

    rgctl_plugin_helpers::apply_framework_catalog(
        "rust",
        source,
        Some(root),
        &mut ctx.symbols,
        &mut relations,
    );

    rgctl_plugin_helpers::callable_identity::finish(
        rgctl_plugin_helpers::callable_identity::Policy::Rust,
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}
