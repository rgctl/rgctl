//! Single-pass AST extraction for Bash (`ScopeStack` + `KindDispatchTable`).

use crate::plugin::Plugin;
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{
    extract_name_from_node, node_to_location, record_call_site_spans,
    record_symbol_spans_from_source,
};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../bash-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("arithmetic_expansion", HandlerMask::AST_SKELETON),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("brace_expression", HandlerMask::AST_SKELETON),
    ("c_style_for_statement", HandlerMask::CFG_STMT),
    ("case_item", HandlerMask::CFG_STMT),
    ("case_statement", HandlerMask::CFG_STMT),
    ("command", HandlerMask::RELATION),
    ("command_substitution", HandlerMask::RELATION),
    ("compound_statement", HandlerMask::CFG_STMT),
    ("declaration_command", HandlerMask::SYMBOL),
    ("elif_clause", HandlerMask::CFG_STMT),
    ("else_clause", HandlerMask::CFG_STMT),
    ("expansion", HandlerMask::AST_SKELETON),
    ("for_statement", HandlerMask::CFG_STMT),
    ("function_definition", HandlerMask::SYMBOL),
    ("if_statement", HandlerMask::CFG_STMT),
    ("list", HandlerMask::CFG_STMT),
    ("negated_command", HandlerMask::CFG_STMT),
    ("pipeline", HandlerMask::CFG_STMT),
    ("postfix_expression", HandlerMask::AST_SKELETON),
    ("redirected_statement", HandlerMask::CFG_STMT),
    ("simple_expansion", HandlerMask::AST_SKELETON),
    ("subshell", HandlerMask::CFG_STMT),
    ("ternary_expression", HandlerMask::CFG_STMT),
    ("test_command", HandlerMask::CFG_STMT),
    ("unary_expression", HandlerMask::CFG_STMT),
    ("unset_command", HandlerMask::AST_SKELETON),
    ("variable_assignment", HandlerMask::AST_SKELETON),
    ("while_statement", HandlerMask::CFG_STMT),
];

const AST_MAX_DEPTH: usize = 2048;

/// Control-flow hosts that contribute cyclomatic / cognitive complexity.
const BRANCH_KINDS: &[&str] = &[
    "binary_expression",
    "c_style_for_statement",
    "case_item",
    "case_statement",
    "elif_clause",
    "for_statement",
    "if_statement",
    "list",
    "negated_command",
    "pipeline",
    "subshell",
    "ternary_expression",
    "test_command",
    "while_statement",
];

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn grammar() -> tree_sitter::Language {
    tree_sitter_bash::LANGUAGE.into()
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
    callee: String,
}

enum Visit<'a> {
    Pre(Node<'a>, usize),
    LeaveScope,
    LeaveGuard,
}

/// Bash `command` name is `command_name` → `word` (not `identifier`).
fn bash_command_word(node: Node, source: &[u8]) -> Option<String> {
    let name_node = node.child_by_field_name("name").or_else(|| {
        let mut c = node.walk();
        node.children(&mut c).find(|ch| ch.kind() == "command_name")
    })?;
    if name_node.kind() == "word" {
        return name_node.utf8_text(source).ok().map(|s| s.to_string());
    }
    let mut c = name_node.walk();
    for ch in name_node.children(&mut c) {
        if ch.kind() == "word" {
            return ch.utf8_text(source).ok().map(|s| s.to_string());
        }
    }
    name_node.utf8_text(source).ok().map(|s| s.to_string())
}

fn first_command_word_in(node: Node, source: &[u8]) -> Option<String> {
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if n.kind() == "command" {
            if let Some(w) = bash_command_word(n, source) {
                return Some(w);
            }
        }
        for i in (0..n.child_count()).rev() {
            if let Some(ch) = n.child(i) {
                if ch.is_named() {
                    stack.push(ch);
                }
            }
        }
    }
    None
}

fn declaration_var_names(node: Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "variable_name" => {
                if let Ok(t) = n.utf8_text(source) {
                    let t = t.trim();
                    if !t.is_empty() {
                        names.push(t.to_string());
                    }
                }
            }
            "variable_assignment" => {
                if let Some(name) = n.child_by_field_name("name").or_else(|| {
                    let mut c = n.walk();
                    n.children(&mut c).find(|ch| ch.kind() == "variable_name")
                }) {
                    if let Ok(t) = name.utf8_text(source) {
                        let t = t.trim();
                        if !t.is_empty() {
                            names.push(t.to_string());
                        }
                    }
                }
            }
            _ => {
                for i in (0..n.child_count()).rev() {
                    if let Some(ch) = n.child(i) {
                        if ch.is_named() {
                            stack.push(ch);
                        }
                    }
                }
            }
        }
    }
    names
}

fn flush_bash_call(
    pending: &PendingCall<'_>,
    symbols: &[Symbol],
    file_path: &Path,
    relations: &mut Vec<Relation>,
) {
    let Some(from_fn) = symbols.get(pending.from_symbol_idx) else {
        return;
    };
    let from = from_fn
        .qualified_name
        .clone()
        .unwrap_or_else(|| from_fn.name.clone());
    let same_file_matches: Vec<_> = symbols
        .iter()
        .filter(|s| {
            s.name == pending.callee
                && s.symbol_type == SymbolType::Function
                && s.location.file == file_path.to_string_lossy()
        })
        .collect();
    let local_target = match same_file_matches.as_slice() {
        [only] => only
            .qualified_name
            .clone()
            .unwrap_or_else(|| pending.callee.clone()),
        _ => pending.callee.clone(),
    };
    let mut rel = Relation {
        from,
        to: local_target,
        relation_type: RelationType::Calls,
        location: node_to_location(pending.node, &file_path.to_string_lossy()),
        metadata: serde_json::json!({ "language": "bash" }),
        to_qualified_hint: None,
        to_type_hint: None,
        ..Default::default()
    };
    record_call_site_spans(&mut rel, pending.node);
    relations.push(rel);
}

/// Single iterative DFS: function symbols + command Calls + complexity.
pub fn extract_all_single_pass(
    _plugin: &Plugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("bash", grammar(), source).map_err(|e| Error::ParseError {
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

                if kind == "function_definition" {
                    if let Some(name) =
                        extract_name_from_node(node, source)
                            .ok()
                            .flatten()
                            .or_else(|| {
                                node.child_by_field_name("name")
                                    .and_then(|n| n.utf8_text(source).ok().map(|s| s.to_string()))
                            })
                    {
                        let loc = ((node.end_position().row - node.start_position().row) + 1).max(1)
                            as u32;
                        let qn = name.clone();
                        let mut sym = Symbol {
                            name: name.clone(),
                            symbol_type: SymbolType::Function,
                            qualified_name: Some(qn.clone()),
                            location: node_to_location(node, &file_path_str),
                            signature: None,
                            return_type: None,
                            parameters: vec![],
                            fields: vec![],
                            modifiers: vec![],
                            documentation: None,
                            metadata: serde_json::json!({ "language": "bash" }),
                            ..Default::default()
                        };
                        record_symbol_spans_from_source(&mut sym, node, source);
                        ctx.symbols
                            .push(rgctl_plugin_helpers::callable_identity::symbol(
                                sym,
                                node,
                                source,
                                rgctl_plugin_helpers::callable_identity::Policy::Bash,
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

                if kind == "declaration_command" {
                    for name in declaration_var_names(node, source) {
                        let mut sym = Symbol {
                            name: name.clone(),
                            symbol_type: SymbolType::Variable,
                            qualified_name: Some(name),
                            location: node_to_location(node, &file_path_str),
                            signature: None,
                            return_type: None,
                            parameters: vec![],
                            fields: vec![],
                            modifiers: vec![],
                            documentation: None,
                            metadata: serde_json::json!({
                                "language": "bash",
                                "kind": "declaration_command"
                            }),
                            ..Default::default()
                        };
                        record_symbol_spans_from_source(&mut sym, node, source);
                        ctx.symbols.push(sym);
                    }
                }

                if kind == "command" {
                    if let Some(word) = bash_command_word(node, source) {
                        if matches!(word.as_str(), "return" | "exit") {
                            if ctx.scopes.current_function().is_some() {
                                ctx.scopes.note_return();
                            }
                        } else if let Some(frame) = ctx.scopes.current_function() {
                            if let Some(idx) = frame.symbol_idx {
                                pending_calls.push(PendingCall {
                                    node,
                                    from_symbol_idx: idx,
                                    callee: word,
                                });
                            }
                        }
                    }
                }

                if kind == "command_substitution" {
                    if let Some(frame) = ctx.scopes.current_function() {
                        if let Some(idx) = frame.symbol_idx {
                            let callee = first_command_word_in(node, source)
                                .unwrap_or_else(|| "$(...)".to_string());
                            pending_calls.push(PendingCall {
                                node,
                                from_symbol_idx: idx,
                                callee,
                            });
                        }
                    }
                }

                if ctx.scopes.current_function().is_some() && BRANCH_KINDS.contains(&kind) {
                    // binary_expression / list: only && and || add a decision point.
                    let counts = match kind {
                        "binary_expression" | "list" => {
                            let text = node.utf8_text(source).unwrap_or("");
                            text.contains("&&") || text.contains("||")
                        }
                        _ => true,
                    };
                    if counts {
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
        flush_bash_call(&pending, &ctx.symbols, file_path, &mut relations);
        rgctl_plugin_helpers::callable_identity::call(
            pending.node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Bash,
            ctx.symbols.get(pending.from_symbol_idx),
            &mut relations[identity_relation_start..],
        );
    }

    rgctl_plugin_helpers::callable_identity::finish(
        rgctl_plugin_helpers::callable_identity::Policy::Bash,
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
    fn bash_extracts_function_call_and_complexity() {
        let src = r#"
foo() {
  if true; then
    bar
    return 1
  fi
}
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("t.sh");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Function && s.name == "foo"),
            "expected function foo"
        );
        assert!(
            result
                .relations
                .iter()
                .any(|r| { r.relation_type == RelationType::Calls && r.to == "bar" }),
            "expected Calls to bar, got {:?}",
            result.relations
        );
        let foo = result
            .symbols
            .iter()
            .find(|s| s.name == "foo")
            .expect("foo");
        let metrics = complexity_from_metadata(foo).expect("rgctl_complexity");
        assert!(
            metrics.cyclomatic >= 2,
            "expected cyclomatic >= 2, got {}",
            metrics.cyclomatic
        );
    }

    #[test]
    fn bash_extracts_declaration_and_command_substitution() {
        let src = r#"
demo() {
  declare -i COUNT=0
  echo "$(helper)"
}
helper() { :; }
"#;
        let plugin = Plugin::new().unwrap();
        let path = PathBuf::from("decl.sh");
        let result = extract_all_single_pass(&plugin, &path, src.as_bytes()).unwrap();
        assert!(
            result
                .symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Variable && s.name == "COUNT"),
            "expected COUNT variable, got {:?}",
            result.symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            result
                .relations
                .iter()
                .any(|r| r.relation_type == RelationType::Calls && r.to == "helper"),
            "expected Calls to helper from substitution, got {:?}",
            result.relations
        );
    }
}
