//! Single-pass AST extraction for Scala (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::Plugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{extract_name_from_node, node_to_location};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../scala-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("alternative_pattern", HandlerMask::AST_SKELETON),
    ("annotation", HandlerMask::RELATION),
    ("ascription_expression", HandlerMask::AST_SKELETON),
    ("assignment_expression", HandlerMask::AST_SKELETON),
    ("binding", HandlerMask::AST_SKELETON),
    ("bindings", HandlerMask::AST_SKELETON),
    ("block", HandlerMask::CFG_STMT),
    ("call_expression", HandlerMask::RELATION),
    ("case_class_pattern", HandlerMask::AST_SKELETON),
    ("case_clause", HandlerMask::CFG_STMT),
    ("catch_clause", HandlerMask::CFG_STMT),
    ("class_definition", HandlerMask::SYMBOL),
    ("class_parameter", HandlerMask::SYMBOL),
    ("class_parameters", HandlerMask::AST_SKELETON),
    ("colon_argument", HandlerMask::AST_SKELETON),
    ("context_bound", HandlerMask::AST_SKELETON),
    ("contravariant_type_parameter", HandlerMask::AST_SKELETON),
    ("covariant_type_parameter", HandlerMask::AST_SKELETON),
    ("derives_clause", HandlerMask::RELATION),
    ("do_while_expression", HandlerMask::CFG_STMT),
    ("enum_definition", HandlerMask::SYMBOL),
    ("enumerator", HandlerMask::AST_SKELETON),
    ("enumerators", HandlerMask::AST_SKELETON),
    ("export_declaration", HandlerMask::RELATION),
    ("extends_clause", HandlerMask::RELATION),
    ("extension_definition", HandlerMask::SYMBOL),
    ("field_expression", HandlerMask::RELATION),
    ("finally_clause", HandlerMask::CFG_STMT),
    ("for_expression", HandlerMask::CFG_STMT),
    ("full_enum_case", HandlerMask::SYMBOL),
    ("function_declaration", HandlerMask::SYMBOL),
    ("function_definition", HandlerMask::SYMBOL),
    ("generic_function", HandlerMask::RELATION),
    ("given_definition", HandlerMask::SYMBOL),
    ("guard", HandlerMask::CFG_STMT),
    ("if_expression", HandlerMask::CFG_STMT),
    ("import_declaration", HandlerMask::RELATION),
    ("infix_expression", HandlerMask::RELATION),
    ("infix_pattern", HandlerMask::AST_SKELETON),
    ("instance_expression", HandlerMask::RELATION),
    ("lambda_expression", HandlerMask::SYMBOL),
    ("lower_bound", HandlerMask::AST_SKELETON),
    ("match_expression", HandlerMask::CFG_STMT),
    ("name_and_type", HandlerMask::AST_SKELETON),
    ("named_pattern", HandlerMask::AST_SKELETON),
    ("object_definition", HandlerMask::SYMBOL),
    ("package_clause", HandlerMask::SYMBOL),
    ("package_object", HandlerMask::SYMBOL),
    ("parameter", HandlerMask::AST_SKELETON),
    ("parameters", HandlerMask::AST_SKELETON),
    ("postfix_expression", HandlerMask::AST_SKELETON),
    ("prefix_expression", HandlerMask::AST_SKELETON),
    ("return_expression", HandlerMask::CFG_STMT),
    ("self_type", HandlerMask::AST_SKELETON),
    ("simple_enum_case", HandlerMask::SYMBOL),
    ("throw_expression", HandlerMask::CFG_STMT),
    ("trait_definition", HandlerMask::SYMBOL),
    ("try_expression", HandlerMask::CFG_STMT),
    ("tuple_expression", HandlerMask::AST_SKELETON),
    ("tuple_pattern", HandlerMask::AST_SKELETON),
    ("type_arguments", HandlerMask::AST_SKELETON),
    ("type_definition", HandlerMask::SYMBOL),
    ("type_parameters", HandlerMask::AST_SKELETON),
    ("typed_pattern", HandlerMask::AST_SKELETON),
    ("upper_bound", HandlerMask::AST_SKELETON),
    ("val_declaration", HandlerMask::SYMBOL),
    ("val_definition", HandlerMask::SYMBOL),
    ("var_declaration", HandlerMask::SYMBOL),
    ("var_definition", HandlerMask::SYMBOL),
    ("vararg", HandlerMask::AST_SKELETON),
    ("while_expression", HandlerMask::CFG_STMT),
];

/// Call-like nodes flushed as `Calls` after scope walk.
const SCALA_CALL_KINDS: &[&str] = &[
    "call_expression",
    "generic_function",
    "infix_expression",
    "instance_expression",
    "field_expression",
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn grammar() -> tree_sitter::Language {
    tree_sitter_scala::LANGUAGE.into()
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
                    "language": "scala",
                    "kind": kind_meta,
                }),
                ..Default::default()
            },
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Scala,
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

fn push_named_symbol(
    ctx: &mut SinglePassCtx,
    node: Node,
    source: &[u8],
    file_path: &str,
    symbol_type: SymbolType,
    kind_meta: &str,
) {
    let Some(name) = extract_name_from_node(node, source).ok().flatten() else {
        return;
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
                name,
                symbol_type,
                qualified_name: Some(qn),
                location: node_to_location(node, file_path),
                signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: serde_json::json!({
                    "language": "scala",
                    "kind": kind_meta,
                }),
                ..Default::default()
            },
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Scala,
        ));
}

/// Single iterative DFS: symbols + deferred calls + complexity.
pub fn extract_all_single_pass(
    _plugin: &Plugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("scala", grammar(), source).map_err(|e| Error::ParseError {
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
    let mut lambda_idx: u32 = 0;

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
                    "class_definition" => {
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
                    "object_definition" | "package_object" => {
                        pushed_scope = push_type_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Class,
                            if kind == "package_object" {
                                "package_object"
                            } else {
                                "object"
                            },
                            ScopeKind::Class,
                        );
                    }
                    "trait_definition" => {
                        pushed_scope = push_type_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Interface,
                            "trait",
                            ScopeKind::Interface,
                        );
                    }
                    "enum_definition" => {
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
                    "extension_definition" => {
                        pushed_scope = push_type_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Class,
                            "extension",
                            ScopeKind::Class,
                        );
                    }
                    "given_definition" => {
                        if let Some(name) = extract_name_from_node(node, source).ok().flatten() {
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
                                        symbol_type: SymbolType::Function,
                                        qualified_name: Some(qn.clone()),
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
                                            "language": "scala",
                                            "kind": "given",
                                        }),
                                        ..Default::default()
                                    },
                                    node,
                                    source,
                                    rgctl_plugin_helpers::callable_identity::Policy::Scala,
                                ));
                            ctx.enter_scope(
                                ScopeKind::Function,
                                Some(name),
                                qn,
                                node.start_byte(),
                                node.end_byte(),
                                true,
                            );
                            pushed_scope = true;
                        } else {
                            // Anonymous given — still enter a function-like scope for calls.
                            let name = format!("given${}", ctx.symbols.len());
                            let qn = name.clone();
                            ctx.symbols
                                .push(rgctl_plugin_helpers::callable_identity::symbol(
                                    Symbol {
                                        name: name.clone(),
                                        symbol_type: SymbolType::Function,
                                        qualified_name: Some(qn.clone()),
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
                                            "language": "scala",
                                            "kind": "given",
                                        }),
                                        ..Default::default()
                                    },
                                    node,
                                    source,
                                    rgctl_plugin_helpers::callable_identity::Policy::Scala,
                                ));
                            ctx.enter_scope(
                                ScopeKind::Function,
                                Some(name),
                                qn,
                                node.start_byte(),
                                node.end_byte(),
                                true,
                            );
                            pushed_scope = true;
                        }
                    }
                    "package_clause" => {
                        push_named_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Module,
                            "package",
                        );
                    }
                    "function_definition" | "function_declaration" => {
                        if let Some(name) = extract_name_from_node(node, source).ok().flatten() {
                            let owner = ctx
                                .scopes
                                .current_type()
                                .map(|t| t.qualified_prefix.clone());
                            let qn = match owner.as_deref() {
                                Some(o) => format!("{o}.{name}"),
                                None => name.clone(),
                            };
                            let loc = ((node.end_position().row - node.start_position().row) + 1)
                                .max(1) as u32;
                            ctx.symbols
                                .push(rgctl_plugin_helpers::callable_identity::symbol(
                                    Symbol {
                                        name: name.clone(),
                                        symbol_type: SymbolType::Function,
                                        qualified_name: Some(qn.clone()),
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
                                            "language": "scala",
                                            "kind": if kind == "function_declaration" {
                                                "function_decl"
                                            } else {
                                                "function"
                                            },
                                        }),
                                        ..Default::default()
                                    },
                                    node,
                                    source,
                                    rgctl_plugin_helpers::callable_identity::Policy::Scala,
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
                            pushed_scope = true;
                        }
                    }
                    "lambda_expression" => {
                        lambda_idx += 1;
                        let name = format!("lambda${lambda_idx}");
                        let owner = ctx
                            .scopes
                            .current_function()
                            .map(|f| f.qualified_prefix.clone())
                            .or_else(|| {
                                ctx.scopes
                                    .current_type()
                                    .map(|t| t.qualified_prefix.clone())
                            });
                        let qn = match owner.as_deref() {
                            Some(o) => format!("{o}.{name}"),
                            None => name.clone(),
                        };
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
                        ctx.symbols
                            .push(rgctl_plugin_helpers::callable_identity::symbol(
                                Symbol {
                                    name: name.clone(),
                                    symbol_type: SymbolType::Function,
                                    qualified_name: Some(qn.clone()),
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
                                        "language": "scala",
                                        "kind": "lambda",
                                    }),
                                    ..Default::default()
                                },
                                node,
                                source,
                                rgctl_plugin_helpers::callable_identity::Policy::Scala,
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
                        pushed_scope = true;
                    }
                    "val_definition" | "var_definition" | "val_declaration" | "var_declaration"
                    | "class_parameter" | "full_enum_case" | "simple_enum_case" => {
                        push_named_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::Variable,
                            kind,
                        );
                    }
                    "type_definition" => {
                        push_named_symbol(
                            &mut ctx,
                            node,
                            source,
                            &file_path_str,
                            SymbolType::TypeAlias,
                            "type",
                        );
                    }
                    "import_declaration" | "export_declaration" => {
                        if let Some(name) = extract_name_from_node(node, source).ok().flatten() {
                            let keyword = if kind == "export_declaration" {
                                "export"
                            } else {
                                "import"
                            };
                            let full = node
                                .utf8_text(source)
                                .ok()
                                .map(|s| s.trim().trim_start_matches(keyword).trim().to_string())
                                .unwrap_or(name);
                            ctx.symbols.push(Symbol {
                                name: full.clone(),
                                symbol_type: SymbolType::Import,
                                qualified_name: Some(full),
                                location: node_to_location(node, &file_path_str),
                                signature: None,
                                return_type: None,
                                parameters: vec![],
                                fields: vec![],
                                modifiers: vec![],
                                documentation: None,
                                metadata: serde_json::json!({
                                    "language": "scala",
                                    "kind": keyword,
                                }),
                                ..Default::default()
                            });
                        }
                    }
                    "extends_clause" | "derives_clause" => {
                        if let Some(from_type) = ctx.scopes.current_type() {
                            let from = from_type.qualified_prefix.clone();
                            if let Some(base) = node
                                .child_by_field_name("type")
                                .and_then(|n| n.utf8_text(source).ok())
                                .map(str::to_owned)
                                .or_else(|| extract_name_from_node(node, source).ok().flatten())
                            {
                                let rel_ty = if kind == "derives_clause" {
                                    RelationType::Implements
                                } else {
                                    RelationType::Extends
                                };
                                relations.push(Relation {
                                    from,
                                    to: base.clone(),
                                    relation_type: rel_ty,
                                    location: node_to_location(node, &file_path_str),
                                    metadata: serde_json::json!({
                                        "language": "scala",
                                        "kind": kind,
                                    }),
                                    to_qualified_hint: Some(base),
                                    to_type_hint: None,
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    "annotation" => {
                        if let Some(from) = ctx
                            .scopes
                            .current_function()
                            .map(|f| f.qualified_prefix.clone())
                            .or_else(|| {
                                ctx.scopes
                                    .current_type()
                                    .map(|t| t.qualified_prefix.clone())
                            })
                        {
                            // tree-sitter-scala uses `type_identifier` (not `identifier`) for
                            // annotation names; `extract_name_from_node` skips those.
                            let mut ann = None;
                            let mut args = None;
                            let mut c = node.walk();
                            for ch in node.children(&mut c) {
                                match ch.kind() {
                                    "type_identifier" | "identifier" | "simple_identifier"
                                        if ann.is_none() =>
                                    {
                                        ann = ch
                                            .utf8_text(source)
                                            .ok()
                                            .map(|s| s.trim().to_string())
                                            .filter(|s| !s.is_empty());
                                    }
                                    "arguments" => {
                                        args = ch.utf8_text(source).ok().map(|s| s.to_string());
                                    }
                                    _ => {}
                                }
                            }
                            if let Some(ann) = ann {
                                let mut metadata = serde_json::json!({
                                    "language": "scala",
                                    "kind": "annotation",
                                });
                                if let Some(a) = args {
                                    metadata["arguments"] = serde_json::Value::String(a);
                                }
                                relations.push(Relation {
                                    from,
                                    to: ann.clone(),
                                    relation_type: RelationType::AnnotatedWith,
                                    location: node_to_location(node, &file_path_str),
                                    metadata,
                                    to_qualified_hint: Some(ann),
                                    to_type_hint: None,
                                    ..Default::default()
                                });
                            }
                        }
                    }
                    _ => {}
                }

                if SCALA_CALL_KINDS.contains(&kind) {
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
                        "if_expression"
                        | "while_expression"
                        | "for_expression"
                        | "match_expression"
                        | "try_expression"
                        | "do_while_expression"
                        | "case_clause"
                        | "catch_clause"
                        | "guard"
                        | "block" => {
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
                        "finally_clause" => {
                            // Visited for coverage / CFG shape; no extra cyclomatic edge.
                        }
                        "return_expression" | "throw_expression" => {
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
            "scala",
            &mut relations,
        );
        rgctl_plugin_helpers::callable_identity::call(
            pending.node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Scala,
            ctx.symbols.get(pending.from_symbol_idx),
            &mut relations[identity_relation_start..],
        );
    }

    rgctl_plugin_helpers::callable_identity::finish(
        rgctl_plugin_helpers::callable_identity::Policy::Scala,
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
    fn scala_extracts_class_function_call_and_complexity() {
        let src = r#"
class Foo {
  def bar(x: Int): Int = {
    if (true) baz(x)
    return 1
  }
}
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("t.scala");
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
            "expected function bar"
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
    fn scala_nested_type_qualified_names() {
        let src = r#"
object Foo {
  class Bar {
    def baz(): Int = 1
  }
}
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("t.scala");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        let bar = result
            .symbols
            .iter()
            .find(|s| s.name == "Bar")
            .expect("Bar");
        assert_eq!(
            bar.qualified_name.as_deref(),
            Some("Foo.Bar"),
            "nested class must be Foo.Bar"
        );
        let baz = result
            .symbols
            .iter()
            .find(|s| s.name == "baz")
            .expect("baz");
        assert_eq!(
            baz.qualified_name.as_deref(),
            Some("Foo.Bar.baz"),
            "method under nested type must be Foo.Bar.baz"
        );
    }

    #[test]
    fn scala_annotation_arguments_on_annotated_with() {
        let src = r#"
@SerialVersionUID(1L)
class Foo extends Serializable
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("ann.scala");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        let ann = result
            .relations
            .iter()
            .find(|r| {
                r.relation_type == RelationType::AnnotatedWith && r.to.contains("SerialVersionUID")
            })
            .expect("AnnotatedWith SerialVersionUID");
        assert_eq!(
            ann.metadata.get("arguments").and_then(|v| v.as_str()),
            Some("(1L)"),
            "expected arguments metadata: {:?}",
            ann.metadata
        );
    }
}
