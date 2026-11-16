//! Single-pass AST extraction for HCL/Terraform (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::Plugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{
    node_to_location, record_call_site_spans, record_symbol_spans_from_source,
};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../hcl-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
/// Keep in sync with non-Literal active handlers in `hcl-ast-coverage.json`.
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("attribute", HandlerMask::CFG_STMT),
    ("block", HandlerMask::SYMBOL),
    ("conditional", HandlerMask::CFG_STMT),
    ("for_expr", HandlerMask::CFG_STMT),
    ("function_call", HandlerMask::RELATION),
    ("get_attr", HandlerMask::AST_SKELETON),
    ("template_directive", HandlerMask::AST_SKELETON),
    ("variable_expr", HandlerMask::AST_SKELETON),
];

const HCL_CALL_KINDS: &[&str] = &["function_call"];
const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn grammar() -> tree_sitter::Language {
    tree_sitter_hcl::LANGUAGE.into()
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

fn string_lit_inner(node: Node, source: &[u8]) -> Option<String> {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "template_literal" {
            return ch.utf8_text(source).ok().map(|s| s.to_string());
        }
    }
    node.utf8_text(source)
        .ok()
        .map(|s| s.trim_matches('"').to_string())
}

/// `resource "aws_instance" "web"` → (`resource`, `aws_instance.web`).
fn block_type_and_name(node: Node, source: &[u8]) -> Option<(String, String, SymbolType)> {
    let mut block_type: Option<String> = None;
    let mut labels: Vec<String> = Vec::new();
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        match ch.kind() {
            "identifier" if block_type.is_none() => {
                block_type = ch.utf8_text(source).ok().map(|s| s.to_string());
            }
            "string_lit" => {
                if let Some(label) = string_lit_inner(ch, source) {
                    labels.push(label);
                }
            }
            "block_start" => break,
            _ => {}
        }
    }
    let block_type = block_type?;
    let name = if labels.is_empty() {
        block_type.clone()
    } else {
        format!("{block_type}.{}", labels.join("."))
    };
    let symbol_type = match block_type.as_str() {
        "variable" | "output" => SymbolType::Variable,
        "module" | "locals" => SymbolType::Module,
        "provider" => SymbolType::Module,
        "resource" | "data" => SymbolType::Struct,
        _ => SymbolType::Module,
    };
    Some((block_type, name, symbol_type))
}

fn hcl_call_callee(node: Node, source: &[u8]) -> Option<String> {
    let mut c = node.walk();
    for ch in node.children(&mut c) {
        if ch.kind() == "identifier" {
            return ch.utf8_text(source).ok().map(|s| s.to_string());
        }
    }
    None
}

/// Single iterative DFS: Terraform blocks + function_call + conditional complexity.
pub fn extract_all_single_pass(
    _plugin: &Plugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("hcl", grammar(), source).map_err(|e| Error::ParseError {
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

                if kind == "block" {
                    if let Some((block_type, name, symbol_type)) = block_type_and_name(node, source)
                    {
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
                        let qn = name.clone();
                        let mut sym = Symbol {
                            name: name.clone(),
                            symbol_type,
                            qualified_name: Some(qn.clone()),
                            location: node_to_location(node, &file_path_str),
                            signature: None,
                            return_type: None,
                            parameters: vec![],
                            fields: vec![],
                            modifiers: vec![],
                            documentation: None,
                            metadata: serde_json::json!({
                                "language": "hcl",
                                "block_type": block_type,
                            }),
                            ..Default::default()
                        };
                        record_symbol_spans_from_source(&mut sym, node, source);
                        ctx.symbols.push(sym);
                        // Treat blocks as function-like hosts so nested calls/conditionals attach.
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

                if HCL_CALL_KINDS.contains(&kind) {
                    if let Some(frame) = ctx.scopes.current_function() {
                        if let Some(idx) = frame.symbol_idx {
                            pending_calls.push(PendingCall {
                                node,
                                from_symbol_idx: idx,
                            });
                        }
                    }
                }

                if ctx.scopes.current_function().is_some()
                    && matches!(kind, "conditional" | "for_expr")
                {
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
        // Prefer shared helper; fall back for HCL's identifier-first function_call shape.
        let before = relations.len();
        push_call_relation_from(
            pending.node,
            source,
            file_path,
            &ctx.symbols,
            from_fn,
            "hcl",
            &mut relations,
        );
        if relations.len() == before {
            if let Some(callee) = hcl_call_callee(pending.node, source) {
                let from = from_fn
                    .qualified_name
                    .clone()
                    .unwrap_or_else(|| from_fn.name.clone());
                let mut rel = Relation {
                    from,
                    to: callee,
                    relation_type: RelationType::Calls,
                    location: node_to_location(pending.node, &file_path.to_string_lossy()),
                    metadata: serde_json::json!({ "language": "hcl" }),
                    to_qualified_hint: None,
                    to_type_hint: None,
                    ..Default::default()
                };
                record_call_site_spans(&mut rel, pending.node);
                relations.push(rel);
            }
        }
    }

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn hcl_extracts_resource_block_and_call() {
        let src = r#"
resource "aws_instance" "web" {
  count = var.enabled ? 1 : 0
  tags = merge(local.a, {})
}
module "vpc" {
  source = "./vpc"
}
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("main.tf");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.name == "resource.aws_instance.web"),
            "expected resource block, got {:?}",
            result.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            result.symbols.iter().any(|s| s.name == "module.vpc"),
            "expected module.vpc"
        );
        assert!(
            result
                .relations
                .iter()
                .any(|r| r.relation_type == RelationType::Calls && r.to == "merge"),
            "expected Calls to merge, got {:?}",
            result.relations
        );
        let res = result
            .symbols
            .iter()
            .find(|s| s.name == "resource.aws_instance.web")
            .expect("resource");
        let metrics = complexity_from_metadata(res).expect("rgctl_complexity");
        assert!(
            metrics.cyclomatic >= 2,
            "expected cyclomatic >= 2 from conditional, got {}",
            metrics.cyclomatic
        );
    }
}
