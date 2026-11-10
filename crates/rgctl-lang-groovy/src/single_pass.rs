//! Single-pass AST extraction for Groovy (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::{GROOVY_BRANCH_KINDS, GROOVY_CALL_KINDS, GroovyPlugin};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::record_symbol_spans;
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../groovy-ast-coverage.json");

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
    ("closure", HandlerMask::SYMBOL),
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
    ("function_definition", HandlerMask::SYMBOL),
    ("guard", HandlerMask::CFG_STMT),
    ("if_statement", HandlerMask::CFG_STMT),
    ("import_declaration", HandlerMask::RELATION),
    ("inferred_parameters", HandlerMask::AST_SKELETON),
    ("instanceof_expression", HandlerMask::CFG_STMT),
    ("interface_declaration", HandlerMask::SYMBOL),
    ("juxt_function_call", HandlerMask::RELATION),
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
    ("range_expression", HandlerMask::CFG_STMT),
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

fn groovy_grammar() -> tree_sitter::Language {
    tree_sitter_groovy::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn groovy_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&groovy_grammar(), HANDLED_KINDS))
}

fn groovy_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&groovy_grammar(), MANIFEST_JSON))
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

/// Single iterative DFS: symbols + deferred calls + extends/annotations + complexity.
pub fn extract_all_single_pass(
    plugin: &GroovyPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree =
        parse_with_pool("groovy", groovy_grammar(), source).map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 1,
            message: e,
        })?;
    let root = tree.root_node();
    let file = file_path.to_string_lossy();
    let package = GroovyPlugin::package_name(root, source);
    let dispatch = groovy_dispatch();

    let mut ctx = SinglePassCtx::new(dispatch);
    let mut relations = Vec::new();
    let mut pending_calls: Vec<PendingCall<'_>> = Vec::new();
    let mut depth_warned = false;
    let mut lambda_idx: u32 = 0;
    let mut closure_idx: u32 = 0;

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
                    "class_declaration"
                    | "interface_declaration"
                    | "enum_declaration"
                    | "annotation_type_declaration"
                    | "record_declaration" => {
                        if let Some(simple) = GroovyPlugin::type_name(node, source) {
                            let symbol_type = match kind {
                                "interface_declaration" => SymbolType::Interface,
                                "enum_declaration" => SymbolType::Enum,
                                "annotation_type_declaration" => SymbolType::Annotation,
                                _ => SymbolType::Class,
                            };
                            let scope_kind = match kind {
                                "interface_declaration" => ScopeKind::Interface,
                                "enum_declaration" => ScopeKind::Enum,
                                _ => ScopeKind::Class,
                            };
                            let qn = GroovyPlugin::qualify(package.as_deref(), &simple);
                            let mut metadata = serde_json::json!({ "language": "groovy" });
                            if kind == "record_declaration" {
                                metadata["is_record"] = serde_json::Value::Bool(true);
                            }
                            let mut symbol = Symbol {
                                name: simple.clone(),
                                symbol_type,
                                qualified_name: Some(qn.clone()),
                                location: GroovyPlugin::loc(&file, node),
                                signature: node
                                    .utf8_text(source)
                                    .ok()
                                    .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
                                return_type: None,
                                parameters: vec![],
                                fields: vec![],
                                modifiers: vec![],
                                documentation: None,
                                metadata,
                                ..Default::default()
                            };
                            record_symbol_spans(
                                &mut symbol,
                                node,
                                node.child_by_field_name("name"),
                            );
                            symbol = rgctl_plugin_helpers::callable_identity::symbol(
                                symbol,
                                node,
                                source,
                                rgctl_plugin_helpers::callable_identity::Policy::Groovy,
                            );
                            ctx.symbols.push(symbol);
                            ctx.enter_scope(
                                scope_kind,
                                Some(simple),
                                qn,
                                node.start_byte(),
                                node.end_byte(),
                                true,
                            );
                            pushed_scope = true;
                        }
                    }
                    "method_declaration" | "function_definition" => {
                        if let Some(sym) =
                            plugin.extract_method_symbol(node, source, &file, package.as_deref())
                        {
                            let params = sym.parameters.len() as u32;
                            let loc = ((node.end_position().row - node.start_position().row) + 1)
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
                    }
                    "constructor_declaration" | "compact_constructor_declaration" => {
                        if let Some(sym) = plugin.extract_constructor_symbol(
                            node,
                            source,
                            &file,
                            package.as_deref(),
                        ) {
                            let params = sym.parameters.len() as u32;
                            let loc = ((node.end_position().row - node.start_position().row) + 1)
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
                    }
                    "static_initializer" => {
                        if let Some(sym) = plugin.extract_static_initializer(
                            node,
                            source,
                            &file,
                            package.as_deref(),
                        ) {
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
                                f.complexity.loc =
                                    ((node.end_position().row - node.start_position().row) + 1)
                                        .max(1) as u32;
                            }
                            pushed_scope = true;
                        }
                    }
                    "lambda_expression" => {
                        lambda_idx += 1;
                        if let Some(sym) = plugin.extract_lambda_symbol(
                            node,
                            source,
                            &file,
                            package.as_deref(),
                            lambda_idx,
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
                            pushed_scope = true;
                        }
                    }
                    "closure" => {
                        closure_idx += 1;
                        if let Some(sym) = plugin.extract_closure_symbol(
                            node,
                            source,
                            &file,
                            package.as_deref(),
                            closure_idx,
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
                            pushed_scope = true;
                        }
                    }
                    "annotation_type_element_declaration" => {
                        if let Some(sym) = plugin.extract_annotation_element(
                            node,
                            source,
                            &file,
                            package.as_deref(),
                        ) {
                            ctx.symbols.push(sym);
                        }
                    }
                    "module_declaration" => {
                        if let Some(sym) = plugin.extract_module_symbol(node, source, &file) {
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
                    }
                    "package_declaration" => {
                        if let Some(sym) = plugin.extract_package_symbol(node, source, &file) {
                            ctx.symbols.push(sym);
                        }
                    }
                    "import_declaration" => {
                        if let Some(sym) = GroovyPlugin::extract_import_symbol(node, source, &file)
                        {
                            ctx.symbols.push(sym);
                        }
                    }
                    "field_declaration" | "constant_declaration" => {
                        if let Some(sym) =
                            plugin.extract_field_symbol(node, source, &file, package.as_deref())
                        {
                            ctx.symbols.push(sym);
                        }
                    }
                    "enum_constant" => {
                        if let Some(sym) =
                            plugin.extract_enum_constant(node, source, &file, package.as_deref())
                        {
                            ctx.symbols.push(sym);
                        }
                    }
                    _ => {}
                }

                if GROOVY_CALL_KINDS.contains(&kind) {
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
                    if GROOVY_BRANCH_KINDS.contains(&kind) {
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
            "groovy",
            &mut relations,
        );
        rgctl_plugin_helpers::callable_identity::call(
            pending.node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Groovy,
            ctx.symbols.get(pending.from_symbol_idx),
            &mut relations[identity_relation_start..],
        );
    }

    // Extends / implements / AnnotatedWith / field_access / array / class_literal / module.
    plugin.append_type_and_annotation_relations(root, source, file_path, &mut relations)?;

    rgctl_plugin_helpers::callable_identity::finish(
        rgctl_plugin_helpers::callable_identity::Policy::Groovy,
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}
