//! Single-pass AST extraction for Puppet (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::{PUPPET_BRANCH_KINDS, PuppetPlugin};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../puppet-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("assignment", HandlerMask::AST_SKELETON),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("block", HandlerMask::CFG_STMT),
    ("case_item", HandlerMask::CFG_STMT),
    ("case_statement", HandlerMask::CFG_STMT),
    ("class_definition", HandlerMask::SYMBOL),
    ("class_inherits", HandlerMask::RELATION),
    ("default_case", HandlerMask::CFG_STMT),
    ("defined_resource_type", HandlerMask::SYMBOL),
    ("else_statement", HandlerMask::CFG_STMT),
    ("elsif_statement", HandlerMask::CFG_STMT),
    ("field_expression", HandlerMask::AST_SKELETON),
    ("function_call", HandlerMask::RELATION),
    ("function_declaration", HandlerMask::SYMBOL),
    ("if_statement", HandlerMask::CFG_STMT),
    ("include_statement", HandlerMask::RELATION),
    ("iterator_statement", HandlerMask::CFG_STMT),
    ("lambda", HandlerMask::SYMBOL),
    ("node_definition", HandlerMask::SYMBOL),
    ("relation", HandlerMask::RELATION),
    ("require_statement", HandlerMask::RELATION),
    ("resource_collector", HandlerMask::RELATION),
    ("resource_declaration", HandlerMask::SYMBOL),
    ("resource_default", HandlerMask::RELATION),
    ("resource_reference", HandlerMask::RELATION),
    ("search_expression", HandlerMask::RELATION),
    ("selector", HandlerMask::CFG_STMT),
    ("tag_statement", HandlerMask::RELATION),
    ("type_declaration", HandlerMask::SYMBOL),
    ("unary_expression", HandlerMask::AST_SKELETON),
    ("unless_statement", HandlerMask::CFG_STMT),
    ("variable", HandlerMask::SYMBOL),
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn puppet_grammar() -> tree_sitter::Language {
    tree_sitter_puppet::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn puppet_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&puppet_grammar(), HANDLED_KINDS))
}

fn puppet_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&puppet_grammar(), MANIFEST_JSON))
}

enum Visit<'a> {
    Pre(Node<'a>, usize),
    LeaveScope,
    LeaveGuard,
}

/// Single iterative DFS: symbols + relations + complexity on CFG hosts.
pub fn extract_all_single_pass(
    plugin: &PuppetPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree =
        parse_with_pool("puppet", puppet_grammar(), source).map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: e,
        })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = puppet_dispatch();

    let mut ctx = SinglePassCtx::new(dispatch);
    let mut relations = Vec::new();
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
                let before = ctx.symbols.len();
                plugin.collect_symbol_at(node, source, &file_path_str, &mut ctx.symbols);

                let mut pushed_scope = false;
                // Bind function scope to CFG host / function / lambda just pushed.
                if matches!(
                    kind,
                    "class_definition"
                        | "defined_resource_type"
                        | "node_definition"
                        | "function_declaration"
                        | "lambda"
                ) {
                    // CFG host is the last Function symbol pushed for this node (if any).
                    if let Some((idx, sym)) = ctx.symbols[before..]
                        .iter()
                        .enumerate()
                        .rev()
                        .find(|(_, s)| s.symbol_type == SymbolType::Function)
                        .map(|(i, s)| (before + i, s))
                    {
                        let params = sym.parameters.len() as u32;
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
                        let name = sym.name.clone();
                        let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                        let depth_s = ctx.scopes.depth();
                        let mut frame = ScopeFrame::new(
                            ScopeKind::Function,
                            Some(name),
                            qn,
                            node.start_byte(),
                            node.end_byte(),
                            depth_s,
                        );
                        frame.symbol_idx = Some(idx);
                        frame.complexity.cyclomatic = 1;
                        frame.complexity.param_count = params;
                        frame.complexity.loc = loc;
                        ctx.scopes.push(frame);
                        pushed_scope = true;
                    }
                }

                plugin.collect_relation_at(node, source, &file_path_str, &mut relations);

                if ctx.scopes.current_function().is_some() {
                    if PUPPET_BRANCH_KINDS.contains(&kind) {
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

    plugin.maybe_module_from_metadata(file_path, &mut ctx.symbols, &mut relations);
    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}
