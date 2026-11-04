//! Single-pass AST extraction for Java (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::{BRANCH_KINDS, ExtractCtx, JAVA_CALL_KINDS, JavaPlugin};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../java-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("annotated_type", HandlerMask::RELATION),
    ("annotation", HandlerMask::RELATION),
    ("annotation_type_declaration", HandlerMask::SYMBOL),
    ("annotation_type_element_declaration", HandlerMask::SYMBOL),
    ("array_creation_expression", HandlerMask::RELATION),
    ("assert_statement", HandlerMask::CFG_STMT),
    ("assignment_expression", HandlerMask::AST_SKELETON),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("block", HandlerMask::CFG_STMT),
    ("break_statement", HandlerMask::CFG_STMT),
    ("catch_clause", HandlerMask::CFG_STMT),
    ("catch_formal_parameter", HandlerMask::AST_SKELETON),
    ("class_declaration", HandlerMask::SYMBOL),
    ("class_literal", HandlerMask::RELATION),
    ("compact_constructor_declaration", HandlerMask::SYMBOL),
    ("constant_declaration", HandlerMask::SYMBOL),
    ("constructor_declaration", HandlerMask::SYMBOL),
    ("continue_statement", HandlerMask::CFG_STMT),
    ("do_statement", HandlerMask::CFG_STMT),
    ("enhanced_for_statement", HandlerMask::CFG_STMT),
    ("enum_constant", HandlerMask::SYMBOL),
    ("enum_declaration", HandlerMask::SYMBOL),
    ("explicit_constructor_invocation", HandlerMask::RELATION),
    ("exports_module_directive", HandlerMask::RELATION),
    ("extends_interfaces", HandlerMask::RELATION),
    ("field_access", HandlerMask::RELATION),
    ("field_declaration", HandlerMask::SYMBOL),
    ("finally_clause", HandlerMask::CFG_STMT),
    ("for_statement", HandlerMask::CFG_STMT),
    ("formal_parameter", HandlerMask::AST_SKELETON),
    ("formal_parameters", HandlerMask::AST_SKELETON),
    ("guard", HandlerMask::CFG_STMT),
    ("if_statement", HandlerMask::CFG_STMT),
    ("import_declaration", HandlerMask::RELATION),
    ("inferred_parameters", HandlerMask::AST_SKELETON),
    ("instanceof_expression", HandlerMask::CFG_STMT),
    ("interface_declaration", HandlerMask::SYMBOL),
    ("labeled_statement", HandlerMask::CFG_STMT),
    ("lambda_expression", HandlerMask::SYMBOL),
    ("local_variable_declaration", HandlerMask::AST_SKELETON),
    ("marker_annotation", HandlerMask::RELATION),
    ("method_declaration", HandlerMask::SYMBOL),
    ("method_invocation", HandlerMask::RELATION),
    ("method_reference", HandlerMask::RELATION),
    ("module_declaration", HandlerMask::SYMBOL),
    ("object_creation_expression", HandlerMask::RELATION),
    ("opens_module_directive", HandlerMask::RELATION),
    ("package_declaration", HandlerMask::SYMBOL),
    ("permits", HandlerMask::RELATION),
    ("provides_module_directive", HandlerMask::RELATION),
    ("record_declaration", HandlerMask::SYMBOL),
    ("record_pattern", HandlerMask::AST_SKELETON),
    ("record_pattern_component", HandlerMask::AST_SKELETON),
    ("requires_module_directive", HandlerMask::RELATION),
    ("resource", HandlerMask::CFG_STMT),
    ("resource_specification", HandlerMask::CFG_STMT),
    ("return_statement", HandlerMask::CFG_STMT),
    ("spread_parameter", HandlerMask::AST_SKELETON),
    ("static_initializer", HandlerMask::SYMBOL),
    ("super_interfaces", HandlerMask::RELATION),
    ("superclass", HandlerMask::RELATION),
    ("switch_block", HandlerMask::CFG_STMT),
    ("switch_block_statement_group", HandlerMask::CFG_STMT),
    ("switch_expression", HandlerMask::CFG_STMT),
    ("switch_label", HandlerMask::CFG_STMT),
    ("switch_rule", HandlerMask::CFG_STMT),
    ("synchronized_statement", HandlerMask::CFG_STMT),
    ("ternary_expression", HandlerMask::CFG_STMT),
    ("throw_statement", HandlerMask::CFG_STMT),
    ("throws", HandlerMask::RELATION),
    ("try_statement", HandlerMask::CFG_STMT),
    ("try_with_resources_statement", HandlerMask::CFG_STMT),
    ("type_arguments", HandlerMask::AST_SKELETON),
    ("type_bound", HandlerMask::AST_SKELETON),
    ("type_parameter", HandlerMask::AST_SKELETON),
    ("type_parameters", HandlerMask::AST_SKELETON),
    ("type_pattern", HandlerMask::AST_SKELETON),
    ("unary_expression", HandlerMask::AST_SKELETON),
    ("update_expression", HandlerMask::AST_SKELETON),
    ("uses_module_directive", HandlerMask::RELATION),
    ("variable_declarator", HandlerMask::AST_SKELETON),
    ("while_statement", HandlerMask::CFG_STMT),
    ("yield_statement", HandlerMask::CFG_STMT),
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn java_grammar() -> tree_sitter::Language {
    tree_sitter_java::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn java_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&java_grammar(), HANDLED_KINDS))
}

fn java_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&java_grammar(), MANIFEST_JSON))
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

/// Single iterative DFS: symbols + deferred calls + complexity; then non-call relations.
pub fn extract_all_single_pass(
    plugin: &JavaPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("java", java_grammar(), source).map_err(|e| Error::ParseError {
        file: file_path.to_path_buf(),
        line: 0,
        message: e,
    })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let extract_ctx = ExtractCtx::new(root, source);
    let dispatch = java_dispatch();

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
                    "method_declaration" => {
                        let sym =
                            plugin.extract_method(node, source, &file_path_str, &extract_ctx)?;
                        enter_function_scope(&mut ctx, node, sym, ScopeKind::Method);
                        pushed_scope = true;
                    }
                    "constructor_declaration" => {
                        let sym = plugin.extract_constructor(
                            node,
                            source,
                            &file_path_str,
                            &extract_ctx,
                        )?;
                        enter_function_scope(&mut ctx, node, sym, ScopeKind::Function);
                        pushed_scope = true;
                    }
                    "compact_constructor_declaration" => {
                        let sym = plugin.extract_compact_constructor(
                            node,
                            source,
                            &file_path_str,
                            &extract_ctx,
                        )?;
                        enter_function_scope(&mut ctx, node, sym, ScopeKind::Function);
                        pushed_scope = true;
                    }
                    "static_initializer" => {
                        let sym = plugin.extract_static_initializer(
                            node,
                            source,
                            &file_path_str,
                            &extract_ctx,
                        )?;
                        enter_function_scope(&mut ctx, node, sym, ScopeKind::Function);
                        pushed_scope = true;
                    }
                    "block" if node.parent().map(|p| p.kind()) == Some("class_body") => {
                        let sym = plugin.extract_instance_initializer(
                            node,
                            source,
                            &file_path_str,
                            &extract_ctx,
                        )?;
                        enter_function_scope(&mut ctx, node, sym, ScopeKind::Function);
                        pushed_scope = true;
                    }
                    "class_declaration" => {
                        let sym = plugin.extract_type(
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Class,
                            &extract_ctx,
                        )?;
                        enter_type_scope(&mut ctx, node, sym, ScopeKind::Class);
                        pushed_scope = true;
                    }
                    "interface_declaration" => {
                        let sym = plugin.extract_type(
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Interface,
                            &extract_ctx,
                        )?;
                        enter_type_scope(&mut ctx, node, sym, ScopeKind::Interface);
                        pushed_scope = true;
                    }
                    "enum_declaration" => {
                        let sym = plugin.extract_type(
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Enum,
                            &extract_ctx,
                        )?;
                        enter_type_scope(&mut ctx, node, sym, ScopeKind::Enum);
                        pushed_scope = true;
                    }
                    "annotation_type_declaration" => {
                        let sym = plugin.extract_type(
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Annotation,
                            &extract_ctx,
                        )?;
                        enter_type_scope(&mut ctx, node, sym, ScopeKind::Interface);
                        pushed_scope = true;
                    }
                    "record_declaration" => {
                        let sym =
                            plugin.extract_record(node, source, &file_path_str, &extract_ctx)?;
                        enter_type_scope(&mut ctx, node, sym, ScopeKind::Class);
                        pushed_scope = true;
                    }
                    "annotation_type_element_declaration" => {
                        ctx.symbols.push(plugin.extract_annotation_element(
                            node,
                            source,
                            &file_path_str,
                            &extract_ctx,
                        )?);
                    }
                    "lambda_expression" => {
                        let sym =
                            plugin.extract_lambda(node, source, &file_path_str, &extract_ctx)?;
                        enter_function_scope(&mut ctx, node, sym, ScopeKind::Lambda);
                        pushed_scope = true;
                    }
                    "package_declaration" => {
                        ctx.symbols.push(plugin.extract_package_symbol(
                            node,
                            source,
                            &file_path_str,
                        )?);
                    }
                    "module_declaration" => {
                        let sym = plugin.extract_module_symbol(node, source, &file_path_str)?;
                        let name = sym.name.clone();
                        let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                        ctx.symbols.push(sym);
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
                    "field_declaration" => {
                        let owner_qn = ctx
                            .scopes
                            .current_type()
                            .map(|f| f.qualified_prefix.clone());
                        plugin.push_field_declaration_variables(
                            node,
                            source,
                            &file_path_str,
                            owner_qn.as_deref(),
                            &mut ctx.symbols,
                        );
                    }
                    "import_declaration" => {
                        let text = node.utf8_text(source)?.trim().to_string();
                        ctx.symbols.push(Symbol {
                            name: text.clone(),
                            symbol_type: SymbolType::Import,
                            qualified_name: None,
                            location: SourceLocation {
                                file: file_path_str.to_string(),
                                start_line: node.start_position().row + 1,
                                end_line: node.end_position().row + 1,
                                start_column: 0,
                                end_column: 0,
                            },
                            signature: None,
                            return_type: None,
                            parameters: vec![],
                            fields: vec![],
                            modifiers: vec![],
                            documentation: None,
                            metadata: serde_json::json!({ "language": "java" }),
                        });
                    }
                    _ => {}
                }

                if JAVA_CALL_KINDS.contains(&kind) {
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
                        k if BRANCH_KINDS.contains(&k) => {
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
                                    | "enhanced_for_statement"
                                    | "do_statement"
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
                        "binary_expression" => {
                            if let Some(op) = node.child_by_field_name("operator")
                                && matches!(op.kind(), "&&" | "||")
                            {
                                ctx.scopes.note_branch(1);
                            }
                        }
                        "switch_label" => {
                            let mut cursor = node.walk();
                            if node.children(&mut cursor).next().is_some() {
                                let nest = ctx
                                    .scopes
                                    .current_function()
                                    .map(|f| f.complexity.cognitive_nesting)
                                    .unwrap_or(0);
                                ctx.scopes.note_branch(1 + nest);
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

    plugin.apply_lombok_synthesis(root, source, &file_path_str, &extract_ctx, &mut ctx.symbols);
    let same_file_types = JavaPlugin::same_file_types(&ctx.symbols);
    plugin.enrich_symbol_type_fqns(&extract_ctx, &same_file_types, &mut ctx.symbols);

    // Java-specific flush: resolve receivers via ExtractCtx (imports / static
    // imports / locals / fields) rather than the language-agnostic hint path.
    plugin.flush_pending_calls(
        &pending_calls
            .iter()
            .map(|p| (p.node, p.from_symbol_idx))
            .collect::<Vec<_>>(),
        source,
        file_path,
        &extract_ctx,
        &ctx.symbols,
        &mut relations,
    );

    plugin.collect_non_call_relations(
        root,
        source,
        file_path,
        &extract_ctx,
        &ctx.symbols,
        &mut relations,
    )?;

    rgctl_plugin_helpers::apply_framework_catalog(
        "java",
        source,
        Some(root),
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}

fn enter_function_scope(ctx: &mut SinglePassCtx, node: Node, sym: Symbol, kind: ScopeKind) {
    let params = sym.parameters.len() as u32;
    let loc = ((node.end_position().row - node.start_position().row) + 1).max(1) as u32;
    let name = sym.name.clone();
    let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
    ctx.symbols.push(sym);
    ctx.enter_scope(
        kind,
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
}

fn enter_type_scope(ctx: &mut SinglePassCtx, node: Node, sym: Symbol, kind: ScopeKind) {
    let name = sym.name.clone();
    let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
    ctx.symbols.push(sym);
    ctx.enter_scope(
        kind,
        Some(name),
        qn,
        node.start_byte(),
        node.end_byte(),
        true,
    );
}
