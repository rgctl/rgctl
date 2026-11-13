//! Shared helpers for extracting `Calls` relations from tree-sitter ASTs.

use crate::{Relation, RelationType, ScopeStack, SourceLocation, Symbol, SymbolType};
use std::path::Path;
use tree_sitter::Node;

/// Call node kinds per language grammar.
pub const RUST_CALL_KINDS: &[&str] = &["call_expression", "macro_invocation"];
pub const GO_CALL_KINDS: &[&str] = &["call_expression"];
pub const PYTHON_CALL_KINDS: &[&str] = &["call"];
pub const CSHARP_CALL_KINDS: &[&str] = &["invocation_expression"];
pub const C_CALL_KINDS: &[&str] = &["call_expression"];
pub const CPP_CALL_KINDS: &[&str] = &["call_expression"];
pub const JS_CALL_KINDS: &[&str] = &["call_expression"];
pub const TS_CALL_KINDS: &[&str] = &["call_expression"];
pub const PHP_CALL_KINDS: &[&str] = &[
    "function_call_expression",
    "member_call_expression",
    "scoped_call_expression",
    "nullsafe_member_call_expression",
];
pub const RUBY_CALL_KINDS: &[&str] = &["call"];
pub const KOTLIN_CALL_KINDS: &[&str] = &["call_expression"];

/// Callee name from a Kotlin `call_expression` (`foo()`, `recv.method()`).
pub fn kotlin_call_callee(call: Node, source: &[u8]) -> Option<String> {
    if call.kind() != "call_expression" {
        return None;
    }
    let mut cursor = call.walk();
    for child in call.children(&mut cursor) {
        match child.kind() {
            "navigation_expression" => {
                let mut last = None;
                let mut nc = child.walk();
                for nchild in child.children(&mut nc) {
                    if nchild.kind() == "identifier" {
                        last = nchild.utf8_text(source).ok().map(str::to_string);
                    }
                }
                if let Some(name) = last {
                    return Some(name);
                }
            }
            "identifier" => {
                return child.utf8_text(source).ok().map(str::to_string);
            }
            _ => {}
        }
    }
    callee_name(call, source)
}

/// Callee name from a Ruby `call` node (`receiver.method`, command call, or operator).
pub fn ruby_call_callee(call: Node, source: &[u8]) -> Option<String> {
    if call.kind() != "call" {
        return None;
    }
    if let Some(method) = call.child_by_field_name("method") {
        if method.kind() == "operator" {
            return method.utf8_text(source).ok().map(str::to_string);
        }
        return callee_name(method, source).or_else(|| {
            method
                .utf8_text(source)
                .ok()
                .map(|s| s.trim_start_matches(':').to_string())
        });
    }
    if let Some(op) = call.child_by_field_name("operator") {
        return op.utf8_text(source).ok().map(str::to_string);
    }
    None
}

/// Whether a Ruby call has a non-static callee (variable receiver or dynamic method).
pub fn ruby_call_unresolved(call: Node, source: &[u8]) -> bool {
    if call.kind() != "call" {
        return false;
    }
    if let Some(recv) = call.child_by_field_name("receiver")
        && !matches!(recv.kind(), "constant" | "scope_resolution" | "self")
    {
        return true;
    }
    if let Some(method) = call.child_by_field_name("method") {
        if !matches!(
            method.kind(),
            "identifier" | "constant" | "simple_symbol" | "operator"
        ) {
            return true;
        }
        if method.kind() == "identifier"
            && method
                .utf8_text(source)
                .ok()
                .is_some_and(|t| t.starts_with('@'))
        {
            return true;
        }
    }
    false
}

/// Find the innermost function symbol containing `node`.
///
/// `function_symbols` SHOULD already be filtered to [`SymbolType::Function`].
/// Prefer [`containing_function_from_scope`] during single-pass extraction.
pub fn containing_function<'a>(node: Node, function_symbols: &[&'a Symbol]) -> Option<&'a Symbol> {
    let line = node.start_position().row + 1;
    function_symbols
        .iter()
        .copied()
        .filter(|s| line >= s.location.start_line && line <= s.location.end_line)
        .min_by_key(|s| s.location.end_line - s.location.start_line)
}

/// O(1) enclosing function via [`ScopeStack`] (single-pass AST engine).
///
/// Returns the `symbols[symbol_idx]` entry when the active function frame has
/// recorded a `symbol_idx`; otherwise `None` (caller may fall back to
/// [`containing_function`]).
pub fn containing_function_from_scope<'a>(
    stack: &ScopeStack,
    symbols: &'a [Symbol],
) -> Option<&'a Symbol> {
    let frame = stack.current_function()?;
    let idx = frame.symbol_idx?;
    let sym = symbols.get(idx)?;
    // Methods may be `SymbolType::Method` (Rust impl) or still `Function` elsewhere.
    if matches!(sym.symbol_type, SymbolType::Function | SymbolType::Method) {
        Some(sym)
    } else {
        None
    }
}

/// Best-effort callee name from a call expression subtree (iterative; bounded depth).
pub fn callee_name(root: Node, source: &[u8]) -> Option<String> {
    const MAX_DEPTH: usize = 512;
    let mut stack = vec![(root, 0usize)];

    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            continue;
        }

        match node.kind() {
            // Go method/field selectors use `field_identifier` (not `identifier`).
            // PHP method names use the `name` node kind.
            "identifier"
            | "type_identifier"
            | "field_identifier"
            | "property_identifier"
            | "name"
            | "variable_name" => {
                return node
                    .utf8_text(source)
                    .ok()
                    .map(|s| s.trim_start_matches('$').to_string());
            }
            "field_expression"
            | "selector_expression"
            | "attribute"
            | "member_expression"
            | "member_access_expression" => {
                if let Some(n) = node
                    .child_by_field_name("field")
                    .or_else(|| node.child_by_field_name("attribute"))
                    .or_else(|| node.child_by_field_name("property"))
                    .or_else(|| node.child_by_field_name("name"))
                {
                    stack.push((n, depth + 1));
                }
            }
            // C++ `obj.template meth<T>()` wraps the method in `dependent_name`.
            "dependent_name" => {
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if matches!(
                        ch.kind(),
                        "template_method" | "template_function" | "field_identifier" | "identifier"
                    ) {
                        stack.push((ch, depth + 1));
                        break;
                    }
                }
            }
            "scoped_identifier" | "qualified_type" | "qualified_identifier" => {
                if let Some(n) = last_named_child_by_field(node, "name") {
                    stack.push((n, depth + 1));
                }
            }
            "template_function" | "template_method" => {
                if let Some(n) = node.child_by_field_name("name") {
                    stack.push((n, depth + 1));
                }
            }
            // Rust turbofish: `foo::<T>()` / `Widget::new::<T>()` / `obj.method::<T>()`
            "generic_function" => {
                if let Some(n) = node.child_by_field_name("function") {
                    stack.push((n, depth + 1));
                }
            }
            "operator_name" => {
                return node.utf8_text(source).ok().map(str::to_string);
            }
            "parenthesized_expression" => {
                if let Some(inner) = node.named_child(0) {
                    stack.push((inner, depth + 1));
                }
            }
            "pointer_expression" => {
                if let Some(arg) = node
                    .child_by_field_name("argument")
                    .or_else(|| node.named_child(0))
                {
                    stack.push((arg, depth + 1));
                }
            }
            "cast_expression" => {
                if let Some(inner) = node.child_by_field_name("expression") {
                    stack.push((inner, depth + 1));
                }
            }
            "invocation_expression" => {
                if let Some(n) = node.named_child(0) {
                    stack.push((n, depth + 1));
                }
            }
            _ => {
                if let Some(func) = node.child_by_field_name("function") {
                    stack.push((func, depth + 1));
                } else if let Some(name) = node.child_by_field_name("name") {
                    stack.push((name, depth + 1));
                }
            }
        }
    }
    None
}

/// Push a `Calls` relation when `node` is a recognized call site.
///
/// Resolves the enclosing function via an `O(F)` line scan. Prefer
/// [`push_call_relation_scoped`] during single-pass extraction.
#[allow(clippy::too_many_arguments)]
pub fn push_call_relation(
    node: Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[Symbol],
    function_symbols: &[&Symbol],
    call_kinds: &[&str],
    language: &str,
    relations: &mut Vec<Relation>,
) {
    if !call_kinds.contains(&node.kind()) {
        return;
    }
    let Some(from_fn) = containing_function(node, function_symbols) else {
        return;
    };
    push_call_relation_from(
        node, source, file_path, symbols, from_fn, language, relations,
    );
}

/// Push a `Calls` relation using [`ScopeStack`] for `O(1)` enclosing-function lookup.
///
/// Call sites visited before later same-file callees are extracted still resolve
/// correctly because `symbols` is the in-progress vec and local-target matching
/// runs at emission time — defer emission until the pass finishes if forward
/// references must see the full symbol list (see Rust single-pass flush).
#[allow(clippy::too_many_arguments)]
pub fn push_call_relation_scoped(
    node: Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[Symbol],
    scopes: &ScopeStack,
    call_kinds: &[&str],
    language: &str,
    relations: &mut Vec<Relation>,
) {
    if !call_kinds.contains(&node.kind()) {
        return;
    }
    let Some(from_fn) = containing_function_from_scope(scopes, symbols) else {
        return;
    };
    push_call_relation_from(
        node, source, file_path, symbols, from_fn, language, relations,
    );
}

/// Emit a `Calls` relation with a known enclosing function symbol.
pub fn push_call_relation_from(
    node: Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[Symbol],
    from_fn: &Symbol,
    language: &str,
    relations: &mut Vec<Relation>,
) {
    let callee = if language == "ruby" && node.kind() == "call" {
        ruby_call_callee(node, source)
    } else if language == "kotlin" && node.kind() == "call_expression" {
        kotlin_call_callee(node, source)
    } else {
        None
    }
    .or_else(|| {
        node.child_by_field_name("function")
            .or_else(|| node.child_by_field_name("macro"))
            .or_else(|| node.child_by_field_name("name"))
            .and_then(|n| callee_name(n, source))
            .or_else(|| callee_name(node, source))
    });

    let Some(callee) = callee else {
        return;
    };
    if callee.is_empty() {
        return;
    }

    let from = from_fn
        .qualified_name
        .clone()
        .unwrap_or_else(|| from_fn.name.clone());

    let (to_type_hint, to_qualified_hint) = match language {
        "go" => {
            let ty = go_call_type_hint(node, source, symbols, from_fn);
            let qh = ty.as_ref().map(|t| format!("{t}.{callee}"));
            (ty, qh)
        }
        "rust" => {
            let ty = rust_call_type_hint(node, source, symbols, from_fn);
            let qh = ty.as_ref().map(|t| format!("{t}.{callee}"));
            (ty, qh)
        }
        "cpp" => {
            let qh = cpp_call_qualified_hint(node, source);
            if qh.is_some() {
                (None, qh)
            } else {
                oop_call_type_hint(node, source, symbols, from_fn, &callee, true)
            }
        }
        "python" => {
            let ty = python_call_type_hint(node, source, symbols, from_fn);
            let qh = ty.as_ref().map(|t| format!("{t}.{callee}"));
            (ty, qh)
        }
        "java" => java_call_type_hint(node, source, symbols, from_fn, &callee),
        "ruby" => ruby_call_type_hint(node, source, from_fn, &callee),
        "kotlin" | "groovy" | "scala" | "csharp" | "dart" => {
            oop_call_type_hint(node, source, symbols, from_fn, &callee, true)
        }
        "typescript" | "javascript" | "php" => {
            oop_call_type_hint(node, source, symbols, from_fn, &callee, false)
        }
        _ => (None, None),
    };

    let mut meta = serde_json::json!({ "language": language });
    if language == "cpp" && cpp_call_is_operator(node, source) {
        meta["is_operator"] = serde_json::Value::Bool(true);
    }
    if language == "go"
        && let Some((recv_ty, field)) = go_field_selector_meta(node, source, from_fn)
    {
        meta["go_recv_type"] = serde_json::Value::String(recv_ty);
        meta["go_field"] = serde_json::Value::String(field);
        meta["go_callee"] = serde_json::Value::String(callee.clone());
    }
    if language == "rust"
        && let Some(unresolved) = rust_call_unresolved(node, source)
    {
        meta["unresolved"] = serde_json::Value::Bool(unresolved);
    }
    if language == "c"
        && let Some(unresolved) = c_call_unresolved(node, source, from_fn)
    {
        meta["unresolved"] = serde_json::Value::Bool(unresolved);
    }
    if language == "python"
        && let Some(unresolved) = python_call_unresolved(node, source)
    {
        meta["unresolved"] = serde_json::Value::Bool(unresolved);
    }
    if language == "ruby" && ruby_call_unresolved(node, source) {
        meta["unresolved"] = serde_json::Value::Bool(true);
    }

    // Prefer a unique same-file match unless an MRO type hint points to a different owner
    // (otherwise an unrelated `Other.save` in the same file would hijack `this.save()`
    // on `Impl extends Base` when `Base` lives in another file).
    let same_file_matches: Vec<_> = symbols
        .iter()
        .filter(|s| {
            s.name == callee
                && s.symbol_type == SymbolType::Function
                && s.location.file == file_path.to_string_lossy()
        })
        .collect();
    let local_target = match same_file_matches.as_slice() {
        [only]
            if to_type_hint.as_deref().is_none_or(|hint_ty| {
                same_file_match_compatible_with_hint(only, symbols, hint_ty)
            }) =>
        {
            only.qualified_name
                .clone()
                .unwrap_or_else(|| callee.clone())
        }
        _ => callee.clone(),
    };

    let (call_site_start_byte, call_site_byte_len) = call_site_span(node);
    relations.push(Relation {
        from,
        to: local_target,
        relation_type: RelationType::Calls,
        location: SourceLocation {
            file: file_path.to_string_lossy().to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_column: node.start_position().column,
            end_column: node.end_position().column,
        },
        metadata: meta,
        to_qualified_hint,
        to_type_hint,
        call_site_start_byte,
        call_site_byte_len,
    });
}

/// Best-effort identifier span for a call expression / invocation node.
fn call_site_span(node: Node) -> (Option<u32>, Option<u16>) {
    let name = node
        .child_by_field_name("name")
        .or_else(|| node.child_by_field_name("function"))
        .or_else(|| node.child_by_field_name("method"))
        .unwrap_or(node);
    let start = name.start_byte() as u32;
    let len = name.end_byte().saturating_sub(name.start_byte());
    (Some(start), u16::try_from(len).ok())
}

/// Last child bound to a multi-valued tree-sitter field (e.g. C++ `qualified_identifier::name`).
fn last_named_child_by_field<'a>(node: Node<'a>, field: &str) -> Option<Node<'a>> {
    let mut last = None;
    for i in 0..node.child_count() {
        if node.field_name_for_child(i as u32) == Some(field) {
            let child = node.child(i)?;
            if child.is_named() {
                last = Some(child);
            }
        }
    }
    last
}

/// Best-effort fully-qualified callee for C++ `call_expression` sites.
fn cpp_call_qualified_hint(call: Node, source: &[u8]) -> Option<String> {
    let func = call
        .child_by_field_name("function")
        .or_else(|| call.child_by_field_name("name"))?;
    match func.kind() {
        "qualified_identifier" => qualified_identifier_text(func, source),
        "field_expression" => {
            let value = func.child_by_field_name("value")?;
            let field = func.child_by_field_name("field")?;
            let val = cpp_expression_hint_text(value, source)?;
            let field_name = field.utf8_text(source).ok()?.trim().to_string();
            Some(format!("{val}.{field_name}"))
        }
        "template_function" | "template_method" => {
            let name = func
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok().map(str::to_string))?;
            let scope = func
                .child_by_field_name("scope")
                .and_then(|s| cpp_expression_hint_text(s, source));
            scope.map(|s| format!("{s}::{name}")).or(Some(name))
        }
        _ => None,
    }
}

fn cpp_call_is_operator(call: Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() == "operator_name" {
        return true;
    }
    if func.kind() == "qualified_identifier" {
        return last_named_child_by_field(func, "name")
            .is_some_and(|n| n.kind() == "operator_name");
    }
    func.kind() == "field_expression"
        && func
            .child_by_field_name("field")
            .is_some_and(|f| f.kind() == "operator_name")
        || func
            .child_by_field_name("field")
            .and_then(|f| f.utf8_text(source).ok())
            .is_some_and(|t| t.starts_with("operator"))
}

fn qualified_identifier_text(node: Node, source: &[u8]) -> Option<String> {
    let scope = node
        .child_by_field_name("scope")
        .and_then(|s| cpp_expression_hint_text(s, source));
    let name =
        last_named_child_by_field(node, "name").and_then(|n| cpp_name_component_text(n, source))?;
    scope.map(|s| format!("{s}::{name}")).or(Some(name))
}

fn cpp_expression_hint_text(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "namespace_identifier" | "type_identifier" | "field_identifier" => {
            node.utf8_text(source).ok().map(str::to_string)
        }
        "qualified_identifier" => qualified_identifier_text(node, source),
        "field_expression" => {
            let value = node.child_by_field_name("value")?;
            let field = node.child_by_field_name("field")?;
            let val = cpp_expression_hint_text(value, source)?;
            let field_name = field.utf8_text(source).ok()?.trim().to_string();
            Some(format!("{val}.{field_name}"))
        }
        _ => node
            .utf8_text(source)
            .ok()
            .map(str::trim)
            .map(str::to_string),
    }
}

fn cpp_name_component_text(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier"
        | "field_identifier"
        | "namespace_identifier"
        | "type_identifier"
        | "destructor_name"
        | "operator_name" => node
            .utf8_text(source)
            .ok()
            .map(str::trim)
            .map(str::to_string),
        "template_function" | "template_method" => node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok().map(str::trim).map(str::to_string)),
        "qualified_identifier" => qualified_identifier_text(node, source),
        _ => node
            .utf8_text(source)
            .ok()
            .map(str::trim)
            .map(str::to_string),
    }
}

/// `recv.field.Method` → (receiver_type, field_name) for late resolution in GraphBuilder.
fn go_field_selector_meta(call: Node, source: &[u8], from_fn: &Symbol) -> Option<(String, String)> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "selector_expression" {
        return None;
    }
    let operand = func.child_by_field_name("operand")?;
    if operand.kind() != "selector_expression" {
        return None;
    }
    let inner_op = operand.child_by_field_name("operand")?;
    let field = operand.child_by_field_name("field")?;
    if inner_op.kind() != "identifier" {
        return None;
    }
    let recv_name = inner_op.utf8_text(source).ok()?;
    let field_name = field.utf8_text(source).ok()?.to_string();
    let recv_ok = from_fn
        .metadata
        .get("receiver_name")
        .and_then(|v| v.as_str())
        == Some(recv_name);
    if !recv_ok {
        return None;
    }
    let recv_ty = from_fn
        .metadata
        .get("receiver_type")
        .and_then(|v| v.as_str())?
        .trim_start_matches('*')
        .to_string();
    Some((recv_ty, field_name))
}

/// Best-effort Go receiver/field type for `x.Method` / `x.field.Method` call sites.
fn go_call_type_hint(
    call: Node,
    source: &[u8],
    symbols: &[Symbol],
    from_fn: &Symbol,
) -> Option<String> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "selector_expression" {
        return None;
    }
    let operand = func.child_by_field_name("operand")?;

    // `recv.Method` where recv is the method receiver variable.
    if operand.kind() == "identifier" {
        let recv_name = operand.utf8_text(source).ok()?;
        if let Some(rt) = from_fn
            .metadata
            .get("receiver_name")
            .and_then(|v| v.as_str())
            && rt == recv_name
        {
            return from_fn
                .metadata
                .get("receiver_type")
                .and_then(|v| v.as_str())
                .map(|s| s.trim_start_matches('*').to_string());
        }
        return None;
    }

    // `recv.field.Method` — resolve `field` on the receiver struct type.
    if operand.kind() == "selector_expression" {
        let inner_op = operand.child_by_field_name("operand")?;
        let field = operand.child_by_field_name("field")?;
        if inner_op.kind() != "identifier" {
            return None;
        }
        let recv_name = inner_op.utf8_text(source).ok()?;
        let field_name = field.utf8_text(source).ok()?;
        let recv_ok = from_fn
            .metadata
            .get("receiver_name")
            .and_then(|v| v.as_str())
            == Some(recv_name);
        if !recv_ok {
            return None;
        }
        let recv_ty = from_fn
            .metadata
            .get("receiver_type")
            .and_then(|v| v.as_str())?
            .trim_start_matches('*');
        let owner = symbols.iter().find(|s| {
            matches!(
                s.symbol_type,
                SymbolType::Struct
                    | SymbolType::Class
                    | SymbolType::Interface
                    | SymbolType::Annotation
            ) && s.name == recv_ty
        })?;
        if owner.symbol_type == SymbolType::Interface || owner.symbol_type == SymbolType::Annotation
        {
            return Some(owner.name.clone());
        }
        let ft = owner
            .fields
            .iter()
            .find(|f| f.name == field_name)?
            .field_type
            .as_deref()?;
        return Some(go_simple_type_name(ft));
    }

    None
}

/// Best-effort Rust type for scoped / turbofish / method calls.
///
/// Handles:
/// - `Widget::new()` / `crate::mod::f()` via `scoped_identifier`
/// - `Self::helper()` via enclosing impl type from `from_fn.qualified_name`
/// - `foo::<T>()` / `Widget::new::<T>()` via `generic_function` unwrap
/// - `receiver.field.method()` and param-typed receivers via `field_expression`
fn rust_call_type_hint(
    call: Node,
    source: &[u8],
    symbols: &[Symbol],
    from_fn: &Symbol,
) -> Option<String> {
    let mut func = call
        .child_by_field_name("function")
        .or_else(|| call.child_by_field_name("macro"))?;
    // Unwrap turbofish wrappers so scoped / field receivers stay visible.
    while func.kind() == "generic_function" {
        func = func.child_by_field_name("function")?;
    }
    if func.kind() == "scoped_identifier" || func.kind() == "scoped_type_identifier" {
        return rust_scoped_path_type_hint(func, source, from_fn);
    }
    if func.kind() == "field_expression" {
        let value = func.child_by_field_name("value")?;
        let field = func.child_by_field_name("field")?;
        let field_name = field.utf8_text(source).ok()?;
        if value.kind() == "identifier" {
            let recv = value.utf8_text(source).ok()?;
            if let Some(param) = from_fn
                .parameters
                .iter()
                .find(|p| p.name == recv)
                .and_then(|p| p.param_type.as_deref())
            {
                return Some(rust_simple_type_name(param));
            }
        }
        if value.kind() == "field_expression" {
            let inner = value.child_by_field_name("value")?;
            let inner_field = value.child_by_field_name("field")?;
            if inner.kind() == "identifier" {
                let recv = inner.utf8_text(source).ok()?;
                let inner_name = inner_field.utf8_text(source).ok()?;
                if let Some(param) = from_fn
                    .parameters
                    .iter()
                    .find(|p| p.name == recv)
                    .and_then(|p| p.param_type.as_deref())
                {
                    let owner_ty = rust_simple_type_name(param);
                    let owner = symbols.iter().find(|s| {
                        matches!(s.symbol_type, SymbolType::Struct | SymbolType::Enum)
                            && s.name == owner_ty
                    })?;
                    return owner
                        .fields
                        .iter()
                        .find(|f| f.name == inner_name)
                        .and_then(|f| f.field_type.as_deref())
                        .map(rust_simple_type_name);
                }
            }
        }
        // `self.field` in inherent methods — match param named self with &Type
        if (value.kind() == "self" || value.utf8_text(source).ok() == Some("self"))
            && let Some(self_ty) = from_fn
                .parameters
                .iter()
                .find(|p| p.name == "self" || p.name == "&self" || p.name == "&mut self")
                .and_then(|p| p.param_type.as_deref())
        {
            let owner_ty = rust_simple_type_name(self_ty);
            let owner = symbols.iter().find(|s| {
                matches!(s.symbol_type, SymbolType::Struct | SymbolType::Enum) && s.name == owner_ty
            })?;
            return owner
                .fields
                .iter()
                .find(|f| f.name == field_name)
                .and_then(|f| f.field_type.as_deref())
                .map(rust_simple_type_name);
        }
    }
    None
}

fn rust_call_unresolved(call: Node, source: &[u8]) -> Option<bool> {
    let mut func = call
        .child_by_field_name("function")
        .or_else(|| call.child_by_field_name("macro"))?;
    if call.kind() == "macro_invocation" {
        return Some(true);
    }
    while func.kind() == "generic_function" {
        match func.child_by_field_name("function") {
            Some(inner) => func = inner,
            None => return Some(true),
        }
    }
    if matches!(
        func.kind(),
        "scoped_identifier" | "scoped_type_identifier" | "identifier"
    ) {
        return Some(false);
    }
    if func.kind() == "field_expression" {
        let value = func.child_by_field_name("value")?;
        if value.kind() != "identifier"
            && value.kind() != "self"
            && value.kind() != "field_expression"
        {
            return Some(true);
        }
        return Some(false);
    }
    if func.kind() == "parenthesized_expression" {
        return Some(true);
    }
    let _ = source;
    None
}

/// `Widget::new` / `Self::helper` / `crate::mod::f` → receiver / path type hint.
fn rust_scoped_path_type_hint(func: Node, source: &[u8], from_fn: &Symbol) -> Option<String> {
    let path = func.child_by_field_name("path")?;
    let path_text = path.utf8_text(source).ok()?.trim();
    if path_text == "Self" || path_text.ends_with("::Self") {
        return rust_enclosing_impl_type(from_fn);
    }
    // `crate::foo::Bar` / `super::Bar` / `Bar` — take the rightmost path segment.
    Some(rust_simple_type_name(path_text))
}

/// Inherent / trait-impl methods store `Type::method` or `Type::<init>` in `qualified_name`.
///
/// Only [`SymbolType::Method`] resolves `Self` — free functions whose QN is
/// `module::path::fn` must not treat the last path segment as an impl type.
fn rust_enclosing_impl_type(from_fn: &Symbol) -> Option<String> {
    if from_fn.symbol_type != SymbolType::Method {
        return None;
    }
    let qn = from_fn.qualified_name.as_deref()?;
    let (owner, _) = qn.rsplit_once("::")?;
    let simple = owner
        .rsplit("::")
        .next()
        .unwrap_or(owner)
        .split('<')
        .next()
        .unwrap_or(owner)
        .trim();
    if simple.is_empty() || simple == "Self" {
        return None;
    }
    Some(simple.to_string())
}

/// Best-effort: function-pointer / indirect C calls.
fn c_call_unresolved(call: Node, source: &[u8], from_fn: &Symbol) -> Option<bool> {
    let func = call.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => {
            let name = func.utf8_text(source).ok()?;
            if from_fn.parameters.iter().any(|p| p.name == name) {
                return Some(true);
            }
        }
        "pointer_expression" | "parenthesized_expression" => {
            if let Some(name) = callee_name(func, source)
                && from_fn.parameters.iter().any(|p| p.name == name)
            {
                return Some(true);
            }
            return Some(true);
        }
        _ => {}
    }
    None
}

fn rust_simple_type_name(ty: &str) -> String {
    ty.trim_start_matches('&')
        .trim_start_matches("mut ")
        .trim()
        .rsplit("::")
        .next()
        .unwrap_or(ty)
        .split('<')
        .next()
        .unwrap_or(ty)
        .to_string()
}

/// `*pkg.Type` / `pkg.Type` → `Type` for Go resolution indexes.
fn go_simple_type_name(ty: &str) -> String {
    ty.trim_start_matches('*')
        .rsplit(['.', '/'])
        .next()
        .unwrap_or(ty)
        .to_string()
}

/// Best-effort Python type for `self.field.method()` call sites.
pub fn infer_python_method_target(
    call: Node,
    source: &[u8],
    symbols: &[Symbol],
    from_fn: &Symbol,
) -> (Option<String>, Option<String>) {
    python_call_type_hint(call, source, symbols, from_fn)
        .map(|ty| {
            let method = call
                .child_by_field_name("function")
                .and_then(|f| callee_name(f, source))
                .unwrap_or_default();
            let qualified = if method.is_empty() {
                ty.clone()
            } else {
                format!("{ty}.{method}")
            };
            (Some(qualified), Some(ty))
        })
        .unwrap_or((None, None))
}

fn python_call_type_hint(
    call: Node,
    source: &[u8],
    symbols: &[Symbol],
    from_fn: &Symbol,
) -> Option<String> {
    let func = call.child_by_field_name("function")?;
    if func.kind() != "attribute" {
        return None;
    }
    let method_name = callee_name(func, source)?;
    let object = func.child_by_field_name("object")?;
    let (field_name, owner_class) = match object.kind() {
        "attribute" => {
            let inner_obj = object.child_by_field_name("object")?;
            let field = object.child_by_field_name("attribute")?;
            if inner_obj.utf8_text(source).ok()? != "self" {
                return None;
            }
            let field_name = field.utf8_text(source).ok()?.to_string();
            let owner = containing_python_class_name(from_fn)?;
            (field_name, owner)
        }
        "identifier" if matches!(object.utf8_text(source).ok()?, "self" | "cls") => {
            let field = func.child_by_field_name("attribute")?;
            let field_name = field.utf8_text(source).ok()?.to_string();
            let owner = containing_python_class_name(from_fn)?;
            if let Some(class_sym) = symbols
                .iter()
                .find(|s| s.symbol_type == SymbolType::Class && s.name == owner)
                && let Some(ft) = class_sym
                    .fields
                    .iter()
                    .find(|f| f.name == field_name)
                    .and_then(|f| f.field_type.as_deref())
            {
                return Some(python_simple_type_name_str(ft));
            }
            return Some(owner);
        }
        _ => return None,
    };
    let _ = method_name;
    let class_sym = symbols
        .iter()
        .find(|s| s.symbol_type == SymbolType::Class && s.name == owner_class)?;
    class_sym
        .fields
        .iter()
        .find(|f| f.name == field_name)
        .and_then(|f| f.field_type.as_deref())
        .map(python_simple_type_name_str)
}

fn java_call_type_hint(
    call: Node,
    source: &[u8],
    symbols: &[Symbol],
    from_fn: &Symbol,
    callee: &str,
) -> (Option<String>, Option<String>) {
    if call.kind() != "method_invocation" {
        return (None, None);
    }
    let owner = from_fn
        .qualified_name
        .as_deref()
        .and_then(|qn| qn.rsplit_once('.').map(|(o, _)| o.to_string()));
    let Some(object) = call.child_by_field_name("object") else {
        let qh = owner.as_ref().map(|o| format!("{o}.{callee}"));
        return (owner, qh);
    };
    let Ok(obj_text) = object.utf8_text(source).map(str::trim) else {
        return (None, None);
    };
    if object.kind() == "this" || obj_text == "this" {
        let qh = owner.as_ref().map(|o| format!("{o}.{callee}"));
        return (owner, qh);
    }
    if object.kind() == "super" || obj_text == "super" {
        return (owner, Some(format!("super.{callee}")));
    }
    if object.kind() == "identifier" {
        if let Some(param_ty) = from_fn
            .parameters
            .iter()
            .find(|p| p.name == obj_text)
            .and_then(|p| p.param_type.as_deref())
        {
            let simple = java_simple_type_name_str(param_ty);
            let qh = Some(format!("{simple}.{callee}"));
            return (Some(simple), qh);
        }
        if let Some(ref owner_name) = owner {
            let owner_simple = java_simple_type_name_str(owner_name);
            if let Some(class_sym) = symbols.iter().find(|s| {
                matches!(s.symbol_type, SymbolType::Class | SymbolType::Interface)
                    && (s.name == owner_simple
                        || s.qualified_name.as_deref() == Some(owner_name.as_str()))
            }) && let Some(ft) = class_sym
                .fields
                .iter()
                .find(|f| f.name == obj_text)
                .and_then(|f| f.field_type.as_deref())
            {
                let simple = java_simple_type_name_str(ft);
                let qh = Some(format!("{simple}.{callee}"));
                return (Some(simple), qh);
            }
        }
    }
    (None, None)
}

fn same_file_match_compatible_with_hint(only: &Symbol, symbols: &[Symbol], hint_ty: &str) -> bool {
    let Some(qn) = only.qualified_name.as_deref() else {
        return true;
    };
    let Some((owner_prefix, _)) = qn
        .rsplit_once("::")
        .or_else(|| qn.rsplit_once('.'))
        .or_else(|| qn.rsplit_once('#'))
    else {
        return true;
    };
    let owner_is_type = symbols.iter().any(|s| {
        matches!(
            s.symbol_type,
            SymbolType::Class | SymbolType::Struct | SymbolType::Interface | SymbolType::Enum
        ) && (s.qualified_name.as_deref() == Some(owner_prefix) || s.name == owner_prefix)
    });
    if !owner_is_type {
        // Package/namespace-prefixed free function (`pkg.helper`), not an unrelated class method.
        return true;
    }
    let owner_simple = owner_prefix
        .rsplit(['.', ':', '\\', '#'])
        .next()
        .unwrap_or(owner_prefix);
    let hint_simple = hint_ty
        .rsplit(['.', ':', '\\', '#'])
        .next()
        .unwrap_or(hint_ty);
    owner_simple == hint_simple || owner_prefix == hint_ty
}

/// Shared OOP receiver / implicit-`this` type hint for Kotlin, Groovy, Scala,
/// C#, C++, Dart, TypeScript, JavaScript, and PHP.
fn oop_call_type_hint(
    call: Node,
    source: &[u8],
    symbols: &[Symbol],
    from_fn: &Symbol,
    callee: &str,
    allow_implicit_this: bool,
) -> (Option<String>, Option<String>) {
    let head_end = {
        let mut end = call.end_byte();
        let mut c = call.walk();
        for ch in call.children(&mut c) {
            if matches!(
                ch.kind(),
                "arguments"
                    | "argument_list"
                    | "value_arguments"
                    | "formal_parameters"
                    | "lambda_literal"
                    | "annotated_lambda"
                    | "closure"
                    | "block"
            ) {
                end = end.min(ch.start_byte());
            }
        }
        end
    };
    let raw_slice = source
        .get(call.start_byte()..head_end)
        .and_then(|b| std::str::from_utf8(b).ok())
        .unwrap_or("");
    let head = raw_slice
        .split(['(', '{'])
        .next()
        .unwrap_or(raw_slice)
        .trim();
    if head.is_empty() || head.starts_with("new ") {
        return (None, None);
    }

    let enclosing_owner = from_fn.qualified_name.as_deref().and_then(|qn| {
        let (prefix, _) = qn
            .rsplit_once("::")
            .or_else(|| qn.rsplit_once('.'))
            .or_else(|| qn.rsplit_once('#'))?;
        let is_type = symbols.iter().any(|s| {
            matches!(
                s.symbol_type,
                SymbolType::Class | SymbolType::Struct | SymbolType::Interface | SymbolType::Enum
            ) && (s.qualified_name.as_deref() == Some(prefix)
                || s.name
                    == prefix
                        .rsplit(['.', ':', '\\', '#'])
                        .next()
                        .unwrap_or(prefix))
        });
        is_type.then(|| prefix.to_string())
    });

    // Extract explicit receiver before `->`, `?.`, `::`, or `.`.
    let recv = head
        .rsplit_once("->")
        .or_else(|| head.rsplit_once("?."))
        .or_else(|| head.rsplit_once("::"))
        .or_else(|| head.rsplit_once('.'))
        .map(|(r, _)| r.trim());

    match recv {
        None => {
            if allow_implicit_this && let Some(owner) = enclosing_owner {
                let qh = format!("{owner}.{callee}");
                return (Some(owner), Some(qh));
            }
            (None, None)
        }
        Some("this" | "$this" | "self" | "static") => {
            if let Some(owner) = enclosing_owner {
                let qh = format!("{owner}.{callee}");
                return (Some(owner), Some(qh));
            }
            (None, None)
        }
        Some("super" | "base" | "parent") => {
            if let Some(owner) = enclosing_owner {
                return (Some(owner), Some(format!("super.{callee}")));
            }
            (None, None)
        }
        Some(r) => {
            let clean_r = r.trim_start_matches('$').trim();
            if clean_r.is_empty() {
                return (None, None);
            }
            if let Some(param_ty) = from_fn
                .parameters
                .iter()
                .find(|p| p.name.trim_start_matches('$') == clean_r)
                .and_then(|p| p.param_type.as_deref())
                .or_else(|| {
                    from_fn
                        .signature
                        .as_deref()
                        .and_then(|sig| infer_param_type_from_signature(sig, clean_r))
                })
            {
                let simple = oop_simple_type_name(param_ty);
                if !simple.is_empty() {
                    let qh = format!("{simple}.{callee}");
                    return (Some(simple), Some(qh));
                }
            }
            if symbols.iter().any(|s| {
                matches!(
                    s.symbol_type,
                    SymbolType::Class
                        | SymbolType::Struct
                        | SymbolType::Interface
                        | SymbolType::Enum
                        | SymbolType::Module
                ) && (s.name == clean_r || s.qualified_name.as_deref() == Some(clean_r))
            }) {
                let simple = oop_simple_type_name(clean_r);
                let qh = format!("{simple}.{callee}");
                return (Some(simple), Some(qh));
            }
            (None, None)
        }
    }
}

/// Parse `name: Type` (Kotlin/Scala/TS/Swift/Rust) or `Type name` / `Type $name` (Java/C#/Dart/PHP/C++/Groovy)
/// inside a declaration header signature when `Symbol.parameters` was left unpopulated.
pub fn infer_param_type_from_signature<'a>(sig: &'a str, param_name: &str) -> Option<&'a str> {
    let start = sig.find('(')?;
    let end = sig.rfind(')')?;
    if end <= start + 1 {
        return None;
    }
    for raw_part in sig[start + 1..end].split(',') {
        let part = raw_part.split('=').next().unwrap_or(raw_part).trim();
        if part.is_empty() {
            continue;
        }
        if let Some((lhs, rhs)) = part.split_once(':') {
            // `app: App` or Swift `for app: App`
            let lhs_ident = lhs
                .split_whitespace()
                .last()
                .unwrap_or("")
                .trim_start_matches('$');
            if lhs_ident == param_name {
                let ty = rhs.trim();
                if !ty.is_empty() {
                    return Some(ty);
                }
            }
        } else {
            let mut toks = part.split_whitespace().rev();
            let name_tok = toks
                .next()
                .unwrap_or("")
                .trim_start_matches(['$', '&', '*']);
            if name_tok == param_name
                && let Some(ty_tok) = toks.next()
            {
                let ty = ty_tok.trim_matches(['&', '*', '?']);
                if !ty.is_empty()
                    && !matches!(
                        ty,
                        "final" | "val" | "var" | "in" | "out" | "ref" | "params" | "required"
                    )
                {
                    return Some(ty);
                }
            }
        }
    }
    None
}

fn oop_simple_type_name(ty: &str) -> String {
    ty.trim()
        .trim_start_matches(['&', '*', '?'])
        .split(['<', '['])
        .next()
        .unwrap_or(ty)
        .trim_end_matches(['?', '*', '&'])
        .rsplit(['.', ':', '\\'])
        .next()
        .unwrap_or(ty)
        .trim()
        .to_string()
}

fn java_simple_type_name_str(ty: &str) -> String {
    ty.trim()
        .split('<')
        .next()
        .unwrap_or(ty)
        .trim_end_matches("[]")
        .trim_end_matches("...")
        .rsplit('.')
        .next()
        .unwrap_or(ty)
        .trim()
        .to_string()
}

fn ruby_call_type_hint(
    call: Node,
    source: &[u8],
    from_fn: &Symbol,
    callee: &str,
) -> (Option<String>, Option<String>) {
    if call.kind() != "call" {
        return (None, None);
    }
    if matches!(
        callee,
        "require"
            | "require_relative"
            | "include"
            | "prepend"
            | "extend"
            | "attr_reader"
            | "attr_writer"
            | "attr_accessor"
            | "private"
            | "protected"
            | "public"
            | "raise"
            | "puts"
            | "print"
    ) {
        return (None, None);
    }
    let owner = from_fn.qualified_name.as_deref().and_then(|qn| {
        qn.rsplit_once('#')
            .or_else(|| qn.rsplit_once('.'))
            .map(|(o, _)| o.to_string())
    });
    match call.child_by_field_name("receiver") {
        None => {
            let qh = owner.as_ref().map(|o| format!("{o}#{callee}"));
            (owner, qh)
        }
        Some(recv) if recv.kind() == "self" || recv.utf8_text(source).ok() == Some("self") => {
            let qh = owner.as_ref().map(|o| format!("{o}#{callee}"));
            (owner, qh)
        }
        Some(recv) if matches!(recv.kind(), "constant" | "scope_resolution") => {
            let Ok(txt) = recv.utf8_text(source).map(str::trim) else {
                return (None, None);
            };
            if txt.is_empty() {
                (None, None)
            } else {
                (Some(txt.to_string()), Some(format!("{txt}.{callee}")))
            }
        }
        _ => (None, None),
    }
}

fn containing_python_class_name(from_fn: &Symbol) -> Option<String> {
    from_fn.qualified_name.as_ref().and_then(|qn| {
        qn.strip_suffix(".<init>")
            .or_else(|| qn.rsplit_once('.').map(|(c, _)| c))
            .map(str::to_string)
    })
}

fn python_simple_type_name_str(ty: &str) -> String {
    ty.trim()
        .split('[')
        .next()
        .unwrap_or(ty)
        .rsplit('.')
        .next()
        .unwrap_or(ty)
        .to_string()
}

fn python_call_unresolved(call: Node, source: &[u8]) -> Option<bool> {
    let func = call.child_by_field_name("function")?;
    if func.kind() == "subscript" {
        return Some(true);
    }
    if func.kind() != "attribute" {
        return None;
    }
    let attr = func.child_by_field_name("attribute")?;
    match attr.kind() {
        "identifier" => Some(false),
        _ => attr
            .utf8_text(source)
            .ok()
            .map(|t| t.starts_with('[') || t.contains('(')),
    }
}

/// Iterative tree walk that records call relations (heap stack; bounded depth).
pub fn walk_calls(
    root: Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[Symbol],
    call_kinds: &[&str],
    language: &str,
    relations: &mut Vec<Relation>,
) {
    const MAX_DEPTH: usize = 2048;
    let function_symbols: Vec<&Symbol> = symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Function)
        .collect();
    let mut stack = vec![(root, 0usize)];

    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            tracing::warn!(
                file = ?file_path,
                depth = depth,
                "AST depth limit exceeded during walk_calls; skipping deep branches"
            );
            continue;
        }

        push_call_relation(
            node,
            source,
            file_path,
            symbols,
            &function_symbols,
            call_kinds,
            language,
            relations,
        );

        let child_count = node.child_count();
        for i in (0..child_count).rev() {
            if let Some(child) = node.child(i) {
                stack.push((child, depth + 1));
            }
        }
    }
}
