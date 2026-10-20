//! Best-effort local/param type recovery for field-write indexing (Layer F5).
//!
//! Bound resolution only: formal parameters and explicitly typed locals inside the
//! target function. No full type inference / reflection.

use crate::hash_maps::FxHashMap;
use crate::language_profile::{function_kinds_for, parse_source};
use std::collections::HashMap;
use tree_sitter::{Node, Tree};

type VisitFn = fn(Node, &[u8], &str, &mut HashMap<String, String>, bool);

/// Pre-parsed source for merging locals across many functions in one file.
///
/// Tree-sitter [`Tree`] is not shared across threads; keep one context per Rayon task.
pub struct LocalsParseContext<'a> {
    source: &'a str,
    tree: Tree,
    visit: VisitFn,
}

impl<'a> LocalsParseContext<'a> {
    /// Parse `source` once for `language` via thread-local [`parse_source`].
    /// Returns `None` for unknown languages or parse failure.
    pub fn try_new(language: &str, source: &'a str) -> Option<Self> {
        let visit = language_visit_fn(language)?;
        let tree = parse_source(language, source.as_bytes()).ok()?;
        Some(Self {
            source,
            tree,
            visit,
        })
    }

    /// Reuse an already-parsed tree (no re-parse).
    pub fn from_tree(language: &str, source: &'a str, tree: Tree) -> Option<Self> {
        let visit = language_visit_fn(language)?;
        Some(Self {
            source,
            tree,
            visit,
        })
    }

    /// Merge typed locals + formals for `function_name` into `env` using the cached tree.
    pub fn merge_into(&self, function_name: &str, env: &mut HashMap<String, String>) {
        (self.visit)(
            self.tree.root_node(),
            self.source.as_bytes(),
            function_name,
            env,
            false,
        );
    }

    /// Borrow the cached tree.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
}

fn language_visit_fn(language: &str) -> Option<VisitFn> {
    Some(match language {
        "java" => visit_java,
        "csharp" => visit_csharp,
        "go" => visit_go,
        "rust" => visit_rust,
        "python" => visit_python,
        "javascript" => visit_javascript,
        "typescript" => visit_typescript,
        "c" => visit_c_family,
        "cpp" => visit_c_family,
        "php" => visit_php,
        "ruby" => visit_ruby,
        "puppet" => visit_puppet,
        "kotlin" | "kt" => visit_kotlin,
        "groovy" => visit_groovy,
        _ => return None,
    })
}

/// Single AST walk: local-type maps keyed by `(start_line, function_name)`.
pub fn build_file_local_types_from_tree(
    language: &str,
    root: Node<'_>,
    source: &[u8],
) -> FxHashMap<(usize, String), HashMap<String, String>> {
    let Some(visit) = language_visit_fn(language) else {
        return FxHashMap::default();
    };
    let Ok(kinds) = function_kinds_for(language) else {
        return FxHashMap::default();
    };
    let mut out: FxHashMap<(usize, String), HashMap<String, String>> = FxHashMap::default();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if kinds.contains(&node.kind()) {
            if let Some(name) = function_name_for_locals(language, node, source) {
                let start_line = node.start_position().row + 1;
                let mut env = HashMap::new();
                // Start at the function node so `visit_*` sets `in_target` for this name.
                visit(node, source, &name, &mut env, false);
                if !env.is_empty() {
                    out.insert((start_line, name), env);
                }
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            stack.push(child);
        }
    }
    out
}

fn function_name_for_locals(language: &str, node: Node<'_>, source: &[u8]) -> Option<String> {
    if matches!(language, "java" | "csharp" | "kotlin" | "kt" | "groovy")
        && matches!(
            node.kind(),
            "constructor_declaration" | "primary_constructor" | "secondary_constructor"
        )
    {
        return find_ancestor_name(node, source, "class_declaration")
            .or_else(|| find_ancestor_name(node, source, "object_declaration"));
    }
    node.child_by_field_name("name")
        .and_then(|n| text_of(n, source))
        .or_else(|| {
            rgctl_plugin_helpers::extract_name_from_node(node, source)
                .ok()
                .flatten()
        })
}

/// Merge typed locals + formals for `function_name` into `env` (name → bare type).
pub fn merge_local_types(
    language: &str,
    source: &str,
    function_name: &str,
    env: &mut HashMap<String, String>,
) {
    if let Some(ctx) = LocalsParseContext::try_new(language, source) {
        ctx.merge_into(function_name, env);
    }
}

/// Extract formal parameter types for one function (shallow; no body walk).
pub fn extract_param_types_in_function(
    language: &str,
    func_node: Node,
    source: &[u8],
    _function_name: &str,
) -> HashMap<String, String> {
    let mut env = HashMap::new();
    collect_param_types_shallow(func_node, source, language, &mut env);
    env
}

fn collect_param_types_shallow(
    node: Node,
    source: &[u8],
    language: &str,
    env: &mut HashMap<String, String>,
) {
    match node.kind() {
        "parameter_declaration" => {
            if matches!(language, "c" | "cpp" | "go") {
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|n| text_of(n, source));
                let name = node
                    .child_by_field_name("declarator")
                    .and_then(|d| declarator_ident(d, source))
                    .or_else(|| {
                        node.child_by_field_name("name")
                            .and_then(|n| text_of(n, source))
                    });
                if let (Some(name), Some(ty)) = (name, ty) {
                    insert_ty(env, &name, &ty);
                }
            }
        }
        "formal_parameter" | "required_parameter" | "optional_parameter" => {
            let ty = node
                .child_by_field_name("type")
                .and_then(|n| text_of(n, source));
            let name = node
                .child_by_field_name("name")
                .and_then(|n| text_of(n, source));
            if let (Some(name), Some(ty)) = (name, ty) {
                insert_ty(env, &name, &ty);
            }
        }
        "parameter" => {
            let ty = node
                .child(0)
                .filter(|c| c.is_named())
                .and_then(|n| text_of(n, source));
            let name = node
                .child_by_field_name("pattern")
                .or_else(|| node.child_by_field_name("name"))
                .and_then(|n| text_of(n, source));
            if let (Some(name), Some(ty)) = (name, ty) {
                insert_ty(env, &name, &ty);
            }
        }
        "parameter_list"
        | "formal_parameters"
        | "function_declarator"
        | "function_signature"
        | "method_declaration"
        | "constructor_declaration"
        | "function_definition"
        | "method_definition" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                collect_param_types_shallow(child, source, language, env);
            }
        }
        _ => {}
    }
}

/// Extract typed locals + formals for one function AST node (CFG build time).
pub fn extract_local_types_in_function(
    language: &str,
    func_node: Node,
    source: &[u8],
    function_name: &str,
) -> HashMap<String, String> {
    extract_param_types_in_function(language, func_node, source, function_name)
}

/// One parse + one AST walk: all function local-type maps in a source file (fallback).
pub fn build_file_local_types_index(
    language: &str,
    source: &str,
    function_names: &[String],
) -> HashMap<String, HashMap<String, String>> {
    let Some(ctx) = LocalsParseContext::try_new(language, source) else {
        return HashMap::new();
    };
    let mut out = HashMap::with_capacity(function_names.len());
    for name in function_names {
        let mut env = HashMap::new();
        ctx.merge_into(name, &mut env);
        if !env.is_empty() {
            out.insert(name.clone(), env);
        }
    }
    out
}

/// Extension → language id for field-write locals fallback.
pub fn language_from_path(path: &str) -> String {
    std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|ext| match ext {
            "java" => "java",
            "cs" => "csharp",
            "go" => "go",
            "rs" => "rust",
            "py" => "python",
            "js" | "jsx" | "mjs" | "cjs" => "javascript",
            "ts" | "tsx" => "typescript",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" | "hh" => "cpp",
            "php" => "php",
            "rb" => "ruby",
            "kt" | "kts" => "kotlin",
            "groovy" | "gradle" => "groovy",
            _ => "unknown",
        })
        .unwrap_or("unknown")
        .to_string()
}

fn normalize_type_name(name: &str) -> String {
    let bare = name.split('<').next().unwrap_or(name).trim();
    let bare = bare.trim_start_matches('*').trim().trim_start_matches('&');
    bare.rsplit('.')
        .next()
        .unwrap_or(bare)
        .rsplit("::")
        .next()
        .unwrap_or(bare)
        .trim()
        .to_string()
}

fn insert_ty(env: &mut HashMap<String, String>, name: &str, ty: &str) {
    let n = name.trim();
    if n.is_empty() || n == "_" || n == "self" || n == "this" {
        return;
    }
    env.entry(n.to_string())
        .or_insert_with(|| normalize_type_name(ty));
}

fn text_of(node: Node, source: &[u8]) -> Option<String> {
    node.utf8_text(source).ok().map(|s| s.to_string())
}

fn walk_children(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
    visit: fn(Node, &[u8], &str, &mut HashMap<String, String>, bool),
    reset_kinds: &[&str],
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if reset_kinds.iter().any(|k| child.kind() == *k) {
            visit(child, source, function_name, env, false);
        } else {
            visit(child, source, function_name, env, in_target);
        }
    }
}

fn visit_java(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if kind == "method_declaration" || kind == "constructor_declaration" {
        let name = if kind == "constructor_declaration" {
            find_ancestor_name(node, source, "class_declaration").unwrap_or_default()
        } else {
            node.child_by_field_name("name")
                .and_then(|n| text_of(n, source))
                .unwrap_or_default()
        };
        now_in = name == function_name;
    }
    if now_in && kind == "local_variable_declaration" {
        collect_java_style_local(node, source, env);
    }
    if now_in && kind == "formal_parameter" {
        if let (Some(name), Some(ty)) = (
            node.child_by_field_name("name")
                .and_then(|n| text_of(n, source)),
            node.child_by_field_name("type")
                .and_then(|n| text_of(n, source)),
        ) {
            insert_ty(env, &name, &ty);
        }
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_java,
        &["method_declaration", "constructor_declaration"],
    );
}

fn collect_java_style_local(node: Node, source: &[u8], env: &mut HashMap<String, String>) {
    let mut ty = None;
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "type_identifier"
                | "generic_type"
                | "integral_type"
                | "floating_point_type"
                | "boolean_type"
                | "array_type"
                | "nullable_type"
                | "predefined_type"
        ) {
            ty = text_of(child, source);
        }
        if child.kind() == "variable_declarator" {
            if let (Some(name), Some(t)) = (
                child
                    .child_by_field_name("name")
                    .and_then(|n| text_of(n, source)),
                ty.as_ref(),
            ) {
                insert_ty(env, &name, t);
            }
        }
    }
}

fn visit_csharp(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if matches!(
        kind,
        "method_declaration" | "local_function_statement" | "constructor_declaration"
    ) {
        let name = if kind == "constructor_declaration" {
            find_ancestor_name(node, source, "class_declaration")
                .or_else(|| find_ancestor_name(node, source, "struct_declaration"))
                .unwrap_or_default()
        } else {
            node.child_by_field_name("name")
                .and_then(|n| text_of(n, source))
                .unwrap_or_default()
        };
        now_in = name == function_name;
    }
    if now_in && matches!(kind, "local_declaration_statement" | "variable_declaration") {
        // local_declaration_statement wraps variable_declaration
        if kind == "variable_declaration" || kind == "local_declaration_statement" {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "variable_declaration" {
                    collect_csharp_var_decl(child, source, env);
                }
            }
            if kind == "variable_declaration" {
                collect_csharp_var_decl(node, source, env);
            }
        }
    }
    if now_in && kind == "parameter" {
        if let (Some(name), Some(ty)) = (
            node.child_by_field_name("name")
                .and_then(|n| text_of(n, source)),
            node.child_by_field_name("type")
                .and_then(|n| text_of(n, source)),
        ) {
            insert_ty(env, &name, &ty);
        }
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_csharp,
        &[
            "method_declaration",
            "local_function_statement",
            "constructor_declaration",
        ],
    );
}

fn collect_csharp_var_decl(node: Node, source: &[u8], env: &mut HashMap<String, String>) {
    let ty = node
        .child_by_field_name("type")
        .and_then(|n| text_of(n, source));
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            if let (Some(name), Some(t)) = (
                child
                    .child_by_field_name("name")
                    .and_then(|n| text_of(n, source)),
                ty.as_ref(),
            ) {
                insert_ty(env, &name, t);
            }
        }
    }
}

fn visit_go(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if matches!(kind, "function_declaration" | "method_declaration") {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| text_of(n, source))
            .unwrap_or_default();
        now_in = name == function_name;
    }
    if now_in && kind == "parameter_declaration" {
        let ty = node
            .child_by_field_name("type")
            .and_then(|n| text_of(n, source));
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                if let (Ok(name), Some(t)) = (child.utf8_text(source), ty.as_ref()) {
                    insert_ty(env, name, t);
                }
            }
        }
    }
    if now_in && kind == "var_spec" {
        let ty = node
            .child_by_field_name("type")
            .and_then(|n| text_of(n, source));
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "identifier" {
                if let (Ok(name), Some(t)) = (child.utf8_text(source), ty.as_ref()) {
                    insert_ty(env, name, t);
                }
            }
        }
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_go,
        &["function_declaration", "method_declaration"],
    );
}

fn visit_rust(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if kind == "function_item" {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| text_of(n, source))
            .unwrap_or_default();
        now_in = name == function_name;
    }
    if now_in && kind == "parameter" {
        let pat = node.child_by_field_name("pattern");
        let ty = node
            .child_by_field_name("type")
            .and_then(|n| text_of(n, source));
        if let (Some(pat), Some(t)) = (pat, ty) {
            if let Ok(name) = pat.utf8_text(source) {
                // Strip ref patterns like `&mut order`
                let name = name
                    .trim()
                    .trim_start_matches("&mut ")
                    .trim_start_matches('&')
                    .trim();
                insert_ty(env, name, &t);
            }
        }
    }
    if now_in && kind == "let_declaration" {
        let ty = node
            .child_by_field_name("type")
            .and_then(|n| text_of(n, source));
        if let (Some(pat), Some(t)) = (node.child_by_field_name("pattern"), ty) {
            if let Ok(name) = pat.utf8_text(source) {
                insert_ty(env, name.trim(), &t);
            }
        }
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_rust,
        &["function_item"],
    );
}

fn visit_python(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if kind == "function_definition" {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| text_of(n, source))
            .unwrap_or_default();
        now_in = name == function_name;
    }
    if now_in && matches!(kind, "typed_parameter" | "typed_default_parameter") {
        let mut name = None;
        let mut ty = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" if name.is_none() => name = text_of(child, source),
                "type" => ty = text_of(child, source),
                _ => {
                    if child.kind() == "identifier" && name.is_none() {
                        name = text_of(child, source);
                    }
                }
            }
        }
        if let (Some(name), Some(ty)) = (name, ty) {
            insert_ty(env, &name, &ty);
        }
    }
    if now_in && kind == "annotated_assignment" {
        let name = node
            .child_by_field_name("left")
            .and_then(|n| text_of(n, source));
        let ty = node
            .child_by_field_name("type")
            .and_then(|n| text_of(n, source));
        if let (Some(name), Some(ty)) = (name, ty) {
            insert_ty(env, &name, &ty);
        }
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_python,
        &["function_definition"],
    );
}

fn visit_php(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if matches!(kind, "function_definition" | "method_declaration") {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| text_of(n, source))
            .unwrap_or_default();
        now_in = name == function_name;
    }
    if now_in
        && matches!(
            kind,
            "simple_parameter" | "property_promotion_parameter" | "optional_parameter"
        )
    {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| text_of(n, source))
            .map(|s| s.trim_start_matches('$').to_string());
        let ty = node
            .child_by_field_name("type")
            .and_then(|n| text_of(n, source));
        if let (Some(name), Some(ty)) = (name, ty) {
            insert_ty(env, &name, &ty);
        }
    }
    if now_in && kind == "assignment_expression" {
        if let (Some(left), Some(right)) = (
            node.child_by_field_name("left"),
            node.child_by_field_name("right"),
        ) {
            if left.kind() == "variable_name" {
                if let (Some(dst), Some(src)) = (text_of(left, source), text_of(right, source)) {
                    let dst = dst.trim_start_matches('$');
                    if let Some(ty) = env.get(&src.trim_start_matches('$').to_string()).cloned() {
                        insert_ty(env, dst, &ty);
                    }
                }
            }
        }
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_php,
        &["function_definition", "method_declaration"],
    );
}

fn visit_ruby(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if matches!(kind, "method" | "singleton_method") {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| text_of(n, source))
            .unwrap_or_default();
        now_in = name == function_name;
    }
    if now_in && kind == "identifier" {
        if let Some(parent) = node.parent() {
            if parent.kind() == "method_parameters" || parent.kind() == "block_parameters" {
                if let Some(n) = text_of(node, source) {
                    insert_ty(env, &n, "Object");
                }
            }
        }
    }
    if now_in && kind == "self" {
        let mut cur = node.parent();
        while let Some(n) = cur {
            if n.kind() == "class" {
                if let Some(cn) = n
                    .child_by_field_name("name")
                    .and_then(|x| text_of(x, source))
                {
                    insert_ty(env, "self", &cn);
                }
                break;
            }
            cur = n.parent();
        }
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_ruby,
        &["method", "singleton_method"],
    );
}

fn visit_kotlin(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if matches!(
        kind,
        "function_declaration" | "primary_constructor" | "secondary_constructor" | "anonymous_function"
    ) {
        let name = if matches!(kind, "primary_constructor" | "secondary_constructor") {
            find_ancestor_name(node, source, "class_declaration")
                .or_else(|| find_ancestor_name(node, source, "object_declaration"))
                .unwrap_or_default()
        } else {
            node.child_by_field_name("name")
                .and_then(|n| text_of(n, source))
                .or_else(|| {
                    let mut c = node.walk();
                    node.children(&mut c).find_map(|ch| {
                        if matches!(ch.kind(), "identifier" | "simple_identifier") {
                            text_of(ch, source)
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default()
        };
        now_in = name == function_name;
    }
    if now_in && matches!(kind, "parameter" | "class_parameter") {
        collect_kotlin_param(node, source, env);
    }
    if now_in && kind == "property_declaration" {
        collect_kotlin_property_local(node, source, env);
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_kotlin,
        &[
            "function_declaration",
            "primary_constructor",
            "secondary_constructor",
            "anonymous_function",
        ],
    );
}

fn collect_kotlin_param(node: Node, source: &[u8], env: &mut HashMap<String, String>) {
    let mut name = node
        .child_by_field_name("name")
        .and_then(|n| text_of(n, source));
    let mut ty = node
        .child_by_field_name("type")
        .and_then(|n| text_of(n, source));
    if name.is_none() || ty.is_none() {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if name.is_none() && matches!(child.kind(), "identifier" | "simple_identifier") {
                name = text_of(child, source);
            }
            if ty.is_none()
                && matches!(
                    child.kind(),
                    "user_type"
                        | "nullable_type"
                        | "type_identifier"
                        | "function_type"
                        | "parenthesized_type"
                )
            {
                ty = text_of(child, source);
            }
        }
    }
    if let (Some(n), Some(t)) = (name, ty) {
        insert_ty(env, &n, &t);
    }
}

fn collect_kotlin_property_local(node: Node, source: &[u8], env: &mut HashMap<String, String>) {
    let mut name = node
        .child_by_field_name("name")
        .and_then(|n| text_of(n, source));
    let mut ty = node
        .child_by_field_name("type")
        .and_then(|n| text_of(n, source));
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declaration" {
            let mut c2 = child.walk();
            for g in child.children(&mut c2) {
                if name.is_none() && matches!(g.kind(), "identifier" | "simple_identifier") {
                    name = text_of(g, source);
                }
                if ty.is_none()
                    && matches!(
                        g.kind(),
                        "user_type" | "nullable_type" | "type_identifier" | "parenthesized_type"
                    )
                {
                    ty = text_of(g, source);
                }
            }
        }
        if name.is_none() && matches!(child.kind(), "identifier" | "simple_identifier") {
            name = text_of(child, source);
        }
        if ty.is_none()
            && matches!(
                child.kind(),
                "user_type" | "nullable_type" | "type_identifier" | "parenthesized_type"
            )
        {
            ty = text_of(child, source);
        }
    }
    if let (Some(n), Some(t)) = (name, ty) {
        insert_ty(env, &n, &t);
    }
}

/// Groovy: Java-shaped methods / constructors / typed locals.
fn visit_groovy(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if matches!(
        kind,
        "method_declaration"
            | "function_definition"
            | "constructor_declaration"
            | "compact_constructor_declaration"
    ) {
        let name = if matches!(
            kind,
            "constructor_declaration" | "compact_constructor_declaration"
        ) {
            find_ancestor_name(node, source, "class_declaration")
                .or_else(|| find_ancestor_name(node, source, "enum_declaration"))
                .unwrap_or_default()
        } else {
            node.child_by_field_name("name")
                .and_then(|n| text_of(n, source))
                .unwrap_or_default()
        };
        now_in = name == function_name;
    }
    if now_in && kind == "local_variable_declaration" {
        collect_java_style_local(node, source, env);
    }
    if now_in && kind == "formal_parameter" {
        if let (Some(name), Some(ty)) = (
            node.child_by_field_name("name")
                .and_then(|n| text_of(n, source)),
            node.child_by_field_name("type")
                .and_then(|n| text_of(n, source)),
        ) {
            insert_ty(env, &name, &ty);
        }
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_groovy,
        &[
            "method_declaration",
            "function_definition",
            "constructor_declaration",
            "compact_constructor_declaration",
        ],
    );
}

/// Puppet: merge typed parameters from class / define / function hosts into `env`.
fn visit_puppet(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if matches!(
        kind,
        "class_definition" | "defined_resource_type" | "function_declaration" | "node_definition"
    ) {
        let mut name = None;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(
                child.kind(),
                "class_identifier" | "identifier" | "node_name" | "string"
            ) {
                name = text_of(child, source).map(|s| {
                    let t = s.trim_matches('\'').trim_matches('"').to_string();
                    if kind == "node_definition" {
                        format!("node:{t}")
                    } else {
                        t
                    }
                });
                break;
            }
        }
        now_in = name.as_deref() == Some(function_name);
        if now_in {
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.kind() != "parameter_list" {
                    continue;
                }
                let mut pc = child.walk();
                for param in child.children(&mut pc) {
                    if param.kind() != "parameter" {
                        continue;
                    }
                    let mut pname = None;
                    let mut pty = None;
                    let mut pp = param.walk();
                    for part in param.children(&mut pp) {
                        match part.kind() {
                            "variable" => {
                                pname = text_of(part, source)
                                    .map(|s| s.trim_start_matches('$').to_string());
                            }
                            "type"
                            | "builtin_type"
                            | "array_type"
                            | "composite_type"
                            | "attribute_type" => {
                                if pty.is_none() {
                                    pty = text_of(part, source);
                                }
                            }
                            _ => {}
                        }
                    }
                    if let Some(n) = pname {
                        insert_ty(env, &n, pty.as_deref().unwrap_or("Any"));
                    }
                }
            }
        }
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_puppet,
        &[
            "class_definition",
            "defined_resource_type",
            "function_declaration",
            "node_definition",
        ],
    );
}

fn visit_javascript(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    // JS has no static types — copy-assign inference only: `const other = order` when order already typed.
    visit_js_ts_like(node, source, function_name, env, in_target, false);
}

fn visit_typescript(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    visit_js_ts_like(node, source, function_name, env, in_target, true);
}

fn visit_js_ts_like(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
    with_types: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if matches!(
        kind,
        "function_declaration" | "method_definition" | "function_expression"
    ) {
        let name = if kind == "method_definition" {
            let raw = node
                .child_by_field_name("name")
                .and_then(|n| text_of(n, source))
                .unwrap_or_default();
            if raw == "constructor" {
                find_ancestor_name(node, source, "class_declaration").unwrap_or(raw)
            } else {
                raw
            }
        } else {
            node.child_by_field_name("name")
                .and_then(|n| text_of(n, source))
                .unwrap_or_default()
        };
        now_in = name == function_name;
    }
    if now_in
        && with_types
        && matches!(
            kind,
            "required_parameter" | "optional_parameter" | "formal_parameter"
        )
    {
        let name = node
            .child_by_field_name("pattern")
            .or_else(|| node.child_by_field_name("name"))
            .and_then(|n| text_of(n, source));
        let ty = node
            .child_by_field_name("type")
            .and_then(|n| text_of(n, source))
            .or_else(|| find_child_kind_text(node, source, "type_annotation"));
        if let (Some(name), Some(ty)) = (name, ty) {
            insert_ty(env, &name, &ty);
        }
    }
    if now_in && kind == "lexical_declaration" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() != "variable_declarator" {
                continue;
            }
            let name = child
                .child_by_field_name("name")
                .and_then(|n| text_of(n, source));
            let ty = if with_types {
                child
                    .child_by_field_name("type")
                    .and_then(|n| text_of(n, source))
                    .or_else(|| find_child_kind_text(child, source, "type_annotation"))
            } else {
                None
            };
            // Copy inference: `const other = order` when order known
            if ty.is_none() {
                if let Some(value) = child.child_by_field_name("value") {
                    if value.kind() == "identifier" {
                        if let (Some(name), Ok(rhs)) = (name.as_ref(), value.utf8_text(source)) {
                            if let Some(t) = env.get(rhs).cloned() {
                                insert_ty(env, name, &t);
                            }
                        }
                    }
                }
            } else if let (Some(name), Some(t)) = (name, ty) {
                insert_ty(env, &name, &t);
            }
        }
    }
    let visit = if with_types {
        visit_typescript
    } else {
        visit_javascript
    };
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit,
        &[
            "function_declaration",
            "method_definition",
            "function_expression",
        ],
    );
}

fn visit_c_family(
    node: Node,
    source: &[u8],
    function_name: &str,
    env: &mut HashMap<String, String>,
    in_target: bool,
) {
    let kind = node.kind();
    let mut now_in = in_target;
    if kind == "function_definition" {
        let name = function_declarator_name(node, source).unwrap_or_default();
        now_in = name == function_name;
    }
    if now_in && kind == "parameter_declaration" {
        let ty = node
            .child_by_field_name("type")
            .and_then(|n| text_of(n, source));
        let name = node
            .child_by_field_name("declarator")
            .and_then(|d| declarator_ident(d, source));
        if let (Some(name), Some(ty)) = (name, ty) {
            insert_ty(env, &name, &ty);
        }
    }
    if now_in && kind == "declaration" {
        let ty = node
            .child_by_field_name("type")
            .and_then(|n| text_of(n, source));
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "init_declarator" || child.kind().contains("declarator") {
                if let (Some(name), Some(t)) = (declarator_ident(child, source), ty.as_ref()) {
                    insert_ty(env, &name, t);
                }
            }
        }
    }
    walk_children(
        node,
        source,
        function_name,
        env,
        now_in,
        visit_c_family,
        &["function_definition"],
    );
}

fn function_declarator_name(node: Node, source: &[u8]) -> Option<String> {
    let decl = node.child_by_field_name("declarator")?;
    declarator_ident(decl, source)
}

fn declarator_ident(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" => text_of(node, source),
        "pointer_declarator"
        | "function_declarator"
        | "array_declarator"
        | "parenthesized_declarator"
        | "reference_declarator" => node
            .child_by_field_name("declarator")
            .and_then(|d| declarator_ident(d, source)),
        "init_declarator" => node
            .child_by_field_name("declarator")
            .and_then(|d| declarator_ident(d, source)),
        _ => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(n) = declarator_ident(child, source) {
                    return Some(n);
                }
            }
            None
        }
    }
}

fn first_named_child_text(node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|ch| ch.is_named())
        .and_then(|ch| text_of(ch, source))
}

fn find_child_kind_text(node: Node, source: &[u8], kind: &str) -> Option<String> {
    let mut cursor = node.walk();
    let children: Vec<_> = node.children(&mut cursor).collect();
    children
        .into_iter()
        .find(|ch| ch.kind() == kind)
        .and_then(|ch| {
            if kind == "type_annotation" {
                first_named_child_text(ch, source)
            } else {
                text_of(ch, source)
            }
        })
}

fn find_ancestor_name(node: Node, source: &[u8], kind: &str) -> Option<String> {
    let mut cur = node;
    while let Some(parent) = cur.parent() {
        if parent.kind() == kind {
            return parent
                .child_by_field_name("name")
                .and_then(|n| text_of(n, source));
        }
        cur = parent;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_locals_merge() {
        let source = r#"
public class OrderProcessor {
    public void process(OrderDTO order) {
        OrderDTO other = order;
        other.status = "X";
    }
}
"#;
        let mut env = HashMap::new();
        merge_local_types("java", source, "process", &mut env);
        assert_eq!(env.get("order").map(String::as_str), Some("OrderDTO"));
        assert_eq!(env.get("other").map(String::as_str), Some("OrderDTO"));
    }

    #[test]
    fn kotlin_locals_merge() {
        let source = r#"
class OrderProcessor {
    fun process(order: OrderDTO): OrderDTO {
        val other: OrderDTO = order
        other.status = "X"
        return other
    }
}
"#;
        let mut env = HashMap::new();
        merge_local_types("kotlin", source, "process", &mut env);
        assert_eq!(env.get("order").map(String::as_str), Some("OrderDTO"));
        assert_eq!(env.get("other").map(String::as_str), Some("OrderDTO"));
    }

    #[test]
    fn groovy_locals_merge() {
        let source = r#"
class OrderProcessor {
  OrderDTO process(OrderDTO order) {
    OrderDTO other = order
    other.status = "X"
    return other
  }
}
"#;
        let mut env = HashMap::new();
        merge_local_types("groovy", source, "process", &mut env);
        assert_eq!(env.get("order").map(String::as_str), Some("OrderDTO"));
        assert_eq!(env.get("other").map(String::as_str), Some("OrderDTO"));
    }

    #[test]
    fn csharp_locals_merge() {
        let source = r#"
class OrderProcessor {
  public void Process(OrderDTO order) {
    OrderDTO other = order;
    other.status = "X";
  }
}
"#;
        let mut env = HashMap::new();
        merge_local_types("csharp", source, "Process", &mut env);
        assert_eq!(env.get("order").map(String::as_str), Some("OrderDTO"));
        assert_eq!(env.get("other").map(String::as_str), Some("OrderDTO"));
    }

    #[test]
    fn go_params_merge() {
        let source = r#"
package demo
func Process(order *OrderDTO) {
  order.Status = "X"
}
"#;
        let mut env = HashMap::new();
        merge_local_types("go", source, "Process", &mut env);
        assert!(
            env.get("order")
                .map(|s| s.contains("OrderDTO"))
                .unwrap_or(false),
            "env={env:?}"
        );
    }

    #[test]
    fn parse_once_resolves_two_functions_in_same_file() {
        let source = r#"
public class Dual {
    public void first(OrderDTO a) {
        OrderDTO x = a;
        x.status = "A";
    }
    public void second(OrderDTO b) {
        OrderDTO y = b;
        y.status = "B";
    }
}
"#;
        let ctx = LocalsParseContext::try_new("java", source).expect("parse");
        let mut env_first = HashMap::new();
        ctx.merge_into("first", &mut env_first);
        assert_eq!(env_first.get("a").map(String::as_str), Some("OrderDTO"));
        assert_eq!(env_first.get("x").map(String::as_str), Some("OrderDTO"));
        assert!(
            env_first.get("y").is_none(),
            "first must not see second locals"
        );

        let mut env_second = HashMap::new();
        ctx.merge_into("second", &mut env_second);
        assert_eq!(env_second.get("b").map(String::as_str), Some("OrderDTO"));
        assert_eq!(env_second.get("y").map(String::as_str), Some("OrderDTO"));
        assert!(
            env_second.get("x").is_none(),
            "second must not see first locals"
        );
    }
}
