//! Single-pass AST extraction for TypeScript (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::{TypeScriptPlugin, source_location};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{
    extract_class_extends_relations, extract_import_symbols, find_child_kind,
    is_ecmascript_function_node, is_function_expression_kind, simple_type_name,
    type_name_from_node,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../typescript-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("abstract_class_declaration", HandlerMask::SYMBOL),
    ("abstract_method_signature", HandlerMask::SYMBOL),
    ("accessibility_modifier", HandlerMask::AST_SKELETON),
    ("ambient_declaration", HandlerMask::SYMBOL),
    ("array_pattern", HandlerMask::AST_SKELETON),
    ("arrow_function", HandlerMask::SYMBOL),
    ("as_expression", HandlerMask::AST_SKELETON),
    ("assignment_expression", HandlerMask::AST_SKELETON),
    ("assignment_pattern", HandlerMask::AST_SKELETON),
    ("augmented_assignment_expression", HandlerMask::AST_SKELETON),
    ("await_expression", HandlerMask::CFG_STMT),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("break_statement", HandlerMask::CFG_STMT),
    ("call_expression", HandlerMask::RELATION),
    ("catch_clause", HandlerMask::CFG_STMT),
    ("class", HandlerMask::SYMBOL),
    ("class_declaration", HandlerMask::SYMBOL),
    ("class_heritage", HandlerMask::RELATION),
    ("class_static_block", HandlerMask::SYMBOL),
    ("continue_statement", HandlerMask::CFG_STMT),
    ("debugger_statement", HandlerMask::CFG_STMT),
    ("decorator", HandlerMask::RELATION),
    ("do_statement", HandlerMask::CFG_STMT),
    ("else_clause", HandlerMask::CFG_STMT),
    ("empty_statement", HandlerMask::CFG_STMT),
    ("enum_assignment", HandlerMask::SYMBOL),
    ("enum_declaration", HandlerMask::SYMBOL),
    ("export_clause", HandlerMask::RELATION),
    ("export_specifier", HandlerMask::RELATION),
    ("export_statement", HandlerMask::RELATION),
    ("extends_clause", HandlerMask::RELATION),
    ("extends_type_clause", HandlerMask::RELATION),
    ("finally_clause", HandlerMask::CFG_STMT),
    ("for_in_statement", HandlerMask::CFG_STMT),
    ("for_statement", HandlerMask::CFG_STMT),
    ("formal_parameters", HandlerMask::AST_SKELETON),
    ("function_declaration", HandlerMask::SYMBOL),
    ("function_expression", HandlerMask::SYMBOL),
    ("generator_function", HandlerMask::SYMBOL),
    ("generator_function_declaration", HandlerMask::SYMBOL),
    ("if_statement", HandlerMask::CFG_STMT),
    ("implements_clause", HandlerMask::RELATION),
    ("import", HandlerMask::RELATION),
    ("import_alias", HandlerMask::RELATION),
    ("import_attribute", HandlerMask::RELATION),
    ("import_clause", HandlerMask::RELATION),
    ("import_require_clause", HandlerMask::RELATION),
    ("import_specifier", HandlerMask::RELATION),
    ("import_statement", HandlerMask::RELATION),
    ("instantiation_expression", HandlerMask::RELATION),
    ("interface_declaration", HandlerMask::SYMBOL),
    ("internal_module", HandlerMask::SYMBOL),
    ("labeled_statement", HandlerMask::CFG_STMT),
    ("lexical_declaration", HandlerMask::AST_SKELETON),
    ("mapped_type_clause", HandlerMask::AST_SKELETON),
    ("member_expression", HandlerMask::RELATION),
    ("method_definition", HandlerMask::SYMBOL),
    ("method_signature", HandlerMask::SYMBOL),
    ("module", HandlerMask::SYMBOL),
    ("named_imports", HandlerMask::RELATION),
    ("namespace_export", HandlerMask::RELATION),
    ("namespace_import", HandlerMask::RELATION),
    ("new_expression", HandlerMask::RELATION),
    ("non_null_expression", HandlerMask::AST_SKELETON),
    ("object_assignment_pattern", HandlerMask::AST_SKELETON),
    ("object_pattern", HandlerMask::AST_SKELETON),
    ("optional_parameter", HandlerMask::AST_SKELETON),
    ("pair_pattern", HandlerMask::AST_SKELETON),
    ("property_signature", HandlerMask::SYMBOL),
    ("public_field_definition", HandlerMask::SYMBOL),
    ("required_parameter", HandlerMask::AST_SKELETON),
    ("rest_pattern", HandlerMask::AST_SKELETON),
    ("return_statement", HandlerMask::CFG_STMT),
    ("satisfies_expression", HandlerMask::AST_SKELETON),
    ("sequence_expression", HandlerMask::AST_SKELETON),
    (
        "shorthand_property_identifier_pattern",
        HandlerMask::AST_SKELETON,
    ),
    ("statement_block", HandlerMask::CFG_STMT),
    ("subscript_expression", HandlerMask::RELATION),
    ("switch_case", HandlerMask::CFG_STMT),
    ("switch_default", HandlerMask::CFG_STMT),
    ("switch_statement", HandlerMask::CFG_STMT),
    ("ternary_expression", HandlerMask::CFG_STMT),
    ("throw_statement", HandlerMask::CFG_STMT),
    ("try_statement", HandlerMask::CFG_STMT),
    ("type_alias_declaration", HandlerMask::SYMBOL),
    ("unary_expression", HandlerMask::AST_SKELETON),
    ("update_expression", HandlerMask::AST_SKELETON),
    ("variable_declaration", HandlerMask::AST_SKELETON),
    ("variable_declarator", HandlerMask::AST_SKELETON),
    ("while_statement", HandlerMask::CFG_STMT),
    ("with_statement", HandlerMask::CFG_STMT),
    ("yield_expression", HandlerMask::CFG_STMT),
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn ts_grammar() -> tree_sitter::Language {
    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn typescript_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&ts_grammar(), HANDLED_KINDS))
}

fn ts_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&ts_grammar(), MANIFEST_JSON))
}

fn function_scope_kind(node: Node) -> ScopeKind {
    match node.kind() {
        "method_definition" => ScopeKind::Method,
        k if is_function_expression_kind(k)
            || (k == "function"
                && node.is_named()
                && node.child_by_field_name("name").is_none()) =>
        {
            ScopeKind::Lambda
        }
        _ => ScopeKind::Function,
    }
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

/// Single iterative DFS: symbols + deferred calls + heritage/decorators/new + complexity.
pub fn extract_all_single_pass(
    plugin: &TypeScriptPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree =
        parse_with_pool("typescript", ts_grammar(), source).map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: e,
        })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = ts_dispatch();

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

                if is_ecmascript_function_node(node) {
                    let sym = plugin.extract_function(node, source, &file_path_str)?;
                    let params = sym.parameters.len() as u32;
                    let loc =
                        ((node.end_position().row - node.start_position().row) + 1).max(1) as u32;
                    let name = sym.name.clone();
                    let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                    let sk = function_scope_kind(node);
                    ctx.symbols.push(sym);
                    ctx.enter_scope(sk, Some(name), qn, node.start_byte(), node.end_byte(), true);
                    if let Some(f) = ctx.scopes.current_function_mut() {
                        f.complexity.cyclomatic = 1;
                        f.complexity.param_count = params;
                        f.complexity.loc = loc;
                    }
                    pushed_scope = true;
                    plugin.emit_decorators_for_node(node, source, file_path, &mut relations);
                }

                match kind {
                    "class_declaration" | "abstract_class_declaration" => {
                        let sym = plugin.extract_class(node, source, &file_path_str)?;
                        let name = sym.name.clone();
                        ctx.symbols.push(sym);
                        ctx.enter_scope(
                            ScopeKind::Class,
                            Some(name.clone()),
                            name.clone(),
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        pushed_scope = true;

                        extract_class_extends_relations(
                            node,
                            source,
                            file_path,
                            &name,
                            "typescript",
                            &mut relations,
                        );
                        if let Some(heritage) = find_child_kind(node, "class_heritage") {
                            let mut hc = heritage.walk();
                            for child in heritage.children(&mut hc) {
                                if child.kind() == "implements_clause" {
                                    plugin.collect_implements(
                                        &name,
                                        child,
                                        source,
                                        file_path,
                                        &mut relations,
                                    );
                                }
                            }
                        }
                        plugin.emit_decorators_for_node(node, source, file_path, &mut relations);
                    }
                    "interface_declaration" => {
                        let iface = plugin.extract_interface(node, source, &file_path_str)?;
                        let iface_name = iface.name.clone();
                        ctx.symbols.push(iface);
                        ctx.enter_scope(
                            ScopeKind::Interface,
                            Some(iface_name.clone()),
                            iface_name.clone(),
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        pushed_scope = true;

                        if let Some(body) = node.child_by_field_name("body") {
                            let mut bc = body.walk();
                            for child in body.children(&mut bc) {
                                if child.kind() == "method_signature" {
                                    ctx.symbols.push(plugin.extract_method_signature(
                                        child,
                                        source,
                                        &file_path_str,
                                        &iface_name,
                                    )?);
                                }
                            }
                        }
                        if let Some(extends) = find_child_kind(node, "extends_type_clause") {
                            let mut ec = extends.walk();
                            for child in extends.children(&mut ec) {
                                if let Some(target) = type_name_from_node(child, source) {
                                    relations.push(Relation {
                                        from: iface_name.clone(),
                                        to: simple_type_name(&target),
                                        relation_type: RelationType::Extends,
                                        location: source_location(child, &file_path_str),
                                        metadata: serde_json::json!({ "language": "typescript" }),
                                        to_qualified_hint: Some(target),
                                        to_type_hint: None,
                                        ..Default::default()
                                    });
                                }
                            }
                        }
                    }
                    "enum_declaration" => {
                        let sym = plugin.extract_enum(node, source, &file_path_str)?;
                        let name = sym.name.clone();
                        ctx.symbols.push(sym);
                        ctx.enter_scope(
                            ScopeKind::Enum,
                            Some(name.clone()),
                            name,
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        pushed_scope = true;
                    }
                    "type_alias_declaration" => {
                        ctx.symbols.push(plugin.extract_type_alias(
                            node,
                            source,
                            &file_path_str,
                        )?);
                    }
                    "import_statement" => {
                        ctx.symbols.extend(extract_import_symbols(
                            node,
                            source,
                            &file_path_str,
                            "typescript",
                        ));
                    }
                    "export_statement" => {
                        if let Some(sym) =
                            plugin.extract_export_type_symbol(node, source, &file_path_str)
                        {
                            ctx.symbols.push(sym);
                        }
                    }
                    "export_specifier" => {
                        plugin.collect_export_specifier_relations(
                            node,
                            source,
                            &file_path_str,
                            &mut relations,
                        );
                    }
                    "abstract_method_signature" => {
                        if let Some(owner) = ctx.scopes.current_type().and_then(|f| f.name.clone())
                        {
                            ctx.symbols.push(plugin.extract_abstract_method_signature(
                                node,
                                source,
                                &file_path_str,
                                &owner,
                            )?);
                            plugin.emit_decorators_for_node(
                                node,
                                source,
                                file_path,
                                &mut relations,
                            );
                        }
                    }
                    "internal_module" | "module" => {
                        let sym = plugin.extract_internal_module(node, source, &file_path_str)?;
                        let name = sym.name.clone();
                        ctx.symbols.push(sym);
                        ctx.enter_scope(
                            ScopeKind::Module,
                            Some(name.clone()),
                            name,
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        pushed_scope = true;
                    }
                    "new_expression" => {
                        if let Some(from_fn) =
                            containing_function_from_scope(&ctx.scopes, &ctx.symbols)
                        {
                            let from = from_fn
                                .qualified_name
                                .clone()
                                .unwrap_or_else(|| from_fn.name.clone());
                            if let Some(ctor) = node.child_by_field_name("constructor")
                                && let Some(target) = type_name_from_node(ctor, source)
                            {
                                relations.push(Relation {
                                    from,
                                    to: simple_type_name(&target),
                                    relation_type: RelationType::Instantiates,
                                    location: source_location(node, &file_path_str),
                                    metadata: serde_json::json!({ "language": "typescript" }),
                                    to_qualified_hint: Some(target),
                                    to_type_hint: None,
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    _ => {}
                }

                if TS_CALL_KINDS.contains(&kind)
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
                        "if_statement" | "switch_statement" | "while_statement"
                        | "for_statement" | "for_in_statement" | "catch_clause"
                        | "ternary_expression" => {
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
                        }
                        "case" | "switch_case" => {
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

    let class_fields: HashMap<String, Vec<Field>> = ctx
        .symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Class)
        .map(|s| (s.name.clone(), s.fields.clone()))
        .collect();

    for pending in pending_calls {
        let Some(from_fn) = ctx.symbols.get(pending.from_symbol_idx) else {
            continue;
        };
        let before = relations.len();
        plugin.emit_ts_call(
            pending.node,
            source,
            file_path,
            &ctx.symbols,
            from_fn,
            &class_fields,
            &mut relations,
        );
        if relations.len() == before {
            push_call_relation_from(
                pending.node,
                source,
                file_path,
                &ctx.symbols,
                from_fn,
                "typescript",
                &mut relations,
            );
        }
    }

    rgctl_plugin_helpers::apply_framework_catalog(
        "typescript",
        source,
        Some(root),
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}
