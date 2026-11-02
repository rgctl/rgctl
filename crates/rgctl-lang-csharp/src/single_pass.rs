//! Single-pass AST extraction for C# (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::{CSharpPlugin, ExtractCtx};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../csharp-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("accessor_declaration", HandlerMask::SYMBOL),
    ("and_pattern", HandlerMask::AST_SKELETON),
    ("anonymous_method_expression", HandlerMask::SYMBOL),
    ("anonymous_object_creation_expression", HandlerMask::RELATION),
    ("argument", HandlerMask::AST_SKELETON),
    ("argument_list", HandlerMask::AST_SKELETON),
    ("array_creation_expression", HandlerMask::RELATION),
    ("arrow_expression_clause", HandlerMask::AST_SKELETON),
    ("as_expression", HandlerMask::CFG_STMT),
    ("assignment_expression", HandlerMask::AST_SKELETON),
    ("attribute", HandlerMask::RELATION),
    ("await_expression", HandlerMask::CFG_STMT),
    ("base_list", HandlerMask::RELATION),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("block", HandlerMask::CFG_STMT),
    ("bracketed_argument_list", HandlerMask::AST_SKELETON),
    ("bracketed_parameter_list", HandlerMask::AST_SKELETON),
    ("break_statement", HandlerMask::CFG_STMT),
    ("catch_clause", HandlerMask::CFG_STMT),
    ("catch_declaration", HandlerMask::CFG_STMT),
    ("catch_filter_clause", HandlerMask::CFG_STMT),
    ("checked_expression", HandlerMask::CFG_STMT),
    ("checked_statement", HandlerMask::CFG_STMT),
    ("class_declaration", HandlerMask::SYMBOL),
    ("collection_element", HandlerMask::AST_SKELETON),
    ("collection_expression", HandlerMask::CFG_STMT),
    ("conditional_access_expression", HandlerMask::RELATION),
    ("conditional_expression", HandlerMask::CFG_STMT),
    ("constant_pattern", HandlerMask::AST_SKELETON),
    ("constructor_constraint", HandlerMask::AST_SKELETON),
    ("constructor_declaration", HandlerMask::SYMBOL),
    ("constructor_initializer", HandlerMask::AST_SKELETON),
    ("continue_statement", HandlerMask::CFG_STMT),
    ("conversion_operator_declaration", HandlerMask::SYMBOL),
    ("declaration_expression", HandlerMask::AST_SKELETON),
    ("declaration_pattern", HandlerMask::AST_SKELETON),
    ("default_expression", HandlerMask::AST_SKELETON),
    ("delegate_declaration", HandlerMask::SYMBOL),
    ("destructor_declaration", HandlerMask::SYMBOL),
    ("discard", HandlerMask::AST_SKELETON),
    ("do_statement", HandlerMask::CFG_STMT),
    ("element_access_expression", HandlerMask::RELATION),
    ("element_binding_expression", HandlerMask::AST_SKELETON),
    ("empty_statement", HandlerMask::CFG_STMT),
    ("enum_declaration", HandlerMask::SYMBOL),
    ("enum_member_declaration", HandlerMask::SYMBOL),
    ("event_declaration", HandlerMask::SYMBOL),
    ("event_field_declaration", HandlerMask::SYMBOL),
    ("expression_element", HandlerMask::AST_SKELETON),
    ("extern_alias_directive", HandlerMask::RELATION),
    ("field_declaration", HandlerMask::SYMBOL),
    ("file_scoped_namespace_declaration", HandlerMask::SYMBOL),
    ("finally_clause", HandlerMask::CFG_STMT),
    ("fixed_statement", HandlerMask::CFG_STMT),
    ("for_statement", HandlerMask::CFG_STMT),
    ("foreach_statement", HandlerMask::CFG_STMT),
    ("from_clause", HandlerMask::CFG_STMT),
    ("function_pointer_parameter", HandlerMask::AST_SKELETON),
    ("global_attribute", HandlerMask::RELATION),
    ("global_statement", HandlerMask::CFG_STMT),
    ("goto_statement", HandlerMask::CFG_STMT),
    ("group_clause", HandlerMask::CFG_STMT),
    ("if_statement", HandlerMask::CFG_STMT),
    ("implicit_array_creation_expression", HandlerMask::RELATION),
    ("implicit_object_creation_expression", HandlerMask::RELATION),
    ("implicit_parameter", HandlerMask::AST_SKELETON),
    ("implicit_stackalloc_expression", HandlerMask::RELATION),
    ("indexer_declaration", HandlerMask::SYMBOL),
    ("initializer_expression", HandlerMask::AST_SKELETON),
    ("interface_declaration", HandlerMask::SYMBOL),
    ("interpolation_alignment_clause", HandlerMask::AST_SKELETON),
    ("interpolation_format_clause", HandlerMask::AST_SKELETON),
    ("invocation_expression", HandlerMask::RELATION),
    ("is_expression", HandlerMask::CFG_STMT),
    ("is_pattern_expression", HandlerMask::CFG_STMT),
    ("join_clause", HandlerMask::CFG_STMT),
    ("join_into_clause", HandlerMask::CFG_STMT),
    ("labeled_statement", HandlerMask::CFG_STMT),
    ("lambda_expression", HandlerMask::SYMBOL),
    ("let_clause", HandlerMask::CFG_STMT),
    ("list_pattern", HandlerMask::AST_SKELETON),
    ("local_declaration_statement", HandlerMask::AST_SKELETON),
    ("local_function_statement", HandlerMask::SYMBOL),
    ("lock_statement", HandlerMask::CFG_STMT),
    ("makeref_expression", HandlerMask::AST_SKELETON),
    ("member_access_expression", HandlerMask::RELATION),
    ("member_binding_expression", HandlerMask::AST_SKELETON),
    ("method_declaration", HandlerMask::SYMBOL),
    ("namespace_declaration", HandlerMask::SYMBOL),
    ("negated_pattern", HandlerMask::AST_SKELETON),
    ("object_creation_expression", HandlerMask::RELATION),
    ("operator_declaration", HandlerMask::SYMBOL),
    ("or_pattern", HandlerMask::AST_SKELETON),
    ("order_by_clause", HandlerMask::CFG_STMT),
    ("parameter", HandlerMask::AST_SKELETON),
    ("parameter_list", HandlerMask::AST_SKELETON),
    ("parenthesized_pattern", HandlerMask::AST_SKELETON),
    ("parenthesized_variable_designation", HandlerMask::AST_SKELETON),
    ("positional_pattern_clause", HandlerMask::AST_SKELETON),
    ("postfix_unary_expression", HandlerMask::CFG_STMT),
    ("prefix_unary_expression", HandlerMask::CFG_STMT),
    ("primary_constructor_base_type", HandlerMask::AST_SKELETON),
    ("property_declaration", HandlerMask::SYMBOL),
    ("property_pattern_clause", HandlerMask::AST_SKELETON),
    ("query_expression", HandlerMask::CFG_STMT),
    ("range_expression", HandlerMask::CFG_STMT),
    ("record_declaration", HandlerMask::SYMBOL),
    ("recursive_pattern", HandlerMask::AST_SKELETON),
    ("ref_expression", HandlerMask::AST_SKELETON),
    ("reftype_expression", HandlerMask::AST_SKELETON),
    ("refvalue_expression", HandlerMask::AST_SKELETON),
    ("relational_pattern", HandlerMask::AST_SKELETON),
    ("return_statement", HandlerMask::CFG_STMT),
    ("select_clause", HandlerMask::CFG_STMT),
    ("sizeof_expression", HandlerMask::RELATION),
    ("spread_element", HandlerMask::AST_SKELETON),
    ("stackalloc_expression", HandlerMask::RELATION),
    ("struct_declaration", HandlerMask::SYMBOL),
    ("subpattern", HandlerMask::AST_SKELETON),
    ("switch_body", HandlerMask::CFG_STMT),
    ("switch_expression", HandlerMask::CFG_STMT),
    ("switch_expression_arm", HandlerMask::CFG_STMT),
    ("switch_section", HandlerMask::CFG_STMT),
    ("switch_statement", HandlerMask::CFG_STMT),
    ("throw_expression", HandlerMask::CFG_STMT),
    ("throw_statement", HandlerMask::CFG_STMT),
    ("try_statement", HandlerMask::CFG_STMT),
    ("tuple_element", HandlerMask::AST_SKELETON),
    ("tuple_expression", HandlerMask::AST_SKELETON),
    ("tuple_pattern", HandlerMask::AST_SKELETON),
    ("type_parameter", HandlerMask::AST_SKELETON),
    ("type_parameter_constraint", HandlerMask::AST_SKELETON),
    ("type_parameter_constraints_clause", HandlerMask::AST_SKELETON),
    ("type_parameter_list", HandlerMask::AST_SKELETON),
    ("type_pattern", HandlerMask::AST_SKELETON),
    ("typeof_expression", HandlerMask::RELATION),
    ("unary_expression", HandlerMask::CFG_STMT),
    ("unsafe_statement", HandlerMask::CFG_STMT),
    ("using_directive", HandlerMask::SYMBOL),
    ("using_statement", HandlerMask::CFG_STMT),
    ("variable_declaration", HandlerMask::AST_SKELETON),
    ("variable_declarator", HandlerMask::AST_SKELETON),
    ("when_clause", HandlerMask::CFG_STMT),
    ("where_clause", HandlerMask::CFG_STMT),
    ("while_statement", HandlerMask::CFG_STMT),
    ("with_expression", HandlerMask::CFG_STMT),
    ("with_initializer", HandlerMask::AST_SKELETON),
    ("yield_statement", HandlerMask::CFG_STMT),
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn csharp_grammar() -> tree_sitter::Language {
    tree_sitter_c_sharp::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn csharp_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&csharp_grammar(), HANDLED_KINDS))
}

fn csharp_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&csharp_grammar(), MANIFEST_JSON))
}

struct PendingCall<'a> {
    node: Node<'a>,
    from_symbol_idx: usize,
}

enum Visit<'a> {
    Pre(Node<'a>, usize),
    LeaveScope,
    LeaveGuard,
    LeaveNamespace,
}

/// Single iterative DFS: symbols + deferred calls + complexity; then non-call relations.
pub fn extract_all_single_pass(
    plugin: &CSharpPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree =
        parse_with_pool("csharp", csharp_grammar(), source).map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: e,
        })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = csharp_dispatch();

    let mut extract_ctx = ExtractCtx::default();
    plugin.seed_ctx_from_tree(root, source, &mut extract_ctx);

    let mut ctx = SinglePassCtx::new(dispatch);
    let mut relations = Vec::new();
    let mut pending_calls: Vec<PendingCall<'_>> = Vec::new();

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
            Visit::LeaveNamespace => {
                extract_ctx.pop_namespace();
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

                if kind == "namespace_declaration" {
                    if let Some(ns) = crate::plugin::namespace_name(node, source) {
                        extract_ctx.push_qualified_namespace(&ns);
                        stack.push(Visit::LeaveNamespace);
                        if let Some(body) = node.child_by_field_name("body") {
                            for i in (0..body.child_count()).rev() {
                                if let Some(child) = body.child(i) {
                                    stack.push(Visit::Pre(child, depth + 1));
                                }
                            }
                        }
                    }
                    continue;
                }
                if kind == "file_scoped_namespace_declaration" {
                    // Namespace name was seeded into extract_ctx; members are compilation_unit siblings.
                    if let Some(ns) = crate::plugin::namespace_name(node, source) {
                        ctx.symbols.push(Symbol {
                            name: ns.clone(),
                            symbol_type: SymbolType::Module,
                            qualified_name: Some(ns),
                            location: crate::plugin::source_location(node, &file_path_str),
                            signature: None,
                            return_type: None,
                            parameters: vec![],
                            fields: vec![],
                            modifiers: vec![],
                            documentation: None,
                            metadata: serde_json::json!({
                                "language": "csharp",
                                "file_scoped": true
                            }),
                        });
                    }
                    continue;
                }

                let mut pushed_scope = false;

                match kind {
                    "method_declaration" | "local_function_statement" => {
                        let sym =
                            plugin.extract_method(node, source, &file_path_str, &extract_ctx)?;
                        let params = sym.parameters.len() as u32;
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
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
                    "constructor_declaration" => {
                        let sym = plugin.extract_constructor(
                            node,
                            source,
                            &file_path_str,
                            &extract_ctx,
                        )?;
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
                        pushed_scope = true;
                    }
                    "delegate_declaration" => {
                        ctx.symbols.push(plugin.extract_delegate(
                            node,
                            source,
                            &file_path_str,
                            &extract_ctx,
                        )?);
                    }
                    "class_declaration" | "struct_declaration" => {
                        let sym = plugin.extract_type(
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Class,
                            &extract_ctx,
                            serde_json::json!({}),
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
                    "record_declaration" => {
                        let sym = plugin.extract_type(
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Class,
                            &extract_ctx,
                            serde_json::json!({ "is_record": true }),
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
                        let sym = plugin.extract_type(
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Interface,
                            &extract_ctx,
                            serde_json::json!({}),
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
                    "enum_declaration" => {
                        let sym = plugin.extract_type(
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Enum,
                            &extract_ctx,
                            serde_json::json!({}),
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
                    "using_directive" => {
                        let text = node.utf8_text(source)?.trim().to_string();
                        extract_ctx.register_using(&text);
                        ctx.symbols.push(Symbol {
                            name: text.clone(),
                            symbol_type: SymbolType::Import,
                            qualified_name: None,
                            location: crate::plugin::source_location(node, &file_path_str),
                            signature: None,
                            return_type: None,
                            parameters: vec![],
                            fields: vec![],
                            modifiers: vec![],
                            documentation: None,
                            metadata: serde_json::json!({ "language": "csharp" }),
                        });
                    }
                    "indexer_declaration"
                    | "operator_declaration"
                    | "conversion_operator_declaration"
                    | "destructor_declaration"
                    | "accessor_declaration"
                    | "lambda_expression"
                    | "anonymous_method_expression" => {
                        ctx.symbols.push(plugin.extract_lightweight_callable(
                            node,
                            source,
                            &file_path_str,
                            &extract_ctx,
                        )?);
                    }
                    _ => {}
                }

                if CSHARP_CALL_KINDS.contains(&kind) {
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
                        | "while_statement"
                        | "for_statement"
                        | "foreach_statement"
                        | "catch_clause"
                        | "conditional_expression"
                        | "switch_expression"
                        | "switch_statement" => {
                            let delta = 1 + ctx
                                .scopes
                                .current_function()
                                .map(|f| f.complexity.cognitive_nesting)
                                .unwrap_or(0);
                            ctx.scopes.note_branch(delta);
                            if matches!(
                                kind,
                                "if_statement"
                                    | "while_statement"
                                    | "for_statement"
                                    | "foreach_statement"
                                    | "switch_statement"
                                    | "switch_expression"
                            ) {
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
                        }
                        "switch_section" => {
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
        push_call_relation_from(
            pending.node,
            source,
            file_path,
            &ctx.symbols,
            from_fn,
            "csharp",
            &mut relations,
        );
    }

    plugin.collect_non_call_relations(
        root,
        source,
        file_path,
        &ctx.symbols,
        &extract_ctx,
        &mut relations,
    )?;

    rgctl_plugin_helpers::apply_framework_catalog(
        "csharp",
        source,
        Some(root),
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}
