//! Single-pass AST extraction for PHP (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::{
    PHP_COMPLEXITY_BRANCH_KINDS, PHP_FUNCTION_KINDS, PhpPlugin, import_map_from_symbols,
    normalize_namespace, php_grammar, push_php_call_relation,
};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../php-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("anonymous_function", HandlerMask::SYMBOL),
    ("anonymous_function_use_clause", HandlerMask::AST_SKELETON),
    ("array_creation_expression", HandlerMask::AST_SKELETON),
    ("arrow_function", HandlerMask::SYMBOL),
    ("assignment_expression", HandlerMask::AST_SKELETON),
    ("attribute", HandlerMask::RELATION),
    ("attribute_group", HandlerMask::RELATION),
    ("augmented_assignment_expression", HandlerMask::AST_SKELETON),
    ("base_clause", HandlerMask::RELATION),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("break_statement", HandlerMask::CFG_STMT),
    ("case_statement", HandlerMask::CFG_STMT),
    ("cast_expression", HandlerMask::AST_SKELETON),
    ("catch_clause", HandlerMask::CFG_STMT),
    ("class_constant_access_expression", HandlerMask::RELATION),
    ("class_declaration", HandlerMask::SYMBOL),
    ("class_interface_clause", HandlerMask::RELATION),
    ("clone_expression", HandlerMask::AST_SKELETON),
    ("compound_statement", HandlerMask::CFG_STMT),
    ("conditional_expression", HandlerMask::CFG_STMT),
    ("const_declaration", HandlerMask::SYMBOL),
    ("const_element", HandlerMask::SYMBOL),
    ("continue_statement", HandlerMask::CFG_STMT),
    ("declare_directive", HandlerMask::CFG_STMT),
    ("declare_statement", HandlerMask::CFG_STMT),
    ("default_statement", HandlerMask::CFG_STMT),
    ("do_statement", HandlerMask::CFG_STMT),
    ("echo_statement", HandlerMask::CFG_STMT),
    ("else_clause", HandlerMask::CFG_STMT),
    ("else_if_clause", HandlerMask::CFG_STMT),
    ("empty_statement", HandlerMask::CFG_STMT),
    ("enum_case", HandlerMask::SYMBOL),
    ("enum_declaration", HandlerMask::SYMBOL),
    ("error_suppression_expression", HandlerMask::AST_SKELETON),
    ("exit_statement", HandlerMask::CFG_STMT),
    ("finally_clause", HandlerMask::CFG_STMT),
    ("for_statement", HandlerMask::CFG_STMT),
    ("foreach_statement", HandlerMask::CFG_STMT),
    ("formal_parameters", HandlerMask::AST_SKELETON),
    ("function_call_expression", HandlerMask::RELATION),
    ("function_definition", HandlerMask::SYMBOL),
    ("function_static_declaration", HandlerMask::CFG_STMT),
    ("global_declaration", HandlerMask::CFG_STMT),
    ("goto_statement", HandlerMask::CFG_STMT),
    ("if_statement", HandlerMask::CFG_STMT),
    ("include_expression", HandlerMask::RELATION),
    ("include_once_expression", HandlerMask::RELATION),
    ("interface_declaration", HandlerMask::SYMBOL),
    ("match_block", HandlerMask::CFG_STMT),
    ("match_conditional_expression", HandlerMask::CFG_STMT),
    ("match_default_expression", HandlerMask::CFG_STMT),
    ("match_expression", HandlerMask::CFG_STMT),
    ("member_access_expression", HandlerMask::RELATION),
    ("member_call_expression", HandlerMask::RELATION),
    ("method_declaration", HandlerMask::SYMBOL),
    ("named_label_statement", HandlerMask::CFG_STMT),
    ("namespace_definition", HandlerMask::SYMBOL),
    ("namespace_use_clause", HandlerMask::RELATION),
    ("namespace_use_declaration", HandlerMask::SYMBOL),
    ("nullsafe_member_access_expression", HandlerMask::RELATION),
    ("nullsafe_member_call_expression", HandlerMask::RELATION),
    ("object_creation_expression", HandlerMask::RELATION),
    ("property_declaration", HandlerMask::SYMBOL),
    ("property_element", HandlerMask::SYMBOL),
    ("property_promotion_parameter", HandlerMask::AST_SKELETON),
    ("reference_assignment_expression", HandlerMask::AST_SKELETON),
    ("require_expression", HandlerMask::RELATION),
    ("require_once_expression", HandlerMask::RELATION),
    ("return_statement", HandlerMask::CFG_STMT),
    ("scoped_call_expression", HandlerMask::RELATION),
    ("scoped_property_access_expression", HandlerMask::RELATION),
    ("sequence_expression", HandlerMask::CFG_STMT),
    ("shell_command_expression", HandlerMask::CFG_STMT),
    ("simple_parameter", HandlerMask::AST_SKELETON),
    ("static_variable_declaration", HandlerMask::AST_SKELETON),
    ("subscript_expression", HandlerMask::AST_SKELETON),
    ("switch_statement", HandlerMask::CFG_STMT),
    ("throw_expression", HandlerMask::CFG_STMT),
    ("trait_declaration", HandlerMask::SYMBOL),
    ("try_statement", HandlerMask::CFG_STMT),
    ("unary_op_expression", HandlerMask::AST_SKELETON),
    ("unset_statement", HandlerMask::CFG_STMT),
    ("update_expression", HandlerMask::AST_SKELETON),
    ("use_as_clause", HandlerMask::RELATION),
    ("use_declaration", HandlerMask::RELATION),
    ("use_instead_of_clause", HandlerMask::RELATION),
    ("variadic_parameter", HandlerMask::AST_SKELETON),
    ("while_statement", HandlerMask::CFG_STMT),
    ("yield_expression", HandlerMask::CFG_STMT),
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

/// Process-wide AST handler table (coverage sync + introspection).
pub fn php_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&php_grammar(), HANDLED_KINDS))
}

fn php_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&php_grammar(), MANIFEST_JSON))
}

struct PendingCall<'a> {
    node: Node<'a>,
    /// Enclosing function at call site (forward-ref flush uses full symbol list).
    #[allow(dead_code)]
    from_symbol_idx: usize,
}

enum Visit<'a> {
    Pre(Node<'a>, usize),
    LeaveScope,
    LeaveGuard,
}

/// Single iterative DFS: symbols + deferred PHP calls + inheritance + complexity.
pub fn extract_all_single_pass(
    plugin: &PhpPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("php", php_grammar(), source).map_err(|e| Error::ParseError {
        file: file_path.to_path_buf(),
        line: 0,
        message: e,
    })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = php_dispatch();

    let mut ctx = SinglePassCtx::new(dispatch);
    let mut relations = Vec::new();
    let mut pending_calls: Vec<PendingCall<'_>> = Vec::new();
    let mut namespace: Option<String> = None;
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

                if kind == "namespace_definition" {
                    if let Some(ns) = node
                        .child_by_field_name("name")
                        .and_then(|n| normalize_namespace(n, source))
                    {
                        namespace = Some(ns);
                    }
                }

                match kind {
                    "function_definition" => {
                        if let Some(sym) = plugin.extract_function(
                            node,
                            source,
                            &file_path_str,
                            namespace.as_deref(),
                        )? {
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
                        plugin.extract_embedded_symbols(
                            node,
                            source,
                            &file_path_str,
                            namespace.as_deref(),
                            &mut ctx.symbols,
                        )?;
                    }
                    "method_declaration" => {
                        // Prior extract only pulled methods from `class_declaration` bodies
                        // (not interface/trait/enum) plus orphan methods with no enclosing class.
                        let type_frame = ctx.scopes.current_type();
                        let in_class = type_frame.is_some_and(|t| t.kind == ScopeKind::Class);
                        let owner = if in_class {
                            type_frame.map(|t| t.qualified_prefix.clone())
                        } else {
                            None
                        };
                        if in_class
                            || (type_frame.is_none()
                                && plugin.find_enclosing_class_name(node, source).is_none())
                        {
                            if let Some(sym) = plugin.extract_method(
                                node,
                                source,
                                &file_path_str,
                                namespace.as_deref(),
                                owner.as_deref(),
                            )? {
                                let params = sym.parameters.len() as u32;
                                let loc = ((node.end_position().row - node.start_position().row)
                                    + 1)
                                .max(1) as u32;
                                let name = sym.name.clone();
                                let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                                ctx.symbols.push(sym);
                                ctx.enter_scope(
                                    ScopeKind::Method,
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
                            plugin.extract_embedded_symbols(
                                node,
                                source,
                                &file_path_str,
                                namespace.as_deref(),
                                &mut ctx.symbols,
                            )?;
                        }
                    }
                    "arrow_function" | "anonymous_function" => {
                        if let Some(sym) = plugin.extract_anonymous_function(
                            node,
                            source,
                            &file_path_str,
                            namespace.as_deref(),
                        )? {
                            let params = sym.parameters.len() as u32;
                            let loc = ((node.end_position().row - node.start_position().row) + 1)
                                .max(1) as u32;
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
                                f.complexity.param_count = params;
                                f.complexity.loc = loc;
                            }
                            pushed_scope = true;
                        }
                    }
                    "class_declaration" => {
                        let sym = plugin.extract_class(
                            node,
                            source,
                            &file_path_str,
                            namespace.as_deref(),
                        )?;
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
                        pushed_scope = true;
                    }
                    "interface_declaration" => {
                        let sym = plugin.extract_interface(
                            node,
                            source,
                            &file_path_str,
                            namespace.as_deref(),
                        )?;
                        let name = sym.name.clone();
                        let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                        ctx.symbols.push(sym);
                        ctx.enter_scope(
                            ScopeKind::Interface,
                            Some(name),
                            qn,
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        pushed_scope = true;
                    }
                    "trait_declaration" => {
                        let sym = plugin.extract_trait(
                            node,
                            source,
                            &file_path_str,
                            namespace.as_deref(),
                        )?;
                        let name = sym.name.clone();
                        let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                        ctx.symbols.push(sym);
                        ctx.enter_scope(
                            ScopeKind::Trait,
                            Some(name),
                            qn,
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        pushed_scope = true;
                    }
                    "enum_declaration" => {
                        let sym = plugin.extract_enum(
                            node,
                            source,
                            &file_path_str,
                            namespace.as_deref(),
                        )?;
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
                    "const_declaration" => {
                        if let Some(frame) = ctx.scopes.current_type() {
                            if let Some(idx) = frame.symbol_idx {
                                if let Some(class_sym) = ctx.symbols.get_mut(idx) {
                                    plugin.append_const_fields(node, source, class_sym)?;
                                }
                            }
                        } else {
                            ctx.symbols.extend(plugin.extract_const(
                                node,
                                source,
                                &file_path_str,
                                namespace.as_deref(),
                            )?);
                        }
                    }
                    "namespace_use_declaration" => {
                        plugin.extract_namespace_imports(
                            node,
                            source,
                            &file_path_str,
                            &mut ctx.symbols,
                        )?;
                    }
                    _ => {}
                }

                let from_for_rel = ctx
                    .scopes
                    .current_function()
                    .and_then(|f| f.symbol_idx)
                    .and_then(|idx| ctx.symbols.get(idx))
                    .map(|s| s.qualified_name.clone().unwrap_or_else(|| s.name.clone()))
                    .or_else(|| {
                        ctx.scopes
                            .current_type()
                            .map(|t| t.qualified_prefix.clone())
                    });
                plugin.collect_depth_relation_at(
                    node,
                    source,
                    &file_path_str,
                    from_for_rel.as_deref(),
                    &mut relations,
                );

                if PHP_CALL_KINDS.contains(&kind) {
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
                    if PHP_COMPLEXITY_BRANCH_KINDS.contains(&kind) {
                        let delta = 1 + ctx
                            .scopes
                            .current_function()
                            .map(|f| f.complexity.cognitive_nesting)
                            .unwrap_or(0);
                        ctx.scopes.note_branch(delta);
                        ctx.scopes.enter_branch();
                        let d = ctx.scopes.depth();
                        ctx.scopes.push(ScopeFrame::new(
                            ScopeKind::GuardBlock,
                            None,
                            String::new(),
                            node.start_byte(),
                            node.end_byte(),
                            d,
                        ));
                        stack.push(Visit::LeaveGuard);
                    } else if kind == "return_statement" {
                        ctx.scopes.note_return();
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

    let import_map = import_map_from_symbols(&ctx.symbols);
    let function_symbols: Vec<&Symbol> = ctx
        .symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Function)
        .collect();
    for pending in pending_calls {
        push_php_call_relation(
            pending.node,
            source,
            file_path,
            &ctx.symbols,
            &function_symbols,
            &import_map,
            &mut relations,
        );
    }

    plugin.extract_inheritance(root, source, file_path, None, &mut relations)?;

    let _ = PHP_FUNCTION_KINDS;

    rgctl_plugin_helpers::apply_framework_catalog(
        "php",
        source,
        Some(root),
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}
