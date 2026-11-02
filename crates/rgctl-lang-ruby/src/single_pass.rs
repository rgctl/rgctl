//! Single-pass AST extraction for Ruby (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::{RUBY_COMPLEXITY_BRANCH_KINDS, RUBY_FUNCTION_KINDS, RubyPlugin};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../ruby-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("alias", HandlerMask::RELATION),
    ("alternative_pattern", HandlerMask::AST_SKELETON),
    ("array_pattern", HandlerMask::AST_SKELETON),
    ("as_pattern", HandlerMask::AST_SKELETON),
    ("assignment", HandlerMask::AST_SKELETON),
    ("begin", HandlerMask::CFG_STMT),
    ("begin_block", HandlerMask::CFG_STMT),
    ("binary", HandlerMask::CFG_STMT),
    ("block", HandlerMask::CFG_STMT),
    ("body_statement", HandlerMask::CFG_STMT),
    ("break", HandlerMask::CFG_STMT),
    ("call", HandlerMask::RELATION),
    ("case", HandlerMask::CFG_STMT),
    ("case_match", HandlerMask::CFG_STMT),
    ("class", HandlerMask::SYMBOL),
    ("conditional", HandlerMask::CFG_STMT),
    ("destructured_left_assignment", HandlerMask::AST_SKELETON),
    ("do_block", HandlerMask::CFG_STMT),
    ("element_reference", HandlerMask::AST_SKELETON),
    ("else", HandlerMask::CFG_STMT),
    ("elsif", HandlerMask::CFG_STMT),
    ("empty_statement", HandlerMask::CFG_STMT),
    ("end_block", HandlerMask::CFG_STMT),
    ("ensure", HandlerMask::CFG_STMT),
    ("expression_reference_pattern", HandlerMask::AST_SKELETON),
    ("find_pattern", HandlerMask::AST_SKELETON),
    ("for", HandlerMask::CFG_STMT),
    ("hash_pattern", HandlerMask::AST_SKELETON),
    ("if", HandlerMask::CFG_STMT),
    ("if_modifier", HandlerMask::CFG_STMT),
    ("in_clause", HandlerMask::CFG_STMT),
    ("keyword_pattern", HandlerMask::AST_SKELETON),
    ("lambda", HandlerMask::SYMBOL),
    ("lambda_parameters", HandlerMask::AST_SKELETON),
    ("left_assignment_list", HandlerMask::AST_SKELETON),
    ("match_pattern", HandlerMask::AST_SKELETON),
    ("method", HandlerMask::SYMBOL),
    ("module", HandlerMask::SYMBOL),
    ("next", HandlerMask::CFG_STMT),
    ("operator_assignment", HandlerMask::AST_SKELETON),
    ("parenthesized_statements", HandlerMask::CFG_STMT),
    ("range", HandlerMask::AST_SKELETON),
    ("redo", HandlerMask::CFG_STMT),
    ("rescue", HandlerMask::CFG_STMT),
    ("rescue_modifier", HandlerMask::CFG_STMT),
    ("rest_assignment", HandlerMask::AST_SKELETON),
    ("retry", HandlerMask::CFG_STMT),
    ("return", HandlerMask::CFG_STMT),
    ("right_assignment_list", HandlerMask::AST_SKELETON),
    ("singleton_class", HandlerMask::SYMBOL),
    ("singleton_method", HandlerMask::SYMBOL),
    ("subshell", HandlerMask::CFG_STMT),
    ("superclass", HandlerMask::RELATION),
    ("test_pattern", HandlerMask::AST_SKELETON),
    ("then", HandlerMask::CFG_STMT),
    ("unary", HandlerMask::AST_SKELETON),
    ("undef", HandlerMask::CFG_STMT),
    ("unless", HandlerMask::CFG_STMT),
    ("unless_modifier", HandlerMask::CFG_STMT),
    ("until", HandlerMask::CFG_STMT),
    ("until_modifier", HandlerMask::CFG_STMT),
    ("variable_reference_pattern", HandlerMask::AST_SKELETON),
    ("when", HandlerMask::CFG_STMT),
    ("while", HandlerMask::CFG_STMT),
    ("while_modifier", HandlerMask::CFG_STMT),
    ("yield", HandlerMask::CFG_STMT),
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn ruby_grammar() -> tree_sitter::Language {
    tree_sitter_ruby::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn ruby_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&ruby_grammar(), HANDLED_KINDS))
}

fn ruby_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&ruby_grammar(), MANIFEST_JSON))
}

struct PendingCall<'a> {
    node: Node<'a>,
    from_symbol_idx: usize,
}

enum Visit<'a> {
    Pre(Node<'a>, usize),
    LeaveScope,
    LeaveGuard,
    /// Pop a name from the Ruby namespace path (`Shop::Order`).
    LeaveNameScope,
}

/// Single iterative DFS: symbols + deferred calls + mixin edges + complexity.
pub fn extract_all_single_pass(
    plugin: &RubyPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("ruby", ruby_grammar(), source).map_err(|e| Error::ParseError {
        file: file_path.to_path_buf(),
        line: 0,
        message: e,
    })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = ruby_dispatch();

    let mut ctx = SinglePassCtx::new(dispatch);
    let mut relations = Vec::new();
    let mut pending_calls: Vec<PendingCall<'_>> = Vec::new();
    let mut name_scope: Vec<String> = Vec::new();
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
            Visit::LeaveNameScope => {
                name_scope.pop();
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
                let mut pushed_name = false;

                let owner_type_qn = ctx
                    .scopes
                    .current_type()
                    .map(|t| t.qualified_prefix.clone());

                match kind {
                    "alias" => {
                        if let (Some(new_n), Some(old_n)) = (
                            node.child_by_field_name("alias"),
                            node.child_by_field_name("name"),
                        ) {
                            if let (Some(new_name), Some(old_name)) = (
                                RubyPlugin::constant_text(new_n, source),
                                RubyPlugin::constant_text(old_n, source),
                            ) {
                                let from = match owner_type_qn.as_deref() {
                                    Some(o) => format!("{o}#{new_name}"),
                                    None => new_name.clone(),
                                };
                                relations.push(Relation {
                                    from,
                                    to: old_name.clone(),
                                    relation_type: RelationType::References,
                                    location: RubyPlugin::loc(node, &file_path_str),
                                    metadata: serde_json::json!({
                                        "language": "ruby",
                                        "kind": "alias",
                                    }),
                                    to_qualified_hint: Some(old_name),
                                    to_type_hint: None,
                                });
                            }
                        }
                    }
                    "class" if node.is_named() && node.child_by_field_name("name").is_some() => {
                        let (sym, type_qn) = plugin.extract_class_or_module(
                            node,
                            source,
                            &file_path_str,
                            &name_scope,
                            false,
                        )?;
                        let name = sym.name.clone();
                        if let Some(sc) = node.child_by_field_name("superclass") {
                            if let Some(base) =
                                RubyPlugin::constant_text(sc, source).or_else(|| {
                                    let mut c = sc.walk();
                                    sc.children(&mut c)
                                        .find_map(|ch| RubyPlugin::constant_text(ch, source))
                                })
                            {
                                relations.push(Relation {
                                    from: type_qn.clone(),
                                    to: base.clone(),
                                    relation_type: RelationType::Extends,
                                    location: RubyPlugin::loc(sc, &file_path_str),
                                    metadata: serde_json::json!({
                                        "language": "ruby",
                                        "kind": "superclass",
                                    }),
                                    to_qualified_hint: Some(base),
                                    to_type_hint: None,
                                });
                            }
                        }
                        ctx.symbols.push(sym);
                        name_scope.push(name.clone());
                        pushed_name = true;
                        ctx.enter_scope(
                            ScopeKind::Class,
                            Some(name),
                            type_qn,
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        pushed_scope = true;
                    }
                    "module" if node.is_named() && node.child_by_field_name("name").is_some() => {
                        let (sym, type_qn) = plugin.extract_class_or_module(
                            node,
                            source,
                            &file_path_str,
                            &name_scope,
                            true,
                        )?;
                        let name = sym.name.clone();
                        ctx.symbols.push(sym);
                        name_scope.push(name.clone());
                        pushed_name = true;
                        ctx.enter_scope(
                            ScopeKind::Module,
                            Some(name),
                            type_qn,
                            node.start_byte(),
                            node.end_byte(),
                            true,
                        );
                        pushed_scope = true;
                    }
                    "singleton_class" => {
                        let outer = owner_type_qn
                            .clone()
                            .unwrap_or_else(|| RubyPlugin::join_qn(&name_scope, "Object"));
                        ctx.enter_scope(
                            ScopeKind::Impl,
                            None,
                            outer,
                            node.start_byte(),
                            node.end_byte(),
                            false,
                        );
                        pushed_scope = true;
                    }
                    "method" | "singleton_method" => {
                        let singleton = kind == "singleton_method";
                        let sym = plugin.extract_method(
                            node,
                            source,
                            &file_path_str,
                            &name_scope,
                            owner_type_qn.as_deref(),
                            singleton,
                        )?;
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
                    "call" => {
                        if let Some(callee) = ruby_call_callee(node, source) {
                            if matches!(callee.as_str(), "require" | "require_relative") {
                                if let Some(path) = plugin.require_path_arg(node, source) {
                                    ctx.symbols.push(Symbol {
                                        name: path.clone(),
                                        symbol_type: SymbolType::Import,
                                        qualified_name: Some(path),
                                        location: RubyPlugin::loc(node, &file_path_str),
                                        signature: None,
                                        return_type: None,
                                        parameters: vec![],
                                        fields: vec![],
                                        modifiers: vec![],
                                        documentation: None,
                                        metadata: serde_json::json!({
                                            "language": "ruby",
                                            "kind": callee,
                                        }),
                                    });
                                }
                            }
                        }
                    }
                    "lambda" => {
                        let line = node.start_position().row + 1;
                        let name = format!("<lambda@{line}>");
                        let qn = match owner_type_qn.as_deref() {
                            Some(o) => format!("{o}.{name}"),
                            None => name.clone(),
                        };
                        ctx.symbols.push(Symbol {
                            name,
                            symbol_type: SymbolType::Function,
                            qualified_name: Some(qn.clone()),
                            location: RubyPlugin::loc(node, &file_path_str),
                            signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
                            return_type: None,
                            parameters: vec![],
                            fields: vec![],
                            modifiers: vec![],
                            documentation: None,
                            metadata: serde_json::json!({
                                "language": "ruby",
                                "kind": "lambda",
                            }),
                        });
                        let params = 0u32;
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
                        ctx.enter_scope(
                            ScopeKind::Lambda,
                            None,
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
                    _ => {}
                }

                if RUBY_CALL_KINDS.contains(&kind) {
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
                    if RUBY_COMPLEXITY_BRANCH_KINDS.contains(&kind) {
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
                    } else if kind == "return" {
                        ctx.scopes.note_return();
                    }
                }

                if pushed_name {
                    stack.push(Visit::LeaveNameScope);
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
            "ruby",
            &mut relations,
        );
    }

    plugin.extract_mixin_and_new_edges(
        root,
        source,
        &file_path_str,
        &ctx.symbols,
        &mut relations,
    )?;

    let _ = RUBY_FUNCTION_KINDS;

    rgctl_plugin_helpers::apply_framework_catalog(
        "ruby",
        source,
        Some(root),
        &mut ctx.symbols,
        &mut relations,
    );

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}
