//! Go language plugin
//!
//! Extracts symbols, relationships, and complexity metrics from Go source code
//! using TreeSitter.

use crate::single_pass::{extract_all_single_pass, go_ast_handlers};
use rgctl_plugin_api::*;
use rgctl_plugin_api::{Error, Result};
use std::path::Path;
use tree_sitter::Node;

/// Go language plugin
pub struct GoPlugin;

impl GoPlugin {
    /// Create a new Go plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    pub(crate) fn extract_function(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let mut name = None;
        let mut parameters = Vec::new();
        let mut return_type = None;
        let mut receiver_name: Option<String> = None;
        let mut receiver_type: Option<String> = None;

        if let Some(name_node) = node.child_by_field_name("name") {
            name = Some(name_node.utf8_text(source)?.to_string());
        }

        if let Some(recv) = node.child_by_field_name("receiver") {
            let (rn, rt) = self.extract_receiver(recv, source)?;
            receiver_name = rn;
            receiver_type = rt;
        }

        if let Some(params) = node.child_by_field_name("parameters") {
            parameters = self.extract_parameters(params, source)?;
        }

        if let Some(result) = node.child_by_field_name("result") {
            return_type = Some(result.utf8_text(source)?.to_string());
        } else {
            // Fallback walk for older paths / free functions without field names
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "identifier" | "field_identifier" if name.is_none() => {
                        name = Some(child.utf8_text(source)?.to_string());
                    }
                    "parameter_list"
                        if parameters.is_empty() && node.kind() == "function_declaration" =>
                    {
                        parameters = self.extract_parameters(child, source)?;
                    }
                    "type_identifier" | "pointer_type" | "slice_type" | "qualified_type"
                        if return_type.is_none() =>
                    {
                        return_type = Some(child.utf8_text(source)?.to_string());
                    }
                    _ => {}
                }
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Function missing name".to_string(),
        })?;

        let (qualified_name, metadata) =
            if let Some(type_name) = self.constructor_type_name(&name, return_type.as_deref()) {
                (
                    Some(format!("{type_name}.<init>")),
                    serde_json::json!({
                        "language": "go",
                        "is_constructor": true
                    }),
                )
            } else if let Some(rt) = receiver_type.as_ref() {
                let bare = rt.trim_start_matches('*').to_string();
                let mut meta = serde_json::json!({
                    "language": "go",
                    "receiver_type": bare,
                });
                if let Some(rn) = &receiver_name {
                    meta["receiver_name"] = serde_json::Value::String(rn.clone());
                }
                (Some(format!("{bare}.{name}")), meta)
            } else {
                (None, serde_json::json!({ "language": "go" }))
            };

        let mut symbol = Symbol {
            name: name.clone(),
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
        symbol = rgctl_plugin_helpers::callable_identity::symbol(
            symbol,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Go,
        );
        Ok(symbol)
    }

    fn extract_receiver(
        &self,
        recv_list: Node,
        source: &[u8],
    ) -> Result<(Option<String>, Option<String>)> {
        let mut cursor = recv_list.walk();
        for child in recv_list.children(&mut cursor) {
            if child.kind() != "parameter_declaration" {
                continue;
            }
            let mut name = None;
            let mut ty = None;
            let mut c2 = child.walk();
            for part in child.children(&mut c2) {
                match part.kind() {
                    "identifier" => name = Some(part.utf8_text(source)?.to_string()),
                    "type_identifier" | "pointer_type" | "qualified_type" => {
                        ty = Some(part.utf8_text(source)?.to_string());
                    }
                    _ => {}
                }
            }
            if let Some(t) = child.child_by_field_name("type") {
                ty = Some(t.utf8_text(source)?.to_string());
            }
            if let Some(n) = child.child_by_field_name("name") {
                name = Some(n.utf8_text(source)?.to_string());
            }
            return Ok((name, ty));
        }
        Ok((None, None))
    }

    /// `NewXxx` returning `Xxx` or `*Xxx` → treat as constructor for `Xxx`.
    fn constructor_type_name(&self, name: &str, return_type: Option<&str>) -> Option<String> {
        let type_name = name.strip_prefix("New").filter(|s| !s.is_empty())?;
        let rt = return_type?.trim();
        let bare = rt.trim_start_matches('*').trim();
        if bare == type_name {
            Some(type_name.to_string())
        } else {
            None
        }
    }

    fn extract_parameters(&self, params_node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut parameters = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            if child.kind() == "parameter_declaration" {
                let mut names = Vec::new();
                let mut param_type = child
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                let mut param_cursor = child.walk();

                for param_child in child.children(&mut param_cursor) {
                    match param_child.kind() {
                        "identifier" => {
                            names.push(param_child.utf8_text(source)?.to_string());
                        }
                        "type_identifier" | "pointer_type" | "slice_type" | "qualified_type"
                        | "map_type" | "channel_type" | "function_type" | "array_type"
                            if param_type.is_none() =>
                        {
                            param_type = Some(param_child.utf8_text(source)?.to_string());
                        }
                        _ => {}
                    }
                }

                // Go allows `func Foo(a, b int)` — one type shared by multiple names
                if names.is_empty() {
                    // unnamed parameter (rare in decls) — skip
                    continue;
                }
                for name in names {
                    parameters.push(Parameter {
                        name,
                        param_type: param_type.clone(),
                        default_value: None,
                    });
                }
            }
        }

        Ok(parameters)
    }

    pub(crate) fn extract_struct(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut name_node = node.child_by_field_name("name");
        let mut fields = Vec::new();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "type_identifier" => {
                    name = Some(child.utf8_text(source)?.to_string());
                    if name_node.is_none() {
                        name_node = Some(child);
                    }
                }
                "struct_type" => {
                    fields = self.extract_struct_fields(child, source)?;
                }
                _ => {}
            }
        }

        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Struct missing name".to_string(),
        })?;

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Struct,
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
        rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, name_node);
        Ok(symbol)
    }

    fn extract_struct_fields(&self, struct_type_node: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut cursor = struct_type_node.walk();

        for child in struct_type_node.children(&mut cursor) {
            if child.kind() == "field_declaration_list" {
                let mut field_cursor = child.walk();
                for field_child in child.children(&mut field_cursor) {
                    if field_child.kind() == "field_declaration" {
                        let mut names = Vec::new();
                        let mut field_type = None;
                        let mut embedded = false;

                        let mut decl_cursor = field_child.walk();
                        for decl_child in field_child.children(&mut decl_cursor) {
                            match decl_child.kind() {
                                "field_identifier" => {
                                    names.push(decl_child.utf8_text(source)?.to_string());
                                }
                                "type_identifier" | "pointer_type" | "slice_type"
                                | "qualified_type" | "generic_type" => {
                                    field_type = Some(decl_child.utf8_text(source)?.to_string());
                                }
                                _ => {}
                            }
                        }
                        if let Some(t) = field_child.child_by_field_name("type") {
                            field_type = Some(t.utf8_text(source)?.to_string());
                        }

                        // Embedded field: no explicit name (LF-06).
                        if names.is_empty() {
                            if let Some(ty) = &field_type {
                                names.push(ty.trim_start_matches('*').to_string());
                                embedded = true;
                            }
                        }

                        for name in names {
                            fields.push(Field {
                                source_span: rgctl_plugin_helpers::field_source_span(
                                    field_child,
                                    source,
                                    &name,
                                ),
                                name,
                                field_type: field_type.clone(),
                                visibility: if embedded {
                                    Some("embedded".into())
                                } else {
                                    None
                                },
                            });
                        }
                    }
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
        let mut name_node = node.child_by_field_name("name");

        for child in node.children(&mut cursor) {
            if child.kind() == "type_identifier" {
                name = Some(child.utf8_text(source)?.to_string());
                if name_node.is_none() {
                    name_node = Some(child);
                }
                break;
            }
        }

        // type_spec wraps name + interface_type — walk parent if needed via caller
        let name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Interface missing name".to_string(),
        })?;

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Interface,
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
            metadata: serde_json::json!({ "language": "go" }),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, name_node);
        Ok(symbol)
    }

    /// Interface method elements → Function symbols with `Iface.Method` FQN.
    /// Also promotes methods from embedded interfaces (LF-04 / CRI RuntimeService pattern).
    pub(crate) fn extract_interface_methods(
        &self,
        interface_type: Node,
        iface_name: &str,
        source: &[u8],
        file_path: &str,
        prior_symbols: &[Symbol],
    ) -> Result<Vec<Symbol>> {
        let mut methods = Vec::new();
        let mut embedded: Vec<String> = Vec::new();
        let mut cursor = interface_type.walk();
        for child in interface_type.children(&mut cursor) {
            match child.kind() {
                "method_elem" | "method_spec" => {
                    let mut name = None;
                    let mut parameters = Vec::new();
                    let mut return_type = None;
                    if let Some(n) = child.child_by_field_name("name") {
                        name = Some(n.utf8_text(source)?.to_string());
                    }
                    if let Some(p) = child.child_by_field_name("parameters") {
                        parameters = self.extract_parameters(p, source)?;
                    }
                    if let Some(r) = child.child_by_field_name("result") {
                        return_type = Some(r.utf8_text(source)?.to_string());
                    }
                    let mut c2 = child.walk();
                    for part in child.children(&mut c2) {
                        if name.is_none() && part.kind() == "field_identifier" {
                            name = Some(part.utf8_text(source)?.to_string());
                        }
                        if part.kind() == "parameter_list" && parameters.is_empty() {
                            parameters = self.extract_parameters(part, source)?;
                        }
                    }
                    let Some(method_name) = name else {
                        continue;
                    };
                    let mut symbol = Symbol {
                        name: method_name.clone(),
                        symbol_type: SymbolType::Function,
                        qualified_name: Some(format!("{iface_name}.{method_name}")),
                        location: SourceLocation {
                            file: file_path.to_string(),
                            start_line: child.start_position().row + 1,
                            end_line: child.end_position().row + 1,
                            start_column: child.start_position().column,
                            end_column: child.end_position().column,
                        },
                        signature: Some(child.utf8_text(source)?.trim().to_string()),
                        return_type,
                        parameters,
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({
                            "language": "go",
                            "interface_method": true,
                            "receiver_type": iface_name,
                        }),
                        ..Default::default()
                    };
                    rgctl_plugin_helpers::record_symbol_spans(
                        &mut symbol,
                        child,
                        child.child_by_field_name("name"),
                    );
                    symbol = rgctl_plugin_helpers::callable_identity::symbol(
                        symbol,
                        interface_type,
                        source,
                        rgctl_plugin_helpers::callable_identity::Policy::Go,
                    );
                    methods.push(symbol);
                }
                "type_identifier" | "qualified_type" | "type_elem" => {
                    // Embedded interface (e.g. RuntimeService embeds PodSandboxManager).
                    if let Ok(t) = child.utf8_text(source) {
                        let bare = t.trim().rsplit('.').next().unwrap_or(t).trim().to_string();
                        if !bare.is_empty() && bare != iface_name {
                            embedded.push(bare);
                        }
                    }
                }
                _ => {}
            }
        }

        for embed in &embedded {
            for s in prior_symbols {
                if s.symbol_type != SymbolType::Function {
                    continue;
                }
                let Some(qn) = s.qualified_name.as_deref() else {
                    continue;
                };
                let prefix = format!("{embed}.");
                if let Some(method_name) = qn.strip_prefix(&prefix) {
                    if methods.iter().any(|m| m.name == method_name) {
                        continue;
                    }
                    let mut promoted = s.clone();
                    promoted.qualified_name = Some(format!("{iface_name}.{method_name}"));
                    promoted.metadata = serde_json::json!({
                        "language": "go",
                        "interface_method": true,
                        "receiver_type": iface_name,
                        "promoted_from": embed,
                    });
                    methods.push(promoted);
                }
            }
        }
        Ok(methods)
    }

    fn calculate_cyclomatic(&self, node: Node) -> usize {
        let mut complexity = 1;

        fn traverse(node: Node, complexity: &mut usize) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "if_statement"
                    | "for_statement"
                    | "expression_switch_statement"
                    | "type_switch_statement"
                    | "select_statement"
                    | "expression_case"
                    | "type_case"
                    | "default_case"
                    | "communication_case" => {
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
                    "if_statement" | "for_statement" => {
                        *cognitive += 1 + nesting;
                        traverse(child, cognitive, nesting + 1);
                    }
                    "expression_switch_statement"
                    | "type_switch_statement"
                    | "select_statement" => {
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
                if matches!(child.kind(), "if_statement" | "for_statement" | "block") {
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

    pub(crate) fn type_params_of(&self, node: Node, source: &[u8]) -> Option<String> {
        let tp = node.child_by_field_name("type_parameters")?;
        tp.utf8_text(source).ok().map(|s| s.to_string())
    }

    pub(crate) fn extract_type_alias(
        &self,
        type_spec: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Option<Symbol>> {
        let name = type_spec
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);
        let Some(name) = name else {
            return Ok(None);
        };
        let underlying = type_spec
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok())
            .map(str::to_string);
        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::TypeAlias,
            qualified_name: Some(name),
            location: SourceLocation {
                file: file_path.to_string(),
                start_line: type_spec.start_position().row + 1,
                end_line: type_spec.end_position().row + 1,
                start_column: type_spec.start_position().column,
                end_column: type_spec.end_position().column,
            },
            signature: underlying.clone(),
            return_type: underlying,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "go" }),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(
            &mut symbol,
            type_spec,
            type_spec.child_by_field_name("name"),
        );
        Ok(Some(symbol))
    }

    pub(crate) fn extract_imports(
        &self,
        import_decl: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Vec<Symbol>> {
        let mut out = Vec::new();
        let mut stack = vec![import_decl];
        while let Some(node) = stack.pop() {
            if node.kind() == "import_spec" {
                let path = node
                    .child_by_field_name("path")
                    .and_then(|n| {
                        n.utf8_text(source)
                            .ok()
                            .map(|s| s.trim_matches('"').to_string())
                    })
                    .or_else(|| {
                        let mut c = node.walk();
                        let mut found = None;
                        for ch in node.children(&mut c) {
                            if ch.kind() == "interpreted_string_literal" {
                                found = ch
                                    .utf8_text(source)
                                    .ok()
                                    .map(|s| s.trim_matches('"').to_string());
                                break;
                            }
                        }
                        found
                    });
                let alias = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                if let Some(path) = path {
                    let name = alias
                        .clone()
                        .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(&path).to_string());
                    let mut symbol = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::Import,
                        qualified_name: Some(path.clone()),
                        location: SourceLocation {
                            file: file_path.to_string(),
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            start_column: node.start_position().column,
                            end_column: node.end_position().column,
                        },
                        signature: Some(path),
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({
                            "language": "go",
                            "import_alias": alias,
                        }),
                        ..Default::default()
                    };
                    rgctl_plugin_helpers::record_symbol_spans(
                        &mut symbol,
                        node,
                        node.child_by_field_name("name"),
                    );
                    out.push(symbol);
                }
            }
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                stack.push(ch);
            }
        }
        Ok(out)
    }

    pub(crate) fn extract_consts(
        &self,
        const_decl: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Vec<Symbol>> {
        let mut out = Vec::new();
        let mut stack = vec![const_decl];
        while let Some(node) = stack.pop() {
            if node.kind() == "const_spec" {
                let mut names = Vec::new();
                if let Some(n) = node.child_by_field_name("name") {
                    if let Ok(s) = n.utf8_text(source) {
                        names.push(s.to_string());
                    }
                }
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.kind() == "identifier" {
                        if let Ok(s) = ch.utf8_text(source) {
                            if !names.iter().any(|n| n == s) {
                                names.push(s.to_string());
                            }
                        }
                    }
                }
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                for name in names {
                    let name_node = node
                        .child_by_field_name("name")
                        .filter(|n| n.utf8_text(source).ok() == Some(name.as_str()))
                        .or_else(|| {
                            let mut c = node.walk();
                            node.children(&mut c).find(|ch| {
                                ch.kind() == "identifier"
                                    && ch.utf8_text(source).ok() == Some(name.as_str())
                            })
                        });
                    let mut symbol = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::Variable,
                        qualified_name: Some(name),
                        location: SourceLocation {
                            file: file_path.to_string(),
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            start_column: node.start_position().column,
                            end_column: node.end_position().column,
                        },
                        signature: ty.clone(),
                        return_type: ty.clone(),
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec!["const".into()],
                        documentation: None,
                        metadata: serde_json::json!({
                            "language": "go",
                            "is_const": true,
                        }),
                        ..Default::default()
                    };
                    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, name_node);
                    out.push(symbol);
                }
            }
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                stack.push(ch);
            }
        }
        Ok(out)
    }

    pub(crate) fn extract_vars(
        &self,
        var_decl: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Vec<Symbol>> {
        let mut out = Vec::new();
        let mut stack = vec![var_decl];
        while let Some(node) = stack.pop() {
            if node.kind() == "var_spec" {
                let mut names = Vec::new();
                if let Some(n) = node.child_by_field_name("name") {
                    if let Ok(s) = n.utf8_text(source) {
                        names.push(s.to_string());
                    }
                }
                let mut c = node.walk();
                for ch in node.children(&mut c) {
                    if ch.kind() == "identifier" {
                        if let Ok(s) = ch.utf8_text(source) {
                            if !names.iter().any(|n| n == s) {
                                names.push(s.to_string());
                            }
                        }
                    }
                }
                let ty = node
                    .child_by_field_name("type")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                for name in names {
                    let name_node = node
                        .child_by_field_name("name")
                        .filter(|n| n.utf8_text(source).ok() == Some(name.as_str()))
                        .or_else(|| {
                            let mut c = node.walk();
                            node.children(&mut c).find(|ch| {
                                ch.kind() == "identifier"
                                    && ch.utf8_text(source).ok() == Some(name.as_str())
                            })
                        });
                    let mut symbol = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::Variable,
                        qualified_name: Some(name),
                        location: SourceLocation {
                            file: file_path.to_string(),
                            start_line: node.start_position().row + 1,
                            end_line: node.end_position().row + 1,
                            start_column: node.start_position().column,
                            end_column: node.end_position().column,
                        },
                        signature: ty.clone(),
                        return_type: ty.clone(),
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({
                            "language": "go",
                            "kind": "var_spec",
                        }),
                        ..Default::default()
                    };
                    rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, name_node);
                    out.push(symbol);
                }
            }
            let mut c = node.walk();
            for ch in node.children(&mut c) {
                stack.push(ch);
            }
        }
        Ok(out)
    }

    /// Best-effort method-set satisfaction → `Implements`, and embed → `Extends`.
    pub(crate) fn emit_implements_and_embeds(
        &self,
        symbols: &[Symbol],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) {
        let file = file_path.to_string_lossy().to_string();
        let loc = SourceLocation {
            file: file.clone(),
            start_line: 1,
            end_line: 1,
            start_column: 0,
            end_column: 0,
        };

        let mut type_methods: std::collections::HashMap<String, std::collections::HashSet<String>> =
            std::collections::HashMap::new();
        for s in symbols {
            if s.symbol_type != SymbolType::Function {
                continue;
            }
            // Interface method stubs are not concrete implementors.
            if s.metadata.get("interface_method").and_then(|v| v.as_bool()) == Some(true) {
                continue;
            }
            if let Some(rt) = s.metadata.get("receiver_type").and_then(|v| v.as_str()) {
                type_methods
                    .entry(rt.to_string())
                    .or_default()
                    .insert(s.name.clone());
            }
        }

        let mut iface_methods: std::collections::HashMap<
            String,
            std::collections::HashSet<String>,
        > = std::collections::HashMap::new();
        for s in symbols {
            if s.symbol_type != SymbolType::Function {
                continue;
            }
            if s.metadata.get("interface_method").and_then(|v| v.as_bool()) != Some(true) {
                continue;
            }
            if let Some(rt) = s.metadata.get("receiver_type").and_then(|v| v.as_str()) {
                iface_methods
                    .entry(rt.to_string())
                    .or_default()
                    .insert(s.name.clone());
            }
        }

        for (ty, methods) in &type_methods {
            for (iface, required) in &iface_methods {
                if required.is_empty() {
                    continue;
                }
                if required.iter().all(|m| methods.contains(m)) {
                    relations.push(Relation {
                        from: ty.clone(),
                        to: iface.clone(),
                        relation_type: RelationType::Implements,
                        location: loc.clone(),
                        metadata: serde_json::json!({ "language": "go" }),
                        to_qualified_hint: Some(iface.clone()),
                        to_type_hint: None,
                        ..Default::default()
                    });
                }
            }
        }

        for s in symbols {
            if s.symbol_type != SymbolType::Struct {
                continue;
            }
            for f in &s.fields {
                if f.visibility.as_deref() != Some("embedded") {
                    continue;
                }
                let embed = f
                    .field_type
                    .as_deref()
                    .unwrap_or(f.name.as_str())
                    .trim_start_matches('*');
                relations.push(Relation {
                    from: s.name.clone(),
                    to: embed.to_string(),
                    relation_type: RelationType::Extends,
                    location: loc.clone(),
                    metadata: serde_json::json!({
                        "language": "go",
                        "embed": true,
                    }),
                    to_qualified_hint: Some(embed.to_string()),
                    to_type_hint: None,
                    ..Default::default()
                });
            }
        }
    }

    fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        parse_with_pool("go", tree_sitter_go::LANGUAGE.into(), source).map_err(|e| {
            Error::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message: e,
            }
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
        go_traverse_for_symbols(root, source, &file_path_str, &mut symbols, self)?;
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
        walk_calls(
            root,
            source,
            file_path,
            symbols,
            GO_CALL_KINDS,
            "go",
            &mut relations,
        );
        self.emit_implements_and_embeds(symbols, file_path, &mut relations);
        Ok(relations)
    }
}

fn go_traverse_for_symbols(
    node: Node,
    source: &[u8],
    file_path: &str,
    symbols: &mut Vec<Symbol>,
    plugin: &GoPlugin,
) -> Result<()> {
    match node.kind() {
        "type_declaration" => {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "type_alias" => {
                        if let Some(alias) = plugin.extract_type_alias(child, source, file_path)? {
                            symbols.push(alias);
                        }
                    }
                    "type_spec" => {
                        let mut handled = false;
                        let mut spec_cursor = child.walk();
                        for spec_child in child.children(&mut spec_cursor) {
                            match spec_child.kind() {
                                "struct_type" => {
                                    let mut st = plugin.extract_struct(child, source, file_path)?;
                                    if let Some(tp) = plugin.type_params_of(child, source) {
                                        st.metadata["type_params"] = serde_json::Value::String(tp);
                                    }
                                    symbols.push(st);
                                    handled = true;
                                    break;
                                }
                                "interface_type" => {
                                    let iface =
                                        plugin.extract_interface(child, source, file_path)?;
                                    let iface_name = iface.name.clone();
                                    symbols.push(iface);
                                    let methods = plugin.extract_interface_methods(
                                        spec_child,
                                        &iface_name,
                                        source,
                                        file_path,
                                        symbols,
                                    )?;
                                    symbols.extend(methods);
                                    handled = true;
                                    break;
                                }
                                _ => {}
                            }
                        }
                        if !handled {
                            // Defined type (LF-10): `type UserID string`
                            if let Some(alias) =
                                plugin.extract_type_alias(child, source, file_path)?
                            {
                                symbols.push(alias);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        "import_declaration" => {
            symbols.extend(plugin.extract_imports(node, source, file_path)?);
        }
        "const_declaration" => {
            symbols.extend(plugin.extract_consts(node, source, file_path)?);
        }
        "function_declaration" | "method_declaration" => {
            let mut func = plugin.extract_function(node, source, file_path)?;
            if let Some(tp) = plugin.type_params_of(node, source) {
                func.metadata["type_params"] = serde_json::Value::String(tp);
            }
            symbols.push(func);
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        go_traverse_for_symbols(child, source, file_path, symbols, plugin)?;
    }

    Ok(())
}

impl Default for GoPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create GoPlugin")
    }
}

impl LanguagePlugin for GoPlugin {
    fn language_id(&self) -> &str {
        "go"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["go"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_go::LANGUAGE.into())
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
            "go",
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
        Some(go_ast_handlers())
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

        let tree = parse_with_pool("go", tree_sitter_go::LANGUAGE.into(), source).map_err(|e| {
            Error::ParseError {
                file: symbol.location.file.clone().into(),
                line: symbol.location.start_line,
                message: e,
            }
        })?;

        let root = tree.root_node();
        let target_line = symbol.location.start_line - 1;

        fn find_function_at_line(node: Node, line: usize) -> Option<Node> {
            if matches!(node.kind(), "function_declaration" | "method_declaration")
                && node.start_position().row == line
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
    fn test_go_plugin_language_id() {
        let plugin = GoPlugin::new().unwrap();
        assert_eq!(plugin.language_id(), "go");
    }

    #[test]
    fn test_go_plugin_file_extensions() {
        let plugin = GoPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["go"]);
    }

    #[test]
    fn test_extract_function() {
        let plugin = GoPlugin::new().unwrap();
        let source = b"func Add(a int, b int) int { return a + b }";
        let symbols = plugin
            .extract_symbols(Path::new("test.go"), source)
            .unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Add");
        assert_eq!(symbols[0].symbol_type, SymbolType::Function);
        assert_eq!(symbols[0].parameters.len(), 2);
    }

    #[test]
    fn test_extract_struct_fields_typed_params_and_new_ctor() {
        let source = br#"
package demo

type User struct {
	Name string
	Age  int
}

func NewUser(name string, age int) *User {
	return &User{Name: name, Age: age}
}

func Sum(a, b int) int {
	return a + b
}
"#;
        let plugin = GoPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("user.go"), source)
            .unwrap();
        let st = symbols
            .iter()
            .find(|s| s.name == "User" && s.symbol_type == SymbolType::Struct)
            .expect("struct");
        assert!(st.fields.iter().any(|f| f.name == "Name"));
        assert!(st.fields.iter().any(|f| f.name == "Age"));
        assert_eq!(
            st.fields
                .iter()
                .find(|f| f.name == "Name")
                .and_then(|f| f.field_type.as_deref()),
            Some("string")
        );

        let sum = symbols.iter().find(|s| s.name == "Sum").expect("Sum");
        assert_eq!(sum.parameters.len(), 2);
        assert_eq!(sum.parameters[0].name, "a");
        assert_eq!(sum.parameters[1].name, "b");
        assert_eq!(sum.parameters[0].param_type.as_deref(), Some("int"));
        assert_eq!(sum.parameters[1].param_type.as_deref(), Some("int"));

        let ctor = symbols
            .iter()
            .find(|s| {
                s.symbol_type == SymbolType::Function
                    && s.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true)
            })
            .expect("constructor");
        assert_eq!(ctor.name, "NewUser");
        assert_eq!(ctor.qualified_name.as_deref(), Some("User.<init>"));
        assert_eq!(ctor.parameters.len(), 2);
        assert_eq!(ctor.parameters[0].param_type.as_deref(), Some("string"));
        assert_eq!(ctor.parameters[1].param_type.as_deref(), Some("int"));
    }

    #[test]
    fn test_extract_struct() {
        let plugin = GoPlugin::new().unwrap();
        let source = b"type User struct { Name string; Age int }";
        let symbols = plugin
            .extract_symbols(Path::new("test.go"), source)
            .unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].symbol_type, SymbolType::Struct);
        assert_eq!(symbols[0].fields.len(), 2);
    }

    #[test]
    fn test_extract_interface() {
        let plugin = GoPlugin::new().unwrap();
        let source = b"type Reader interface { Read(p []byte) (n int, err error) }";
        let symbols = plugin
            .extract_symbols(Path::new("test.go"), source)
            .unwrap();

        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Reader" && s.symbol_type == SymbolType::Interface)
        );
        let method = symbols
            .iter()
            .find(|s| s.name == "Read" && s.symbol_type == SymbolType::Function)
            .expect("interface method");
        assert_eq!(method.qualified_name.as_deref(), Some("Reader.Read"));
    }

    #[test]
    fn test_method_receiver_qualified_and_selector_calls() {
        let source = br#"
package demo

type Alpha struct{}
func (a *Alpha) ListItems() int { return 1 }

type Beta struct{}
func (b *Beta) ListItems() int { return 2 }

type Orch struct {
	beta *Beta
}
func (o *Orch) Run() int {
	return o.beta.ListItems()
}
"#;
        let plugin = GoPlugin::new().unwrap();
        let path = Path::new("demo.go");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let run = symbols.iter().find(|s| s.name == "Run").expect("Run");
        assert_eq!(run.qualified_name.as_deref(), Some("Orch.Run"));
        let rels = plugin.extract_relations(path, source, &symbols).unwrap();
        let hit = rels.iter().find(|r| {
            matches!(r.relation_type, RelationType::Calls)
                && (r.from == "Orch.Run" || r.from == "Run")
                && (r.to.contains("ListItems"))
        });
        assert!(hit.is_some(), "expected Run → ListItems, got {rels:?}");
        let hit = hit.unwrap();
        assert_eq!(hit.to_type_hint.as_deref(), Some("Beta"));
        assert!(
            hit.to.ends_with("Beta.ListItems")
                || hit.to_qualified_hint.as_deref() == Some("Beta.ListItems"),
            "got to={} hint={:?}",
            hit.to,
            hit.to_qualified_hint
        );
    }

    #[test]
    fn test_extract_all_single_pass_forward_calls_and_complexity() {
        let source = br#"
package demo

func caller() {
	helper()
	if true {
		return
	}
}

func helper() {}
"#;
        let plugin = GoPlugin::new().unwrap();
        let path = Path::new("test.go");
        let result = plugin.extract_all(path, source).unwrap();
        assert!(
            result
                .relations
                .iter()
                .any(|r| { r.relation_type == RelationType::Calls && r.to.contains("helper") }),
            "forward Calls -> helper: {:?}",
            result.relations
        );
        let caller = result
            .symbols
            .iter()
            .find(|s| s.name == "caller")
            .expect("caller");
        let from_meta = complexity_from_metadata(caller).expect("rgctl_complexity on extract_all");
        assert_eq!(from_meta.cyclomatic, 2);
        assert_eq!(from_meta.returns, 1);
        let via_api = plugin
            .calculate_complexity(caller, source)
            .unwrap()
            .unwrap();
        assert_eq!(via_api, from_meta);
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
package demo

func caller() {
    helper()
}

func helper() {}
"#;
        let plugin = GoPlugin::new().unwrap();
        let path = Path::new("test.go");
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
    fn test_implements_and_embed_extends() {
        let source = br#"
package demo

type Runner interface {
	Run()
}

type Remote struct{}
func (r *Remote) Run() {}

type Base struct{}
func (b *Base) BaseMethod() {}

type Derived struct {
	Base
}
"#;
        let plugin = GoPlugin::new().unwrap();
        let path = Path::new("demo.go");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations.iter().any(|r| {
                matches!(r.relation_type, RelationType::Implements)
                    && r.from == "Remote"
                    && r.to == "Runner"
            }),
            "expected Remote IMPLEMENTS Runner, got {relations:?}"
        );
        assert!(
            !relations.iter().any(|r| {
                matches!(r.relation_type, RelationType::Implements) && r.from == "Runner"
            }),
            "interface must not IMPLEMENTS itself: {relations:?}"
        );
        assert!(
            relations.iter().any(|r| {
                matches!(r.relation_type, RelationType::Extends)
                    && r.from == "Derived"
                    && r.to == "Base"
            }),
            "expected Derived EXTENDS Base, got {relations:?}"
        );
    }

    #[test]
    fn test_imports_consts_alias_generics_metadata() {
        let source = br#"
package demo

import (
	"fmt"
	tu "example.com/timeutil"
)

type Status int
const (
	StatusPending Status = iota
	StatusActive
)

type UserID string

type Box[T any] struct { Value T }

func Identity[T any](v T) T { return v }
"#;
        let plugin = GoPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("demo.go"), source)
            .unwrap();

        assert!(
            symbols.iter().any(|s| {
                s.symbol_type == SymbolType::Import
                    && s.name == "fmt"
                    && s.qualified_name.as_deref() == Some("fmt")
            }),
            "missing fmt Import: {symbols:?}"
        );
        assert!(
            symbols.iter().any(|s| {
                s.symbol_type == SymbolType::Import
                    && s.name == "tu"
                    && s.qualified_name.as_deref() == Some("example.com/timeutil")
            }),
            "missing aliased Import: {symbols:?}"
        );
        assert!(
            symbols.iter().any(|s| {
                s.name == "StatusPending"
                    && s.modifiers.iter().any(|m| m == "const")
                    && s.metadata.get("is_const").and_then(|v| v.as_bool()) == Some(true)
            }),
            "missing const StatusPending"
        );
        assert!(
            symbols
                .iter()
                .any(|s| { s.name == "UserID" && s.symbol_type == SymbolType::TypeAlias }),
            "missing TypeAlias UserID"
        );
        let box_sym = symbols
            .iter()
            .find(|s| s.name == "Box" && s.symbol_type == SymbolType::Struct)
            .expect("Box");
        assert!(
            box_sym
                .metadata
                .get("type_params")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t.contains("T")),
            "Box type_params: {:?}",
            box_sym.metadata
        );
        let id = symbols
            .iter()
            .find(|s| s.name == "Identity")
            .expect("Identity");
        assert!(
            id.metadata
                .get("type_params")
                .and_then(|v| v.as_str())
                .is_some_and(|t| t.contains("T")),
            "Identity type_params: {:?}",
            id.metadata
        );
    }
}

#[cfg(test)]
mod cri_promote {
    use super::*;
    #[test]
    fn runtime_service_promotes_runpodsandbox() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../example/kubernetes/staging/src/k8s.io/cri-api/pkg/apis/services.go"
        ));
        let Ok(src) = std::fs::read(path) else {
            eprintln!(
                "skipping: kubernetes corpus not present at {}",
                path.display()
            );
            return;
        };
        let plugin = GoPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(path, &src).unwrap();
        let qns: Vec<_> = symbols
            .iter()
            .filter(|s| s.name == "RunPodSandbox")
            .filter_map(|s| s.qualified_name.clone())
            .collect();
        assert!(
            qns.iter().any(|q| q == "RuntimeService.RunPodSandbox"),
            "{qns:?}"
        );
        assert!(
            qns.iter().any(|q| q == "PodSandboxManager.RunPodSandbox"),
            "{qns:?}"
        );
    }
}
