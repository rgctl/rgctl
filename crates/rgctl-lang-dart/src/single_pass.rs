//! Single-pass AST extraction for Dart (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::Plugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{extract_name_from_node, node_to_location};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../dart-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("additive_expression", HandlerMask::CFG_STMT),
    ("annotation", HandlerMask::RELATION),
    ("assert_statement", HandlerMask::CFG_STMT),
    ("assignable_expression", HandlerMask::AST_SKELETON),
    ("assignment_expression", HandlerMask::AST_SKELETON),
    ("await_expression", HandlerMask::CFG_STMT),
    ("bitwise_and_expression", HandlerMask::CFG_STMT),
    ("bitwise_or_expression", HandlerMask::CFG_STMT),
    ("bitwise_xor_expression", HandlerMask::CFG_STMT),
    ("break_statement", HandlerMask::CFG_STMT),
    ("call_expression", HandlerMask::RELATION),
    ("cascade_call_expression", HandlerMask::RELATION),
    ("cascade_index_expression", HandlerMask::RELATION),
    ("cascade_member_expression", HandlerMask::RELATION),
    ("cascade_null_assertion_expression", HandlerMask::CFG_STMT),
    ("cascade_null_aware_index_expression", HandlerMask::RELATION),
    (
        "cascade_null_aware_member_expression",
        HandlerMask::RELATION,
    ),
    ("cast_pattern", HandlerMask::AST_SKELETON),
    ("catch_clause", HandlerMask::CFG_STMT),
    ("class_declaration", HandlerMask::SYMBOL),
    ("conditional_expression", HandlerMask::CFG_STMT),
    ("const_object_expression", HandlerMask::RELATION),
    ("constant_constructor_signature", HandlerMask::SYMBOL),
    ("constant_pattern", HandlerMask::AST_SKELETON),
    ("constructor_param", HandlerMask::AST_SKELETON),
    ("constructor_signature", HandlerMask::SYMBOL),
    ("continue_statement", HandlerMask::CFG_STMT),
    ("do_statement", HandlerMask::CFG_STMT),
    ("empty_statement", HandlerMask::CFG_STMT),
    ("enum_constant", HandlerMask::SYMBOL),
    ("enum_declaration", HandlerMask::SYMBOL),
    ("equality_expression", HandlerMask::CFG_STMT),
    ("extension_declaration", HandlerMask::SYMBOL),
    ("extension_type_declaration", HandlerMask::SYMBOL),
    ("external_function_declaration", HandlerMask::SYMBOL),
    ("external_getter_declaration", HandlerMask::SYMBOL),
    ("external_setter_declaration", HandlerMask::SYMBOL),
    ("external_variable_declaration", HandlerMask::SYMBOL),
    ("factory_constructor_signature", HandlerMask::SYMBOL),
    ("finally_clause", HandlerMask::CFG_STMT),
    ("for_statement", HandlerMask::CFG_STMT),
    ("formal_parameter", HandlerMask::AST_SKELETON),
    ("formal_parameter_list", HandlerMask::AST_SKELETON),
    ("function_declaration", HandlerMask::SYMBOL),
    ("function_expression", HandlerMask::SYMBOL),
    ("getter_declaration", HandlerMask::SYMBOL),
    ("if_null_expression", HandlerMask::CFG_STMT),
    ("if_statement", HandlerMask::CFG_STMT),
    ("import_or_export", HandlerMask::RELATION),
    ("index_expression", HandlerMask::RELATION),
    ("initialized_identifier", HandlerMask::AST_SKELETON),
    ("initialized_identifier_list", HandlerMask::AST_SKELETON),
    ("initialized_variable_definition", HandlerMask::AST_SKELETON),
    ("instantiation_expression", HandlerMask::RELATION),
    ("interfaces", HandlerMask::RELATION),
    ("labeled_statement", HandlerMask::CFG_STMT),
    ("library_export", HandlerMask::RELATION),
    ("library_import", HandlerMask::RELATION),
    ("list_pattern", HandlerMask::AST_SKELETON),
    ("local_function_declaration", HandlerMask::SYMBOL),
    ("local_variable_declaration", HandlerMask::AST_SKELETON),
    ("logical_and_expression", HandlerMask::CFG_STMT),
    ("logical_or_expression", HandlerMask::CFG_STMT),
    ("map_pattern", HandlerMask::AST_SKELETON),
    ("member_expression", HandlerMask::RELATION),
    ("method_declaration", HandlerMask::SYMBOL),
    ("mixin_application_class", HandlerMask::SYMBOL),
    ("mixin_declaration", HandlerMask::SYMBOL),
    ("mixins", HandlerMask::RELATION),
    ("multiplicative_expression", HandlerMask::CFG_STMT),
    ("new_expression", HandlerMask::RELATION),
    ("null_assert_pattern", HandlerMask::AST_SKELETON),
    ("null_assertion_expression", HandlerMask::CFG_STMT),
    ("null_aware_index_expression", HandlerMask::RELATION),
    ("null_aware_member_expression", HandlerMask::RELATION),
    ("null_check_pattern", HandlerMask::AST_SKELETON),
    ("object_pattern", HandlerMask::AST_SKELETON),
    ("operator_signature", HandlerMask::SYMBOL),
    ("optional_formal_parameters", HandlerMask::AST_SKELETON),
    ("part_directive", HandlerMask::RELATION),
    ("part_of_directive", HandlerMask::RELATION),
    ("pattern_assignment", HandlerMask::AST_SKELETON),
    ("pattern_variable_declaration", HandlerMask::AST_SKELETON),
    ("postfix_expression", HandlerMask::CFG_STMT),
    ("record_pattern", HandlerMask::AST_SKELETON),
    (
        "redirecting_factory_constructor_signature",
        HandlerMask::SYMBOL,
    ),
    ("relational_expression", HandlerMask::CFG_STMT),
    ("rest_pattern", HandlerMask::AST_SKELETON),
    ("rethrow_statement", HandlerMask::CFG_STMT),
    ("return_statement", HandlerMask::CFG_STMT),
    ("setter_declaration", HandlerMask::SYMBOL),
    ("shift_expression", HandlerMask::CFG_STMT),
    ("static_final_declaration", HandlerMask::SYMBOL),
    ("super_formal_parameter", HandlerMask::AST_SKELETON),
    ("superclass", HandlerMask::RELATION),
    ("switch_expression", HandlerMask::CFG_STMT),
    ("switch_statement", HandlerMask::CFG_STMT),
    ("throw_expression", HandlerMask::CFG_STMT),
    ("top_level_variable_declaration", HandlerMask::AST_SKELETON),
    ("try_statement", HandlerMask::CFG_STMT),
    ("type_alias", HandlerMask::SYMBOL),
    ("type_arguments", HandlerMask::AST_SKELETON),
    ("type_cast_expression", HandlerMask::CFG_STMT),
    ("type_parameter", HandlerMask::AST_SKELETON),
    ("type_parameters", HandlerMask::AST_SKELETON),
    ("type_test_expression", HandlerMask::CFG_STMT),
    ("unary_expression", HandlerMask::CFG_STMT),
    ("variable_pattern", HandlerMask::AST_SKELETON),
    ("while_statement", HandlerMask::CFG_STMT),
    ("yield_each_statement", HandlerMask::CFG_STMT),
    ("yield_statement", HandlerMask::CFG_STMT),
    ("superclass", HandlerMask::RELATION),
];

const DART_CALL_KINDS: &[&str] = &[
    "call_expression",
    "cascade_call_expression",
    "new_expression",
    "const_object_expression",
    "instantiation_expression",
];
const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn grammar() -> tree_sitter::Language {
    tree_sitter_dart::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&grammar(), HANDLED_KINDS))
}

fn dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&grammar(), MANIFEST_JSON))
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

fn push_type_symbol(
    ctx: &mut SinglePassCtx,
    node: Node,
    source: &[u8],
    file_path: &str,
    symbol_type: SymbolType,
    kind_meta: &str,
    scope_kind: ScopeKind,
) -> bool {
    let Some(name) = extract_name_from_node(node, source).ok().flatten() else {
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
                    "language": "dart",
                    "kind": kind_meta,
                }),
                ..Default::default()
            },
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Dart,
        ));
    ctx.enter_scope(
        scope_kind,
        Some(name),
        qn,
        node.start_byte(),
        node.end_byte(),
        true,
    );
    true
}

fn push_function(
    ctx: &mut SinglePassCtx,
    node: Node,
    source: &[u8],
    file_path: &str,
    name: String,
    kind_meta: &str,
) -> bool {
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
                symbol_type: SymbolType::Function,
                qualified_name: Some(qn.clone()),
                location: node_to_location(node, file_path),
                signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: serde_json::json!({
                    "language": "dart",
                    "kind": kind_meta,
                }),
                ..Default::default()
            },
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Dart,
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
    true
}

fn import_uri(node: Node, source: &[u8]) -> Option<String> {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if matches!(n.kind(), "uri" | "configurable_uri") {
            if let Ok(t) = n.utf8_text(source) {
                let cleaned = t.trim().trim_matches('\'').trim_matches('"').to_string();
                if !cleaned.is_empty() {
                    return Some(cleaned);
                }
            }
        }
        let mut c = n.walk();
        for ch in n.children(&mut c) {
            stack.push(ch);
        }
    }
    None
}

/// Single iterative DFS: symbols + deferred calls + complexity.
pub fn extract_all_single_pass(
    _plugin: &Plugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("dart", grammar(), source).map_err(|e| Error::ParseError {
        file: file_path.to_path_buf(),
        line: 0,
        message: e,
    })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = dispatch();

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
                    "superclass" => {
                        if let (Some(owner), Some(parent)) =
                            (ctx.scopes.current_type(), node.child_by_field_name("type"))
                        {
                            let parent = parent.utf8_text(source)?.to_string();
                            relations.push(Relation {
                                from: owner.qualified_prefix.clone(),
                                to: rgctl_plugin_helpers::simple_type_name(&parent),
                                relation_type: RelationType::Extends,
                                location: node_to_location(node, &file_path_str),
                                metadata: serde_json::json!({"language":"dart"}),
                                to_qualified_hint: Some(parent),
                                to_type_hint: None,
                                ..Default::default()
                            });
                        }
                    }
                    "class_declaration" => {
                        pushed_scope = push_type_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Class,
                            "class",
                            ScopeKind::Class,
                        );
                    }
                    "enum_declaration" => {
                        pushed_scope = push_type_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Enum,
                            "enum",
                            ScopeKind::Enum,
                        );
                    }
                    "mixin_declaration" => {
                        pushed_scope = push_type_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Class,
                            "mixin",
                            ScopeKind::Class,
                        );
                    }
                    "extension_declaration" | "extension_type_declaration" => {
                        let meta = if kind == "extension_type_declaration" {
                            "extension_type"
                        } else {
                            "extension"
                        };
                        pushed_scope = push_type_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Class,
                            meta,
                            ScopeKind::Class,
                        );
                    }
                    "mixin_application_class" => {
                        pushed_scope = push_type_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Class,
                            "mixin_application_class",
                            ScopeKind::Class,
                        );
                    }
                    "type_alias" => {
                        if let Some(name) = extract_name_from_node(node, source).ok().flatten() {
                            let qn = name.clone();
                            ctx.symbols.push(Symbol {
                                name: name.clone(),
                                symbol_type: SymbolType::TypeAlias,
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
                                    "language": "dart",
                                    "kind": "type_alias",
                                }),
                                ..Default::default()
                            });
                        }
                    }
                    "enum_constant" => {
                        if let Some(name) = extract_name_from_node(node, source).ok().flatten() {
                            let owner = ctx
                                .scopes
                                .current_type()
                                .map(|t| t.qualified_prefix.clone());
                            let qn = match owner.as_deref() {
                                Some(o) => format!("{o}.{name}"),
                                None => name.clone(),
                            };
                            ctx.symbols.push({
                                let mut field_symbol = Symbol {
                                    name,
                                    symbol_type: SymbolType::Variable,
                                    qualified_name: Some(qn),
                                    location: node_to_location(node, &file_path_str),
                                    signature: None,
                                    return_type: None,
                                    parameters: vec![],
                                    fields: vec![],
                                    modifiers: vec![],
                                    documentation: None,
                                    metadata: serde_json::json!({
                                        "language": "dart",
                                        "kind": "enum_constant",
                                    }),
                                    ..Default::default()
                                };
                                rgctl_plugin_helpers::spans::field_symbol_spans(
                                    &mut field_symbol,
                                    node,
                                    source,
                                );
                                field_symbol
                            });
                        }
                    }
                    "function_declaration"
                    | "method_declaration"
                    | "local_function_declaration"
                    | "getter_declaration"
                    | "setter_declaration"
                    | "external_function_declaration"
                    | "external_getter_declaration"
                    | "external_setter_declaration"
                    | "operator_signature"
                    | "constructor_signature"
                    | "factory_constructor_signature"
                    | "constant_constructor_signature"
                    | "redirecting_factory_constructor_signature"
                    | "static_final_declaration"
                    | "external_variable_declaration" => {
                        if let Some(name) = extract_name_from_node(node, source).ok().flatten() {
                            let meta = match kind {
                                "method_declaration" => "method",
                                "getter_declaration" | "external_getter_declaration" => "getter",
                                "setter_declaration" | "external_setter_declaration" => "setter",
                                "local_function_declaration" => "local_function",
                                "operator_signature" => "operator",
                                "constructor_signature"
                                | "factory_constructor_signature"
                                | "constant_constructor_signature"
                                | "redirecting_factory_constructor_signature" => "constructor",
                                "static_final_declaration" | "external_variable_declaration" => {
                                    "field"
                                }
                                _ => "function",
                            };
                            // Fields: record symbol without function scope.
                            if matches!(
                                kind,
                                "static_final_declaration" | "external_variable_declaration"
                            ) {
                                let owner = ctx
                                    .scopes
                                    .current_type()
                                    .map(|t| t.qualified_prefix.clone());
                                let qn = match owner.as_deref() {
                                    Some(o) => format!("{o}.{name}"),
                                    None => name.clone(),
                                };
                                ctx.symbols.push({
                                    let mut field_symbol = Symbol {
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
                                            "language": "dart",
                                            "kind": meta,
                                        }),
                                        ..Default::default()
                                    };
                                    rgctl_plugin_helpers::spans::field_symbol_spans(
                                        &mut field_symbol,
                                        node,
                                        source,
                                    );
                                    field_symbol
                                });
                            } else {
                                pushed_scope = push_function(
                                    &mut ctx,
                                    node,
                                    source,
                                    &file_path_str,
                                    name,
                                    meta,
                                );
                            }
                        }
                    }
                    "function_expression" => {
                        let line = node.start_position().row + 1;
                        let name = format!("<lambda@{line}>");
                        pushed_scope = push_function(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            name,
                            "function_expression",
                        );
                    }
                    "import_or_export" | "library_import" | "library_export" | "part_directive"
                    | "part_of_directive" => {
                        // Prefer handling once at `import_or_export` to avoid duplicates.
                        if kind == "library_import" {
                            if let Some(parent) = node.parent() {
                                if parent.kind() == "import_or_export" {
                                    // handled by parent
                                } else if let Some(uri) = import_uri(node, source) {
                                    ctx.symbols.push(Symbol {
                                        name: uri.clone(),
                                        symbol_type: SymbolType::Import,
                                        qualified_name: Some(uri),
                                        location: node_to_location(node, &file_path_str),
                                        signature: None,
                                        return_type: None,
                                        parameters: vec![],
                                        fields: vec![],
                                        modifiers: vec![],
                                        documentation: None,
                                        metadata: serde_json::json!({
                                            "language": "dart",
                                            "kind": "import",
                                        }),
                                        ..Default::default()
                                    });
                                }
                            }
                        } else if let Some(uri) = import_uri(node, source) {
                            ctx.symbols.push(Symbol {
                                name: uri.clone(),
                                symbol_type: SymbolType::Import,
                                qualified_name: Some(uri),
                                location: node_to_location(node, &file_path_str),
                                signature: None,
                                return_type: None,
                                parameters: vec![],
                                fields: vec![],
                                modifiers: vec![],
                                documentation: None,
                                metadata: serde_json::json!({
                                    "language": "dart",
                                    "kind": "import",
                                }),
                                ..Default::default()
                            });
                        }
                    }
                    _ => {}
                }

                if DART_CALL_KINDS.contains(&kind) {
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
                        | "for_statement"
                        | "while_statement"
                        | "do_statement"
                        | "switch_statement"
                        | "try_statement"
                        | "switch_expression"
                        | "catch_clause"
                        | "assert_statement"
                        | "conditional_expression"
                        | "if_null_expression"
                        | "logical_and_expression"
                        | "logical_or_expression" => {
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
            "dart",
            &mut relations,
        );
        rgctl_plugin_helpers::callable_identity::call(
            pending.node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Dart,
            ctx.symbols.get(pending.from_symbol_idx),
            &mut relations[identity_relation_start..],
        );
    }

    rgctl_plugin_helpers::callable_identity::finish(
        rgctl_plugin_helpers::callable_identity::Policy::Dart,
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
    fn dart_extracts_class_method_call_and_complexity() {
        let src = r#"
class Foo {
  void bar() {
    if (true) { baz(); }
    return;
  }
}
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("t.dart");
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
            "expected function bar, got {:?}",
            result.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
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
    fn dart_method_qualified_under_enclosing_class() {
        let src = r#"
class Foo {
  void bar() {}
}
class Outer {
  // Nested type scopes: extension / mixin style ownership still prefixes methods.
}
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("t.dart");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        let bar = result
            .symbols
            .iter()
            .find(|s| s.name == "bar")
            .expect("bar");
        assert_eq!(
            bar.qualified_name.as_deref(),
            Some("Foo.bar"),
            "method must be qualified under enclosing class"
        );
    }
}
