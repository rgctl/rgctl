//! PHP language plugin using Tree-sitter.

use crate::single_pass::{extract_all_single_pass, php_ast_handlers};
use rgctl_plugin_api::{
    PHP_CALL_KINDS, callee_name, complexity_from_metadata, containing_function, parse_with_pool, *,
};
use rgctl_plugin_helpers::ComplexityCalculator;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::{Node, Parser};

pub const PHP_FUNCTION_KINDS: &[&str] = &[
    "function_definition",
    "method_declaration",
    "arrow_function",
    "anonymous_function",
];

pub(crate) const PHP_COMPLEXITY_BRANCH_KINDS: &[&str] = &[
    "if_statement",
    "else_if_clause",
    "while_statement",
    "for_statement",
    "foreach_statement",
    "switch_statement",
    "match_expression",
    "match_conditional_expression",
    "conditional_expression",
    "catch_clause",
];

/// PHP Tier 1 language plugin.
pub struct PhpPlugin {
    _parser: Parser,
}

pub(crate) fn php_grammar() -> tree_sitter::Language {
    if std::env::var("RGCTL_PHP_ONLY").is_ok() {
        tree_sitter_php::LANGUAGE_PHP_ONLY.into()
    } else {
        tree_sitter_php::LANGUAGE_PHP.into()
    }
}

impl PhpPlugin {
    /// Create a new PHP plugin.
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&php_grammar())
            .map_err(|e| Error::PluginError(format!("Failed to set PHP grammar: {e}")))?;
        Ok(Self { _parser: parser })
    }

    pub(crate) fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        parse_with_pool("php", php_grammar(), source).map_err(|e| Error::ParseError {
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
        let mut namespace = None;
        let file_path_str = file_path.to_string_lossy();
        self.traverse_symbols(root, source, &file_path_str, &mut namespace, &mut symbols)?;
        Ok(symbols)
    }

    fn traverse_symbols(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        namespace: &mut Option<String>,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        if node.kind() == "namespace_definition" {
            if let Some(ns) = node
                .child_by_field_name("name")
                .and_then(|n| normalize_namespace(n, source))
            {
                *namespace = Some(ns);
            }
        }

        match node.kind() {
            "function_definition" => {
                if let Some(sym) =
                    self.extract_function(node, source, file_path, namespace.as_deref())?
                {
                    symbols.push(sym);
                }
                self.extract_embedded_symbols(
                    node,
                    source,
                    file_path,
                    namespace.as_deref(),
                    symbols,
                )?;
            }
            "method_declaration" => {
                if self.find_enclosing_class_name(node, source).is_none() {
                    if let Some(sym) =
                        self.extract_method(node, source, file_path, namespace.as_deref(), None)?
                    {
                        symbols.push(sym);
                    }
                    self.extract_embedded_symbols(
                        node,
                        source,
                        file_path,
                        namespace.as_deref(),
                        symbols,
                    )?;
                }
            }
            "arrow_function" | "anonymous_function" => {
                if let Some(sym) =
                    self.extract_anonymous_function(node, source, file_path, namespace.as_deref())?
                {
                    symbols.push(sym);
                }
            }
            "class_declaration" => {
                symbols.push(self.extract_class(node, source, file_path, namespace.as_deref())?);
                self.traverse_class_members(
                    node,
                    source,
                    file_path,
                    namespace.as_deref(),
                    symbols,
                )?;
            }
            "interface_declaration" => {
                symbols.push(self.extract_interface(
                    node,
                    source,
                    file_path,
                    namespace.as_deref(),
                )?);
            }
            "trait_declaration" => {
                symbols.push(self.extract_trait(node, source, file_path, namespace.as_deref())?);
            }
            "enum_declaration" => {
                symbols.push(self.extract_enum(node, source, file_path, namespace.as_deref())?);
            }
            "namespace_use_declaration" => {
                self.extract_namespace_imports(node, source, file_path, symbols)?;
            }
            _ => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    self.traverse_symbols(child, source, file_path, namespace, symbols)?;
                }
            }
        }
        Ok(())
    }

    fn traverse_class_members(
        &self,
        class_node: Node,
        source: &[u8],
        file_path: &str,
        namespace: Option<&str>,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        let class_name = type_name(class_node, source).unwrap_or_default();
        let qclass = qualify(namespace, &class_name);
        let body = class_node
            .child_by_field_name("body")
            .or_else(|| find_child_kind(class_node, "declaration_list"));
        let Some(body) = body else {
            return Ok(());
        };

        let mut cursor = body.walk();
        for child in body.children(&mut cursor) {
            if child.kind() == "method_declaration" {
                if let Some(sym) =
                    self.extract_method(child, source, file_path, namespace, Some(qclass.as_str()))?
                {
                    symbols.push(sym);
                }
                self.extract_embedded_symbols(child, source, file_path, namespace, symbols)?;
            }
        }
        Ok(())
    }

    pub(crate) fn extract_function(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        namespace: Option<&str>,
    ) -> Result<Option<Symbol>> {
        let name = node_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Function missing name".to_string(),
        })?;
        let parameters = self.extract_parameters(node, source)?;
        let return_type = node
            .child_by_field_name("return_type")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);
        let qualified_name = qualify(namespace, &name);
        let metadata = php_metadata(node, source);
        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name: Some(qualified_name),
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type,
            parameters,
            fields: vec![],
            modifiers: visibility_modifiers(node, source),
            documentation: None,
            metadata,
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(Some(symbol))
    }

    pub(crate) fn extract_method(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        namespace: Option<&str>,
        enclosing_class: Option<&str>,
    ) -> Result<Option<Symbol>> {
        let name = node_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Method missing name".to_string(),
        })?;

        let class_name = enclosing_class
            .map(str::to_string)
            .or_else(|| self.find_enclosing_class_name(node, source));
        let parameters = self.extract_parameters(node, source)?;
        let return_type = node
            .child_by_field_name("return_type")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);

        let is_constructor = name == "__construct";
        let qualified_name = if is_constructor {
            class_name
                .as_ref()
                .map(|c| format!("{c}.<init>"))
                .or_else(|| Some(format!("{}.<init>", name)))
        } else {
            class_name
                .as_ref()
                .map(|c| format!("{c}.{name}"))
                .or_else(|| Some(qualify(namespace, &name)))
        };

        let mut metadata = php_metadata(node, source);
        if is_constructor {
            metadata["is_constructor"] = serde_json::json!(true);
        }

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type,
            parameters,
            fields: vec![],
            modifiers: visibility_modifiers(node, source),
            documentation: None,
            metadata,
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(Some(symbol))
    }

    pub(crate) fn extract_anonymous_function(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        namespace: Option<&str>,
    ) -> Result<Option<Symbol>> {
        let line = node.start_position().row + 1;
        let name = if node.kind() == "arrow_function" {
            format!("$arrow${line}")
        } else {
            format!("$anonymous${line}")
        };
        let parameters = self.extract_parameters(node, source)?;
        let return_type = node
            .child_by_field_name("return_type")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);
        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name: Some(qualify(namespace, &name)),
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type,
            parameters,
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "php", "is_anonymous": true }),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, None);
        Ok(Some(symbol))
    }

    pub(crate) fn extract_class(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        namespace: Option<&str>,
    ) -> Result<Symbol> {
        let name = type_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Class missing name".to_string(),
        })?;
        let fields = self.extract_type_fields(node, source)?;
        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Class,
            qualified_name: Some(qualify(namespace, &name)),
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields,
            modifiers: visibility_modifiers(node, source),
            documentation: None,
            metadata: php_metadata(node, source),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(symbol)
    }

    pub(crate) fn extract_interface(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        namespace: Option<&str>,
    ) -> Result<Symbol> {
        let name = type_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Interface missing name".to_string(),
        })?;
        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Interface,
            qualified_name: Some(qualify(namespace, &name)),
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: visibility_modifiers(node, source),
            documentation: None,
            metadata: serde_json::json!({ "language": "php" }),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(symbol)
    }

    pub(crate) fn extract_trait(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        namespace: Option<&str>,
    ) -> Result<Symbol> {
        let name = type_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Trait missing name".to_string(),
        })?;
        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Class,
            qualified_name: Some(qualify(namespace, &name)),
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: visibility_modifiers(node, source),
            documentation: None,
            metadata: serde_json::json!({ "language": "php", "is_trait": true }),
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
        namespace: Option<&str>,
    ) -> Result<Symbol> {
        let name = type_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Enum missing name".to_string(),
        })?;
        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Enum,
            qualified_name: Some(qualify(namespace, &name)),
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: visibility_modifiers(node, source),
            documentation: None,
            metadata: serde_json::json!({ "language": "php" }),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            node,
            node.child_by_field_name("name"),
        );
        Ok(symbol)
    }

    fn extract_type_fields(&self, type_node: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let body = type_node
            .child_by_field_name("body")
            .or_else(|| find_child_kind(type_node, "declaration_list"));
        let Some(body) = body else {
            return Ok(fields);
        };

        let mut body_cursor = body.walk();
        for child in body.children(&mut body_cursor) {
            if child.kind() == "property_declaration" {
                let field_type = child
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                let visibility = visibility_modifiers(child, source).join(" ");
                let vis = if visibility.is_empty() {
                    None
                } else {
                    Some(visibility)
                };
                let mut elem_cursor = child.walk();
                for elem in child.children(&mut elem_cursor) {
                    if elem.kind() != "property_element" {
                        continue;
                    }
                    if let Some(name_node) = elem.child_by_field_name("name") {
                        if let Some(name) = variable_text(name_node, source) {
                            let mut visibility = vis.clone();
                            if find_child_kind(elem, "property_hook_list").is_some() {
                                visibility = visibility.map(|v| format!("{v} hooks"));
                            }
                            fields.push(Field {
                                name,
                                field_type: field_type.clone(),
                                visibility,
                            });
                        }
                    }
                }
            } else if child.kind() == "const_declaration" {
                let visibility = visibility_modifiers(child, source).join(" ");
                let vis = if visibility.is_empty() {
                    Some("const".to_string())
                } else {
                    Some(format!("{visibility} const"))
                };
                let mut elem_cursor = child.walk();
                for elem in child.children(&mut elem_cursor) {
                    if elem.kind() != "const_element" {
                        continue;
                    }
                    if let Some(name_node) =
                        elem.children(&mut elem.walk()).find(|c| c.kind() == "name")
                    {
                        if let Ok(name) = name_node.utf8_text(source) {
                            fields.push(Field {
                                name: name.to_string(),
                                field_type: None,
                                visibility: vis.clone(),
                            });
                        }
                    }
                }
            }
        }

        let body = type_node
            .child_by_field_name("body")
            .or_else(|| find_child_kind(type_node, "declaration_list"));
        if let Some(body) = body {
            let mut body_cursor = body.walk();
            for child in body.children(&mut body_cursor) {
                if child.kind() == "method_declaration" {
                    if node_name(child, source).as_deref() == Some("__construct") {
                        if let Some(params) = child.child_by_field_name("parameters") {
                            self.collect_promoted_fields(params, source, &mut fields)?;
                        }
                    }
                }
            }
        }

        Ok(fields)
    }

    fn collect_promoted_fields(
        &self,
        params_node: Node,
        source: &[u8],
        fields: &mut Vec<Field>,
    ) -> Result<()> {
        let mut cursor = params_node.walk();
        for child in params_node.children(&mut cursor) {
            if child.kind() != "property_promotion_parameter" {
                continue;
            }
            let name = child
                .child_by_field_name("name")
                .and_then(|n| variable_text(n, source));
            let field_type = child
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string);
            let visibility = visibility_modifiers(child, source).join(" ");
            if let Some(name) = name {
                fields.push(Field {
                    name,
                    field_type,
                    visibility: if visibility.is_empty() {
                        None
                    } else {
                        Some(visibility)
                    },
                });
            }
        }
        Ok(())
    }

    fn extract_parameters(&self, node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut parameters = Vec::new();
        let Some(params_node) = node.child_by_field_name("parameters") else {
            return Ok(parameters);
        };

        let mut cursor = params_node.walk();
        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "simple_parameter"
                | "property_promotion_parameter"
                | "optional_parameter"
                | "variadic_parameter" => {
                    let name = child
                        .child_by_field_name("name")
                        .and_then(|n| variable_text(n, source));
                    let param_type = child
                        .child_by_field_name("type")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(str::to_string);
                    let default_value = child
                        .child_by_field_name("default_value")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(str::to_string);
                    if let Some(name) = name {
                        parameters.push(Parameter {
                            name,
                            param_type,
                            default_value,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(parameters)
    }

    pub(crate) fn find_enclosing_class_name(&self, node: Node, source: &[u8]) -> Option<String> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "class_declaration" {
                return type_name(parent, source);
            }
            current = parent;
        }
        None
    }

    fn relations_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let mut relations = Vec::new();
        let import_map = import_map_from_symbols(symbols);
        walk_php_calls(
            root,
            source,
            file_path,
            symbols,
            &import_map,
            &mut relations,
        );
        self.extract_inheritance(root, source, file_path, None, &mut relations)?;
        Ok(relations)
    }

    pub(crate) fn extract_inheritance(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        namespace: Option<&str>,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        if node.kind() == "class_declaration" {
            let name = type_name(node, source).unwrap_or_default();
            if !name.is_empty() {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "base_clause" => {
                            for base in type_refs(child, source) {
                                relations.push(relation(
                                    &name,
                                    &base,
                                    RelationType::Extends,
                                    child,
                                    file_path,
                                ));
                            }
                        }
                        "class_interface_clause" => {
                            for iface in type_refs(child, source) {
                                relations.push(relation(
                                    &name,
                                    &iface,
                                    RelationType::Implements,
                                    child,
                                    file_path,
                                ));
                            }
                        }
                        _ => {}
                    }
                }
                if let Some(body) = node
                    .child_by_field_name("body")
                    .or_else(|| find_child_kind(node, "declaration_list"))
                {
                    let mut body_cursor = body.walk();
                    for child in body.children(&mut body_cursor) {
                        if child.kind() == "use_declaration" {
                            for trait_name in trait_names_from_use(child, source) {
                                let resolved = qualify(namespace, &trait_name);
                                relations.push(relation(
                                    &name,
                                    &resolved,
                                    RelationType::Uses,
                                    child,
                                    file_path,
                                ));
                            }
                        }
                    }
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let child_ns = if child.kind() == "namespace_definition" {
                child
                    .child_by_field_name("name")
                    .and_then(|n| normalize_namespace(n, source))
                    .or_else(|| namespace.map(str::to_string))
            } else {
                namespace.map(str::to_string)
            };
            self.extract_inheritance(child, source, file_path, child_ns.as_deref(), relations)?;
        }
        Ok(())
    }

    /// Top-level / class `const FOO = 1;` → Variable symbols (top-level) or Field names.
    pub(crate) fn extract_const_elements(
        &self,
        node: Node,
        source: &[u8],
    ) -> Result<Vec<(String, Option<String>)>> {
        if node.kind() != "const_declaration" {
            return Ok(Vec::new());
        }
        let visibility = visibility_modifiers(node, source).join(" ");
        let vis = if visibility.is_empty() {
            Some("const".to_string())
        } else {
            Some(format!("{visibility} const"))
        };
        let mut out = Vec::new();
        let mut cursor = node.walk();
        for elem in node.children(&mut cursor) {
            if elem.kind() != "const_element" {
                continue;
            }
            if let Some(name_node) = elem.children(&mut elem.walk()).find(|c| c.kind() == "name") {
                let name = name_node.utf8_text(source)?.to_string();
                out.push((name, vis.clone()));
            }
        }
        Ok(out)
    }

    /// Emit top-level `const` as Variable symbols.
    pub(crate) fn extract_const(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        namespace: Option<&str>,
    ) -> Result<Vec<Symbol>> {
        let mut symbols = Vec::new();
        for (name, _) in self.extract_const_elements(node, source)? {
            let qn = qualify(namespace, &name);
            let name_node = {
                let mut found = None;
                let mut cursor = node.walk();
                for elem in node.children(&mut cursor) {
                    if elem.kind() != "const_element" {
                        continue;
                    }
                    if let Some(n) = elem.children(&mut elem.walk()).find(|c| c.kind() == "name") {
                        if n.utf8_text(source).ok() == Some(name.as_str()) {
                            found = Some(n);
                            break;
                        }
                    }
                }
                found
            };
            let mut symbol = Symbol {
                name: name.clone(),
                symbol_type: SymbolType::Variable,
                qualified_name: Some(qn),
                location: source_location(node, file_path),
                signature: node.utf8_text(source).ok().map(|s| s.trim().to_string()),
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: visibility_modifiers(node, source),
                documentation: None,
                metadata: serde_json::json!({
                    "language": "php",
                    "kind": "const_declaration",
                }),
                ..Default::default()
            };
            rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, name_node);
            symbols.push(symbol);
        }
        Ok(symbols)
    }

    /// Append class-constant fields onto an existing class/interface/trait/enum symbol.
    pub(crate) fn append_const_fields(
        &self,
        node: Node,
        source: &[u8],
        class_sym: &mut Symbol,
    ) -> Result<()> {
        for (name, vis) in self.extract_const_elements(node, source)? {
            if class_sym.fields.iter().any(|f| f.name == name) {
                continue;
            }
            class_sym.fields.push(Field {
                name,
                field_type: None,
                visibility: vis,
            });
        }
        Ok(())
    }

    pub(crate) fn extract_namespace_imports(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        let prefix =
            find_child_kind(node, "namespace_name").and_then(|n| normalize_namespace(n, source));
        if let Some(body) = node.child_by_field_name("body") {
            if body.kind() == "namespace_use_group" {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if child.kind() == "namespace_use_clause" {
                        push_import_symbol(child, source, file_path, prefix.as_deref(), symbols);
                    }
                }
                return Ok(());
            }
        }
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == "namespace_use_clause" {
                push_import_symbol(child, source, file_path, None, symbols);
            }
        }
        Ok(())
    }

    pub(crate) fn extract_embedded_symbols(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        namespace: Option<&str>,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        let owner = enclosing_owner_name(node, symbols);
        walk_anonymous_classes(
            self,
            node,
            source,
            file_path,
            namespace,
            owner.as_deref(),
            symbols,
        );
        Ok(())
    }

    /// Emit depth relations for include/require, `new`, member/scoped access, and attributes.
    pub(crate) fn collect_depth_relation_at(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        from_symbol: Option<&str>,
        relations: &mut Vec<Relation>,
    ) {
        let from = from_symbol
            .map(str::to_string)
            .unwrap_or_else(|| file_path.to_string());
        match node.kind() {
            "include_expression"
            | "include_once_expression"
            | "require_expression"
            | "require_once_expression" => {
                let Some(to) = include_path_text(node, source) else {
                    return;
                };
                relations.push(Relation {
                    from,
                    to,
                    relation_type: RelationType::DependsOn,
                    location: source_location(node, file_path),
                    metadata: serde_json::json!({
                        "language": "php",
                        "kind": node.kind(),
                    }),
                    to_qualified_hint: None,
                    to_type_hint: None,
                    ..Default::default()
                });
            }
            "object_creation_expression" => {
                let Some(to) = object_creation_type_name(node, source) else {
                    return;
                };
                relations.push(Relation {
                    from,
                    to,
                    relation_type: RelationType::Instantiates,
                    location: source_location(node, file_path),
                    metadata: serde_json::json!({ "language": "php" }),
                    to_qualified_hint: None,
                    to_type_hint: None,
                    ..Default::default()
                });
            }
            "member_access_expression"
            | "nullsafe_member_access_expression"
            | "scoped_property_access_expression"
            | "class_constant_access_expression" => {
                let Some(to) = access_member_name(node, source) else {
                    return;
                };
                relations.push(Relation {
                    from,
                    to,
                    relation_type: RelationType::Uses,
                    location: source_location(node, file_path),
                    metadata: serde_json::json!({
                        "language": "php",
                        "kind": node.kind(),
                    }),
                    to_qualified_hint: None,
                    to_type_hint: None,
                    ..Default::default()
                });
            }
            "attribute" => {
                let mut attrs = Vec::new();
                push_attribute_text(node, source, &mut attrs);
                for attr in attrs {
                    relations.push(Relation {
                        from: from.clone(),
                        to: attr,
                        relation_type: RelationType::AnnotatedWith,
                        location: source_location(node, file_path),
                        metadata: serde_json::json!({ "language": "php" }),
                        to_qualified_hint: None,
                        to_type_hint: None,
                        ..Default::default()
                    });
                }
            }
            "attribute_group" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "attribute" {
                        self.collect_depth_relation_at(
                            child,
                            source,
                            file_path,
                            from_symbol,
                            relations,
                        );
                    }
                }
            }
            "namespace_use_clause" | "use_as_clause" | "use_instead_of_clause" => {
                let Ok(to) = node.utf8_text(source) else {
                    return;
                };
                let to = to.trim();
                if to.is_empty() {
                    return;
                }
                relations.push(Relation {
                    from,
                    to: to.to_string(),
                    relation_type: RelationType::Uses,
                    location: source_location(node, file_path),
                    metadata: serde_json::json!({
                        "language": "php",
                        "kind": node.kind(),
                    }),
                    to_qualified_hint: None,
                    to_type_hint: None,
                    ..Default::default()
                });
            }
            _ => {}
        }
    }
}

impl Default for PhpPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create PhpPlugin")
    }
}

impl LanguagePlugin for PhpPlugin {
    fn language_id(&self) -> &str {
        "php"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["php"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(php_grammar())
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
        Some(php_ast_handlers())
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
        let kinds: Vec<&str> = PHP_FUNCTION_KINDS.to_vec();

        if let Some(func_node) = find_node_at_line(tree.root_node(), target_line, &kinds) {
            Ok(Some(ComplexityMetrics {
                cyclomatic: ComplexityCalculator::cyclomatic(
                    func_node,
                    PHP_COMPLEXITY_BRANCH_KINDS,
                ),
                cognitive: ComplexityCalculator::cognitive(func_node, PHP_COMPLEXITY_BRANCH_KINDS),
                loc: ComplexityCalculator::loc(func_node),
                parameters: symbol.parameters.len(),
                nesting_depth: ComplexityCalculator::nesting_depth(
                    func_node,
                    &["compound_statement", "declaration_list"],
                ),
                returns: ComplexityCalculator::return_count(func_node, "return_statement"),
            }))
        } else {
            Ok(None)
        }
    }
}

fn find_node_at_line<'a>(node: Node<'a>, line: usize, kinds: &[&str]) -> Option<Node<'a>> {
    if kinds.contains(&node.kind()) && node.start_position().row == line {
        return Some(node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_node_at_line(child, line, kinds) {
            return Some(found);
        }
    }
    None
}

fn qualify(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(ns) if !ns.is_empty() => format!("{ns}\\{name}"),
        _ => name.to_string(),
    }
}

pub(crate) fn normalize_namespace(node: Node, source: &[u8]) -> Option<String> {
    let text = node.utf8_text(source).ok()?;
    Some(text.trim().trim_start_matches('\\').to_string())
}

fn type_name(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.trim_start_matches('\\').to_string())
}

fn node_name(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string)
}

fn variable_text(node: Node, source: &[u8]) -> Option<String> {
    node.utf8_text(source)
        .ok()
        .map(|s| s.trim_start_matches('$').to_string())
}

fn visibility_modifiers(node: Node, source: &[u8]) -> Vec<String> {
    let mut mods = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "visibility_modifier"
            || child.kind() == "static_modifier"
            || child.kind() == "abstract_modifier"
            || child.kind() == "final_modifier"
            || child.kind() == "readonly_modifier"
        {
            if let Ok(text) = child.utf8_text(source) {
                mods.push(text.to_string());
            }
        }
    }
    mods
}

fn type_refs(node: Node, source: &[u8]) -> Vec<String> {
    let mut refs = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "name" | "qualified_name" | "relative_name") {
            if let Ok(text) = child.utf8_text(source) {
                refs.push(text.trim_start_matches('\\').to_string());
            }
        }
    }
    refs
}

fn find_child_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor).find(|c| c.kind() == kind)
}

fn source_location(node: Node, file_path: &str) -> SourceLocation {
    SourceLocation {
        file: file_path.to_string(),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        start_column: node.start_position().column,
        end_column: node.end_position().column,
    }
}

fn first_line(node: Node, source: &[u8]) -> String {
    node.utf8_text(source)
        .ok()
        .and_then(|s| s.lines().next())
        .unwrap_or("")
        .trim()
        .to_string()
}

fn relation(
    from: &str,
    to: &str,
    relation_type: RelationType,
    node: Node,
    file_path: &Path,
) -> Relation {
    Relation {
        from: from.to_string(),
        to: to.to_string(),
        relation_type,
        location: SourceLocation {
            file: file_path.to_string_lossy().to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_column: node.start_position().column,
            end_column: node.end_position().column,
        },
        metadata: serde_json::json!({}),
        to_qualified_hint: None,
        to_type_hint: None,
        ..Default::default()
    }
}

fn php_metadata(node: Node, source: &[u8]) -> serde_json::Value {
    let mut meta = serde_json::json!({ "language": "php" });
    let attrs = collect_attributes(node, source);
    if !attrs.is_empty() {
        meta["attributes"] = serde_json::json!(attrs);
    }
    meta
}

fn collect_attributes(node: Node, source: &[u8]) -> Vec<String> {
    let mut attrs = Vec::new();
    if let Some(list) = node.child_by_field_name("attributes") {
        collect_attributes_from_list(list, source, &mut attrs);
    }
    attrs
}

fn collect_attributes_from_list(list: Node, source: &[u8], attrs: &mut Vec<String>) {
    let mut cursor = list.walk();
    for child in list.children(&mut cursor) {
        match child.kind() {
            "attribute_group" => {
                let mut gc = child.walk();
                for attr in child.children(&mut gc) {
                    if attr.kind() == "attribute" {
                        push_attribute_text(attr, source, attrs);
                    }
                }
            }
            "attribute" => push_attribute_text(child, source, attrs),
            _ => {}
        }
    }
}

fn include_path_text(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("argument")
        .or_else(|| node.child_by_field_name("path"))
        .or_else(|| {
            let mut cursor = node.walk();
            node.children(&mut cursor).find(|c| {
                c.is_named()
                    && matches!(
                        c.kind(),
                        "string"
                            | "encapsed_string"
                            | "name"
                            | "variable_name"
                            | "binary_expression"
                    )
            })
        })
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.trim_matches(|c| c == '"' || c == '\'').to_string())
        .filter(|s| !s.is_empty())
}

fn object_creation_type_name(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .or_else(|| {
            let mut cursor = node.walk();
            node.children(&mut cursor).find(|c| {
                c.is_named()
                    && matches!(
                        c.kind(),
                        "name" | "qualified_name" | "relative_name" | "variable_name"
                    )
            })
        })
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.trim_start_matches('\\').to_string())
        .filter(|s| !s.is_empty() && s != "class")
}

fn access_member_name(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .or_else(|| node.child_by_field_name("member"))
        .or_else(|| {
            let mut cursor = node.walk();
            node.children(&mut cursor).find(|c| {
                c.is_named()
                    && matches!(c.kind(), "name" | "variable_name" | "dynamic_variable_name")
            })
        })
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.trim_start_matches('$').to_string())
        .filter(|s| !s.is_empty())
}

fn push_attribute_text(attr: Node, source: &[u8], attrs: &mut Vec<String>) {
    let name = attr
        .child_by_field_name("name")
        .or_else(|| {
            attr.children(&mut attr.walk())
                .find(|c| matches!(c.kind(), "name" | "qualified_name" | "relative_name"))
        })
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.trim_start_matches('\\').to_string());
    if let Some(name) = name {
        if let Some(params) = attr.child_by_field_name("parameters") {
            if let Ok(args) = params.utf8_text(source) {
                attrs.push(format!("{name}{args}"));
                return;
            }
        }
        attrs.push(name);
    }
}

pub(crate) fn import_map_from_symbols(symbols: &[Symbol]) -> HashMap<String, String> {
    symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Import)
        .filter_map(|s| {
            let qn = s.qualified_name.as_ref()?;
            Some((s.name.clone(), qn.clone()))
        })
        .collect()
}

fn push_import_symbol(
    clause: Node,
    source: &[u8],
    file_path: &str,
    prefix: Option<&str>,
    symbols: &mut Vec<Symbol>,
) {
    let Some((local, qualified)) = clause_import_names(clause, source, prefix) else {
        return;
    };
    let mut symbol = Symbol {
        name: local,
        symbol_type: SymbolType::Import,
        qualified_name: Some(qualified),
        location: source_location(clause, file_path),
        signature: Some(first_line(clause, source)),
        return_type: None,
        parameters: vec![],
        fields: vec![],
        modifiers: vec![],
        documentation: None,
        metadata: serde_json::json!({ "language": "php", "kind": "import" }),
        ..Default::default()
    };
    rgctl_plugin_helpers::record_symbol_spans(
        &mut symbol,
        clause,
        clause.child_by_field_name("alias"),
    );
    symbols.push(symbol);
}

fn clause_import_names(
    clause: Node,
    source: &[u8],
    prefix: Option<&str>,
) -> Option<(String, String)> {
    let alias = clause
        .child_by_field_name("alias")
        .and_then(|n| n.utf8_text(source).ok())
        .map(str::to_string);
    let base = clause
        .children(&mut clause.walk())
        .find(|c| matches!(c.kind(), "name" | "qualified_name" | "relative_name"))
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.trim_start_matches('\\').to_string())?;
    let qualified = match prefix {
        Some(p) if !base.contains('\\') => format!("{p}\\{base}"),
        _ => base.clone(),
    };
    let local = alias.unwrap_or_else(|| {
        qualified
            .rsplit('\\')
            .next()
            .unwrap_or(&qualified)
            .to_string()
    });
    Some((local, qualified))
}

fn trait_names_from_use(node: Node, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "name" | "qualified_name" | "relative_name" => {
                if let Ok(text) = child.utf8_text(source) {
                    names.push(text.trim_start_matches('\\').to_string());
                }
            }
            "use_list" => {
                let mut lc = child.walk();
                for item in child.children(&mut lc) {
                    if item.kind() == "use_as_clause" {
                        if let Some(n) = item
                            .children(&mut item.walk())
                            .find(|c| matches!(c.kind(), "name" | "qualified_name"))
                        {
                            if let Ok(text) = n.utf8_text(source) {
                                names.push(text.trim_start_matches('\\').to_string());
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    names
}

fn enclosing_owner_name(node: Node, symbols: &[Symbol]) -> Option<String> {
    let line = node.start_position().row + 1;
    symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Function)
        .filter(|s| line >= s.location.start_line && line <= s.location.end_line)
        .min_by_key(|s| s.location.end_line - s.location.start_line)
        .and_then(|s| s.qualified_name.clone())
}

fn walk_anonymous_classes(
    plugin: &PhpPlugin,
    root: Node,
    source: &[u8],
    file_path: &str,
    namespace: Option<&str>,
    owner: Option<&str>,
    symbols: &mut Vec<Symbol>,
) {
    const MAX_DEPTH: usize = 2048;
    let mut stack = vec![(root, 0usize)];
    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            continue;
        }
        if node.kind() == "anonymous_class" {
            let line = node.start_position().row + 1;
            let anon_name = format!("$Anonymous{line}");
            let qualified = owner
                .map(|o| format!("{o}.{anon_name}"))
                .unwrap_or_else(|| qualify(namespace, &anon_name));
            let mut symbol = Symbol {
                name: anon_name.clone(),
                symbol_type: SymbolType::Class,
                qualified_name: Some(qualified.clone()),
                location: source_location(node, file_path),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: serde_json::json!({
                    "language": "php",
                    "is_anonymous": true,
                    "owner": owner,
                }),
                ..Default::default()
            };
            rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, None);
            symbols.push(symbol);
            if let Some(body) = node
                .child_by_field_name("body")
                .or_else(|| find_child_kind(node, "declaration_list"))
            {
                let mut cursor = body.walk();
                for child in body.children(&mut cursor) {
                    if child.kind() == "method_declaration" {
                        if let Ok(Some(method)) = plugin.extract_method(
                            child,
                            source,
                            file_path,
                            namespace,
                            Some(qualified.as_str()),
                        ) {
                            symbols.push(method);
                        }
                    }
                }
            }
        }
        for i in (0..node.child_count()).rev() {
            if let Some(child) = node.child(i) {
                stack.push((child, depth + 1));
            }
        }
    }
}

fn walk_php_calls(
    root: Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[Symbol],
    import_map: &HashMap<String, String>,
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
            continue;
        }
        if PHP_CALL_KINDS.contains(&node.kind()) {
            push_php_call_relation(
                node,
                source,
                file_path,
                symbols,
                &function_symbols,
                import_map,
                relations,
            );
        }
        for i in (0..node.child_count()).rev() {
            if let Some(child) = node.child(i) {
                stack.push((child, depth + 1));
            }
        }
    }
}

pub(crate) fn push_php_call_relation(
    node: Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[Symbol],
    function_symbols: &[&Symbol],
    import_map: &HashMap<String, String>,
    relations: &mut Vec<Relation>,
) {
    let Some((callee, unresolved, scope_class)) = php_call_target(node, source) else {
        return;
    };
    if callee.is_empty() {
        return;
    }
    let Some(from_fn) = containing_function(node, function_symbols) else {
        return;
    };
    let from = from_fn
        .qualified_name
        .clone()
        .unwrap_or_else(|| from_fn.name.clone());

    let to_qualified_hint = scope_class
        .as_ref()
        .and_then(|cls| import_map.get(cls).map(|fqn| format!("{fqn}.{callee}")));

    let mut meta = serde_json::json!({ "language": "php" });
    if unresolved {
        meta["unresolved"] = serde_json::json!(true);
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
        location: SourceLocation {
            file: file_path.to_string_lossy().to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_column: node.start_position().column,
            end_column: node.end_position().column,
        },
        metadata: meta,
        to_qualified_hint,
        to_type_hint: None,
        ..Default::default()
    });
}

fn php_call_target(node: Node, source: &[u8]) -> Option<(String, bool, Option<String>)> {
    match node.kind() {
        "scoped_call_expression" => {
            let scope = node.child_by_field_name("scope")?;
            let name_node = node.child_by_field_name("name")?;
            let class = type_text(scope, source)?;
            let (method, unresolved) = method_name_from_node(name_node, source);
            Some((method, unresolved, Some(class)))
        }
        "member_call_expression" | "nullsafe_member_call_expression" => {
            let name_node = node.child_by_field_name("name")?;
            let (method, unresolved) = method_name_from_node(name_node, source);
            let target = if unresolved {
                format!("${method}")
            } else {
                method
            };
            Some((target, unresolved, None))
        }
        "function_call_expression" => {
            let func = node.child_by_field_name("function")?;
            if matches!(
                func.kind(),
                "variable_name" | "dynamic_variable_name" | "expression"
            ) {
                let name = func
                    .utf8_text(source)
                    .ok()
                    .map(|s| s.trim_start_matches('$').to_string())
                    .unwrap_or_else(|| "dynamic".to_string());
                return Some((name, true, None));
            }
            callee_name(node, source).map(|c| (c, false, None))
        }
        _ => callee_name(node, source).map(|c| (c, false, None)),
    }
}

fn method_name_from_node(name_node: Node, source: &[u8]) -> (String, bool) {
    match name_node.kind() {
        "name" => (name_node.utf8_text(source).unwrap_or("").to_string(), false),
        "variable_name" | "dynamic_variable_name" => (
            name_node
                .utf8_text(source)
                .unwrap_or("")
                .trim_start_matches('$')
                .to_string(),
            true,
        ),
        _ => (
            callee_name(name_node, source).unwrap_or_else(|| "$dynamic".to_string()),
            true,
        ),
    }
}

fn type_text(node: Node, source: &[u8]) -> Option<String> {
    if let Ok(text) = node.utf8_text(source) {
        return Some(text.trim_start_matches('\\').to_string());
    }
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok())
        .map(|s| s.trim_start_matches('\\').to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_php_plugin_language_id() {
        let plugin = PhpPlugin::new().unwrap();
        assert_eq!(plugin.language_id(), "php");
    }

    #[test]
    fn test_extract_class_method_namespace_and_construct() {
        let source = br#"<?php
namespace App\Service;

class UserService {
    public function __construct(private UserRepository $repo) {}

    public function findById(int $id): ?User {
        return $this->repo->find($id);
    }
}
"#;
        let plugin = PhpPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("UserService.php"), source)
            .unwrap();

        let class_sym = symbols
            .iter()
            .find(|s| s.name == "UserService" && s.symbol_type == SymbolType::Class)
            .expect("class");
        assert_eq!(
            class_sym.qualified_name.as_deref(),
            Some("App\\Service\\UserService")
        );
        assert!(class_sym.fields.iter().any(|f| f.name == "repo"));

        let ctor = symbols
            .iter()
            .find(|s| s.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true))
            .expect("constructor");
        assert_eq!(ctor.name, "__construct");
        assert_eq!(
            ctor.qualified_name.as_deref(),
            Some("App\\Service\\UserService.<init>")
        );

        let method = symbols
            .iter()
            .find(|s| s.name == "findById")
            .expect("method");
        assert_eq!(
            method.qualified_name.as_deref(),
            Some("App\\Service\\UserService.findById")
        );
        let id_param = method
            .parameters
            .iter()
            .find(|p| p.name == "id")
            .expect("id param");
        assert_eq!(id_param.param_type.as_deref(), Some("int"));
    }

    #[test]
    fn test_extract_relations_calls_and_implements() {
        let source = br#"<?php
namespace App;

class User extends BaseUser implements JsonSerializable {
    public function save(): void {
        $this->persist();
        parent::save();
    }
}
"#;
        let plugin = PhpPlugin::new().unwrap();
        let path = Path::new("User.php");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();

        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Extends && r.to == "BaseUser")
        );
        assert!(relations.iter().any(|r| {
            r.relation_type == RelationType::Implements && r.to == "JsonSerializable"
        }));
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Calls)
        );
    }

    #[test]
    fn test_trait_use_relations() {
        let source = br#"<?php
namespace App;
class C {
    use A, B;
}
"#;
        let plugin = PhpPlugin::new().unwrap();
        let path = Path::new("C.php");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| { r.relation_type == RelationType::Uses && r.to == "A" })
        );
        assert!(
            relations
                .iter()
                .any(|r| { r.relation_type == RelationType::Uses && r.to == "B" })
        );
    }

    #[test]
    fn test_namespace_import_symbols() {
        let source = br#"<?php
namespace App;
use App\Service\AuthService;
use App\Model\OrderDTO as Order;
use Foo\{Bar, Baz as Q};
"#;
        let plugin = PhpPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("imports.php"), source)
            .unwrap();
        let imports: Vec<_> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Import)
            .collect();
        assert_eq!(imports.len(), 4);
        assert!(imports.iter().any(|s| {
            s.name == "AuthService"
                && s.qualified_name.as_deref() == Some("App\\Service\\AuthService")
        }));
        assert!(imports.iter().any(|s| s.name == "Order"));
        assert!(imports.iter().any(|s| s.name == "Bar"));
        assert!(imports.iter().any(|s| s.name == "Q"));
    }

    #[test]
    fn test_import_aware_static_call_hint() {
        let source = br#"<?php
namespace App;
use App\Service\AuthService;
function run() {
    AuthService::login('x');
}
"#;
        let plugin = PhpPlugin::new().unwrap();
        let path = Path::new("run.php");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        let call = relations
            .iter()
            .find(|r| r.relation_type == RelationType::Calls)
            .expect("call");
        assert_eq!(
            call.to_qualified_hint.as_deref(),
            Some("App\\Service\\AuthService.login")
        );
    }

    #[test]
    fn test_anonymous_class_and_attributes() {
        let source = br#"<?php
namespace App;
#[Route('/api')]
class C {
    public function factory() {
        return new class {
            public function m(): void {}
        };
    }
}
"#;
        let plugin = PhpPlugin::new().unwrap();
        let path = Path::new("Anon.php");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let class = symbols.iter().find(|s| s.name == "C").expect("class");
        assert!(
            class
                .metadata
                .get("attributes")
                .and_then(|v| v.as_array())
                .is_some_and(|a| a
                    .iter()
                    .any(|v| v.as_str().is_some_and(|s| s.contains("Route"))))
        );
        assert!(symbols.iter().any(|s| s.name.starts_with("$Anonymous")));
        assert!(symbols.iter().any(|s| s.name == "m"));
    }

    #[test]
    fn test_dynamic_call_unresolved() {
        let source = br#"<?php
function dyn($obj, $method) {
    $obj->$method();
}
"#;
        let plugin = PhpPlugin::new().unwrap();
        let path = Path::new("dyn.php");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        let call = relations
            .iter()
            .find(|r| r.relation_type == RelationType::Calls)
            .expect("call");
        assert_eq!(
            call.metadata.get("unresolved").and_then(|v| v.as_bool()),
            Some(true)
        );
    }

    #[test]
    fn test_class_const_field() {
        let source = br#"<?php
class C {
    public const VERSION = '1.0';
}
"#;
        let plugin = PhpPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("C.php"), source).unwrap();
        let class = symbols.iter().find(|s| s.name == "C").expect("class");
        assert!(class.fields.iter().any(|f| f.name == "VERSION"));
    }

    #[test]
    fn test_first_class_callable_strlen() {
        let source = br#"<?php
function f() {
    $g = strlen(...);
    $g('x');
}
"#;
        let plugin = PhpPlugin::new().unwrap();
        let path = Path::new("fcc.php");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| { r.relation_type == RelationType::Calls && r.to == "strlen" })
        );
    }

    #[test]
    fn test_calculate_complexity() {
        let source = br#"<?php
function check($x) {
    if ($x > 0) {
        foreach ($items as $item) {
            if ($item > 10) return true;
        }
    }
    return false;
}
"#;
        let plugin = PhpPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("check.php"), source)
            .unwrap();
        let func = symbols
            .iter()
            .find(|s| s.name == "check")
            .expect("check function");
        let metrics = plugin.calculate_complexity(func, source).unwrap().unwrap();
        assert!(metrics.cyclomatic > 1);
    }
}
