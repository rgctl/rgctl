//! Kotlin language plugin — symbols, calls, inheritance, complexity.
//!
//! Uses a single tree-sitter parse per `extract_all` (AGENTS.md: reuse Tree per file).

use crate::single_pass::{extract_all_single_pass, kotlin_ast_handlers};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use std::path::Path;
use tree_sitter::Node;

const TYPE_KINDS: &[&str] = &[
    "class_declaration",
    "object_declaration",
    "companion_object",
];

const BRANCH_KINDS: &[&str] = &[
    "if_expression",
    "when_expression",
    "when_entry",
    "while_statement",
    "for_statement",
    "do_while_statement",
    "catch_block",
];

pub(crate) struct CtorEmitCtx<'a> {
    file_path: &'a str,
    package: Option<&'a str>,
    type_path: &'a [String],
}

/// Kotlin Tier 1 plugin.
pub struct KotlinPlugin;

impl KotlinPlugin {
    /// Create a new Kotlin plugin.
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        parse_with_pool("kotlin", tree_sitter_kotlin_ng::LANGUAGE.into(), source).map_err(|e| {
            Error::ParseError {
                file: file_path.to_path_buf(),
                line: 1,
                message: e,
            }
        })
    }

    pub(crate) fn package_name(root: Node, source: &[u8]) -> Option<String> {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "package_header" {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "qualified_identifier" {
                        return Self::qualified_identifier_text(child, source);
                    }
                    if matches!(child.kind(), "identifier" | "simple_identifier")
                        && let Ok(t) = child.utf8_text(source)
                    {
                        return Some(t.trim().to_string());
                    }
                }
                if let Some(name) = node.child_by_field_name("identifier")
                    && let Ok(t) = name.utf8_text(source)
                {
                    return Some(t.trim().to_string());
                }
                // Fallback: strip `package ` prefix from full text
                if let Ok(full) = node.utf8_text(source) {
                    let t = full.trim().trim_start_matches("package").trim();
                    if !t.is_empty() {
                        return Some(t.to_string());
                    }
                }
            }
            let mut cursor = node.walk();
            for child in node
                .children(&mut cursor)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                stack.push(child);
            }
        }
        None
    }

    fn qualified_identifier_text(node: Node, source: &[u8]) -> Option<String> {
        if node.kind() != "qualified_identifier" {
            return node.utf8_text(source).ok().map(|s| s.trim().to_string());
        }
        let mut parts = Vec::new();
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if child.kind() == "identifier"
                && let Ok(t) = child.utf8_text(source)
            {
                parts.push(t.to_string());
            }
        }
        if parts.is_empty() {
            node.utf8_text(source).ok().map(|s| s.trim().to_string())
        } else {
            Some(parts.join("."))
        }
    }

    fn qualify(package: Option<&str>, path: &str) -> String {
        match package {
            Some(pkg) if !pkg.is_empty() => format!("{pkg}.{path}"),
            _ => path.to_string(),
        }
    }

    fn type_name(node: Node, source: &[u8]) -> Option<String> {
        if let Some(name) = node.child_by_field_name("name") {
            return name.utf8_text(source).ok().map(|s| s.trim().to_string());
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if matches!(
                child.kind(),
                "type_identifier" | "simple_identifier" | "identifier"
            ) && let Ok(t) = child.utf8_text(source)
            {
                let t = t.trim();
                if !t.is_empty() && t != "class" && t != "object" && t != "interface" && t != "enum"
                {
                    return Some(t.to_string());
                }
            }
        }
        None
    }

    fn enclosing_type_path(node: Node, source: &[u8]) -> Vec<String> {
        let mut path = Vec::new();
        let mut current = node.parent();
        while let Some(n) = current {
            if TYPE_KINDS.contains(&n.kind()) || n.kind() == "class_declaration" {
                // class_declaration also covers interface/enum in kotlin-ng via modifiers
                if let Some(name) = Self::type_name(n, source) {
                    path.push(name);
                }
            }
            current = n.parent();
        }
        path.reverse();
        path
    }

    fn is_interface_or_enum(node: Node, source: &[u8]) -> (&'static str, SymbolType) {
        if let Ok(text) = node.utf8_text(source) {
            let head = text.lines().next().unwrap_or("").trim_start();
            if head.starts_with("interface") || head.contains(" interface ") {
                return ("interface", SymbolType::Interface);
            }
            if head.starts_with("enum") || head.contains(" enum ") {
                return ("enum", SymbolType::Enum);
            }
            if head.starts_with("object") || node.kind() == "object_declaration" {
                return ("object", SymbolType::Class);
            }
            if node.kind() == "companion_object" {
                return ("companion_object", SymbolType::Class);
            }
        }
        ("class", SymbolType::Class)
    }

    fn loc(file_path: &str, node: Node) -> SourceLocation {
        SourceLocation {
            file: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_column: node.start_position().column,
            end_column: node.end_position().column,
        }
    }

    fn extract_parameters(&self, node: Node, source: &[u8]) -> Vec<Parameter> {
        let Some(params) = node.child_by_field_name("parameters").or_else(|| {
            let mut cursor = node.walk();
            node.children(&mut cursor).find(|c| {
                matches!(
                    c.kind(),
                    "function_value_parameters" | "class_parameters" | "lambda_parameters"
                )
            })
        }) else {
            return Vec::new();
        };

        let mut out = Vec::new();
        let mut cursor = params.walk();
        for child in params.children(&mut cursor) {
            if !matches!(child.kind(), "parameter" | "class_parameter") {
                continue;
            }
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.trim().to_string())
                .or_else(|| {
                    let mut c = child.walk();
                    child.children(&mut c).find_map(|n| {
                        if matches!(n.kind(), "simple_identifier" | "identifier") {
                            n.utf8_text(source).ok().map(|s| s.trim().to_string())
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_else(|| "_".to_string());
            let param_type = child
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.trim().to_string());
            out.push(Parameter {
                name,
                param_type,
                default_value: None,
            });
        }
        out
    }

    fn property_fields(&self, body: Node, source: &[u8]) -> Vec<Field> {
        let mut fields = Vec::new();
        let mut stack = vec![body];
        while let Some(node) = stack.pop() {
            if node.kind() == "enum_entry" {
                let name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.trim().to_string())
                    .or_else(|| {
                        let mut c = node.walk();
                        node.children(&mut c).find_map(|n| {
                            if matches!(n.kind(), "simple_identifier" | "identifier") {
                                n.utf8_text(source).ok().map(|s| s.trim().to_string())
                            } else {
                                None
                            }
                        })
                    });
                if let Some(name) = name {
                    fields.push(Field {
                        name,
                        field_type: None,
                        visibility: None,
                    });
                }
                continue;
            }
            if node.kind() == "property_declaration" {
                let name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.trim().to_string())
                    .or_else(|| {
                        let mut c = node.walk();
                        node.children(&mut c).find_map(|n| {
                            if matches!(
                                n.kind(),
                                "simple_identifier" | "identifier" | "variable_declaration"
                            ) {
                                if n.kind() == "variable_declaration" {
                                    let mut c2 = n.walk();
                                    return n.children(&mut c2).find_map(|x| {
                                        if matches!(x.kind(), "simple_identifier" | "identifier") {
                                            x.utf8_text(source).ok().map(|s| s.trim().to_string())
                                        } else {
                                            None
                                        }
                                    });
                                }
                                n.utf8_text(source).ok().map(|s| s.trim().to_string())
                            } else {
                                None
                            }
                        })
                    });
                if let Some(name) = name {
                    let field_type = node
                        .child_by_field_name("type")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.trim().to_string());
                    fields.push(Field {
                        name,
                        field_type,
                        visibility: None,
                    });
                }
                continue; // don't walk into property children as nested types for fields
            }
            // Don't descend into nested type bodies for field collection of outer
            if TYPE_KINDS.contains(&node.kind()) && node.id() != body.id() {
                continue;
            }
            let mut cursor = node.walk();
            for child in node
                .children(&mut cursor)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                stack.push(child);
            }
        }
        fields
    }

    pub(crate) fn push_ctor(
        &self,
        symbols: &mut Vec<Symbol>,
        ctx: &CtorEmitCtx<'_>,
        ctor_node: Node,
        source: &[u8],
        is_primary: bool,
    ) {
        let type_simple = ctx
            .type_path
            .last()
            .cloned()
            .unwrap_or_else(|| "Unknown".into());
        let type_qn = Self::qualify(ctx.package, &ctx.type_path.join("."));
        let parameters = self.extract_parameters(ctor_node, source);
        // Primary ctor params also become fields when class_parameter
        let mut fields = Vec::new();
        if is_primary {
            for p in &parameters {
                fields.push(Field {
                    name: p.name.clone(),
                    field_type: p.param_type.clone(),
                    visibility: None,
                });
            }
        }
        symbols.push(Symbol {
            name: type_simple.clone(),
            symbol_type: SymbolType::Function,
            qualified_name: Some(format!("{type_qn}.<init>")),
            location: Self::loc(ctx.file_path, ctor_node),
            signature: ctor_node
                .utf8_text(source)
                .ok()
                .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
            return_type: None,
            parameters,
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "kotlin",
                "is_constructor": true,
                "primary": is_primary,
            }),
        });
        // Attach primary-ctor fields onto the class symbol if we already emitted it —
        // handled when emitting the class by merging class_parameters.
        let _ = fields;
    }

    /// Emit class/object/companion symbol plus primary/secondary constructors.
    pub(crate) fn emit_type_and_ctors(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
        symbols: &mut Vec<Symbol>,
    ) {
        let Some(simple) = Self::type_name(node, source).or_else(|| {
            if node.kind() == "companion_object" {
                Some("Companion".to_string())
            } else {
                None
            }
        }) else {
            return;
        };
        let mut type_path = Self::enclosing_type_path(node, source);
        if type_path.last() != Some(&simple) {
            type_path.push(simple.clone());
        }
        let (kind_meta, symbol_type) = Self::is_interface_or_enum(node, source);
        let qn = Self::qualify(package, &type_path.join("."));

        let mut fields = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "primary_constructor" || child.kind() == "class_parameters" {
                for p in self.extract_parameters(
                    if child.kind() == "class_parameters" {
                        node
                    } else {
                        child
                    },
                    source,
                ) {
                    fields.push(Field {
                        name: p.name,
                        field_type: p.param_type,
                        visibility: None,
                    });
                }
            }
            if child.kind() == "class_body" || child.kind() == "enum_class_body" {
                fields.extend(self.property_fields(child, source));
            }
        }

        symbols.push(Symbol {
            name: simple.clone(),
            symbol_type,
            qualified_name: Some(qn.clone()),
            location: Self::loc(file, node),
            signature: node
                .utf8_text(source)
                .ok()
                .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
            return_type: None,
            parameters: vec![],
            fields,
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "kotlin",
                "kind": kind_meta,
            }),
        });

        let mut c2 = node.walk();
        for child in node.children(&mut c2) {
            let ctor_ctx = CtorEmitCtx {
                file_path: file,
                package,
                type_path: &type_path,
            };
            if child.kind() == "primary_constructor" {
                self.push_ctor(symbols, &ctor_ctx, child, source, true);
            }
            if child.kind() == "secondary_constructor" {
                self.push_ctor(symbols, &ctor_ctx, child, source, false);
            }
        }
    }

    pub(crate) fn emit_function(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
    ) -> Option<Symbol> {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string())
            .or_else(|| {
                let mut c = node.walk();
                node.children(&mut c).find_map(|n| {
                    if matches!(n.kind(), "simple_identifier" | "identifier") {
                        n.utf8_text(source).ok().map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
            })?;
        let type_path = Self::enclosing_type_path(node, source);
        let qn = if type_path.is_empty() {
            Self::qualify(package, &name)
        } else {
            Self::qualify(package, &format!("{}.{}", type_path.join("."), name))
        };
        let return_type = node
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string());
        let parameters = self.extract_parameters(node, source);
        Some(Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name: Some(qn),
            location: Self::loc(file, node),
            signature: node
                .utf8_text(source)
                .ok()
                .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
            return_type,
            parameters,
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "kotlin" }),
        })
    }

    /// Synthetic function symbol for `lambda_literal` / `anonymous_function`.
    pub(crate) fn emit_lambda_like(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
        idx: u32,
        kind_tag: &str,
    ) -> Option<Symbol> {
        let name = format!("${kind_tag}${idx}");
        let type_path = Self::enclosing_type_path(node, source);
        let qn = if type_path.is_empty() {
            Self::qualify(package, &name)
        } else {
            Self::qualify(package, &format!("{}.{}", type_path.join("."), name))
        };
        let parameters = self.extract_parameters(node, source);
        Some(Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name: Some(qn),
            location: Self::loc(file, node),
            signature: node
                .utf8_text(source)
                .ok()
                .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
            return_type: None,
            parameters,
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "kotlin",
                "is_lambda": kind_tag == "lambda",
                "is_anonymous_function": kind_tag == "anon_fn",
            }),
        })
    }

    /// Anonymous `object : Type { ... }` literal.
    pub(crate) fn emit_object_literal(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
        idx: u32,
    ) -> Option<Symbol> {
        let name = format!("$object${idx}");
        let type_path = Self::enclosing_type_path(node, source);
        let qn = if type_path.is_empty() {
            Self::qualify(package, &name)
        } else {
            Self::qualify(package, &format!("{}.{}", type_path.join("."), name))
        };
        Some(Symbol {
            name,
            symbol_type: SymbolType::Class,
            qualified_name: Some(qn),
            location: Self::loc(file, node),
            signature: node
                .utf8_text(source)
                .ok()
                .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "kotlin",
                "is_object_literal": true,
            }),
        })
    }

    /// Class `init { ... }` block.
    pub(crate) fn emit_anonymous_initializer(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
        idx: u32,
    ) -> Option<Symbol> {
        let name = format!("$init${idx}");
        let type_path = Self::enclosing_type_path(node, source);
        let qn = if type_path.is_empty() {
            Self::qualify(package, &name)
        } else {
            Self::qualify(package, &format!("{}.{}", type_path.join("."), name))
        };
        Some(Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name: Some(qn),
            location: Self::loc(file, node),
            signature: Some("init".into()),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "kotlin",
                "is_initializer": true,
            }),
        })
    }

    /// `@Ann` / `@file:Ann` → `AnnotatedWith` from nearest enclosing symbol.
    pub(crate) fn emit_annotation_relation(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) {
        let text = node.utf8_text(source).unwrap_or("").trim();
        let ann = text
            .trim_start_matches("@file:")
            .trim_start_matches('@')
            .split(|c: char| c == '(' || c == '<' || c.is_whitespace())
            .next()
            .unwrap_or("")
            .trim();
        if ann.is_empty() {
            return;
        }
        let from = symbols
            .last()
            .and_then(|s| s.qualified_name.clone().or_else(|| Some(s.name.clone())))
            .unwrap_or_else(|| file_path.to_string_lossy().into_owned());
        relations.push(Relation {
            from,
            to: ann.to_string(),
            relation_type: RelationType::AnnotatedWith,
            location: Self::loc(&file_path.to_string_lossy(), node),
            metadata: serde_json::json!({ "language": "kotlin" }),
            to_qualified_hint: Some(ann.to_string()),
            to_type_hint: None,
        });
    }

    /// Best-effort `References` for callable refs / navigation / infix / index / ctor delegation.
    pub(crate) fn emit_reference_relation(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        from_fn: &Symbol,
        relations: &mut Vec<Relation>,
    ) {
        let text = node.utf8_text(source).unwrap_or("").trim();
        if text.is_empty() {
            return;
        }
        let to = text
            .rsplit(|c: char| c == '.' || c == ':' || c == '(' || c == '[' || c.is_whitespace())
            .next()
            .unwrap_or(text)
            .trim()
            .trim_start_matches("::")
            .trim_end_matches(')')
            .trim_end_matches(']');
        if to.is_empty() || to == "this" || to == "super" {
            return;
        }
        let from = from_fn
            .qualified_name
            .clone()
            .unwrap_or_else(|| from_fn.name.clone());
        relations.push(Relation {
            from,
            to: to.to_string(),
            relation_type: RelationType::References,
            location: Self::loc(&file_path.to_string_lossy(), node),
            metadata: serde_json::json!({
                "language": "kotlin",
                "kind": node.kind(),
            }),
            to_qualified_hint: Some(to.to_string()),
            to_type_hint: None,
        });
    }

    pub(crate) fn emit_import(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        symbols: &mut Vec<Symbol>,
    ) {
        let text = node.utf8_text(source).unwrap_or("").trim();
        let imported = text
            .trim_start_matches("import")
            .trim()
            .trim_end_matches(".*")
            .trim();
        if imported.is_empty() {
            return;
        }
        let simple = imported.rsplit('.').next().unwrap_or(imported).to_string();
        symbols.push(Symbol {
            name: simple,
            symbol_type: SymbolType::Import,
            qualified_name: Some(imported.to_string()),
            location: Self::loc(file, node),
            signature: Some(text.to_string()),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "kotlin" }),
        });
    }

    fn symbols_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
    ) -> Result<Vec<Symbol>> {
        let file = file_path.to_string_lossy();
        let package = Self::package_name(root, source);
        let mut symbols = Vec::with_capacity(64);
        let mut stack = vec![root];

        while let Some(node) = stack.pop() {
            match node.kind() {
                "class_declaration" | "object_declaration" | "companion_object" => {
                    self.emit_type_and_ctors(node, source, &file, package.as_deref(), &mut symbols);
                }
                "function_declaration" => {
                    if let Some(sym) = self.emit_function(node, source, &file, package.as_deref()) {
                        symbols.push(sym);
                    }
                }
                "import" => {
                    self.emit_import(node, source, &file, &mut symbols);
                }
                _ => {}
            }

            let mut cursor = node.walk();
            for child in node
                .children(&mut cursor)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                stack.push(child);
            }
        }

        Ok(symbols)
    }

    pub(crate) fn collect_inheritance_relations(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if (node.kind() == "class_declaration" || node.kind() == "object_declaration")
                && let Some(from_name) = Self::type_name(node, source)
            {
                let package = Self::package_name(root, source);
                let mut type_path = Self::enclosing_type_path(node, source);
                if type_path.last() != Some(&from_name) {
                    type_path.push(from_name.clone());
                }
                let from_qn = Self::qualify(package.as_deref(), &type_path.join("."));
                let mut inner = vec![node];
                while let Some(n) = inner.pop() {
                    if n.kind() == "delegation_specifier" {
                        self.emit_delegation_edges(n, source, file_path, &from_qn, relations);
                    }
                    if n.id() != node.id()
                        && (TYPE_KINDS.contains(&n.kind()) || n.kind() == "function_declaration")
                    {
                        continue;
                    }
                    let mut c = n.walk();
                    for ch in n.children(&mut c).collect::<Vec<_>>().into_iter().rev() {
                        inner.push(ch);
                    }
                }
            }
            let mut cursor = node.walk();
            for child in node
                .children(&mut cursor)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                stack.push(child);
            }
        }
    }

    fn relations_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let mut relations = Vec::with_capacity(symbols.len().saturating_mul(2));
        walk_calls(
            root,
            source,
            file_path,
            symbols,
            KOTLIN_CALL_KINDS,
            "kotlin",
            &mut relations,
        );
        self.collect_inheritance_relations(root, source, file_path, &mut relations);
        Ok(relations)
    }

    fn emit_delegation_edges(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        from_qn: &str,
        relations: &mut Vec<Relation>,
    ) {
        let text = node.utf8_text(source).unwrap_or("").trim();
        if text.is_empty() {
            return;
        }
        // Take first type identifier-ish token
        let to_name = text
            .split(|c: char| c == '(' || c == '<' || c == ',' || c.is_whitespace())
            .next()
            .unwrap_or(text)
            .trim();
        if to_name.is_empty() {
            return;
        }
        let rel_type = if text.contains("()") || text.contains("(") {
            // constructor invocation — treat as Extends for class
            RelationType::Extends
        } else {
            RelationType::Implements
        };
        // Prefer Extends for class names without obvious interface — honesty: use Extends
        // when constructor_invocation present, else Implements for bare types.
        let mut cursor = node.walk();
        let has_ctor = node
            .children(&mut cursor)
            .any(|c| c.kind() == "constructor_invocation");
        let relation_type = if has_ctor {
            RelationType::Extends
        } else {
            rel_type
        };
        relations.push(Relation {
            from: from_qn.to_string(),
            to: to_name.to_string(),
            relation_type,
            location: Self::loc(&file_path.to_string_lossy(), node),
            metadata: serde_json::json!({ "language": "kotlin" }),
            to_qualified_hint: Some(to_name.to_string()),
            to_type_hint: None,
        });
    }

    fn calculate_cyclomatic(&self, node: Node) -> usize {
        let mut complexity = 1usize;
        let mut stack = vec![node];
        while let Some(n) = stack.pop() {
            if BRANCH_KINDS.contains(&n.kind()) {
                complexity += 1;
            }
            let mut cursor = n.walk();
            for child in n
                .children(&mut cursor)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                stack.push(child);
            }
        }
        complexity
    }
}

impl Default for KotlinPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create KotlinPlugin")
    }
}

impl LanguagePlugin for KotlinPlugin {
    fn language_id(&self) -> &str {
        "kotlin"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["kt", "kts"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_kotlin_ng::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let tree = self.parse(file_path, source)?;
        self.symbols_from_tree(tree.root_node(), source, file_path)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let tree = self.parse(file_path, source)?;
        self.relations_from_tree(tree.root_node(), source, file_path, symbols)
    }

    fn extract_all(&self, file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
        extract_all_single_pass(self, file_path, source)
    }

    fn declared_ast_handlers(&self) -> Option<&AstHandlerTable> {
        Some(kotlin_ast_handlers())
    }

    fn calculate_complexity(
        &self,
        symbol: &Symbol,
        source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        if symbol.symbol_type != SymbolType::Function {
            return Ok(None);
        }

        if let Some(metrics) = complexity_from_metadata(symbol) {
            return Ok(Some(metrics));
        }

        let tree = parse_with_pool("kotlin", tree_sitter_kotlin_ng::LANGUAGE.into(), source)
            .map_err(|e| Error::ParseError {
                file: symbol.location.file.clone().into(),
                line: symbol.location.start_line,
                message: e,
            })?;
        let target_line = symbol.location.start_line.saturating_sub(1);
        let mut found = None;
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if matches!(
                node.kind(),
                "function_declaration" | "primary_constructor" | "secondary_constructor"
            ) && node.start_position().row == target_line
            {
                found = Some(node);
                break;
            }
            let mut cursor = node.walk();
            for child in node
                .children(&mut cursor)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
            {
                stack.push(child);
            }
        }
        let Some(node) = found else {
            return Ok(None);
        };
        let cyclomatic = self.calculate_cyclomatic(node);
        Ok(Some(ComplexityMetrics {
            cyclomatic,
            cognitive: cyclomatic.saturating_sub(1),
            nesting_depth: 0,
            loc: symbol
                .location
                .end_line
                .saturating_sub(symbol.location.start_line)
                + 1,
            parameters: symbol.parameters.len(),
            returns: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_class_and_function() {
        let src = br#"
package com.example
class OrderService {
  fun findById(id: Long): String {
    return id.toString()
  }
}
"#;
        let plugin = KotlinPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("OrderService.kt"), src)
            .unwrap();
        assert!(
            symbols.iter().any(|s| {
                s.symbol_type == SymbolType::Class
                    && s.qualified_name.as_deref() == Some("com.example.OrderService")
            }),
            "missing class: {:?}",
            symbols
        );
        assert!(
            symbols.iter().any(|s| {
                s.symbol_type == SymbolType::Function
                    && s.name == "findById"
                    && s.qualified_name.as_deref() == Some("com.example.OrderService.findById")
            }),
            "missing method: {:?}",
            symbols
        );
    }

    #[test]
    fn extracts_calls_between_methods() {
        let src = br#"
package com.example
class OrderService {
  fun validate() {}
  fun findAll() { validate() }
}
"#;
        let plugin = KotlinPlugin::new().unwrap();
        let all = plugin
            .extract_all(Path::new("OrderService.kt"), src)
            .unwrap();
        assert!(
            all.relations
                .iter()
                .any(|r| r.relation_type == RelationType::Calls),
            "expected Calls: {:?}",
            all.relations
        );
    }

    #[test]
    fn primary_constructor_is_init() {
        let src = br#"
package com.example
data class User(val email: String)
"#;
        let plugin = KotlinPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("User.kt"), src).unwrap();
        let ctor = symbols
            .iter()
            .find(|s| s.qualified_name.as_deref() == Some("com.example.User.<init>"));
        assert!(ctor.is_some(), "missing ctor: {:?}", symbols);
        assert_eq!(
            ctor.unwrap().metadata.get("is_constructor"),
            Some(&serde_json::json!(true))
        );
        let class = symbols
            .iter()
            .find(|s| s.qualified_name.as_deref() == Some("com.example.User"))
            .expect("class");
        assert!(
            class.fields.iter().any(|f| f.name == "email"),
            "expected email field: {:?}",
            class.fields
        );
    }
}
