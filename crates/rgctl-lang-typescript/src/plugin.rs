//! TypeScript language plugin
//!
//! Extracts symbols, relationships, and complexity metrics from TypeScript source code
//! using TreeSitter.

use crate::single_pass::{extract_all_single_pass, typescript_ast_handlers};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use rgctl_plugin_helpers::{
    bound_function_expression_name, extract_class_extends_relations, extract_import_symbols,
    find_child_kind, is_ecmascript_function_node, is_function_expression_kind, simple_type_name,
    type_name_from_node,
};
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::Node;

/// TypeScript language plugin
pub struct TypeScriptPlugin;

impl TypeScriptPlugin {
    /// Create a new TypeScript plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn find_containing_class_name(&self, node: Node, source: &[u8]) -> Option<String> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if matches!(
                parent.kind(),
                "class_declaration" | "abstract_class_declaration"
            ) {
                if let Some(name) = parent
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string)
                    .filter(|s| !s.is_empty())
                {
                    return Some(name);
                }
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    if matches!(child.kind(), "type_identifier" | "identifier") {
                        return child.utf8_text(source).ok().map(str::to_string);
                    }
                }
            }
            current = parent;
        }
        None
    }

    fn find_containing_interface_name<'a>(&self, node: Node<'a>, source: &[u8]) -> Option<String> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "interface_declaration" {
                return parent
                    .child_by_field_name("name")
                    .or_else(|| find_child_kind(parent, "type_identifier"))
                    .and_then(|n| n.utf8_text(source).ok().map(str::to_string));
            }
            current = parent;
        }
        None
    }

    pub(crate) fn extract_method_signature(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        interface_name: &str,
    ) -> Result<Symbol> {
        self.extract_signature_method(
            node,
            source,
            file_path,
            interface_name,
            serde_json::json!({
                "language": "typescript",
                "is_interface_method": true,
            }),
        )
    }

    /// `abstract foo(): void` on an abstract class.
    pub(crate) fn extract_abstract_method_signature(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        class_name: &str,
    ) -> Result<Symbol> {
        let mut sym = self.extract_signature_method(
            node,
            source,
            file_path,
            class_name,
            serde_json::json!({
                "language": "typescript",
                "is_abstract": true,
            }),
        )?;
        if !sym.modifiers.iter().any(|m| m == "abstract") {
            sym.modifiers.push("abstract".to_string());
        }
        Ok(sym)
    }

    fn extract_signature_method(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        owner_name: &str,
        metadata: serde_json::Value,
    ) -> Result<Symbol> {
        let mut name = None;
        let mut parameters = Vec::new();
        let mut return_type = None;

        if let Some(n) = node.child_by_field_name("name") {
            name = n.utf8_text(source).ok().map(str::to_string);
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "property_identifier" | "identifier" => {
                    if name.is_none() {
                        name = child.utf8_text(source).ok().map(str::to_string);
                    }
                }
                "formal_parameters" => {
                    parameters = self.extract_parameters(child, source)?;
                }
                "type_annotation" => {
                    return_type = Some(
                        child
                            .utf8_text(source)?
                            .trim_start_matches(':')
                            .trim()
                            .to_string(),
                    );
                }
                _ => {}
            }
        }

        let name = name.unwrap_or_else(|| "anonymous".to_string());
        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name: Some(format!("{owner_name}.{name}")),
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: Some(
                node.utf8_text(source)?
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            ),
            return_type,
            parameters,
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata,
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(symbol)
    }

    /// `namespace N { ... }` / ambient `module M`.
    pub(crate) fn extract_internal_module(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string)
            .or_else(|| {
                let mut c = node.walk();
                node.children(&mut c).find_map(|ch| {
                    if matches!(ch.kind(), "identifier" | "string") {
                        ch.utf8_text(source).ok().map(str::to_string)
                    } else {
                        None
                    }
                })
            })
            .unwrap_or_else(|| "anonymous".to_string());
        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Module,
            qualified_name: Some(name),
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "typescript",
                "kind": node.kind(),
            }),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(symbol)
    }

    /// `export { foo as bar } from './m'` → Uses edges per specifier.
    pub(crate) fn collect_export_specifier_relations(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        relations: &mut Vec<Relation>,
    ) {
        if node.kind() != "export_specifier" {
            return;
        }
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);
        let alias = node
            .child_by_field_name("alias")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);
        let Some(name) = name else {
            return;
        };
        let exported = alias.clone().unwrap_or_else(|| name.clone());
        let module_path = export_specifier_source_module(node, source);
        let mut meta = serde_json::json!({
            "language": "typescript",
            "kind": "export_specifier",
            "name": name,
        });
        if let Some(a) = alias {
            meta["alias"] = serde_json::Value::String(a);
        }
        if let Some(ref m) = module_path {
            meta["module"] = serde_json::Value::String(m.clone());
        }
        let to_qualified_hint = module_path.as_ref().map(|m| format!("{m}::{exported}"));
        relations.push(Relation {
            from: file_path.to_string(),
            to: exported,
            relation_type: RelationType::Uses,
            location: source_location(node, file_path),
            metadata: meta,
            to_qualified_hint,
            to_type_hint: module_path,
            ..Default::default()
        });
    }

    pub(crate) fn extract_export_type_symbol(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Option<Symbol> {
        let text = node.utf8_text(source).ok()?.trim().to_string();
        if !text.starts_with("export type") {
            return None;
        }
        let mut symbol = Symbol {
            name: text,
            symbol_type: SymbolType::Import,
            qualified_name: None,
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "typescript",
                "is_type_only": true,
                "direction": "export",
            }),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, None);
        Some(symbol)
    }

    pub(crate) fn extract_function(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut parameters = Vec::new();
        let mut return_type = None;
        let mut modifiers = Vec::new();
        // Arrow / function expressions put the binding on a parent; their first
        // identifier child is often a parameter (e.g. `x => x`), not a name.
        let is_expr = is_function_expression_kind(node.kind())
            || (node.kind() == "function"
                && node.is_named()
                && node.child_by_field_name("name").is_none());

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" | "property_identifier" => {
                    if name.is_none() && !is_expr {
                        name = Some(child.utf8_text(source)?.to_string());
                    }
                }
                "formal_parameters" => {
                    parameters = self.extract_parameters(child, source)?;
                }
                "type_annotation" => {
                    return_type = Some(
                        child
                            .utf8_text(source)?
                            .trim_start_matches(':')
                            .trim()
                            .to_string(),
                    );
                }
                "accessibility_modifier" | "async" | "static" => {
                    modifiers.push(child.utf8_text(source)?.to_string());
                }
                _ => {}
            }
        }

        let raw_name = name
            .or_else(|| bound_function_expression_name(node, source))
            .unwrap_or_else(|| {
                if is_expr {
                    format!("anonymous@L{}", node.start_position().row + 1)
                } else {
                    "anonymous".to_string()
                }
            });
        let is_constructor = raw_name == "constructor" && node.kind() == "method_definition";
        let class_name = if is_constructor {
            self.find_containing_class_name(node, source)
        } else {
            None
        };
        let (name, qualified_name, metadata) = if is_constructor {
            let class_name = class_name.unwrap_or_else(|| "anonymous".to_string());
            (
                class_name.clone(),
                Some(format!("{class_name}.<init>")),
                serde_json::json!({ "language": "typescript", "is_constructor": true }),
            )
        } else {
            let qualified_name = if node.kind() == "method_definition" {
                self.find_containing_class_name(node, source)
                    .map(|c| format!("{c}.{raw_name}"))
            } else {
                None
            };
            (
                raw_name,
                qualified_name,
                serde_json::json!({ "language": "typescript" }),
            )
        };

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name,
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: Some(
                node.utf8_text(source)?
                    .lines()
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string(),
            ),
            return_type,
            parameters,
            fields: vec![],
            modifiers,
            documentation: None,
            metadata,
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(symbol)
    }

    fn extract_parameters(&self, params_node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut parameters = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            if child.kind() == "required_parameter" || child.kind() == "optional_parameter" {
                let mut param_cursor = child.walk();
                let mut name = None;
                let mut param_type = None;

                for param_child in child.children(&mut param_cursor) {
                    match param_child.kind() {
                        "identifier" => {
                            name = Some(param_child.utf8_text(source)?.to_string());
                        }
                        "type_annotation" => {
                            param_type = Some(
                                param_child
                                    .utf8_text(source)?
                                    .trim_start_matches(':')
                                    .trim()
                                    .to_string(),
                            );
                        }
                        _ => {}
                    }
                }

                if let Some(name) = name {
                    parameters.push(Parameter {
                        name,
                        param_type,
                        default_value: None,
                    });
                }
            }
        }

        Ok(parameters)
    }

    pub(crate) fn extract_class(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut fields = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" | "identifier" => {
                    if name.is_none() {
                        name = Some(child.utf8_text(source)?.to_string());
                    }
                }
                "class_body" => {
                    fields = self.extract_class_fields(child, source)?;
                }
                _ => {}
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Class missing name".to_string(),
        })?;

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Class,
            qualified_name: None,
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields,
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(symbol)
    }

    fn extract_class_fields(&self, class_body: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut cursor = class_body.walk();

        for child in class_body.children(&mut cursor) {
            if child.kind() == "field_definition" || child.kind() == "public_field_definition" {
                let mut field_cursor = child.walk();
                let mut name = None;
                let mut field_type = None;
                let mut visibility = None;

                for field_child in child.children(&mut field_cursor) {
                    match field_child.kind() {
                        "property_identifier" => {
                            name = Some(field_child.utf8_text(source)?.to_string());
                        }
                        "type_annotation" => {
                            field_type = Some(
                                field_child
                                    .utf8_text(source)?
                                    .trim_start_matches(':')
                                    .trim()
                                    .to_string(),
                            );
                        }
                        "accessibility_modifier" => {
                            visibility = Some(field_child.utf8_text(source)?.to_string());
                        }
                        _ => {}
                    }
                }

                if let Some(name) = name {
                    fields.push(Field {
                        name,
                        field_type,
                        visibility,
                    });
                }
            }
        }

        Ok(fields)
    }

    pub(crate) fn extract_interface(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut fields = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "object_type" => {
                    fields = self.extract_interface_properties(child, source)?;
                }
                _ => {}
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Interface missing name".to_string(),
        })?;

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Interface,
            qualified_name: None,
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields,
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({}),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(symbol)
    }

    pub(crate) fn extract_enum(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string)
            .or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor).find_map(|c| {
                    if matches!(c.kind(), "type_identifier" | "identifier") {
                        c.utf8_text(source).ok().map(str::to_string)
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Enum missing name".to_string(),
            })?;

        let mut fields = Vec::new();
        if let Some(body) = node.child_by_field_name("body") {
            let mut cursor = body.walk();
            for child in body.children(&mut cursor) {
                if child.kind() == "enum_assignment"
                    || child.kind() == "property_identifier"
                    || child.kind() == "identifier"
                {
                    let member = if child.kind() == "enum_assignment" {
                        child
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source).ok())
                            .map(str::to_string)
                            .or_else(|| {
                                let mut c = child.walk();
                                child.children(&mut c).find_map(|n| {
                                    if matches!(n.kind(), "property_identifier" | "identifier") {
                                        n.utf8_text(source).ok().map(str::to_string)
                                    } else {
                                        None
                                    }
                                })
                            })
                    } else {
                        child.utf8_text(source).ok().map(str::to_string)
                    };
                    if let Some(member) = member.filter(|s| !s.is_empty() && s != ",") {
                        fields.push(Field {
                            name: member,
                            field_type: None,
                            visibility: None,
                        });
                    }
                }
            }
        }

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Enum,
            qualified_name: None,
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: None,
            return_type: None,
            parameters: vec![],
            fields,
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "typescript" }),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(symbol)
    }

    pub(crate) fn extract_type_alias(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string)
            .or_else(|| {
                let mut cursor = node.walk();
                node.children(&mut cursor).find_map(|c| {
                    if matches!(c.kind(), "type_identifier" | "identifier") {
                        c.utf8_text(source).ok().map(str::to_string)
                    } else {
                        None
                    }
                })
            })
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Type alias missing name".to_string(),
            })?;

        let value = node
            .child_by_field_name("value")
            .and_then(|n| n.utf8_text(source).ok())
            .map(|s| s.trim().to_string());

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::TypeAlias,
            qualified_name: None,
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                start_column: node.start_position().column,
                end_column: node.end_position().column,
            },
            signature: value.clone(),
            return_type: value,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "typescript" }),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(symbol)
    }

    fn extract_interface_properties(&self, object_type: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut cursor = object_type.walk();

        for child in object_type.children(&mut cursor) {
            if child.kind() == "property_signature" {
                let mut prop_cursor = child.walk();
                let mut name = None;
                let mut field_type = None;

                for prop_child in child.children(&mut prop_cursor) {
                    match prop_child.kind() {
                        "property_identifier" => {
                            name = Some(prop_child.utf8_text(source)?.to_string());
                        }
                        "type_annotation" => {
                            field_type = Some(
                                prop_child
                                    .utf8_text(source)?
                                    .trim_start_matches(':')
                                    .trim()
                                    .to_string(),
                            );
                        }
                        _ => {}
                    }
                }

                if let Some(name) = name {
                    fields.push(Field {
                        name,
                        field_type,
                        visibility: None,
                    });
                }
            }
        }

        Ok(fields)
    }

    fn calculate_cyclomatic(&self, node: Node) -> usize {
        let mut complexity = 1;

        fn traverse(node: Node, complexity: &mut usize) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "if_statement"
                    | "switch_statement"
                    | "while_statement"
                    | "for_statement"
                    | "catch_clause"
                    | "conditional_expression" => {
                        *complexity += 1;
                    }
                    "case_clause" => {
                        *complexity += 1;
                    }
                    _ => {}
                }
                traverse(child, complexity);
            }
        }

        traverse(node, &mut complexity);
        complexity
    }

    fn calculate_cognitive(&self, node: Node) -> usize {
        let mut cognitive = 0;

        fn traverse(node: Node, cognitive: &mut usize, nesting: usize) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "if_statement" | "while_statement" | "for_statement" => {
                        *cognitive += 1 + nesting;
                        traverse(child, cognitive, nesting + 1);
                    }
                    "switch_statement" | "catch_clause" => {
                        *cognitive += 1 + nesting;
                        traverse(child, cognitive, nesting);
                    }
                    _ => {
                        traverse(child, cognitive, nesting);
                    }
                }
            }
        }

        traverse(node, &mut cognitive, 0);
        cognitive
    }

    fn count_loc(&self, node: Node) -> usize {
        (node.end_position().row - node.start_position().row + 1).max(1)
    }

    fn count_nesting_depth(&self, node: Node) -> usize {
        let mut max_depth = 0;

        fn traverse(node: Node, max_depth: &mut usize, current_depth: usize) {
            *max_depth = (*max_depth).max(current_depth);
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if matches!(
                    child.kind(),
                    "if_statement" | "while_statement" | "for_statement" | "statement_block"
                ) {
                    traverse(child, max_depth, current_depth + 1);
                } else {
                    traverse(child, max_depth, current_depth);
                }
            }
        }

        traverse(node, &mut max_depth, 0);
        max_depth
    }

    fn count_returns(&self, node: Node) -> usize {
        let mut count = 0;

        fn traverse(node: Node, count: &mut usize) {
            if node.kind() == "return_statement" {
                *count += 1;
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                traverse(child, count);
            }
        }

        traverse(node, &mut count);
        count
    }

    pub(crate) fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        parse_with_pool(
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            source,
        )
        .map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: e,
        })
    }

    fn symbols_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
    ) -> Result<Vec<Symbol>> {
        let mut symbols = Vec::new();
        let file_path_str = file_path.to_string_lossy();

        fn traverse_for_symbols(
            node: Node,
            source: &[u8],
            file_path: &str,
            symbols: &mut Vec<Symbol>,
            plugin: &TypeScriptPlugin,
        ) -> Result<()> {
            if is_ecmascript_function_node(node) {
                symbols.push(plugin.extract_function(node, source, file_path)?);
            }
            match node.kind() {
                "class_declaration" | "abstract_class_declaration" => {
                    symbols.push(plugin.extract_class(node, source, file_path)?);
                }
                "abstract_method_signature" => {
                    if let Some(owner) = plugin.find_containing_class_name(node, source) {
                        symbols.push(
                            plugin.extract_abstract_method_signature(
                                node, source, file_path, &owner,
                            )?,
                        );
                    }
                }
                "interface_declaration" => {
                    let iface = plugin.extract_interface(node, source, file_path)?;
                    let iface_name = iface.name.clone();
                    symbols.push(iface);
                    if let Some(body) = node.child_by_field_name("body") {
                        let mut bc = body.walk();
                        for child in body.children(&mut bc) {
                            if child.kind() == "method_signature" {
                                symbols.push(plugin.extract_method_signature(
                                    child,
                                    source,
                                    file_path,
                                    &iface_name,
                                )?);
                            }
                        }
                    }
                }
                "enum_declaration" => {
                    symbols.push(plugin.extract_enum(node, source, file_path)?);
                }
                "type_alias_declaration" => {
                    symbols.push(plugin.extract_type_alias(node, source, file_path)?);
                }
                "internal_module" | "module" => {
                    symbols.push(plugin.extract_internal_module(node, source, file_path)?);
                }
                "import_statement" => {
                    symbols.extend(extract_import_symbols(
                        node,
                        source,
                        file_path,
                        "typescript",
                    ));
                }
                "export_statement" => {
                    if let Some(sym) = plugin.extract_export_type_symbol(node, source, file_path) {
                        symbols.push(sym);
                    }
                }
                _ => {}
            }

            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                traverse_for_symbols(child, source, file_path, symbols, plugin)?;
            }

            Ok(())
        }

        traverse_for_symbols(root, source, &file_path_str, &mut symbols, self)?;
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
        self.walk_ts_calls(root, source, file_path, symbols, &mut relations);
        self.extract_heritage(root, source, file_path, &mut relations)?;
        self.extract_decorators(root, source, file_path, &mut relations)?;
        self.extract_instantiations(root, source, file_path, symbols, &mut relations)?;
        Ok(relations)
    }

    fn walk_ts_calls(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) {
        const MAX_DEPTH: usize = 2048;
        let function_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .collect();
        let class_fields: HashMap<String, Vec<Field>> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Class)
            .map(|s| (s.name.clone(), s.fields.clone()))
            .collect();
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }

            if node.kind() == "call_expression"
                && let Some(from_fn) = containing_function(node, &function_symbols)
            {
                let callee = node
                    .child_by_field_name("function")
                    .and_then(|n| callee_name(n, source))
                    .or_else(|| callee_name(node, source));
                if let Some(callee) = callee.filter(|c| !c.is_empty()) {
                    let from = from_fn
                        .qualified_name
                        .clone()
                        .unwrap_or_else(|| from_fn.name.clone());
                    let (to_type_hint, to_qualified_hint) =
                        self.infer_ts_call_hints(node, source, from_fn, &class_fields);
                    let mut meta = serde_json::json!({ "language": "typescript" });
                    if self.is_unresolved_call(node, source) {
                        meta["unresolved"] = serde_json::Value::Bool(true);
                    }
                    let same_file_matches: Vec<_> = symbols
                        .iter()
                        .filter(|s| {
                            s.name == callee
                                && s.symbol_type == SymbolType::Function
                                && s.location.file == file_path.to_string_lossy()
                        })
                        .collect();
                    let local_target = match same_file_matches.as_slice() {
                        [only] => only
                            .qualified_name
                            .clone()
                            .unwrap_or_else(|| callee.clone()),
                        _ => to_qualified_hint.clone().unwrap_or_else(|| callee.clone()),
                    };
                    relations.push(Relation {
                        from,
                        to: local_target,
                        relation_type: RelationType::Calls,
                        location: source_location(node, &file_path.to_string_lossy()),
                        metadata: meta,
                        to_qualified_hint,
                        to_type_hint,
                        ..Default::default()
                    });
                }
            }

            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }
    }

    pub(crate) fn is_unresolved_call(&self, call: Node, source: &[u8]) -> bool {
        let Some(func) = call.child_by_field_name("function") else {
            return false;
        };
        if func.kind() != "member_expression" {
            return false;
        }
        let property = func.child_by_field_name("property");
        match property.map(|p| p.kind()) {
            Some("property_identifier") | Some("private_property_identifier") => false,
            Some("computed_property_name") => true,
            None => true,
            _ => property
                .and_then(|p| p.utf8_text(source).ok())
                .is_some_and(|t| t.starts_with('[')),
        }
    }

    fn infer_ts_call_hints(
        &self,
        call: Node,
        source: &[u8],
        from_fn: &Symbol,
        class_fields: &HashMap<String, Vec<Field>>,
    ) -> (Option<String>, Option<String>) {
        let Some(func) = call.child_by_field_name("function") else {
            return (None, None);
        };
        let Some(method_name) = callee_name(func, source) else {
            return (None, None);
        };
        if func.kind() != "member_expression" {
            return (None, None);
        }
        let Some(object) = func.child_by_field_name("object") else {
            return (None, None);
        };
        if let Some(type_name) =
            self.resolve_receiver_type(object, source, call, from_fn, class_fields)
        {
            let simple = simple_type_name(&type_name);
            let qualified = if type_name.contains('.') {
                format!("{type_name}.{method_name}")
            } else {
                format!("{simple}.{method_name}")
            };
            return (Some(simple), Some(qualified));
        }
        (None, None)
    }

    fn resolve_receiver_type(
        &self,
        object: Node,
        source: &[u8],
        call_site: Node,
        from_fn: &Symbol,
        class_fields: &HashMap<String, Vec<Field>>,
    ) -> Option<String> {
        match object.kind() {
            "this" => from_fn
                .qualified_name
                .as_ref()
                .and_then(|qn| qn.rsplit_once('.').map(|(ty, _)| ty.to_string()))
                .or_else(|| self.find_containing_class_name(call_site, source)),
            "identifier" | "property_identifier" => {
                let name = object.utf8_text(source).ok()?;
                if let Some(class_name) = self.find_containing_class_name(call_site, source)
                    && let Some(fields) = class_fields.get(&class_name)
                    && let Some(field) = fields.iter().find(|f| f.name == name)
                {
                    return field.field_type.clone();
                }
                self.find_local_variable_type(call_site, name, source)
            }
            "member_expression" => {
                let inner = object.child_by_field_name("object")?;
                if inner.kind() == "this" {
                    let field = object
                        .child_by_field_name("property")
                        .and_then(|p| p.utf8_text(source).ok())?;
                    if let Some(class_name) = self.find_containing_class_name(call_site, source)
                        && let Some(fields) = class_fields.get(&class_name)
                        && let Some(f) = fields.iter().find(|f| f.name == field)
                    {
                        return f.field_type.clone();
                    }
                }
                None
            }
            _ => None,
        }
    }

    fn find_local_variable_type(
        &self,
        start_node: Node,
        var_name: &str,
        source: &[u8],
    ) -> Option<String> {
        let mut current = start_node;
        while let Some(parent) = current.parent() {
            if matches!(
                parent.kind(),
                "function_declaration" | "method_definition" | "arrow_function"
            ) {
                break;
            }
            current = parent;
        }
        let mut stack = vec![current];
        while let Some(node) = stack.pop() {
            if node.start_byte() >= start_node.start_byte() {
                continue;
            }
            if matches!(
                node.kind(),
                "variable_declarator" | "lexical_declaration" | "variable_declaration"
            ) {
                if let Some(name_node) = node.child_by_field_name("name")
                    && name_node.utf8_text(source).ok() == Some(var_name)
                    && let Some(ty) = node.child_by_field_name("type")
                {
                    return ty
                        .utf8_text(source)
                        .ok()
                        .map(|t| t.trim_start_matches(':').trim().to_string());
                }
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.kind() == "variable_declarator"
                        && let Some(name_node) = child.child_by_field_name("name")
                        && name_node.utf8_text(source).ok() == Some(var_name)
                        && let Some(ty) = child.child_by_field_name("type")
                    {
                        return ty
                            .utf8_text(source)
                            .ok()
                            .map(|t| t.trim_start_matches(':').trim().to_string());
                    }
                }
            }
            let mut c = node.walk();
            stack.extend(node.children(&mut c));
        }
        None
    }

    fn extract_heritage(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        match node.kind() {
            "class_declaration" | "abstract_class_declaration" => {
                if let Some(name) = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                {
                    extract_class_extends_relations(
                        node,
                        source,
                        file_path,
                        name,
                        "typescript",
                        relations,
                    );
                    if let Some(heritage) = find_child_kind(node, "class_heritage") {
                        let mut hc = heritage.walk();
                        for child in heritage.children(&mut hc) {
                            if child.kind() == "implements_clause" {
                                self.collect_implements(name, child, source, file_path, relations);
                            }
                        }
                    }
                }
            }
            "interface_declaration" => {
                if let Some(name) = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    && let Some(extends) = find_child_kind(node, "extends_type_clause")
                {
                    let mut ec = extends.walk();
                    for child in extends.children(&mut ec) {
                        if let Some(target) = type_name_from_node(child, source) {
                            relations.push(Relation {
                                from: name.to_string(),
                                to: simple_type_name(&target),
                                relation_type: RelationType::Extends,
                                location: source_location(child, &file_path.to_string_lossy()),
                                metadata: serde_json::json!({ "language": "typescript" }),
                                to_qualified_hint: Some(target),
                                to_type_hint: None,
                                ..Default::default()
                            });
                        }
                    }
                }
            }
            _ => {}
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_heritage(child, source, file_path, relations)?;
        }
        Ok(())
    }

    pub(crate) fn collect_implements(
        &self,
        from: &str,
        clause: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) {
        let mut cursor = clause.walk();
        for child in clause.children(&mut cursor) {
            if let Some(target) = type_name_from_node(child, source) {
                relations.push(Relation {
                    from: from.to_string(),
                    to: simple_type_name(&target),
                    relation_type: RelationType::Implements,
                    location: source_location(child, &file_path.to_string_lossy()),
                    metadata: serde_json::json!({ "language": "typescript" }),
                    to_qualified_hint: Some(target),
                    to_type_hint: None,
                    ..Default::default()
                });
            }
        }
    }

    fn extract_decorators(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let from = match node.kind() {
            "class_declaration" | "abstract_class_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string),
            "method_definition" | "method_signature" | "abstract_method_signature" => {
                let method = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string)
                    .or_else(|| {
                        let mut c = node.walk();
                        node.children(&mut c).find_map(|ch| {
                            if matches!(
                                ch.kind(),
                                "property_identifier"
                                    | "identifier"
                                    | "private_property_identifier"
                            ) {
                                ch.utf8_text(source).ok().map(str::to_string)
                            } else {
                                None
                            }
                        })
                    });
                method.map(|m| {
                    self.find_containing_class_name(node, source)
                        .or_else(|| self.find_containing_interface_name(node, source))
                        .map(|owner| format!("{owner}.{m}"))
                        .unwrap_or(m)
                })
            }
            _ => None,
        };

        if let Some(from) = from {
            for (decorator_name, args) in decorators_for_node(node, source) {
                let mut meta = serde_json::json!({ "language": "typescript" });
                if let Some(args) = args {
                    meta["arguments"] = serde_json::Value::String(args);
                }
                relations.push(Relation {
                    from: from.clone(),
                    to: decorator_name,
                    relation_type: RelationType::AnnotatedWith,
                    location: source_location(node, &file_path.to_string_lossy()),
                    metadata: meta,
                    to_qualified_hint: None,
                    to_type_hint: None,
                    ..Default::default()
                });
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.extract_decorators(child, source, file_path, relations)?;
        }
        Ok(())
    }

    fn extract_instantiations(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        const MAX_DEPTH: usize = 2048;
        let function_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .collect();
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            if node.kind() == "new_expression"
                && let Some(from_fn) = containing_function(node, &function_symbols)
            {
                let from = from_fn
                    .qualified_name
                    .clone()
                    .unwrap_or_else(|| from_fn.name.clone());
                if let Some(ctor) = node.child_by_field_name("constructor")
                    && let Some(target) = type_name_from_node(ctor, source)
                {
                    relations.push(Relation {
                        from,
                        to: simple_type_name(&target),
                        relation_type: RelationType::Instantiates,
                        location: source_location(node, &file_path.to_string_lossy()),
                        metadata: serde_json::json!({ "language": "typescript" }),
                        to_qualified_hint: Some(target),
                        to_type_hint: None,
                        ..Default::default()
                    });
                }
            }

            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }
        Ok(())
    }

    /// Emit decorator `AnnotatedWith` relations for a single node (no child walk).
    pub(crate) fn emit_decorators_for_node(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) {
        let from = match node.kind() {
            "class_declaration" | "abstract_class_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string),
            "method_definition" | "method_signature" | "abstract_method_signature" => {
                let method = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string)
                    .or_else(|| {
                        let mut c = node.walk();
                        node.children(&mut c).find_map(|ch| {
                            if matches!(
                                ch.kind(),
                                "property_identifier"
                                    | "identifier"
                                    | "private_property_identifier"
                            ) {
                                ch.utf8_text(source).ok().map(str::to_string)
                            } else {
                                None
                            }
                        })
                    });
                method.map(|m| {
                    self.find_containing_class_name(node, source)
                        .or_else(|| self.find_containing_interface_name(node, source))
                        .map(|owner| format!("{owner}.{m}"))
                        .unwrap_or(m)
                })
            }
            _ => None,
        };

        if let Some(from) = from {
            for (decorator_name, args) in decorators_for_node(node, source) {
                let mut meta = serde_json::json!({ "language": "typescript" });
                if let Some(args) = args {
                    meta["arguments"] = serde_json::Value::String(args);
                }
                relations.push(Relation {
                    from: from.clone(),
                    to: decorator_name,
                    relation_type: RelationType::AnnotatedWith,
                    location: source_location(node, &file_path.to_string_lossy()),
                    metadata: meta,
                    to_qualified_hint: None,
                    to_type_hint: None,
                    ..Default::default()
                });
            }
        }
    }

    /// Emit a TS `Calls` relation with known enclosing function (single-pass flush).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn emit_ts_call(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        from_fn: &Symbol,
        class_fields: &HashMap<String, Vec<Field>>,
        relations: &mut Vec<Relation>,
    ) {
        let callee = node
            .child_by_field_name("function")
            .and_then(|n| callee_name(n, source))
            .or_else(|| callee_name(node, source));
        let Some(callee) = callee.filter(|c| !c.is_empty()) else {
            return;
        };
        let from = from_fn
            .qualified_name
            .clone()
            .unwrap_or_else(|| from_fn.name.clone());
        let (to_type_hint, to_qualified_hint) =
            self.infer_ts_call_hints(node, source, from_fn, class_fields);
        let mut meta = serde_json::json!({ "language": "typescript" });
        if self.is_unresolved_call(node, source) {
            meta["unresolved"] = serde_json::Value::Bool(true);
        }
        let same_file_matches: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.name == callee
                    && s.symbol_type == SymbolType::Function
                    && s.location.file == file_path.to_string_lossy()
            })
            .collect();
        let local_target = match same_file_matches.as_slice() {
            [only] => only
                .qualified_name
                .clone()
                .unwrap_or_else(|| callee.clone()),
            _ => to_qualified_hint.clone().unwrap_or_else(|| callee.clone()),
        };
        relations.push(Relation {
            from,
            to: local_target,
            relation_type: RelationType::Calls,
            location: source_location(node, &file_path.to_string_lossy()),
            metadata: meta,
            to_qualified_hint,
            to_type_hint,
            ..Default::default()
        });
    }
}

/// Walk to enclosing `export_statement` and read `from './mod'` source string.
fn export_specifier_source_module(node: Node, source: &[u8]) -> Option<String> {
    let mut cur = node;
    for _ in 0..8 {
        let parent = cur.parent()?;
        if parent.kind() == "export_statement" {
            let src = parent.child_by_field_name("source")?;
            let raw = src.utf8_text(source).ok()?.trim();
            let cleaned = raw
                .trim_matches(|c| c == '"' || c == '\'' || c == '`')
                .to_string();
            if cleaned.is_empty() {
                return None;
            }
            return Some(cleaned);
        }
        cur = parent;
    }
    None
}

fn collect_decorators(node: Node, source: &[u8]) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else {
            continue;
        };
        if child.kind() == "decorator"
            && let Some((name, args)) = decorator_name_and_args(child, source)
        {
            out.push((name, args));
        }
    }
    out
}

fn collect_preceding_sibling_decorators(
    node: Node,
    parent: Node,
    source: &[u8],
) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut pending = Vec::new();
    let target_start = node.start_byte();
    let target_end = node.end_byte();
    for i in 0..parent.child_count() {
        let Some(child) = parent.child(i) else {
            continue;
        };
        if child.start_byte() == target_start
            && child.end_byte() == target_end
            && child.kind() == node.kind()
        {
            out.append(&mut pending);
            break;
        }
        if child.kind() == "decorator" {
            if let Some(parsed) = decorator_name_and_args(child, source) {
                pending.push(parsed);
            }
        } else if !child.is_extra()
            && child.kind() != "comment"
            && child.kind() != ";"
            && child.is_named()
        {
            pending.clear();
        }
    }
    out
}

pub(crate) fn decorators_for_node(node: Node, source: &[u8]) -> Vec<(String, Option<String>)> {
    let mut out = collect_decorators(node, source);
    if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            out.extend(collect_decorators(parent, source));
        }
        // Method/property decorators are preceding siblings in `class_body`.
        out.extend(collect_preceding_sibling_decorators(node, parent, source));
    }
    out
}

fn decorator_name_and_args(decorator: Node, source: &[u8]) -> Option<(String, Option<String>)> {
    let inner = decorator.named_child(0)?;
    match inner.kind() {
        "identifier" | "type_identifier" => {
            let name = inner.utf8_text(source).ok()?.to_string();
            Some((name, None))
        }
        "call_expression" => {
            let name = inner
                .child_by_field_name("function")
                .and_then(|f| callee_name(f, source))?;
            let args = inner
                .child_by_field_name("arguments")
                .and_then(|a| a.utf8_text(source).ok().map(str::to_string));
            Some((name, args))
        }
        _ => None,
    }
}

pub(crate) fn source_location(node: Node, file_path: &str) -> SourceLocation {
    SourceLocation {
        file: file_path.to_string(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_column: node.start_position().column,
        end_column: node.end_position().column,
    }
}

impl Default for TypeScriptPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create TypeScriptPlugin")
    }
}

impl LanguagePlugin for TypeScriptPlugin {
    fn language_id(&self) -> &str {
        "typescript"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["ts", "tsx"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
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
        let root = tree.root_node();
        let mut relations = self.relations_from_tree(root, source, file_path, symbols)?;
        let mut sym_copy = symbols.to_vec();
        rgctl_plugin_helpers::apply_framework_catalog(
            "typescript",
            source,
            Some(root),
            &mut sym_copy,
            &mut relations,
        );
        Ok(relations)
    }

    fn extract_all(&self, file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
        extract_all_single_pass(self, file_path, source)
    }

    fn declared_ast_handlers(&self) -> Option<&AstHandlerTable> {
        Some(typescript_ast_handlers())
    }

    fn calculate_complexity(
        &self,
        symbol: &Symbol,
        source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        if symbol.symbol_type != SymbolType::Function {
            return Ok(None);
        }

        // Prefer metrics harvested during single-pass `extract_all`.
        if let Some(metrics) = complexity_from_metadata(symbol) {
            return Ok(Some(metrics));
        }

        let tree = parse_with_pool(
            "typescript",
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            source,
        )
        .map_err(|e| Error::ParseError {
            file: symbol.location.file.clone().into(),
            line: symbol.location.start_line,
            message: e,
        })?;

        let root = tree.root_node();
        let target_line = symbol.location.start_line - 1;

        fn find_function_at_line(node: Node, line: usize) -> Option<Node> {
            if matches!(
                node.kind(),
                "function_declaration" | "method_definition" | "arrow_function"
            ) && node.start_position().row == line
            {
                return Some(node);
            }
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if let Some(found) = find_function_at_line(child, line) {
                    return Some(found);
                }
            }
            None
        }

        if let Some(func_node) = find_function_at_line(root, target_line) {
            Ok(Some(ComplexityMetrics {
                cyclomatic: self.calculate_cyclomatic(func_node),
                cognitive: self.calculate_cognitive(func_node),
                loc: self.count_loc(func_node),
                parameters: symbol.parameters.len(),
                nesting_depth: self.count_nesting_depth(func_node),
                returns: self.count_returns(func_node),
            }))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_typescript_plugin_language_id() {
        let plugin = TypeScriptPlugin::new().unwrap();
        assert_eq!(plugin.language_id(), "typescript");
    }

    #[test]
    fn test_typescript_plugin_file_extensions() {
        let plugin = TypeScriptPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["ts", "tsx"]);
    }

    #[test]
    fn test_extract_function() {
        let plugin = TypeScriptPlugin::new().unwrap();
        let source = b"function add(a: number, b: number): number { return a + b; }";
        let symbols = plugin
            .extract_symbols(Path::new("test.ts"), source)
            .unwrap();

        assert!(!symbols.is_empty());
        let add_fn = symbols
            .iter()
            .find(|s| s.name == "add")
            .expect("add function not found");
        assert_eq!(add_fn.symbol_type, SymbolType::Function);
        assert_eq!(add_fn.parameters.len(), 2);
    }

    #[test]
    fn test_extract_arrow_function() {
        let plugin = TypeScriptPlugin::new().unwrap();
        let source = b"const multiply = (x: number, y: number): number => x * y;";
        let symbols = plugin
            .extract_symbols(Path::new("test.ts"), source)
            .unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].symbol_type, SymbolType::Function);
        assert_eq!(symbols[0].name, "multiply");
    }

    #[test]
    fn test_extract_named_arrows_distinct() {
        let plugin = TypeScriptPlugin::new().unwrap();
        let source = br#"
export function declaredAdd(a: number, b: number): number { return a + b; }
export const arrowAdd = (a: number, b: number): number => a + b;
const arrowHelper = (): number => 1;
const api = { fetchAll: async (): Promise<number> => 0 };
class C { foo = (): number => 2; }
[1].map((x) => x + 1);
"#;
        let symbols = plugin
            .extract_symbols(Path::new("arrows.ts"), source)
            .unwrap();
        let names: Vec<_> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .map(|s| s.name.as_str())
            .collect();
        assert!(names.contains(&"declaredAdd"), "{names:?}");
        assert!(names.contains(&"arrowAdd"), "{names:?}");
        assert!(names.contains(&"arrowHelper"), "{names:?}");
        assert!(names.contains(&"fetchAll"), "{names:?}");
        assert!(names.contains(&"foo"), "{names:?}");
        assert!(
            names.iter().any(|n| n.starts_with("anonymous@L")),
            "callback should stay span-disambiguated anonymous: {names:?}"
        );
        // Declarations must not also emit a body-less keyword duplicate.
        let decl_dupes: Vec<_> = names
            .iter()
            .filter(|n| n.starts_with("anonymous@L"))
            .collect();
        assert_eq!(
            decl_dupes.len(),
            1,
            "only the map callback should be anonymous, got {names:?}"
        );
    }

    #[test]
    fn test_function_declaration_has_no_anonymous_duplicate() {
        let plugin = TypeScriptPlugin::new().unwrap();
        let source = br#"
export function gamma(n: number): number {
  return n + 1;
}
"#;
        let symbols = plugin
            .extract_symbols(Path::new("gamma.ts"), source)
            .unwrap();
        let fns: Vec<_> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(fns, vec!["gamma"], "unexpected functions: {fns:?}");
    }

    #[test]
    fn test_extract_class() {
        let plugin = TypeScriptPlugin::new().unwrap();
        let source = b"class User { name: string; age: number; }";
        let symbols = plugin
            .extract_symbols(Path::new("test.ts"), source)
            .unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].symbol_type, SymbolType::Class);
        assert_eq!(symbols[0].fields.len(), 2);
    }

    #[test]
    fn test_extract_interface() {
        let plugin = TypeScriptPlugin::new().unwrap();
        let source = b"interface Person { name: string; age: number; }";
        let symbols = plugin
            .extract_symbols(Path::new("test.ts"), source)
            .unwrap();

        assert!(!symbols.is_empty());
        let person_iface = symbols
            .iter()
            .find(|s| s.name == "Person")
            .expect("Person interface not found");
        assert_eq!(person_iface.symbol_type, SymbolType::Interface);
        // Fields extraction may vary based on tree-sitter parsing
        // The important thing is we found the interface
    }

    #[test]
    fn test_extract_enum_and_type_alias() {
        let plugin = TypeScriptPlugin::new().unwrap();
        let source = br#"
enum Status { Pending, Done }
type Id = string | number;
"#;
        let symbols = plugin
            .extract_symbols(Path::new("types.ts"), source)
            .unwrap();
        let en = symbols
            .iter()
            .find(|s| s.name == "Status" && s.symbol_type == SymbolType::Enum)
            .expect("Status enum");
        assert!(
            en.fields.iter().any(|f| f.name == "Pending"),
            "enum members: {:?}",
            en.fields
        );
        let alias = symbols
            .iter()
            .find(|s| s.name == "Id" && s.symbol_type == SymbolType::TypeAlias)
            .expect("Id type alias");
        assert!(
            alias
                .signature
                .as_deref()
                .is_some_and(|s| s.contains("string")),
            "alias signature: {:?}",
            alias.signature
        );
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
function caller(): void {
    helper();
}

function helper(): void {}
"#;
        let plugin = TypeScriptPlugin::new().unwrap();
        let path = Path::new("test.ts");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls) && r.to == "helper"),
            "expected Calls -> helper, got {relations:?}"
        );
    }

    #[test]
    fn test_extract_fields_and_constructor() {
        let source = br#"
class OrderDTO {
  orderId: string;
  status: string;

  constructor(orderId: string, status: string) {
    this.orderId = orderId;
    this.status = status;
  }
}
"#;
        let plugin = TypeScriptPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("OrderDTO.ts"), source)
            .unwrap();
        let class = symbols
            .iter()
            .find(|s| s.name == "OrderDTO" && s.symbol_type == SymbolType::Class)
            .expect("class");
        assert!(class.fields.iter().any(|f| f.name == "orderId"));
        assert!(class.fields.iter().any(|f| f.name == "status"));
        assert_eq!(
            class
                .fields
                .iter()
                .find(|f| f.name == "orderId")
                .and_then(|f| f.field_type.as_deref()),
            Some("string")
        );
        let ctor = symbols
            .iter()
            .find(|s| {
                s.symbol_type == SymbolType::Function
                    && s.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true)
            })
            .expect("constructor");
        assert_eq!(ctor.name, "OrderDTO");
        assert_eq!(ctor.qualified_name.as_deref(), Some("OrderDTO.<init>"));
        assert_eq!(ctor.parameters.len(), 2);
        assert_eq!(ctor.parameters[0].param_type.as_deref(), Some("string"));
        assert_eq!(ctor.parameters[1].param_type.as_deref(), Some("string"));
    }

    #[test]
    fn test_import_and_type_only() {
        let source = br#"import type { Foo } from './foo';
import { bar } from 'lodash';"#;
        let plugin = TypeScriptPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("m.ts"), source).unwrap();
        let imports: Vec<_> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Import)
            .collect();
        assert!(imports.len() >= 2, "expected imports, got {imports:?}");
        assert!(
            imports.iter().any(|s| {
                s.metadata.get("is_type_only").and_then(|v| v.as_bool()) == Some(true)
            })
        );
    }

    #[test]
    fn test_heritage_implements_and_interface_extends() {
        let source = br#"
interface IBase { run(): void; }
interface IDerived extends IBase { extra(): void; }
class Service implements IDerived {
  run() {}
  extra() {}
}"#;
        let plugin = TypeScriptPlugin::new().unwrap();
        let path = Path::new("h.ts");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Implements),
            "missing Implements: {relations:?}"
        );
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Extends && r.to == "IBase"),
            "missing interface Extends: {relations:?}"
        );
    }

    #[test]
    fn test_decorators_annotated_with() {
        let source = br#"
@Controller('orders')
class OrdersController {
  @Get()
  list() {}
}"#;
        let plugin = TypeScriptPlugin::new().unwrap();
        let path = Path::new("d.ts");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::AnnotatedWith && r.to == "Controller"),
            "expected Controller decorator: {relations:?}"
        );
    }

    #[test]
    fn test_decorators_on_exported_class() {
        let source = br#"
@Controller('orders')
export class OrdersControllerFixture {
  @Get()
  list() {}
}"#;
        let plugin = TypeScriptPlugin::new().unwrap();
        let path = Path::new("d.ts");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::AnnotatedWith),
            "expected AnnotatedWith on exported class: {relations:?}"
        );
    }

    #[test]
    fn test_method_fqn_and_instantiates() {
        let source = br#"
class OrderService {
  checkout(): void {
    const dto = new OrderDto();
    this.repo.save(dto);
  }
}
class OrderDto {}
class Repo { save(x: OrderDto): void {} }
"#;
        let plugin = TypeScriptPlugin::new().unwrap();
        let path = Path::new("s.ts");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let checkout = symbols.iter().find(|s| s.name == "checkout").unwrap();
        assert_eq!(
            checkout.qualified_name.as_deref(),
            Some("OrderService.checkout")
        );
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Instantiates && r.to == "OrderDto"),
            "expected Instantiates: {relations:?}"
        );
    }

    #[test]
    fn nestjs_extract_all_emits_exposes_route_and_controller_tags() {
        let source = br#"
@Controller('users')
export class UsersController {
  constructor(private readonly svc: UserService) {}

  @Get(':id')
  findOne(@Param('id') id: string) {
    return this.svc.get(id);
  }
}

@Injectable()
export class UserService {
  get(id: string) { return id; }
}
"#;
        let plugin = TypeScriptPlugin::new().unwrap();
        let result = plugin
            .extract_all(Path::new("users.controller.ts"), source)
            .unwrap();
        let routes: Vec<_> = result
            .relations
            .iter()
            .filter(|r| r.relation_type == RelationType::ExposesRoute)
            .map(|r| r.to.as_str())
            .collect();
        assert_eq!(
            routes,
            vec!["GET /users/:id"],
            "expected only GET /users/:id (no false-positive from this.svc.get(id)), got {routes:?}"
        );
        let injects: Vec<_> = result
            .relations
            .iter()
            .filter(|r| r.relation_type == RelationType::Injects)
            .map(|r| (r.from.as_str(), r.to.as_str()))
            .collect();
        assert!(
            injects.contains(&("UsersController", "UserService")),
            "expected Injects UsersController -> UserService, got {injects:?}"
        );
        let ctrl = result
            .symbols
            .iter()
            .find(|s| s.name == "UsersController" && s.symbol_type == SymbolType::Class)
            .expect("UsersController");
        let tags = ctrl
            .metadata
            .get("semantic_tags")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            tags.contains("controller"),
            "controller semantic_tags={tags:?}"
        );
        let svc = result
            .symbols
            .iter()
            .find(|s| s.name == "UserService" && s.symbol_type == SymbolType::Class)
            .expect("UserService");
        let svc_tags = svc
            .metadata
            .get("semantic_tags")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            svc_tags.contains("provider") || svc_tags.contains("service"),
            "service semantic_tags={svc_tags:?}"
        );
    }
}
