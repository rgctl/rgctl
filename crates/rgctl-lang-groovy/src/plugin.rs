//! Groovy language plugin — best-effort symbols/calls with dynamic-call honesty.

use crate::single_pass::{extract_all_single_pass, groovy_ast_handlers};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result, complexity_from_metadata, parse_with_pool};
use rgctl_plugin_helpers::record_symbol_spans;
use std::path::Path;
use tree_sitter::Node;

pub(crate) const GROOVY_BRANCH_KINDS: &[&str] = &[
    "if_statement",
    "while_statement",
    "for_statement",
    "enhanced_for_statement",
    "do_statement",
    "switch_expression",
    "catch_clause",
];

pub(crate) const GROOVY_CALL_KINDS: &[&str] = &[
    "method_invocation",
    "juxt_function_call",
    "object_creation_expression",
];

const BRANCH_KINDS: &[&str] = GROOVY_BRANCH_KINDS;

/// Groovy Tier 1 plugin.
pub struct GroovyPlugin;

impl GroovyPlugin {
    /// Create a new Groovy plugin.
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub(crate) fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        parse_with_pool("groovy", tree_sitter_groovy::LANGUAGE.into(), source).map_err(|e| {
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
            if node.kind() == "package_declaration" {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if matches!(
                        child.kind(),
                        "scoped_identifier" | "identifier" | "type_identifier"
                    ) && let Ok(t) = child.utf8_text(source)
                    {
                        return Some(t.trim().to_string());
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

    pub(crate) fn qualify(package: Option<&str>, path: &str) -> String {
        match package {
            Some(pkg) if !pkg.is_empty() => format!("{pkg}.{path}"),
            _ => path.to_string(),
        }
    }

    pub(crate) fn loc(file_path: &str, node: Node) -> SourceLocation {
        SourceLocation {
            file: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_column: node.start_position().column,
            end_column: node.end_position().column,
        }
    }

    pub(crate) fn type_name(node: Node, source: &[u8]) -> Option<String> {
        node.child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string())
            .or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor).find_map(|c| {
                    if matches!(c.kind(), "identifier" | "type_identifier") {
                        c.utf8_text(source).ok().map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
            })
    }

    pub(crate) fn enclosing_class(node: Node, source: &[u8]) -> Option<String> {
        let mut current = node.parent();
        while let Some(n) = current {
            if matches!(
                n.kind(),
                "class_declaration"
                    | "interface_declaration"
                    | "enum_declaration"
                    | "record_declaration"
                    | "annotation_type_declaration"
            ) {
                return Self::type_name(n, source);
            }
            current = n.parent();
        }
        None
    }

    fn enclosing_callable_qn(node: Node, source: &[u8], package: Option<&str>) -> Option<String> {
        let mut current = node.parent();
        while let Some(n) = current {
            match n.kind() {
                "method_declaration" | "function_definition" => {
                    let name = n
                        .child_by_field_name("name")
                        .and_then(|c| c.utf8_text(source).ok())
                        .map(|s| s.trim().to_string())?;
                    return Some(if let Some(cls) = Self::enclosing_class(n, source) {
                        Self::qualify(package, &format!("{cls}.{name}"))
                    } else {
                        Self::qualify(package, &name)
                    });
                }
                "constructor_declaration" | "compact_constructor_declaration" => {
                    let cls = Self::enclosing_class(n, source).unwrap_or_else(|| "Unknown".into());
                    return Some(Self::qualify(package, &format!("{cls}.<init>")));
                }
                "static_initializer" => {
                    let cls = Self::enclosing_class(n, source).unwrap_or_else(|| "Unknown".into());
                    return Some(Self::qualify(package, &format!("{cls}.<clinit>")));
                }
                "lambda_expression" | "closure" => {
                    // Prefer the outer named callable as the relation source.
                }
                _ => {}
            }
            current = n.parent();
        }
        None
    }

    pub(crate) fn extract_package_symbol(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
    ) -> Option<Symbol> {
        let mut cursor = node.walk();
        let name_node = node.children(&mut cursor).find(|c| {
            matches!(
                c.kind(),
                "identifier" | "scoped_identifier" | "type_identifier"
            )
        })?;
        let name = name_node
            .utf8_text(source)
            .ok()
            .map(|s| s.trim().to_string())?;
        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Module,
            qualified_name: Some(name),
            location: Self::loc(file, node),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "groovy", "groovy_kind": "package" }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, Some(name_node));
        Some(symbol)
    }

    pub(crate) fn extract_static_initializer(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
    ) -> Option<Symbol> {
        let owner = Self::enclosing_class(node, source).unwrap_or_else(|| "Unknown".into());
        let qn = Self::qualify(package, &format!("{owner}.<clinit>"));
        let mut symbol = Symbol {
            name: "<clinit>".into(),
            symbol_type: SymbolType::Function,
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
                "language": "groovy",
                "is_static_initializer": true,
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        symbol = rgctl_plugin_helpers::callable_identity::symbol(
            symbol,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Groovy,
        );
        Some(symbol)
    }

    pub(crate) fn extract_lambda_symbol(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
        idx: u32,
    ) -> Option<Symbol> {
        let name = format!("$lambda${idx}");
        let owner = Self::enclosing_callable_qn(node, source, package)
            .or_else(|| Self::enclosing_class(node, source).map(|c| Self::qualify(package, &c)));
        let qn = owner.map(|o| format!("{o}.{name}"));
        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name: qn,
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
            metadata: serde_json::json!({ "language": "groovy", "is_lambda": true }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        symbol = rgctl_plugin_helpers::callable_identity::symbol(
            symbol,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Groovy,
        );
        Some(symbol)
    }

    pub(crate) fn extract_closure_symbol(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
        idx: u32,
    ) -> Option<Symbol> {
        let name = format!("$closure${idx}");
        let owner = Self::enclosing_callable_qn(node, source, package)
            .or_else(|| Self::enclosing_class(node, source).map(|c| Self::qualify(package, &c)));
        let qn = owner.map(|o| format!("{o}.{name}"));
        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name: qn,
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
            metadata: serde_json::json!({ "language": "groovy", "is_closure": true }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        symbol = rgctl_plugin_helpers::callable_identity::symbol(
            symbol,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Groovy,
        );
        Some(symbol)
    }

    pub(crate) fn extract_annotation_element(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
    ) -> Option<Symbol> {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string())?;
        let owner = Self::enclosing_class(node, source)?;
        let qn = Self::qualify(package, &format!("{owner}.{name}"));
        let return_type = node
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string());
        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name: Some(qn),
            location: Self::loc(file, node),
            signature: node
                .utf8_text(source)
                .ok()
                .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
            return_type,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "groovy",
                "is_annotation_element": true,
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        symbol = rgctl_plugin_helpers::callable_identity::symbol(
            symbol,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Groovy,
        );
        Some(symbol)
    }

    pub(crate) fn extract_module_symbol(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
    ) -> Option<Symbol> {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())?;
        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Module,
            qualified_name: Some(name),
            location: Self::loc(file, node),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "groovy", "groovy_kind": "module" }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Some(symbol)
    }

    pub(crate) fn extract_field_symbol(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
    ) -> Option<Symbol> {
        let mut cursor = node.walk();
        let declarator = node
            .children(&mut cursor)
            .find(|c| c.kind() == "variable_declarator")?;
        let name_node = declarator.child_by_field_name("name").or_else(|| {
            let mut c = declarator.walk();
            declarator
                .children(&mut c)
                .find(|n| n.kind() == "identifier")
        })?;
        let name = name_node
            .utf8_text(source)
            .ok()
            .map(|s| s.trim().to_string())?;
        let owner = Self::enclosing_class(node, source);
        let qn = match &owner {
            Some(cls) => Self::qualify(package, &format!("{cls}.{name}")),
            None => Self::qualify(package, &name),
        };
        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Variable,
            qualified_name: Some(qn),
            location: Self::loc(file, node),
            signature: node
                .utf8_text(source)
                .ok()
                .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
            return_type: node
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.trim().to_string()),
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "groovy", "is_field": true }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, Some(name_node));
        Some(symbol)
    }

    pub(crate) fn extract_enum_constant(
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
                    if n.kind() == "identifier" {
                        n.utf8_text(source).ok().map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
            })?;
        let owner = Self::enclosing_class(node, source).unwrap_or_else(|| "Unknown".into());
        let qn = Self::qualify(package, &format!("{owner}.{name}"));
        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Variable,
            qualified_name: Some(qn),
            location: Self::loc(file, node),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "groovy", "is_enum_constant": true }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Some(symbol)
    }

    pub(crate) fn extract_parameters(&self, node: Node, source: &[u8]) -> Vec<Parameter> {
        let Some(params) = node.child_by_field_name("parameters").or_else(|| {
            let mut c = node.walk();
            node.children(&mut c)
                .find(|ch| matches!(ch.kind(), "formal_parameters" | "inferred_parameters"))
        }) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut cursor = params.walk();
        for child in params.children(&mut cursor) {
            if child.kind() != "formal_parameter" {
                continue;
            }
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.trim().to_string())
                .or_else(|| {
                    let mut c = child.walk();
                    child.children(&mut c).find_map(|n| {
                        if n.kind() == "identifier" {
                            n.utf8_text(source).ok().map(|s| s.trim().to_string())
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_else(|| "_".into());
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

    pub(crate) fn extract_method_symbol(
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
                    if n.kind() == "identifier" {
                        n.utf8_text(source).ok().map(|s| s.trim().to_string())
                    } else {
                        None
                    }
                })
            })?;
        let enclosing = Self::enclosing_class(node, source);
        let is_ctor = enclosing.as_ref().is_some_and(|cls| cls == &name);
        let (sym_name, qn, metadata) = if is_ctor {
            let cls = enclosing.clone().unwrap_or_else(|| name.clone());
            (
                cls.clone(),
                Self::qualify(package, &format!("{cls}.<init>")),
                serde_json::json!({
                    "language": "groovy",
                    "is_constructor": true,
                }),
            )
        } else {
            let qn = if let Some(cls) = &enclosing {
                Self::qualify(package, &format!("{cls}.{name}"))
            } else {
                Self::qualify(package, &name)
            };
            (name, qn, serde_json::json!({ "language": "groovy" }))
        };
        let mut symbol = Symbol {
            name: sym_name,
            symbol_type: SymbolType::Function,
            qualified_name: Some(qn),
            location: Self::loc(file, node),
            signature: node
                .utf8_text(source)
                .ok()
                .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
            return_type: if is_ctor {
                None
            } else {
                node.child_by_field_name("type")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.trim().to_string())
            },
            parameters: self.extract_parameters(node, source),
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata,
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        symbol = rgctl_plugin_helpers::callable_identity::symbol(
            symbol,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Groovy,
        );
        Some(symbol)
    }

    pub(crate) fn extract_constructor_symbol(
        &self,
        node: Node,
        source: &[u8],
        file: &str,
        package: Option<&str>,
    ) -> Option<Symbol> {
        let cls = Self::enclosing_class(node, source)
            .or_else(|| Self::type_name(node, source))
            .unwrap_or_else(|| "Unknown".into());
        let qn = Self::qualify(package, &format!("{cls}.<init>"));
        let mut symbol = Symbol {
            name: cls,
            symbol_type: SymbolType::Function,
            qualified_name: Some(qn),
            location: Self::loc(file, node),
            signature: node
                .utf8_text(source)
                .ok()
                .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
            return_type: None,
            parameters: self.extract_parameters(node, source),
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "groovy",
                "is_constructor": true,
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        symbol = rgctl_plugin_helpers::callable_identity::symbol(
            symbol,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Groovy,
        );
        Some(symbol)
    }

    pub(crate) fn extract_import_symbol(node: Node, source: &[u8], file: &str) -> Option<Symbol> {
        let text = node.utf8_text(source).unwrap_or("").trim();
        let imported = text
            .trim_start_matches("import")
            .trim()
            .trim_end_matches(".*")
            .trim()
            .trim_end_matches(';')
            .trim();
        if imported.is_empty() {
            return None;
        }
        let simple = imported.rsplit('.').next().unwrap_or(imported).to_string();
        let mut symbol = Symbol {
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
            metadata: serde_json::json!({ "language": "groovy" }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Some(symbol)
    }

    pub(crate) fn append_type_and_annotation_relations(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let package = Self::package_name(root, source);
        let file = file_path.to_string_lossy();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            match node.kind() {
                "class_declaration" | "record_declaration" => {
                    if let Some(from) = Self::type_name(node, source) {
                        let from_qn = Self::qualify(package.as_deref(), &from);
                        let header = node
                            .utf8_text(source)
                            .ok()
                            .map(|s| s.lines().next().unwrap_or("").to_string())
                            .unwrap_or_default();
                        if let Some(rest) = header.split_once("extends").map(|(_, r)| r) {
                            let parent = rest.split(['{', ',', 'i']).next().unwrap_or("").trim();
                            if !parent.is_empty() {
                                relations.push(Relation {
                                    from: from_qn.clone(),
                                    to: parent.to_string(),
                                    relation_type: RelationType::Extends,
                                    location: Self::loc(&file, node),
                                    metadata: serde_json::json!({ "language": "groovy" }),
                                    to_qualified_hint: None,
                                    to_type_hint: None,
                                    ..Default::default()
                                });
                            }
                        }
                        if let Some(rest) = header.split_once("implements").map(|(_, r)| r) {
                            for iface in rest
                                .split(['{', ',', 'p'])
                                .map(str::trim)
                                .filter(|s| !s.is_empty() && *s != "permits")
                            {
                                if iface.starts_with("permits") {
                                    continue;
                                }
                                relations.push(Relation {
                                    from: from_qn.clone(),
                                    to: iface.to_string(),
                                    relation_type: RelationType::Implements,
                                    location: Self::loc(&file, node),
                                    metadata: serde_json::json!({ "language": "groovy" }),
                                    to_qualified_hint: None,
                                    to_type_hint: None,
                                    ..Default::default()
                                });
                            }
                        }
                        if let Some(rest) = header.split_once("permits").map(|(_, r)| r) {
                            for permitted in rest
                                .split(['{', ','])
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                            {
                                relations.push(Relation {
                                    from: from_qn.clone(),
                                    to: permitted.to_string(),
                                    relation_type: RelationType::References,
                                    location: Self::loc(&file, node),
                                    metadata: serde_json::json!({
                                        "language": "groovy",
                                        "permits": true
                                    }),
                                    to_qualified_hint: None,
                                    to_type_hint: None,
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
                "field_access" => {
                    if let Some(from) =
                        Self::enclosing_callable_qn(node, source, package.as_deref())
                        && let Some(field) = node.child_by_field_name("field")
                        && let Ok(name) = field.utf8_text(source)
                    {
                        let name = name.trim();
                        if !name.is_empty() {
                            relations.push(Relation {
                                from,
                                to: name.to_string(),
                                relation_type: RelationType::References,
                                location: Self::loc(&file, node),
                                metadata: serde_json::json!({ "language": "groovy" }),
                                to_qualified_hint: None,
                                to_type_hint: None,
                                ..Default::default()
                            });
                        }
                    }
                }
                "array_creation_expression" => {
                    if let Some(from) =
                        Self::enclosing_callable_qn(node, source, package.as_deref())
                        && let Some(type_node) = node.child_by_field_name("type")
                        && let Ok(raw) = type_node.utf8_text(source)
                    {
                        let simple = raw.split('<').next().unwrap_or(raw).trim();
                        let last = simple.rsplit('.').next().unwrap_or(simple);
                        if !last.is_empty() {
                            relations.push(Relation {
                                from,
                                to: last.to_string(),
                                relation_type: RelationType::Instantiates,
                                location: Self::loc(&file, node),
                                metadata: serde_json::json!({
                                    "language": "groovy",
                                    "array": true
                                }),
                                to_qualified_hint: None,
                                to_type_hint: Some(last.to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }
                "class_literal" => {
                    if let Some(from) =
                        Self::enclosing_callable_qn(node, source, package.as_deref())
                        && let Some(type_node) = node.named_child(0)
                        && let Ok(raw) = type_node.utf8_text(source)
                    {
                        let simple = raw.split('<').next().unwrap_or(raw).trim();
                        let last = simple.rsplit('.').next().unwrap_or(simple);
                        if !last.is_empty() {
                            relations.push(Relation {
                                from,
                                to: last.to_string(),
                                relation_type: RelationType::References,
                                location: Self::loc(&file, node),
                                metadata: serde_json::json!({
                                    "language": "groovy",
                                    "class_literal": true
                                }),
                                to_qualified_hint: None,
                                to_type_hint: Some(last.to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }
                "method_reference" => {
                    if let Some(from) =
                        Self::enclosing_callable_qn(node, source, package.as_deref())
                    {
                        let to = node
                            .child_by_field_name("name")
                            .or_else(|| node.child_by_field_name("method"))
                            .and_then(|n| n.utf8_text(source).ok())
                            .map(|s| s.trim().to_string())
                            .unwrap_or_else(|| "method_ref".into());
                        relations.push(Relation {
                            from,
                            to,
                            relation_type: RelationType::Calls,
                            location: Self::loc(&file, node),
                            metadata: serde_json::json!({
                                "language": "groovy",
                                "method_reference": true
                            }),
                            to_qualified_hint: None,
                            to_type_hint: None,
                            ..Default::default()
                        });
                    }
                }
                "method_declaration" | "constructor_declaration" | "function_definition" => {
                    if let Some(throws) = {
                        let mut c = node.walk();
                        node.children(&mut c).find(|ch| ch.kind() == "throws")
                    } {
                        let from = match node.kind() {
                            "constructor_declaration" => {
                                let cls = Self::enclosing_class(node, source)
                                    .unwrap_or_else(|| "Unknown".into());
                                Self::qualify(package.as_deref(), &format!("{cls}.<init>"))
                            }
                            _ => {
                                let name = node
                                    .child_by_field_name("name")
                                    .and_then(|n| n.utf8_text(source).ok())
                                    .map(|s| s.trim().to_string())
                                    .unwrap_or_default();
                                if let Some(cls) = Self::enclosing_class(node, source) {
                                    Self::qualify(package.as_deref(), &format!("{cls}.{name}"))
                                } else {
                                    Self::qualify(package.as_deref(), &name)
                                }
                            }
                        };
                        if let Ok(text) = throws.utf8_text(source) {
                            for raw in text.trim_start_matches("throws").split(',') {
                                let simple = raw.trim().rsplit('.').next().unwrap_or("").trim();
                                if simple.is_empty() {
                                    continue;
                                }
                                relations.push(Relation {
                                    from: from.clone(),
                                    to: simple.to_string(),
                                    relation_type: RelationType::References,
                                    location: Self::loc(&file, throws),
                                    metadata: serde_json::json!({
                                        "language": "groovy",
                                        "throws": true
                                    }),
                                    to_qualified_hint: None,
                                    to_type_hint: Some(simple.to_string()),
                                    ..Default::default()
                                });
                            }
                        }
                    }
                }
                "module_declaration" => {
                    let module_name = node
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if !module_name.is_empty()
                        && let Some(body) = node.child_by_field_name("body")
                    {
                        let mut c = body.walk();
                        for directive in body.children(&mut c) {
                            Self::push_module_directive_relation(
                                directive,
                                source,
                                &file,
                                &module_name,
                                relations,
                            );
                        }
                    }
                }
                "annotated_type" => {
                    // Type-use annotation site; AnnotatedWith emitted from modifiers walk.
                }
                _ => {}
            }
            if matches!(
                node.kind(),
                "class_declaration"
                    | "interface_declaration"
                    | "enum_declaration"
                    | "annotation_type_declaration"
                    | "record_declaration"
                    | "method_declaration"
                    | "function_definition"
                    | "constructor_declaration"
                    | "compact_constructor_declaration"
            ) {
                Self::push_annotated_with_for(node, source, file_path, root, relations);
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
        Ok(())
    }

    fn push_module_directive_relation(
        directive: Node,
        source: &[u8],
        file: &str,
        module_name: &str,
        relations: &mut Vec<Relation>,
    ) {
        let (relation_type, to) = match directive.kind() {
            "requires_module_directive" => {
                let to = directive
                    .child_by_field_name("module")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                (RelationType::DependsOn, to)
            }
            "exports_module_directive" | "opens_module_directive" => {
                let to = directive
                    .child_by_field_name("package")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                (RelationType::Defines, to)
            }
            "uses_module_directive" => {
                let to = directive
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                (RelationType::Uses, to)
            }
            "provides_module_directive" => {
                let to = directive
                    .child_by_field_name("provided")
                    .or_else(|| directive.child_by_field_name("type"))
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.trim().to_string())
                    .unwrap_or_default();
                (RelationType::Uses, to)
            }
            _ => return,
        };
        if to.is_empty() {
            return;
        }
        relations.push(Relation {
            from: module_name.to_string(),
            to,
            relation_type,
            location: Self::loc(file, directive),
            metadata: serde_json::json!({
                "language": "groovy",
                "module_directive": directive.kind()
            }),
            to_qualified_hint: None,
            to_type_hint: None,
            ..Default::default()
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
        let mut symbols = Vec::with_capacity(32);
        let mut stack = vec![root];

        while let Some(node) = stack.pop() {
            match node.kind() {
                "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "annotation_type_declaration" => {
                    let Some(simple) = Self::type_name(node, source) else {
                        let mut cursor = node.walk();
                        for child in node
                            .children(&mut cursor)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                        {
                            stack.push(child);
                        }
                        continue;
                    };
                    let symbol_type = match node.kind() {
                        "interface_declaration" => SymbolType::Interface,
                        "enum_declaration" => SymbolType::Enum,
                        "annotation_type_declaration" => SymbolType::Annotation,
                        _ => SymbolType::Class,
                    };
                    let qn = Self::qualify(package.as_deref(), &simple);
                    let mut symbol = Symbol {
                        name: simple,
                        symbol_type,
                        qualified_name: Some(qn),
                        location: Self::loc(&file, node),
                        signature: node
                            .utf8_text(source)
                            .ok()
                            .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "language": "groovy" }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
                    symbol = rgctl_plugin_helpers::callable_identity::symbol(
                        symbol,
                        node,
                        source,
                        rgctl_plugin_helpers::callable_identity::Policy::Groovy,
                    );
                    symbols.push(symbol);
                }
                "method_declaration" | "function_definition" => {
                    let name = node
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(|s| s.trim().to_string())
                        .or_else(|| {
                            let mut c = node.walk();
                            node.children(&mut c).find_map(|n| {
                                if n.kind() == "identifier" {
                                    n.utf8_text(source).ok().map(|s| s.trim().to_string())
                                } else {
                                    None
                                }
                            })
                        });
                    let Some(name) = name else {
                        let mut cursor = node.walk();
                        for child in node
                            .children(&mut cursor)
                            .collect::<Vec<_>>()
                            .into_iter()
                            .rev()
                        {
                            stack.push(child);
                        }
                        continue;
                    };
                    let enclosing = Self::enclosing_class(node, source);
                    // Groovy grammar often emits constructors as method_declaration
                    // named after the class (no return type) rather than constructor_declaration.
                    let is_ctor = enclosing.as_ref().is_some_and(|cls| cls == &name);
                    let (sym_name, qn, metadata) = if is_ctor {
                        let cls = enclosing.clone().unwrap_or_else(|| name.clone());
                        (
                            cls.clone(),
                            Self::qualify(package.as_deref(), &format!("{cls}.<init>")),
                            serde_json::json!({
                                "language": "groovy",
                                "is_constructor": true,
                            }),
                        )
                    } else {
                        let qn = if let Some(cls) = &enclosing {
                            Self::qualify(package.as_deref(), &format!("{cls}.{name}"))
                        } else {
                            Self::qualify(package.as_deref(), &name)
                        };
                        (name, qn, serde_json::json!({ "language": "groovy" }))
                    };
                    let mut symbol = Symbol {
                        name: sym_name,
                        symbol_type: SymbolType::Function,
                        qualified_name: Some(qn),
                        location: Self::loc(&file, node),
                        signature: node
                            .utf8_text(source)
                            .ok()
                            .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
                        return_type: if is_ctor {
                            None
                        } else {
                            node.child_by_field_name("type")
                                .and_then(|n| n.utf8_text(source).ok())
                                .map(|s| s.trim().to_string())
                        },
                        parameters: self.extract_parameters(node, source),
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata,
                        ..Default::default()
                    };
                    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
                    symbol = rgctl_plugin_helpers::callable_identity::symbol(
                        symbol,
                        node,
                        source,
                        rgctl_plugin_helpers::callable_identity::Policy::Groovy,
                    );
                    symbols.push(symbol);
                }
                "constructor_declaration" | "compact_constructor_declaration" => {
                    let cls = Self::enclosing_class(node, source)
                        .or_else(|| Self::type_name(node, source))
                        .unwrap_or_else(|| "Unknown".into());
                    let qn = Self::qualify(package.as_deref(), &format!("{cls}.<init>"));
                    let mut symbol = Symbol {
                        name: cls,
                        symbol_type: SymbolType::Function,
                        qualified_name: Some(qn),
                        location: Self::loc(&file, node),
                        signature: node
                            .utf8_text(source)
                            .ok()
                            .map(|s| s.lines().next().unwrap_or("").trim().to_string()),
                        return_type: None,
                        parameters: self.extract_parameters(node, source),
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({
                            "language": "groovy",
                            "is_constructor": true,
                        }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
                    symbol = rgctl_plugin_helpers::callable_identity::symbol(
                        symbol,
                        node,
                        source,
                        rgctl_plugin_helpers::callable_identity::Policy::Groovy,
                    );
                    symbols.push(symbol);
                }
                "import_declaration" => {
                    let text = node.utf8_text(source).unwrap_or("").trim();
                    let imported = text
                        .trim_start_matches("import")
                        .trim()
                        .trim_end_matches(".*")
                        .trim()
                        .trim_end_matches(';')
                        .trim();
                    if !imported.is_empty() {
                        let simple = imported.rsplit('.').next().unwrap_or(imported).to_string();
                        let mut symbol = Symbol {
                            name: simple,
                            symbol_type: SymbolType::Import,
                            qualified_name: Some(imported.to_string()),
                            location: Self::loc(&file, node),
                            signature: Some(text.to_string()),
                            return_type: None,
                            parameters: vec![],
                            fields: vec![],
                            modifiers: vec![],
                            documentation: None,
                            metadata: serde_json::json!({ "language": "groovy" }),
                            ..Default::default()
                        };
                        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
                        symbols.push(symbol);
                    }
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

    fn relations_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let mut relations = Vec::new();
        // method_invocation is Java-shaped; walk_calls uses call_kinds
        walk_calls(
            root,
            source,
            file_path,
            symbols,
            GROOVY_CALL_KINDS,
            "groovy",
            &mut relations,
        );

        // Extends / implements from class header text (best-effort)
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "class_declaration"
                && let Some(from) = Self::type_name(node, source)
            {
                let package = Self::package_name(root, source);
                let from_qn = Self::qualify(package.as_deref(), &from);
                let header = node
                    .utf8_text(source)
                    .ok()
                    .map(|s| s.lines().next().unwrap_or("").to_string())
                    .unwrap_or_default();
                if let Some(rest) = header.split_once("extends").map(|(_, r)| r) {
                    let parent = rest.split(['{', ',', 'i']).next().unwrap_or("").trim();
                    if !parent.is_empty() {
                        relations.push(Relation {
                            from: from_qn.clone(),
                            to: parent.to_string(),
                            relation_type: RelationType::Extends,
                            location: Self::loc(&file_path.to_string_lossy(), node),
                            metadata: serde_json::json!({ "language": "groovy" }),
                            to_qualified_hint: None,
                            to_type_hint: None,
                            ..Default::default()
                        });
                    }
                }
                if let Some(rest) = header.split_once("implements").map(|(_, r)| r) {
                    for iface in rest
                        .split(['{', ','])
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                    {
                        relations.push(Relation {
                            from: from_qn.clone(),
                            to: iface.to_string(),
                            relation_type: RelationType::Implements,
                            location: Self::loc(&file_path.to_string_lossy(), node),
                            metadata: serde_json::json!({ "language": "groovy" }),
                            to_qualified_hint: None,
                            to_type_hint: None,
                            ..Default::default()
                        });
                    }
                }
            }
            // AnnotatedWith on types / methods / constructors (Java-shaped modifiers).
            if matches!(
                node.kind(),
                "class_declaration"
                    | "interface_declaration"
                    | "enum_declaration"
                    | "annotation_type_declaration"
                    | "method_declaration"
                    | "function_definition"
                    | "constructor_declaration"
                    | "compact_constructor_declaration"
            ) {
                Self::push_annotated_with_for(node, source, file_path, root, &mut relations);
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
        Ok(relations)
    }

    /// Emit `AnnotatedWith` for `@Foo` / `@Foo(...)` on a declaration.
    fn push_annotated_with_for(
        node: Node,
        source: &[u8],
        file_path: &Path,
        root: Node,
        relations: &mut Vec<Relation>,
    ) {
        let package = Self::package_name(root, source);
        let from = match node.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "annotation_type_declaration"
            | "record_declaration" => {
                Self::type_name(node, source).map(|n| Self::qualify(package.as_deref(), &n))
            }
            "method_declaration" | "function_definition" => {
                let name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(|s| s.trim().to_string());
                name.map(|name| {
                    if let Some(cls) = Self::enclosing_class(node, source) {
                        Self::qualify(package.as_deref(), &format!("{cls}.{name}"))
                    } else {
                        Self::qualify(package.as_deref(), &name)
                    }
                })
            }
            "constructor_declaration" | "compact_constructor_declaration" => {
                let cls = Self::enclosing_class(node, source)
                    .or_else(|| Self::type_name(node, source))
                    .unwrap_or_else(|| "Unknown".into());
                Some(Self::qualify(package.as_deref(), &format!("{cls}.<init>")))
            }
            _ => None,
        };
        let Some(from) = from else {
            return;
        };

        let mut cursor = node.walk();
        let modifiers = node.children(&mut cursor).find(|c| c.kind() == "modifiers");
        let Some(modifiers) = modifiers else {
            return;
        };
        let mut mc = modifiers.walk();
        for child in modifiers.children(&mut mc) {
            if !matches!(child.kind(), "annotation" | "marker_annotation") {
                continue;
            }
            let raw_name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string)
                .or_else(|| {
                    // Fallback: strip leading `@` from annotation text.
                    child.utf8_text(source).ok().map(|t| {
                        t.trim()
                            .trim_start_matches('@')
                            .split(['(', ' ', '\n'])
                            .next()
                            .unwrap_or("")
                            .to_string()
                    })
                })
                .unwrap_or_default();
            let simple = raw_name
                .rsplit('.')
                .next()
                .unwrap_or(&raw_name)
                .trim()
                .to_string();
            if simple.is_empty() {
                continue;
            }
            let args = child
                .child_by_field_name("arguments")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string);
            let mut metadata = serde_json::json!({ "language": "groovy" });
            if let Some(args) = args {
                metadata["arguments"] = serde_json::Value::String(args);
            }
            relations.push(Relation {
                from: from.clone(),
                to: simple,
                relation_type: RelationType::AnnotatedWith,
                location: Self::loc(&file_path.to_string_lossy(), child),
                metadata,
                to_qualified_hint: None,
                to_type_hint: None,
                ..Default::default()
            });
        }
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

impl Default for GroovyPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create GroovyPlugin")
    }
}

impl LanguagePlugin for GroovyPlugin {
    fn language_id(&self) -> &str {
        "groovy"
    }

    fn file_extensions(&self) -> Vec<&str> {
        // `.gradle` scripts (not `build.gradle` basename — Manifest wins in registry)
        vec!["groovy", "gradle"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_groovy::LANGUAGE.into())
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
        Some(groovy_ast_handlers())
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
        let tree = self.parse(Path::new(&symbol.location.file), source)?;
        let target_line = symbol.location.start_line.saturating_sub(1);
        let mut found = None;
        let mut stack = vec![tree.root_node()];
        while let Some(node) = stack.pop() {
            if matches!(
                node.kind(),
                "method_declaration" | "function_definition" | "constructor_declaration"
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
            loc: symbol
                .location
                .end_line
                .saturating_sub(symbol.location.start_line)
                + 1,
            parameters: symbol.parameters.len(),
            nesting_depth: 0,
            returns: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_class_and_method() {
        let src = b"package com.example\nclass OrderService {\n  String findById(Long id) {\n    return id.toString()\n  }\n}\n";
        let plugin = GroovyPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("OrderService.groovy"), src)
            .unwrap();
        assert!(
            symbols.iter().any(|s| {
                s.symbol_type == SymbolType::Class
                    && s.qualified_name.as_deref() == Some("com.example.OrderService")
            }),
            "{symbols:?}"
        );
        assert!(symbols.iter().any(|s| s.name == "findById"), "{symbols:?}");
    }

    #[test]
    fn extracts_same_class_calls() {
        let src = b"class OrderService {\n  def validate() {}\n  def findAll() { validate() }\n}\n";
        let plugin = GroovyPlugin::new().unwrap();
        let all = plugin
            .extract_all(Path::new("OrderService.groovy"), src)
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
    fn extracts_annotated_with() {
        let src = b"@CompileStatic\nclass OrderService {\n  @Override\n  String toString() { \"x\" }\n}\n";
        let plugin = GroovyPlugin::new().unwrap();
        let all = plugin
            .extract_all(Path::new("OrderService.groovy"), src)
            .unwrap();
        assert!(
            all.relations.iter().any(|r| {
                r.relation_type == RelationType::AnnotatedWith
                    && r.to == "CompileStatic"
                    && r.from.contains("OrderService")
            }),
            "expected AnnotatedWith CompileStatic: {:?}",
            all.relations
        );
        assert!(
            all.relations
                .iter()
                .any(|r| { r.relation_type == RelationType::AnnotatedWith && r.to == "Override" }),
            "expected AnnotatedWith Override: {:?}",
            all.relations
        );
    }
}
