//! Single-pass AST extraction for Go (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::GoPlugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../go-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
/// Keep in sync with non-Literal active handlers in `go-ast-coverage.json`.
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("assignment_statement", HandlerMask::AST_SKELETON),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("block", HandlerMask::CFG_STMT),
    ("break_statement", HandlerMask::CFG_STMT),
    ("call_expression", HandlerMask::RELATION),
    ("communication_case", HandlerMask::CFG_STMT),
    ("const_declaration", HandlerMask::SYMBOL),
    ("const_spec", HandlerMask::SYMBOL),
    ("continue_statement", HandlerMask::CFG_STMT),
    ("dec_statement", HandlerMask::CFG_STMT),
    ("default_case", HandlerMask::CFG_STMT),
    ("defer_statement", HandlerMask::CFG_STMT),
    ("empty_statement", HandlerMask::CFG_STMT),
    ("expression_case", HandlerMask::CFG_STMT),
    ("expression_switch_statement", HandlerMask::CFG_STMT),
    ("fallthrough_statement", HandlerMask::CFG_STMT),
    ("field_declaration", HandlerMask::SYMBOL),
    ("for_clause", HandlerMask::CFG_STMT),
    ("for_statement", HandlerMask::CFG_STMT),
    ("function_declaration", HandlerMask::SYMBOL),
    ("go_statement", HandlerMask::CFG_STMT),
    ("goto_statement", HandlerMask::CFG_STMT),
    ("if_statement", HandlerMask::CFG_STMT),
    ("import_declaration", HandlerMask::RELATION),
    ("import_spec", HandlerMask::RELATION),
    ("import_spec_list", HandlerMask::RELATION),
    ("inc_statement", HandlerMask::CFG_STMT),
    ("index_expression", HandlerMask::RELATION),
    ("labeled_statement", HandlerMask::CFG_STMT),
    ("method_declaration", HandlerMask::SYMBOL),
    ("method_elem", HandlerMask::SYMBOL),
    ("package_clause", HandlerMask::AST_SKELETON),
    ("parameter_declaration", HandlerMask::AST_SKELETON),
    ("parameter_list", HandlerMask::AST_SKELETON),
    ("range_clause", HandlerMask::CFG_STMT),
    ("receive_statement", HandlerMask::CFG_STMT),
    ("return_statement", HandlerMask::CFG_STMT),
    ("select_statement", HandlerMask::CFG_STMT),
    ("selector_expression", HandlerMask::RELATION),
    ("send_statement", HandlerMask::CFG_STMT),
    ("short_var_declaration", HandlerMask::AST_SKELETON),
    ("slice_expression", HandlerMask::RELATION),
    ("type_alias", HandlerMask::SYMBOL),
    ("type_arguments", HandlerMask::AST_SKELETON),
    ("type_assertion_expression", HandlerMask::AST_SKELETON),
    ("type_case", HandlerMask::CFG_STMT),
    ("type_constraint", HandlerMask::AST_SKELETON),
    ("type_conversion_expression", HandlerMask::AST_SKELETON),
    ("type_declaration", HandlerMask::SYMBOL),
    ("type_elem", HandlerMask::SYMBOL),
    ("type_parameter_declaration", HandlerMask::AST_SKELETON),
    ("type_parameter_list", HandlerMask::AST_SKELETON),
    ("type_spec", HandlerMask::SYMBOL),
    ("type_switch_statement", HandlerMask::CFG_STMT),
    ("unary_expression", HandlerMask::CFG_STMT),
    ("var_declaration", HandlerMask::SYMBOL),
    ("var_spec", HandlerMask::SYMBOL),
    ("variadic_parameter_declaration", HandlerMask::AST_SKELETON),
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn go_grammar() -> tree_sitter::Language {
    tree_sitter_go::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn go_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&go_grammar(), HANDLED_KINDS))
}

fn go_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&go_grammar(), MANIFEST_JSON))
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

fn extract_type_declaration_symbols(
    plugin: &GoPlugin,
    node: Node,
    source: &[u8],
    file_path: &str,
    symbols: &mut Vec<Symbol>,
) -> Result<()> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            // Go 1.9+ `type ID = string` uses `type_alias`, not `type_spec`.
            "type_alias" => {
                if let Some(alias) = plugin.extract_type_alias(child, source, file_path)? {
                    symbols.push(alias);
                }
            }
            "type_spec" => {
                let mut handled = false;
                let mut spec_cursor = child.walk();
                for spec_child in child.children(&mut spec_cursor) {
                    match spec_child.kind() {
                        "struct_type" => {
                            let mut st = plugin.extract_struct(child, source, file_path)?;
                            if let Some(tp) = plugin.type_params_of(child, source) {
                                st.metadata["type_params"] = serde_json::Value::String(tp);
                            }
                            symbols.push(st);
                            handled = true;
                            break;
                        }
                        "interface_type" => {
                            let iface = plugin.extract_interface(child, source, file_path)?;
                            let iface_name = iface.name.clone();
                            symbols.push(iface);
                            let methods = plugin.extract_interface_methods(
                                spec_child,
                                &iface_name,
                                source,
                                file_path,
                                symbols,
                            )?;
                            symbols.extend(methods);
                            handled = true;
                            break;
                        }
                        _ => {}
                    }
                }
                if !handled {
                    if let Some(alias) = plugin.extract_type_alias(child, source, file_path)? {
                        symbols.push(alias);
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Single iterative DFS: symbols + deferred calls + complexity + implements/embeds.
pub fn extract_all_single_pass(
    plugin: &GoPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("go", go_grammar(), source).map_err(|e| Error::ParseError {
        file: file_path.to_path_buf(),
        line: 0,
        message: e,
    })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = go_dispatch();

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
                    "type_declaration" => {
                        extract_type_declaration_symbols(
                            plugin,
                            node,
                            source,
                            &file_path_str,
                            &mut ctx.symbols,
                        )?;
                    }
                    "import_declaration" => {
                        ctx.symbols
                            .extend(plugin.extract_imports(node, source, &file_path_str)?);
                    }
                    "const_declaration" => {
                        ctx.symbols
                            .extend(plugin.extract_consts(node, source, &file_path_str)?);
                    }
                    "var_declaration" => {
                        ctx.symbols
                            .extend(plugin.extract_vars(node, source, &file_path_str)?);
                    }
                    "function_declaration" | "method_declaration" => {
                        let mut func = plugin.extract_function(node, source, &file_path_str)?;
                        if let Some(tp) = plugin.type_params_of(node, source) {
                            func.metadata["type_params"] = serde_json::Value::String(tp);
                        }
                        let params = func.parameters.len() as u32;
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
                        let name = func.name.clone();
                        let qn = func.qualified_name.clone().unwrap_or_else(|| name.clone());
                        ctx.symbols.push(func);
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
                    _ => {}
                }

                // --- Calls (deferred) ---
                if GO_CALL_KINDS.contains(&kind) {
                    if let Some(frame) = ctx.scopes.current_function() {
                        if let Some(idx) = frame.symbol_idx {
                            pending_calls.push(PendingCall {
                                node,
                                from_symbol_idx: idx,
                            });
                        }
                    }
                }

                // --- Complexity on control-flow / returns ---
                // Approximate existing calculate_cyclomatic / calculate_cognitive.
                if ctx.scopes.current_function().is_some() {
                    match kind {
                        "if_statement" | "for_statement" => {
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
                        "expression_switch_statement"
                        | "type_switch_statement"
                        | "select_statement" => {
                            // Cognitive: +1+nesting without increasing nesting (matches
                            // calculate_cognitive). Cyclomatic also +1 here; cases add more.
                            let delta = 1 + ctx
                                .scopes
                                .current_function()
                                .map(|f| f.complexity.cognitive_nesting)
                                .unwrap_or(0);
                            ctx.scopes.note_branch(delta);
                        }
                        "expression_case" | "type_case" | "default_case" | "communication_case" => {
                            // Cyclomatic only (calculate_cognitive does not count cases).
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
            "go",
            &mut relations,
        );
        rgctl_plugin_helpers::callable_identity::call(
            pending.node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Go,
            ctx.symbols.get(pending.from_symbol_idx),
            &mut relations[identity_relation_start..],
        );
    }

    plugin.emit_implements_and_embeds(&ctx.symbols, file_path, &mut relations);

    rgctl_plugin_helpers::apply_framework_catalog(
        "go",
        source,
        Some(root),
        &mut ctx.symbols,
        &mut relations,
    );

    rgctl_plugin_helpers::callable_identity::finish(
        rgctl_plugin_helpers::callable_identity::Policy::Go,
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}
