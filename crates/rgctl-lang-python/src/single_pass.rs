//! Single-pass AST extraction for Python (`ScopeStack` + `KindDispatchTable`).

use crate::depth;
use crate::plugin::{PythonPlugin, instantiation_target};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{
    decorators_for_node, extract_python_class_extends_relations, extract_python_import_symbols,
    python_source_location,
};
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../python-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("aliased_import", HandlerMask::RELATION),
    ("as_pattern", HandlerMask::AST_SKELETON),
    ("as_pattern_target", HandlerMask::SYMBOL),
    ("assert_statement", HandlerMask::CFG_STMT),
    ("assignment", HandlerMask::AST_SKELETON),
    ("attribute", HandlerMask::RELATION),
    ("augmented_assignment", HandlerMask::AST_SKELETON),
    ("await", HandlerMask::CFG_STMT),
    ("binary_operator", HandlerMask::CFG_STMT),
    ("block", HandlerMask::CFG_STMT),
    ("boolean_operator", HandlerMask::CFG_STMT),
    ("break_statement", HandlerMask::CFG_STMT),
    ("call", HandlerMask::RELATION),
    ("case_clause", HandlerMask::CFG_STMT),
    ("case_pattern", HandlerMask::AST_SKELETON),
    ("class_definition", HandlerMask::SYMBOL),
    ("class_pattern", HandlerMask::AST_SKELETON),
    ("comparison_operator", HandlerMask::CFG_STMT),
    ("complex_pattern", HandlerMask::AST_SKELETON),
    ("conditional_expression", HandlerMask::CFG_STMT),
    ("continue_statement", HandlerMask::CFG_STMT),
    ("decorated_definition", HandlerMask::RELATION),
    ("decorator", HandlerMask::RELATION),
    ("default_parameter", HandlerMask::AST_SKELETON),
    ("delete_statement", HandlerMask::CFG_STMT),
    ("dict_pattern", HandlerMask::AST_SKELETON),
    ("dictionary_comprehension", HandlerMask::CFG_STMT),
    ("dictionary_splat_pattern", HandlerMask::AST_SKELETON),
    ("elif_clause", HandlerMask::CFG_STMT),
    ("else_clause", HandlerMask::CFG_STMT),
    ("except_clause", HandlerMask::CFG_STMT),
    ("exec_statement", HandlerMask::CFG_STMT),
    ("finally_clause", HandlerMask::CFG_STMT),
    ("for_in_clause", HandlerMask::CFG_STMT),
    ("for_statement", HandlerMask::CFG_STMT),
    ("function_definition", HandlerMask::SYMBOL),
    ("future_import_statement", HandlerMask::RELATION),
    ("generator_expression", HandlerMask::CFG_STMT),
    ("global_statement", HandlerMask::CFG_STMT),
    ("if_clause", HandlerMask::CFG_STMT),
    ("if_statement", HandlerMask::CFG_STMT),
    ("import_from_statement", HandlerMask::RELATION),
    ("import_prefix", HandlerMask::RELATION),
    ("import_statement", HandlerMask::RELATION),
    ("keyword_pattern", HandlerMask::AST_SKELETON),
    ("lambda", HandlerMask::SYMBOL),
    ("lambda_parameters", HandlerMask::AST_SKELETON),
    ("list_comprehension", HandlerMask::CFG_STMT),
    ("list_pattern", HandlerMask::AST_SKELETON),
    ("list_splat_pattern", HandlerMask::AST_SKELETON),
    ("match_statement", HandlerMask::CFG_STMT),
    ("named_expression", HandlerMask::SYMBOL),
    ("nonlocal_statement", HandlerMask::CFG_STMT),
    ("not_operator", HandlerMask::AST_SKELETON),
    ("parameters", HandlerMask::AST_SKELETON),
    ("pass_statement", HandlerMask::CFG_STMT),
    ("print_statement", HandlerMask::CFG_STMT),
    ("raise_statement", HandlerMask::CFG_STMT),
    ("relative_import", HandlerMask::RELATION),
    ("return_statement", HandlerMask::CFG_STMT),
    ("set_comprehension", HandlerMask::CFG_STMT),
    ("splat_pattern", HandlerMask::AST_SKELETON),
    ("subscript", HandlerMask::RELATION),
    ("try_statement", HandlerMask::CFG_STMT),
    ("tuple_pattern", HandlerMask::AST_SKELETON),
    ("type_alias_statement", HandlerMask::SYMBOL),
    ("type_parameter", HandlerMask::AST_SKELETON),
    ("typed_default_parameter", HandlerMask::AST_SKELETON),
    ("typed_parameter", HandlerMask::AST_SKELETON),
    ("unary_operator", HandlerMask::AST_SKELETON),
    ("union_pattern", HandlerMask::AST_SKELETON),
    ("while_statement", HandlerMask::CFG_STMT),
    ("wildcard_import", HandlerMask::RELATION),
    ("with_clause", HandlerMask::CFG_STMT),
    ("with_item", HandlerMask::CFG_STMT),
    ("with_statement", HandlerMask::CFG_STMT),
    ("yield", HandlerMask::CFG_STMT),
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn python_grammar() -> tree_sitter::Language {
    tree_sitter_python::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn python_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&python_grammar(), HANDLED_KINDS))
}

fn python_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&python_grammar(), MANIFEST_JSON))
}

/// Pending call site: flush after the full symbol list exists (forward refs).
struct PendingCall<'a> {
    node: Node<'a>,
    from_symbol_idx: usize,
}

/// Pending subscript → `__getitem__` / `__setitem__` after symbols exist.
struct PendingSubscript<'a> {
    node: Node<'a>,
    from_symbol_idx: usize,
    is_store: bool,
}

enum Visit<'a> {
    Pre(Node<'a>, usize),
    /// Pop one scope frame pushed on the matching Pre.
    LeaveScope,
    /// Pop a control-flow GuardBlock and decrement cognitive nesting.
    LeaveGuard,
    /// Leave module-level typing / ImportError guard (binding harvest).
    LeaveTypingGuard,
}

fn decorator_from_name(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "class_definition" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string),
        "function_definition" => {
            let method = node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string)?;
            Some(
                rgctl_plugin_helpers::containing_class_name(node, source)
                    .map(|owner| format!("{owner}.{method}"))
                    .unwrap_or(method),
            )
        }
        _ => None,
    }
}

fn emit_decorators_for_node(
    node: Node,
    source: &[u8],
    file_path: &str,
    relations: &mut Vec<Relation>,
) {
    let Some(from) = decorator_from_name(node, source) else {
        return;
    };
    for (decorator_name, args) in decorators_for_node(node, source) {
        let mut meta = serde_json::json!({ "language": "python" });
        if let Some(args) = args {
            meta["arguments"] = serde_json::Value::String(args);
        }
        relations.push(Relation {
            from: from.clone(),
            to: decorator_name,
            relation_type: RelationType::AnnotatedWith,
            location: python_source_location(node, file_path),
            metadata: meta,
            to_qualified_hint: None,
            to_type_hint: None,
            ..Default::default()
        });
    }
}

/// Single iterative DFS: symbols + depth relations + deferred calls + complexity.
pub fn extract_all_single_pass(
    plugin: &PythonPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree =
        parse_with_pool("python", python_grammar(), source).map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: e,
        })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = python_dispatch();

    let mut ctx = SinglePassCtx::new(dispatch);
    let mut relations = Vec::new();
    let mut pending_calls: Vec<PendingCall<'_>> = Vec::new();
    let mut pending_subscripts: Vec<PendingSubscript<'_>> = Vec::new();
    let mut typing_guard_depth: u32 = 0;
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
            Visit::LeaveTypingGuard => {
                typing_guard_depth = typing_guard_depth.saturating_sub(1);
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
                    "function_definition" => {
                        let sym = plugin.extract_function(node, source, &file_path_str)?;
                        let params = sym.parameters.len() as u32;
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
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
                        emit_decorators_for_node(node, source, &file_path_str, &mut relations);
                        pushed_scope = true;
                    }
                    "class_definition" => {
                        let sym = plugin.extract_class(node, source, &file_path_str)?;
                        let name = sym.name.clone();
                        let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                        ctx.symbols.push(sym);
                        ctx.enter_scope(
                            ScopeKind::Class,
                            Some(name),
                            qn,
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        relations.extend(extract_python_class_extends_relations(
                            node,
                            source,
                            &file_path_str,
                            "python",
                        ));
                        emit_decorators_for_node(node, source, &file_path_str, &mut relations);
                        pushed_scope = true;
                    }
                    "import_statement" | "import_from_statement" => {
                        let mut imports =
                            extract_python_import_symbols(node, source, &file_path_str, "python");
                        if typing_guard_depth > 0 {
                            for sym in &mut imports {
                                sym.metadata["guard"] = serde_json::json!("typing");
                            }
                        }
                        ctx.symbols.extend(imports);
                    }
                    "type_alias_statement" => {
                        if let Some(sym) =
                            depth::extract_type_alias_statement(node, source, &file_path_str)
                        {
                            ctx.symbols.push(sym);
                        }
                    }
                    "as_pattern_target" => {
                        if let Some(sym) =
                            depth::extract_as_pattern_target(node, source, &file_path_str)
                        {
                            ctx.symbols.push(sym);
                        }
                    }
                    "named_expression" => {
                        if let Some(sym) =
                            depth::extract_named_expression(node, source, &file_path_str)
                        {
                            ctx.symbols.push(sym);
                        }
                    }
                    "lambda" => {
                        let class_qn = ctx.scopes.current_type().and_then(|f| {
                            if f.kind == ScopeKind::Class {
                                Some(f.qualified_prefix.clone())
                            } else {
                                None
                            }
                        });
                        if let Some(sym) = depth::extract_lambda_symbol(
                            node,
                            source,
                            &file_path_str,
                            class_qn.as_deref(),
                        ) {
                            ctx.symbols.push(sym);
                        }
                    }
                    "assignment"
                        if typing_guard_depth > 0 && ctx.scopes.current_function().is_none() =>
                    {
                        if let Some(mut sym) = depth::extract_simple_binding(
                            node,
                            source,
                            &file_path_str,
                            "guard_binding",
                        ) {
                            sym.metadata["guard"] = serde_json::json!("typing");
                            ctx.symbols.push(sym);
                        }
                    }
                    _ => {}
                }

                // Module-level typing / ImportError guards (harvest bindings inside).
                if ctx.scopes.current_function().is_none() {
                    let enter_typing = match kind {
                        "if_statement" => depth::is_typing_guard_if(node, source),
                        "try_statement" => depth::is_import_error_try(node, source),
                        _ => false,
                    };
                    if enter_typing {
                        typing_guard_depth = typing_guard_depth.saturating_add(1);
                        stack.push(Visit::LeaveTypingGuard);
                    }
                }

                // --- Calls (deferred) ---
                if PYTHON_CALL_KINDS.contains(&kind) {
                    if let Some(frame) = ctx.scopes.current_function() {
                        if let Some(idx) = frame.symbol_idx {
                            pending_calls.push(PendingCall {
                                node,
                                from_symbol_idx: idx,
                            });
                        }
                    }
                }

                // --- Subscript → __getitem__ / __setitem__ (deferred) ---
                if kind == "subscript" {
                    if let Some(frame) = ctx.scopes.current_function() {
                        if let Some(idx) = frame.symbol_idx {
                            pending_subscripts.push(PendingSubscript {
                                node,
                                from_symbol_idx: idx,
                                is_store: depth::subscript_is_store(node),
                            });
                        }
                    }
                }

                // --- Complexity on control-flow / returns ---
                // Matches existing calculate_cyclomatic / calculate_cognitive, plus
                // match/with as structural nesting and assert/boolean as flat branches.
                if ctx.scopes.current_function().is_some() {
                    match kind {
                        "if_statement" | "while_statement" | "for_statement"
                        | "match_statement" | "with_statement" => {
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
                        "elif_clause" | "except_clause" | "assert_statement"
                        | "boolean_operator" => {
                            let delta = 1 + ctx
                                .scopes
                                .current_function()
                                .map(|f| f.complexity.cognitive_nesting)
                                .unwrap_or(0);
                            ctx.scopes.note_branch(delta);
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

    // Flush calls + instantiations with the complete symbol list (forward-ref safe).
    let class_names: HashSet<String> = ctx
        .symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Class)
        .map(|s| s.name.clone())
        .collect();

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
            "python",
            &mut relations,
        );
        if let Some(target) = instantiation_target(pending.node, source, &class_names) {
            let from = from_fn
                .qualified_name
                .clone()
                .unwrap_or_else(|| from_fn.name.clone());
            relations.push(Relation {
                from,
                to: target.clone(),
                relation_type: RelationType::Instantiates,
                location: python_source_location(pending.node, &file_path_str),
                metadata: serde_json::json!({ "language": "python" }),
                to_qualified_hint: Some(target),
                to_type_hint: None,
                ..Default::default()
            });
        }
        rgctl_plugin_helpers::callable_identity::call(
            pending.node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Python,
            ctx.symbols.get(pending.from_symbol_idx),
            &mut relations[identity_relation_start..],
        );
    }

    for pending in pending_subscripts {
        let Some(from_fn) = ctx.symbols.get(pending.from_symbol_idx) else {
            continue;
        };
        let enclosing_class = from_fn.qualified_name.as_deref().and_then(|qn| {
            let owner = qn.rsplit_once('.').map(|(o, _)| o)?;
            ctx.symbols.iter().find(|s| {
                s.symbol_type == SymbolType::Class
                    && (s.qualified_name.as_deref() == Some(owner) || s.name == owner)
            })
        });
        if let Some(rel) = depth::maybe_subscript_call_relation(
            pending.node,
            source,
            &file_path_str,
            from_fn,
            pending.is_store,
            enclosing_class,
        ) {
            relations.push(rel);
        }
    }

    rgctl_plugin_helpers::apply_framework_catalog(
        "python",
        source,
        Some(root),
        &mut ctx.symbols,
        &mut relations,
    );

    rgctl_plugin_helpers::callable_identity::finish(
        rgctl_plugin_helpers::callable_identity::Policy::Python,
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}
