//! Single-pass AST extraction for C (`ScopeStack` + `KindDispatchTable`).

use crate::depth;
use crate::plugin::{
    CPlugin, extract_struct_fields, include_path_from_node, source_location, struct_name,
    type_definition_typedef_name,
};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::record_symbol_spans;
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../c-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for declared_ast_handlers).
/// Synced with `c-ast-coverage.json` non-passive kinds. AstSkeleton/CfgStatement
/// extras stay zero-cost on cold discover via `c_active_kinds` short-circuit.
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("alignof_expression", HandlerMask::AST_SKELETON),
    ("array_declarator", HandlerMask::AST_SKELETON),
    ("assignment_expression", HandlerMask::AST_SKELETON),
    ("attribute_declaration", HandlerMask::AST_SKELETON),
    ("attributed_statement", HandlerMask::CFG_STMT),
    ("binary_expression", HandlerMask::CFG_STMT),
    ("bitfield_clause", HandlerMask::AST_SKELETON),
    ("break_statement", HandlerMask::CFG_STMT),
    ("call_expression", HandlerMask::RELATION),
    ("case_statement", HandlerMask::CFG_STMT),
    ("cast_expression", HandlerMask::AST_SKELETON),
    ("comma_expression", HandlerMask::CFG_STMT),
    ("compound_statement", HandlerMask::CFG_STMT),
    ("conditional_expression", HandlerMask::CFG_STMT),
    ("continue_statement", HandlerMask::CFG_STMT),
    ("declaration", HandlerMask::AST_SKELETON),
    ("do_statement", HandlerMask::CFG_STMT),
    ("else_clause", HandlerMask::CFG_STMT),
    ("enum_specifier", HandlerMask::SYMBOL),
    ("enumerator", HandlerMask::SYMBOL),
    ("extension_expression", HandlerMask::AST_SKELETON),
    ("field_declaration", HandlerMask::SYMBOL),
    ("field_expression", HandlerMask::AST_SKELETON),
    ("for_statement", HandlerMask::CFG_STMT),
    ("function_declarator", HandlerMask::AST_SKELETON),
    ("function_definition", HandlerMask::SYMBOL),
    ("generic_expression", HandlerMask::AST_SKELETON),
    ("gnu_asm_expression", HandlerMask::AST_SKELETON),
    ("goto_statement", HandlerMask::CFG_STMT),
    ("if_statement", HandlerMask::CFG_STMT),
    ("init_declarator", HandlerMask::AST_SKELETON),
    ("labeled_statement", HandlerMask::CFG_STMT),
    ("ms_call_modifier", HandlerMask::AST_SKELETON),
    ("offsetof_expression", HandlerMask::AST_SKELETON),
    ("parameter_declaration", HandlerMask::AST_SKELETON),
    ("parenthesized_declarator", HandlerMask::AST_SKELETON),
    ("pointer_declarator", HandlerMask::AST_SKELETON),
    ("pointer_expression", HandlerMask::AST_SKELETON),
    ("preproc_call", HandlerMask::AST_SKELETON),
    ("preproc_directive", HandlerMask::AST_SKELETON),
    ("preproc_elif", HandlerMask::CFG_STMT),
    ("preproc_elifdef", HandlerMask::CFG_STMT),
    ("preproc_else", HandlerMask::CFG_STMT),
    ("preproc_if", HandlerMask::CFG_STMT),
    ("preproc_ifdef", HandlerMask::CFG_STMT),
    ("preproc_include", HandlerMask::RELATION),
    ("return_statement", HandlerMask::CFG_STMT),
    ("seh_except_clause", HandlerMask::CFG_STMT),
    ("seh_finally_clause", HandlerMask::CFG_STMT),
    ("seh_leave_statement", HandlerMask::CFG_STMT),
    ("seh_try_statement", HandlerMask::CFG_STMT),
    ("sizeof_expression", HandlerMask::AST_SKELETON),
    ("struct_specifier", HandlerMask::SYMBOL),
    ("subscript_expression", HandlerMask::AST_SKELETON),
    ("switch_statement", HandlerMask::CFG_STMT),
    ("type_definition", HandlerMask::SYMBOL),
    ("unary_expression", HandlerMask::CFG_STMT),
    ("union_specifier", HandlerMask::SYMBOL),
    ("update_expression", HandlerMask::CFG_STMT),
    ("variadic_parameter", HandlerMask::AST_SKELETON),
    ("while_statement", HandlerMask::CFG_STMT),
];

const AST_MAX_DEPTH: usize = 2048;

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();
static ACTIVE_KINDS: OnceLock<Box<[bool]>> = OnceLock::new();

fn c_grammar() -> tree_sitter::Language {
    tree_sitter_c::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn c_ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&c_grammar(), HANDLED_KINDS))
}

fn c_dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&c_grammar(), MANIFEST_JSON))
}

/// Fast `kind_id -> bool` table for kinds that have an action in [`extract_all_single_pass`].
fn c_active_kinds() -> &'static [bool] {
    ACTIVE_KINDS.get_or_init(|| {
        let g = c_grammar();
        let n = g.node_kind_count();
        let mut table = vec![false; n + 1];
        const ACTIVE_NAMES: &[&str] = &[
            "function_definition",
            "declaration",
            "struct_specifier",
            "enum_specifier",
            "type_definition",
            "union_specifier",
            "preproc_include",
            "preprocessor_include",
            "call_expression",
            "if_statement",
            "switch_statement",
            "while_statement",
            "for_statement",
            "do_statement",
            "conditional_expression",
            "case_statement",
            "return_statement",
            "preproc_if",
            "preproc_ifdef",
        ];
        for &name in ACTIVE_NAMES {
            let id = g.id_for_node_kind(name, true) as usize;
            if id > 0 && id < table.len() {
                table[id] = true;
            }
        }
        table.into_boxed_slice()
    })
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

/// Single iterative DFS: symbols + deferred calls + in-pass complexity.
pub fn extract_all_single_pass(
    plugin: &CPlugin,
    file_path: &Path,
    source: &[u8],
) -> Result<ExtractAllResult> {
    let tree = parse_with_pool("c", c_grammar(), source).map_err(|e| Error::ParseError {
        file: file_path.to_path_buf(),
        line: 0,
        message: e,
    })?;
    let root = tree.root_node();
    let file_path_str = file_path.to_string_lossy();
    let dispatch = c_dispatch();
    let active_kinds = c_active_kinds();

    let mut ctx = SinglePassCtx::new(dispatch);
    let mut relations = Vec::new();
    let mut pending_calls: Vec<PendingCall<'_>> = Vec::new();
    let mut depth_warned = false;
    let mut cursor = root.walk();

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

                let kind_id = node.kind_id();
                // Pillar 3: gate on manifest passivity. Passive kinds (SkipWrapper /
                // SkipTypeToken / Literal / Unimplemented) only walk children —
                // no extract work. Keep `active_kinds` short-circuit below so
                // AstSkeleton/CfgStatement expression nodes stay zero-cost on
                // the Linux cold-discover hot path (CFG/DefUse run on --cfg/slice).
                let mask = rgctl_plugin_api::dispatch_lookup_tracked(dispatch, kind_id);
                if mask.is_passive() {
                    let base = stack.len();
                    for child in node.named_children(&mut cursor) {
                        let cid = child.kind_id();
                        if !active_kinds.get(cid as usize).copied().unwrap_or(false)
                            && child.child_count() == 0
                        {
                            let _ = rgctl_plugin_api::dispatch_lookup_tracked(dispatch, cid);
                            continue;
                        }
                        stack.push(Visit::Pre(child, depth + 1));
                    }
                    stack[base..].reverse();
                    continue;
                }
                if !active_kinds.get(kind_id as usize).copied().unwrap_or(false) {
                    let base = stack.len();
                    for child in node.named_children(&mut cursor) {
                        let cid = child.kind_id();
                        if !active_kinds.get(cid as usize).copied().unwrap_or(false)
                            && child.child_count() == 0
                        {
                            let _ = rgctl_plugin_api::dispatch_lookup_tracked(dispatch, cid);
                            continue;
                        }
                        stack.push(Visit::Pre(child, depth + 1));
                    }
                    stack[base..].reverse();
                    continue;
                }
                let kind = node.kind();

                let mut pushed_scope = false;
                match kind {
                    "function_definition" | "declaration" => {
                        if let Some(sym) = plugin.extract_function(node, source, &file_path_str)? {
                            let params = sym.parameters.len() as u32;
                            let loc = ((node.end_position().row - node.start_position().row) + 1)
                                .max(1) as u32;
                            let name = sym.name.clone();
                            let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                            ctx.symbols.push(sym);
                            // Prototypes (`declaration`) have no body — no function scope.
                            if kind == "function_definition" {
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
                        }
                    }
                    "struct_specifier" => {
                        if struct_name(node, source).is_some() {
                            let sym = plugin.extract_struct(node, source, &file_path_str, None)?;
                            let name = sym.name.clone();
                            let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                            ctx.symbols.push(sym);
                            ctx.enter_scope(
                                ScopeKind::Struct,
                                Some(name),
                                qn,
                                node.start_byte(),
                                node.end_byte(),
                                true,
                            );
                            pushed_scope = true;
                        }
                    }
                    "enum_specifier" => {
                        if node.child_by_field_name("name").is_some() {
                            let sym = plugin.extract_enum(node, source, &file_path_str)?;
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
                    }
                    "type_definition" => {
                        let typedef_name = type_definition_typedef_name(node, source);
                        let mut cursor = node.walk();
                        for child in node.children(&mut cursor) {
                            if child.kind() == "struct_specifier"
                                && (struct_name(child, source).is_some() || typedef_name.is_some())
                            {
                                ctx.symbols.push(plugin.extract_struct(
                                    child,
                                    source,
                                    &file_path_str,
                                    typedef_name.as_deref(),
                                )?);
                            } else if child.kind() == "enum_specifier"
                                && (child.child_by_field_name("name").is_some()
                                    || typedef_name.is_some())
                            {
                                ctx.symbols.push(plugin.extract_enum_with_typedef(
                                    child,
                                    source,
                                    &file_path_str,
                                    typedef_name.as_deref(),
                                )?);
                            } else if child.kind() == "union_specifier"
                                && (child.child_by_field_name("name").is_some()
                                    || typedef_name.is_some())
                            {
                                let fields =
                                    extract_struct_fields(child, source).unwrap_or_default();
                                if let Some(sym) = depth::extract_union_specifier_named(
                                    child,
                                    source,
                                    &file_path_str,
                                    fields,
                                    typedef_name.as_deref(),
                                ) {
                                    ctx.symbols.push(sym);
                                }
                            }
                        }
                    }
                    "union_specifier" => {
                        if node.child_by_field_name("name").is_some() {
                            let fields = extract_struct_fields(node, source).unwrap_or_default();
                            if let Some(sym) =
                                depth::extract_union_specifier(node, source, &file_path_str, fields)
                            {
                                let name = sym.name.clone();
                                let qn = sym.qualified_name.clone().unwrap_or_else(|| name.clone());
                                ctx.symbols.push(sym);
                                ctx.enter_scope(
                                    ScopeKind::Struct,
                                    Some(name),
                                    qn,
                                    node.start_byte(),
                                    node.end_byte(),
                                    true,
                                );
                                pushed_scope = true;
                            }
                        }
                    }
                    "preproc_include" | "preprocessor_include" => {
                        let path = include_path_from_node(node, source)?;
                        let mut symbol = Symbol {
                            name: path.clone(),
                            symbol_type: SymbolType::Import,
                            qualified_name: None,
                            location: source_location(node, &file_path_str),
                            signature: None,
                            return_type: None,
                            parameters: vec![],
                            fields: vec![],
                            modifiers: vec![],
                            documentation: None,
                            metadata: serde_json::json!({
                                "language": "c",
                                "kind": "include",
                            }),
                            ..Default::default()
                        };
                        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
                        ctx.symbols.push(symbol);
                    }
                    _ => {}
                }

                if C_CALL_KINDS.contains(&kind) {
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
                        | "switch_statement"
                        | "while_statement"
                        | "for_statement"
                        | "do_statement"
                        | "conditional_expression" => {
                            let delta = 1 + ctx
                                .scopes
                                .current_function()
                                .map(|f| f.complexity.cognitive_nesting)
                                .unwrap_or(0);
                            ctx.scopes.note_branch(delta);
                            ctx.scopes.enter_branch();
                            let depth = ctx.scopes.depth();
                            ctx.scopes.push(ScopeFrame::new(
                                ScopeKind::GuardBlock,
                                None,
                                String::new(),
                                node.start_byte(),
                                node.end_byte(),
                                depth,
                            ));
                            stack.push(Visit::LeaveGuard);
                        }
                        "case_statement" => {
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
                // `#if 0` / `#if false`: skip the dead arm but still walk `#else` / `#elif`.
                let dead_preproc = matches!(kind, "preproc_if" | "preproc_ifdef")
                    && is_dead_preproc_condition(node, source);
                let base = stack.len();
                for child in node.named_children(&mut cursor) {
                    if dead_preproc && !matches!(child.kind(), "preproc_else" | "preproc_elif") {
                        continue;
                    }
                    let cid = child.kind_id();
                    if !active_kinds.get(cid as usize).copied().unwrap_or(false)
                        && child.child_count() == 0
                    {
                        let _ = rgctl_plugin_api::dispatch_lookup_tracked(dispatch, cid);
                        continue;
                    }
                    stack.push(Visit::Pre(child, depth + 1));
                }
                stack[base..].reverse();
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
            "c",
            &mut relations,
        );
    }

    Ok(ExtractAllResult::from_parts(ctx.symbols, relations))
}

/// Literal `0` / `false` preprocessor conditions (CFG / extract dead-branch prune).
fn is_dead_preproc_condition(node: Node<'_>, source: &[u8]) -> bool {
    let cond = node
        .child_by_field_name("condition")
        .or_else(|| node.named_child(0));
    let Some(cond) = cond else {
        return false;
    };
    let text = cond
        .utf8_text(source)
        .unwrap_or("")
        .trim()
        .trim_matches(|c: char| c == '(' || c == ')');
    matches!(text, "0" | "false" | "FALSE")
}
