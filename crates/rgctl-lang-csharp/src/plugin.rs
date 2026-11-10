//! C# language plugin using Tree-sitter.

use crate::single_pass::{csharp_ast_handlers, extract_all_single_pass};
use rgctl_plugin_api::*;
use rgctl_plugin_helpers::record_symbol_spans;
use std::collections::HashMap;
use std::path::Path;
use tree_sitter::{Node, Parser};

/// Cap AST walk depth (matches CFG expression walk limit in `rgctl-analysis`).
const MAX_TREE_DEPTH: usize = 2048;

fn push_traverse_children<'a>(stack: &mut Vec<TraverseFrame<'a>>, node: Node<'a>, depth: usize) {
    let mut cursor = node.walk();
    let children: Vec<Node<'a>> = node.children(&mut cursor).collect();
    for child in children.into_iter().rev() {
        stack.push(TraverseFrame::Node(child, depth + 1));
    }
}

fn push_tree_children<'a>(stack: &mut Vec<(Node<'a>, usize)>, node: Node<'a>, depth: usize) {
    let mut cursor = node.walk();
    let children: Vec<Node<'a>> = node.children(&mut cursor).collect();
    for child in children.into_iter().rev() {
        stack.push((child, depth + 1));
    }
}

enum TraverseFrame<'a> {
    PopNamespace,
    Node(Node<'a>, usize),
}

/// Namespace and using context during a single-file extract pass.
#[derive(Debug, Default)]
pub(crate) struct ExtractCtx {
    namespace_stack: Vec<String>,
    usings: Vec<String>,
}

impl ExtractCtx {
    pub(crate) fn qualify(&self, simple: &str) -> String {
        if self.namespace_stack.is_empty() {
            simple.to_string()
        } else {
            format!("{}.{}", self.namespace_stack.join("."), simple)
        }
    }

    pub(crate) fn push_qualified_namespace(&mut self, qualified: &str) {
        self.namespace_stack.push(qualified.to_string());
    }

    pub(crate) fn pop_namespace(&mut self) {
        self.namespace_stack.pop();
    }

    pub(crate) fn register_using(&mut self, text: &str) {
        let trimmed = text
            .trim()
            .trim_start_matches("using")
            .trim()
            .trim_end_matches(';')
            .trim();
        let ns = trimmed
            .strip_prefix("static ")
            .unwrap_or(trimmed)
            .split('=')
            .next()
            .unwrap_or(trimmed)
            .trim();
        if !ns.is_empty() {
            self.usings.push(ns.to_string());
        }
    }

    fn hint_from_usings(&self, type_name: &str) -> Option<String> {
        for u in &self.usings {
            if u.ends_with(&format!(".{type_name}")) || u == type_name {
                return Some(u.clone());
            }
            if u.rsplit('.').next() == Some(type_name) {
                return Some(u.clone());
            }
        }
        self.usings.first().map(|u| format!("{u}.{type_name}"))
    }
}

/// C# language plugin.
pub struct CSharpPlugin {
    _parser: Parser,
}

impl CSharpPlugin {
    /// Create a new C# plugin.
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set C# grammar: {e}")))?;
        Ok(Self { _parser: parser })
    }

    fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        parse_with_pool("csharp", tree_sitter_c_sharp::LANGUAGE.into(), source).map_err(|e| {
            Error::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message: e,
            }
        })
    }

    pub(crate) fn extract_method(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let name_node = node
            .child_by_field_name("name")
            .or_else(|| find_child_kind(node, "identifier"))
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Method missing name".to_string(),
            })?;
        let name = name_node.utf8_text(source)?.to_string();

        let type_qn = self
            .find_containing_type_name(node, source)
            .map(|ty| ctx.qualify(&ty));
        let qualified_name = type_qn.map(|ty| format!("{ty}.{name}"));

        let return_type = method_return_type(node, source);
        let modifiers = modifier_texts(node, source);
        let parameters = self.extract_parameters(node, source)?;

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type,
            parameters,
            fields: vec![],
            modifiers,
            documentation: None,
            metadata: serde_json::json!({ "language": "csharp" }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, Some(name_node));
        Ok(symbol)
    }

    pub(crate) fn extract_constructor(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let type_name = self
            .find_containing_type_name(node, source)
            .or_else(|| {
                node.child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
            })
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Constructor missing containing type".to_string(),
            })?;

        let type_qn = ctx.qualify(&type_name);
        let parameters = self.extract_parameters(node, source)?;
        let qualified_name = format!("{type_qn}.<init>");

        let mut symbol = Symbol {
            name: type_name,
            symbol_type: SymbolType::Function,
            qualified_name: Some(qualified_name),
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type: None,
            parameters,
            fields: vec![],
            modifiers: modifier_texts(node, source),
            documentation: None,
            metadata: serde_json::json!({
                "language": "csharp",
                "is_constructor": true,
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Ok(symbol)
    }

    pub(crate) fn extract_delegate(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let name = type_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Delegate missing name".to_string(),
        })?;
        let parameters = self.extract_parameters(node, source)?;

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name: Some(ctx.qualify(&name)),
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type: method_return_type(node, source),
            parameters,
            fields: vec![],
            modifiers: modifier_texts(node, source),
            documentation: None,
            metadata: serde_json::json!({
                "language": "csharp",
                "is_delegate": true,
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Ok(symbol)
    }

    /// Lightweight Symbol for indexers / operators / destructors / accessors / lambdas.
    pub(crate) fn extract_lightweight_callable(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &ExtractCtx,
    ) -> Result<Symbol> {
        let kind = node.kind();
        let name = match kind {
            "indexer_declaration" => "this[]".to_string(),
            "destructor_declaration" => self
                .find_containing_type_name(node, source)
                .map(|t| format!("~{t}"))
                .unwrap_or_else(|| "~".to_string()),
            "accessor_declaration" => node
                .child_by_field_name("name")
                .or_else(|| {
                    node.children(&mut node.walk()).find(|c| {
                        matches!(
                            c.kind(),
                            "get_body" | "set_body" | "init_body" | "identifier"
                        ) || matches!(
                            c.utf8_text(source).ok(),
                            Some("get" | "set" | "init" | "add" | "remove")
                        )
                    })
                })
                .and_then(|n| n.utf8_text(source).ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    // Accessor keyword is often an anonymous child token.
                    node.children(&mut node.walk()).find_map(|c| {
                        let t = c.utf8_text(source).ok()?.trim();
                        matches!(t, "get" | "set" | "init" | "add" | "remove")
                            .then(|| t.to_string())
                    })
                })
                .unwrap_or_else(|| "accessor".to_string()),
            "operator_declaration" | "conversion_operator_declaration" => node
                .children(&mut node.walk())
                .find(|c| c.kind() == "operator")
                .and_then(|op| {
                    op.children(&mut op.walk())
                        .find_map(|c| c.utf8_text(source).ok().map(|s| s.trim().to_string()))
                })
                .filter(|s| !s.is_empty())
                .or_else(|| type_name(node, source))
                .unwrap_or_else(|| {
                    if kind == "conversion_operator_declaration" {
                        "op_ImplicitOrExplicit".to_string()
                    } else {
                        "operator".to_string()
                    }
                }),
            "lambda_expression" | "anonymous_method_expression" => {
                format!("<{}@{}>", kind, node.start_position().row + 1)
            }
            _ => type_name(node, source).unwrap_or_else(|| kind.to_string()),
        };

        let type_qn = self
            .find_containing_type_name(node, source)
            .map(|ty| ctx.qualify(&ty));
        let qualified_name = type_qn
            .as_ref()
            .map(|ty| format!("{ty}.{name}"))
            .or_else(|| Some(ctx.qualify(&name)));
        let parameters = self.extract_parameters(node, source).unwrap_or_default();

        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type: method_return_type(node, source),
            parameters,
            fields: vec![],
            modifiers: modifier_texts(node, source),
            documentation: None,
            metadata: serde_json::json!({
                "language": "csharp",
                "ast_kind": kind,
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Ok(symbol)
    }

    fn extract_parameters(&self, node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut parameters = Vec::new();
        let params_node = if let Some(p) = node.child_by_field_name("parameters") {
            p
        } else {
            let mut cursor = node.walk();
            let found = node
                .children(&mut cursor)
                .find(|c| c.kind() == "parameter_list");
            match found {
                Some(p) => p,
                None => return Ok(parameters),
            }
        };

        let mut cursor = params_node.walk();
        for child in params_node.children(&mut cursor) {
            if child.kind() != "parameter" {
                continue;
            }
            let name = child
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string);
            let param_type = child
                .child_by_field_name("type")
                .and_then(|n| type_text(n, source));
            if let Some(name) = name {
                parameters.push(Parameter {
                    name,
                    param_type,
                    default_value: None,
                });
            }
        }
        Ok(parameters)
    }

    fn extract_type_fields(&self, type_node: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let body = type_node
            .child_by_field_name("body")
            .or_else(|| find_direct_child_kind(type_node, "declaration_list"));
        let Some(body) = body else {
            return Ok(fields);
        };

        let mut body_cursor = body.walk();
        for child in body.children(&mut body_cursor) {
            match child.kind() {
                "field_declaration" => {
                    let visibility = field_visibility(child, source);
                    let var_decl = find_direct_child_kind(child, "variable_declaration");
                    let Some(var_decl) = var_decl else {
                        continue;
                    };
                    let field_type = var_decl
                        .child_by_field_name("type")
                        .and_then(|n| type_text(n, source));
                    let mut decl_cursor = var_decl.walk();
                    for declarator in var_decl.children(&mut decl_cursor) {
                        if declarator.kind() != "variable_declarator" {
                            continue;
                        }
                        if let Some(name_node) = declarator.child_by_field_name("name") {
                            fields.push(Field {
                                name: name_node.utf8_text(source)?.to_string(),
                                field_type: field_type.clone(),
                                visibility: visibility.clone(),
                            });
                        }
                    }
                }
                "property_declaration" => {
                    let name = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(str::to_string);
                    let field_type = child
                        .child_by_field_name("type")
                        .and_then(|n| type_text(n, source));
                    if let Some(name) = name {
                        fields.push(Field {
                            name,
                            field_type,
                            visibility: field_visibility(child, source),
                        });
                    }
                }
                "event_declaration" | "event_field_declaration" => {
                    // `event EventHandler Changed;` uses variable_declaration like fields.
                    let visibility = field_visibility(child, source);
                    if let Some(name) = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(str::to_string)
                    {
                        let field_type = child
                            .child_by_field_name("type")
                            .and_then(|n| type_text(n, source));
                        fields.push(Field {
                            name,
                            field_type,
                            visibility,
                        });
                    } else if let Some(var_decl) =
                        find_direct_child_kind(child, "variable_declaration")
                    {
                        let field_type = var_decl
                            .child_by_field_name("type")
                            .and_then(|n| type_text(n, source));
                        let mut decl_cursor = var_decl.walk();
                        for declarator in var_decl.children(&mut decl_cursor) {
                            if declarator.kind() != "variable_declarator" {
                                continue;
                            }
                            if let Some(name_node) = declarator.child_by_field_name("name") {
                                fields.push(Field {
                                    name: name_node.utf8_text(source)?.to_string(),
                                    field_type: field_type.clone(),
                                    visibility: visibility.clone(),
                                });
                            }
                        }
                    }
                }
                "enum_member_declaration" => {
                    let name = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(str::to_string)
                        .or_else(|| {
                            let mut c = child.walk();
                            child.children(&mut c).find_map(|n| {
                                if n.kind() == "identifier" {
                                    n.utf8_text(source).ok().map(str::to_string)
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
                }
                _ => {}
            }
        }
        Ok(fields)
    }

    pub(crate) fn extract_type(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbol_type: SymbolType,
        ctx: &ExtractCtx,
        extra_meta: serde_json::Value,
    ) -> Result<Symbol> {
        let name = type_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Type missing name".to_string(),
        })?;

        let fields = if matches!(
            symbol_type,
            SymbolType::Class | SymbolType::Enum | SymbolType::Interface
        ) {
            self.extract_type_fields(node, source)?
        } else {
            vec![]
        };

        let mut meta = serde_json::json!({ "language": "csharp" });
        if let Some(obj) = extra_meta.as_object() {
            for (k, v) in obj {
                meta[k] = v.clone();
            }
        }

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type,
            qualified_name: Some(ctx.qualify(&name)),
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields,
            modifiers: modifier_texts(node, source),
            documentation: None,
            metadata: meta,
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        Ok(symbol)
    }

    fn symbols_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
    ) -> Result<Vec<Symbol>> {
        let mut symbols = Vec::new();
        let mut ctx = ExtractCtx::default();
        self.seed_ctx_from_tree(root, source, &mut ctx);
        self.traverse(
            root,
            source,
            &file_path.to_string_lossy(),
            &mut ctx,
            &mut symbols,
        )?;
        Ok(symbols)
    }

    pub(crate) fn collect_non_call_relations(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        ctx: &ExtractCtx,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        self.extract_inheritance(root, source, file_path, relations)?;
        self.extract_annotated_with(root, source, file_path, ctx, relations)?;
        self.extract_instantiations(root, source, file_path, symbols, ctx, relations)?;
        Ok(())
    }

    fn relations_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let mut relations = Vec::new();
        let mut ctx = ExtractCtx::default();
        self.seed_ctx_from_tree(root, source, &mut ctx);
        self.walk_csharp_calls(root, source, file_path, symbols, &ctx, &mut relations);
        self.collect_non_call_relations(root, source, file_path, symbols, &ctx, &mut relations)?;
        Ok(relations)
    }

    pub(crate) fn seed_ctx_from_tree(&self, root: Node, source: &[u8], ctx: &mut ExtractCtx) {
        if root.kind() == "compilation_unit" {
            let mut cursor = root.walk();
            for child in root.children(&mut cursor) {
                match child.kind() {
                    "using_directive" => {
                        if let Ok(text) = child.utf8_text(source) {
                            ctx.register_using(text);
                        }
                    }
                    "file_scoped_namespace_declaration" => {
                        if let Some(ns) = namespace_name(child, source) {
                            ctx.push_qualified_namespace(&ns);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn emit_symbol_for_node(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        ctx: &mut ExtractCtx,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        match node.kind() {
            "method_declaration" | "local_function_statement" => {
                symbols.push(self.extract_method(node, source, file_path, ctx)?);
            }
            "constructor_declaration" => {
                symbols.push(self.extract_constructor(node, source, file_path, ctx)?);
            }
            "delegate_declaration" => {
                symbols.push(self.extract_delegate(node, source, file_path, ctx)?);
            }
            "class_declaration" | "struct_declaration" => {
                symbols.push(self.extract_type(
                    node,
                    source,
                    file_path,
                    SymbolType::Class,
                    ctx,
                    serde_json::json!({}),
                )?);
            }
            "record_declaration" => {
                symbols.push(self.extract_type(
                    node,
                    source,
                    file_path,
                    SymbolType::Class,
                    ctx,
                    serde_json::json!({ "is_record": true }),
                )?);
            }
            "interface_declaration" => {
                symbols.push(self.extract_type(
                    node,
                    source,
                    file_path,
                    SymbolType::Interface,
                    ctx,
                    serde_json::json!({}),
                )?);
            }
            "enum_declaration" => {
                symbols.push(self.extract_type(
                    node,
                    source,
                    file_path,
                    SymbolType::Enum,
                    ctx,
                    serde_json::json!({}),
                )?);
            }
            "using_directive" => {
                let text = node.utf8_text(source)?.trim().to_string();
                ctx.register_using(&text);
                let mut symbol = Symbol {
                    name: text.clone(),
                    symbol_type: SymbolType::Import,
                    qualified_name: None,
                    location: source_location(node, file_path),
                    signature: None,
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({ "language": "csharp" }),
                    ..Default::default()
                };
                record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
                symbols.push(symbol);
            }
            _ => {}
        }
        Ok(())
    }

    fn traverse(
        &self,
        root: Node,
        source: &[u8],
        file_path: &str,
        ctx: &mut ExtractCtx,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        let mut stack = vec![TraverseFrame::Node(root, 0)];

        while let Some(frame) = stack.pop() {
            match frame {
                TraverseFrame::PopNamespace => ctx.pop_namespace(),
                TraverseFrame::Node(node, depth) => {
                    if depth > MAX_TREE_DEPTH {
                        continue;
                    }

                    match node.kind() {
                        "namespace_declaration" => {
                            if let Some(ns) = namespace_name(node, source) {
                                ctx.push_qualified_namespace(&ns);
                                stack.push(TraverseFrame::PopNamespace);
                                if let Some(body) = node.child_by_field_name("body") {
                                    let mut cursor = body.walk();
                                    let children: Vec<Node<'_>> =
                                        body.children(&mut cursor).collect();
                                    for child in children.into_iter().rev() {
                                        stack.push(TraverseFrame::Node(child, depth + 1));
                                    }
                                }
                            }
                            continue;
                        }
                        "file_scoped_namespace_declaration" => continue,
                        _ => {
                            self.emit_symbol_for_node(node, source, file_path, ctx, symbols)?;
                            push_traverse_children(&mut stack, node, depth);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn walk_csharp_calls(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        ctx: &ExtractCtx,
        relations: &mut Vec<Relation>,
    ) {
        let function_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .collect();
        let class_fields: HashMap<String, Vec<Field>> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Class)
            .flat_map(|s| {
                let fields = s.fields.clone();
                let mut entries = vec![(s.name.clone(), fields.clone())];
                if let Some(qn) = &s.qualified_name {
                    entries.push((qn.clone(), fields));
                }
                entries
            })
            .collect();
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }

            if node.kind() == "invocation_expression" {
                if let Some(from_fn) = containing_function(node, &function_symbols) {
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
                            self.infer_csharp_call_hints(node, source, ctx, from_fn, &class_fields);
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
                            metadata: serde_json::json!({ "language": "csharp" }),
                            to_qualified_hint,
                            to_type_hint,
                            ..Default::default()
                        });
                    }
                }
            }

            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }
    }

    fn infer_csharp_call_hints(
        &self,
        invocation: Node,
        source: &[u8],
        ctx: &ExtractCtx,
        from_fn: &Symbol,
        class_fields: &HashMap<String, Vec<Field>>,
    ) -> (Option<String>, Option<String>) {
        let Some(func) = invocation.child_by_field_name("function") else {
            return (None, None);
        };
        let Some(method_name) = callee_name(func, source) else {
            return (None, None);
        };
        match func.kind() {
            "member_access_expression" => {
                let Some(object) = func.child_by_field_name("expression") else {
                    return (None, None);
                };
                if let Some(type_name) = self.resolve_receiver_type(
                    object,
                    source,
                    invocation,
                    ctx,
                    from_fn,
                    class_fields,
                ) {
                    let simple = simple_type_name(&type_name);
                    let qualified = if type_name.contains('.') {
                        format!("{type_name}.{method_name}")
                    } else {
                        ctx.hint_from_usings(&simple)
                            .map(|ns| format!("{ns}.{method_name}"))
                            .unwrap_or_else(|| format!("{simple}.{method_name}"))
                    };
                    return (Some(simple), Some(qualified));
                }
            }
            "identifier" | "generic_name" => {
                if let Some(hint) = ctx.hint_from_usings(&method_name) {
                    return (
                        Some(method_name.clone()),
                        Some(format!("{hint}.{method_name}")),
                    );
                }
            }
            _ => {}
        }
        (None, None)
    }

    fn resolve_receiver_type(
        &self,
        object: Node,
        source: &[u8],
        call_site: Node,
        ctx: &ExtractCtx,
        from_fn: &Symbol,
        class_fields: &HashMap<String, Vec<Field>>,
    ) -> Option<String> {
        match object.kind() {
            "identifier" => {
                let name = object.utf8_text(source).ok()?;
                if name == "this" {
                    return from_fn
                        .qualified_name
                        .as_ref()
                        .and_then(|qn| qn.rsplit_once('.').map(|(ty, _)| ty.to_string()));
                }
                if let Some(class_node) = self.find_containing_type_node(call_site) {
                    let class_name = type_name(class_node, source)?;
                    if let Some(fields) = class_fields.get(&class_name) {
                        if let Some(field) = fields.iter().find(|f| f.name == name) {
                            return field.field_type.clone();
                        }
                    }
                }
                if let Some(ns) = ctx.hint_from_usings(name) {
                    return Some(simple_type_name(&ns));
                }
                self.find_local_variable_type(call_site, name, source)
            }
            "member_access_expression" => {
                let inner = object.child_by_field_name("expression")?;
                let field = object
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())?;
                if inner.kind() == "this" || inner.utf8_text(source).ok() == Some("this") {
                    if let Some(class_node) = self.find_containing_type_node(call_site) {
                        let class_name = type_name(class_node, source)?;
                        if let Some(fields) = class_fields.get(&class_name) {
                            if let Some(f) = fields.iter().find(|f| f.name == field) {
                                return f.field_type.clone();
                            }
                        }
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
        let method = self.find_containing_callable_node(start_node)?;
        let mut cursor = method.walk();
        let mut stack: Vec<Node> = method.children(&mut cursor).collect();
        while let Some(node) = stack.pop() {
            match node.kind() {
                "local_declaration_statement" | "variable_declaration" => {
                    let var_decl = if node.kind() == "local_declaration_statement" {
                        find_direct_child_kind(node, "variable_declaration")
                    } else {
                        Some(node)
                    };
                    if let Some(var_decl) = var_decl {
                        let ty = var_decl
                            .child_by_field_name("type")
                            .and_then(|n| type_text(n, source));
                        let mut dc = var_decl.walk();
                        for child in var_decl.children(&mut dc) {
                            if child.kind() == "variable_declarator" {
                                if let Some(name) = child.child_by_field_name("name") {
                                    if name.utf8_text(source).ok() == Some(var_name) {
                                        return ty;
                                    }
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
            if node.start_byte() < start_node.start_byte() {
                let mut c = node.walk();
                stack.extend(node.children(&mut c));
            }
        }
        None
    }

    fn annotated_with_from(
        &self,
        node: Node,
        source: &[u8],
        _file_path: &Path,
        ctx: &ExtractCtx,
    ) -> Option<String> {
        match node.kind() {
            "class_declaration"
            | "struct_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "record_declaration" => type_name(node, source).map(|n| ctx.qualify(&n)),
            "method_declaration" | "local_function_statement" => {
                let name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                name.and_then(|name| {
                    self.find_containing_type_name(node, source)
                        .map(|ty| format!("{}.{}", ctx.qualify(&ty), name))
                })
            }
            "constructor_declaration" => self
                .find_containing_type_name(node, source)
                .map(|ty| format!("{}.{}", ctx.qualify(&ty), "<init>")),
            "property_declaration" | "parameter" => {
                let prop = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                self.find_containing_type_name(node, source)
                    .zip(prop)
                    .map(|(ty, p)| format!("{}.{}", ctx.qualify(&ty), p))
            }
            _ => None,
        }
    }

    fn extract_annotated_with(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        ctx: &ExtractCtx,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let file_path_str = file_path.to_string_lossy();
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }

            if let Some(from) = self.annotated_with_from(node, source, file_path, ctx) {
                for (attr_name, args) in collect_attributes(node, source) {
                    let mut meta = serde_json::json!({ "language": "csharp" });
                    if let Some(args) = args {
                        meta["arguments"] = serde_json::Value::String(args);
                    }
                    relations.push(Relation {
                        from: from.clone(),
                        to: attr_name,
                        relation_type: RelationType::AnnotatedWith,
                        location: source_location(node, &file_path_str),
                        metadata: meta,
                        to_qualified_hint: None,
                        to_type_hint: None,
                        ..Default::default()
                    });
                }
            }

            push_tree_children(&mut stack, node, depth);
        }

        Ok(())
    }

    fn extract_instantiations(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        ctx: &ExtractCtx,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let function_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .collect();
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }

            if matches!(
                node.kind(),
                "object_creation_expression"
                    | "implicit_object_creation_expression"
                    | "array_creation_expression"
                    | "implicit_array_creation_expression"
                    | "anonymous_object_creation_expression"
                    | "stackalloc_expression"
                    | "implicit_stackalloc_expression"
            ) {
                if let Some(from_fn) = containing_function(node, &function_symbols) {
                    let from = from_fn
                        .qualified_name
                        .clone()
                        .unwrap_or_else(|| from_fn.name.clone());
                    if let Some(type_node) = node.child_by_field_name("type") {
                        if let Some(target) = type_text(type_node, source) {
                            let simple = simple_type_name(&target);
                            let scoped = if target.contains('.') {
                                target.clone()
                            } else {
                                ctx.hint_from_usings(&simple)
                                    .unwrap_or_else(|| ctx.qualify(&simple))
                            };
                            relations.push(Relation {
                                from,
                                to: simple,
                                relation_type: RelationType::Instantiates,
                                location: source_location(node, &file_path.to_string_lossy()),
                                metadata: serde_json::json!({
                                    "language": "csharp",
                                    "ast_kind": node.kind(),
                                }),
                                to_qualified_hint: Some(scoped),
                                to_type_hint: None,
                                ..Default::default()
                            });
                        }
                    } else if matches!(
                        node.kind(),
                        "implicit_object_creation_expression"
                            | "anonymous_object_creation_expression"
                            | "implicit_array_creation_expression"
                            | "implicit_stackalloc_expression"
                    ) {
                        let target = self
                            .find_containing_type_name(node, source)
                            .unwrap_or_else(|| node.kind().to_string());
                        let simple = simple_type_name(&target);
                        relations.push(Relation {
                            from,
                            to: simple.clone(),
                            relation_type: RelationType::Instantiates,
                            location: source_location(node, &file_path.to_string_lossy()),
                            metadata: serde_json::json!({
                                "language": "csharp",
                                "ast_kind": node.kind(),
                                "implicit": true,
                            }),
                            to_qualified_hint: Some(ctx.qualify(&simple)),
                            to_type_hint: None,
                            ..Default::default()
                        });
                    }
                }
            } else if matches!(
                node.kind(),
                "member_access_expression"
                    | "conditional_access_expression"
                    | "element_access_expression"
                    | "typeof_expression"
                    | "sizeof_expression"
            ) {
                if let Some(from_fn) = containing_function(node, &function_symbols) {
                    let from = from_fn
                        .qualified_name
                        .clone()
                        .unwrap_or_else(|| from_fn.name.clone());
                    let to = match node.kind() {
                        "typeof_expression" | "sizeof_expression" => node
                            .child_by_field_name("type")
                            .and_then(|n| type_text(n, source))
                            .unwrap_or_else(|| node.kind().to_string()),
                        _ => node
                            .child_by_field_name("name")
                            .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
                            .or_else(|| callee_name(node, source))
                            .unwrap_or_else(|| node.kind().to_string()),
                    };
                    relations.push(Relation {
                        from,
                        to,
                        relation_type: RelationType::References,
                        location: source_location(node, &file_path.to_string_lossy()),
                        metadata: serde_json::json!({
                            "language": "csharp",
                            "ast_kind": node.kind(),
                        }),
                        to_qualified_hint: None,
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

    fn extract_inheritance(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }

            let skip_children = match node.kind() {
                "class_declaration" => {
                    let class_name = type_name(node, source).unwrap_or_default();
                    if class_name.is_empty() {
                        true
                    } else {
                        if let Some(base_list) = node.child_by_field_name("bases") {
                            collect_base_list_relations(
                                &class_name,
                                base_list,
                                source,
                                file_path,
                                true,
                                relations,
                            );
                        } else if let Some(base_list) = find_child_kind(node, "base_list") {
                            collect_base_list_relations(
                                &class_name,
                                base_list,
                                source,
                                file_path,
                                true,
                                relations,
                            );
                        }
                        false
                    }
                }
                "interface_declaration" => {
                    let name = type_name(node, source).unwrap_or_default();
                    if name.is_empty() {
                        true
                    } else {
                        if let Some(base_list) = node.child_by_field_name("bases") {
                            collect_base_list_relations(
                                &name, base_list, source, file_path, false, relations,
                            );
                        } else if let Some(base_list) = find_child_kind(node, "base_list") {
                            collect_base_list_relations(
                                &name, base_list, source, file_path, false, relations,
                            );
                        }
                        false
                    }
                }
                _ => false,
            };

            if !skip_children {
                push_tree_children(&mut stack, node, depth);
            }
        }

        Ok(())
    }

    fn find_containing_type_name(&self, node: Node, source: &[u8]) -> Option<String> {
        self.find_containing_type_node(node)
            .and_then(|n| type_name(n, source))
    }

    fn find_containing_type_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if matches!(
                parent.kind(),
                "class_declaration"
                    | "struct_declaration"
                    | "interface_declaration"
                    | "record_declaration"
            ) {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    fn find_containing_callable_node<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if matches!(
                parent.kind(),
                "method_declaration" | "constructor_declaration" | "local_function_statement"
            ) {
                return Some(parent);
            }
            current = parent;
        }
        None
    }

    fn find_function_at_line<'a>(&self, root: Node<'a>, line: usize) -> Option<Node<'a>> {
        let mut stack = vec![(root, 0usize)];
        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }
            if matches!(
                node.kind(),
                "method_declaration" | "constructor_declaration" | "local_function_statement"
            ) && node.start_position().row == line
            {
                return Some(node);
            }
            push_tree_children(&mut stack, node, depth);
        }
        None
    }

    fn calculate_cyclomatic(&self, node: Node) -> usize {
        let mut complexity = 1;
        let mut stack = vec![(node, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for child in &children {
                match child.kind() {
                    "if_statement"
                    | "while_statement"
                    | "for_statement"
                    | "foreach_statement"
                    | "catch_clause"
                    | "conditional_expression"
                    | "switch_expression"
                    | "switch_section" => {
                        complexity += 1;
                    }
                    _ => {}
                }
            }
            for child in children.into_iter().rev() {
                stack.push((child, depth + 1));
            }
        }

        complexity
    }

    fn calculate_cognitive(&self, node: Node) -> usize {
        let mut cognitive = 0;
        let mut stack = vec![(node, 0usize, 0usize)];

        while let Some((node, depth, nesting)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for child in &children {
                match child.kind() {
                    "if_statement" | "while_statement" | "for_statement" | "foreach_statement" => {
                        cognitive += 1 + nesting;
                    }
                    "switch_expression" | "catch_clause" | "switch_statement" => {
                        cognitive += 1 + nesting;
                    }
                    _ => {}
                }
            }
            for child in children.into_iter().rev() {
                let child_nesting = match child.kind() {
                    "if_statement" | "while_statement" | "for_statement" | "foreach_statement" => {
                        nesting + 1
                    }
                    _ => nesting,
                };
                stack.push((child, depth + 1, child_nesting));
            }
        }

        cognitive
    }

    fn count_loc(&self, node: Node) -> usize {
        (node.end_position().row - node.start_position().row + 1).max(1)
    }

    fn count_nesting_depth(&self, node: Node) -> usize {
        let mut max_depth = 0;
        let mut stack = vec![(node, 0usize, 0usize)];

        while let Some((node, depth, current_depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }
            max_depth = max_depth.max(current_depth);
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                let child_depth = if matches!(
                    child.kind(),
                    "if_statement" | "while_statement" | "for_statement" | "block"
                ) {
                    current_depth + 1
                } else {
                    current_depth
                };
                stack.push((child, depth + 1, child_depth));
            }
        }

        max_depth
    }

    fn count_returns(&self, node: Node) -> usize {
        let mut count = 0;
        let mut stack = vec![(node, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }
            if node.kind() == "return_statement" {
                count += 1;
            }
            push_tree_children(&mut stack, node, depth);
        }

        count
    }
}

impl LanguagePlugin for CSharpPlugin {
    fn language_id(&self) -> &str {
        "csharp"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["cs"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_c_sharp::LANGUAGE.into())
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
            "csharp",
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
        Some(csharp_ast_handlers())
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

        let tree = parse_with_pool("csharp", tree_sitter_c_sharp::LANGUAGE.into(), source)
            .map_err(|e| Error::ParseError {
                file: symbol.location.file.clone().into(),
                line: symbol.location.start_line,
                message: e,
            })?;

        let root = tree.root_node();
        let target_line = symbol.location.start_line.saturating_sub(1);

        if let Some(func_node) = self.find_function_at_line(root, target_line) {
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

fn collect_attributes(node: Node, source: &[u8]) -> Vec<(String, Option<String>)> {
    let mut attrs = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "attribute_list" {
            collect_attributes_from_list(child, source, &mut attrs);
        }
    }
    attrs
}

fn collect_attributes_from_list(
    list: Node,
    source: &[u8],
    attrs: &mut Vec<(String, Option<String>)>,
) {
    let mut cursor = list.walk();
    for child in list.children(&mut cursor) {
        if child.kind() == "attribute" {
            let name = child
                .child_by_field_name("name")
                .and_then(|n| type_text(n, source))
                .unwrap_or_default();
            let args = child
                .child_by_field_name("arguments")
                .or_else(|| find_direct_child_kind(child, "attribute_argument_list"))
                .and_then(|n| n.utf8_text(source).ok().map(str::to_string));
            if !name.is_empty() {
                attrs.push((simple_type_name(&name), args));
            }
        }
    }
}

pub(crate) fn namespace_name(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| type_text(n, source))
}

fn type_text(node: Node, source: &[u8]) -> Option<String> {
    node.utf8_text(source).ok().map(|s| s.trim().to_string())
}

fn simple_type_name(raw: &str) -> String {
    raw.split('<')
        .next()
        .unwrap_or(raw)
        .rsplit('.')
        .next()
        .unwrap_or(raw)
        .to_string()
}

fn collect_base_list_relations(
    from: &str,
    base_list: Node,
    source: &[u8],
    file_path: &Path,
    class_style: bool,
    relations: &mut Vec<Relation>,
) {
    let bases: Vec<String> = base_list
        .children(&mut base_list.walk())
        .filter(|c| c.is_named() && c.kind() == "identifier")
        .filter_map(|c| c.utf8_text(source).ok().map(str::to_string))
        .collect();

    if bases.is_empty() {
        return;
    }

    if class_style {
        if let Some(base) = bases.first() {
            relations.push(relation(
                from,
                base,
                RelationType::Extends,
                base_list,
                file_path,
            ));
        }
        for iface in bases.iter().skip(1) {
            relations.push(relation(
                from,
                iface,
                RelationType::Implements,
                base_list,
                file_path,
            ));
        }
    } else {
        for iface in &bases {
            relations.push(relation(
                from,
                iface,
                RelationType::Implements,
                base_list,
                file_path,
            ));
        }
    }
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
        metadata: serde_json::json!({ "language": "csharp" }),
        to_qualified_hint: None,
        to_type_hint: None,
        ..Default::default()
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

fn first_line(node: Node, source: &[u8]) -> String {
    node.utf8_text(source)
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn modifier_texts(node: Node, source: &[u8]) -> Vec<String> {
    node.children(&mut node.walk())
        .filter(|c| c.kind() == "modifier")
        .filter_map(|c| c.utf8_text(source).ok().map(str::to_string))
        .collect()
}

fn type_name(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .or_else(|| find_child_kind(node, "identifier"))
        .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
}

fn method_return_type(node: Node, source: &[u8]) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(
            child.kind(),
            "predefined_type" | "identifier" | "generic_name" | "nullable_type"
        ) {
            return type_text(child, source);
        }
    }
    None
}

fn find_child_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == kind {
            return Some(child);
        }
        if let Some(found) = find_child_kind(child, kind) {
            return Some(found);
        }
    }
    None
}

fn find_direct_child_kind<'a>(node: Node<'a>, kind: &str) -> Option<Node<'a>> {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|&child| child.kind() == kind)
}

fn field_visibility(node: Node, source: &[u8]) -> Option<String> {
    let mods = modifier_texts(node, source);
    if mods.is_empty() {
        None
    } else {
        Some(mods.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn plugin() -> CSharpPlugin {
        CSharpPlugin::new().unwrap()
    }

    #[test]
    fn test_extract_csharp_class_and_method() {
        let source = br#"
using System;

public class UserService {
    public string Authenticate(string token) {
        return token;
    }
}
"#;
        let symbols = plugin()
            .extract_symbols(Path::new("UserService.cs"), source)
            .unwrap();
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "Authenticate"));
        let auth = symbols.iter().find(|s| s.name == "Authenticate").unwrap();
        assert_eq!(auth.parameters.len(), 1);
        assert_eq!(auth.parameters[0].name, "token");
        assert!(
            auth.parameters[0]
                .param_type
                .as_deref()
                .is_some_and(|t| !t.is_empty()),
            "expected typed parameter, got {:?}",
            auth.parameters[0].param_type
        );
    }

    #[test]
    fn test_extract_csharp_fields_and_constructor() {
        let source = br#"
public class OrderDTO {
    private string orderId;
    private string status;

    public OrderDTO(string orderId, string status) {
        this.orderId = orderId;
        this.status = status;
    }

    public void MarkProcessed() {
        this.status = "PROCESSED";
    }
}
"#;
        let symbols = plugin()
            .extract_symbols(Path::new("OrderDTO.cs"), source)
            .unwrap();
        let class = symbols
            .iter()
            .find(|s| s.name == "OrderDTO" && s.symbol_type == SymbolType::Class)
            .expect("class");
        let status = class
            .fields
            .iter()
            .find(|f| f.name == "status")
            .expect("status field");
        assert!(
            status.field_type.as_deref().is_some_and(|t| !t.is_empty()),
            "expected status field type, got {:?}",
            status.field_type
        );
        let ctor = symbols
            .iter()
            .find(|s| {
                s.symbol_type == SymbolType::Function
                    && s.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true)
            })
            .expect("constructor");
        assert!(
            ctor.qualified_name
                .as_deref()
                .is_some_and(|qn| qn.ends_with(".<init>")),
            "expected .<init> qn, got {:?}",
            ctor.qualified_name
        );
        assert_eq!(ctor.parameters.len(), 2);
        let method = symbols.iter().find(|s| s.name == "MarkProcessed").unwrap();
        assert!(method.parameters.is_empty());
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
public class Example {
    public void Foo() {
        Bar();
    }
    public void Bar() {}
}
"#;
        let path = Path::new("Example.cs");
        let p = plugin();
        let symbols = p.extract_symbols(path, source).unwrap();
        let relations = p.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls)),
            "expected Calls relations, got {relations:?}"
        );
    }

    #[test]
    fn test_extract_relations_inheritance() {
        let source = br#"public class ServiceImpl : BaseService, IService {}"#;
        let path = Path::new("Service.cs");
        let p = plugin();
        let symbols = p.extract_symbols(path, source).unwrap();
        let relations = p.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Extends)),
            "missing Extends: {relations:?}"
        );
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Implements)),
            "missing Implements: {relations:?}"
        );
    }

    #[test]
    fn test_nested_namespace_fqn() {
        let source = br#"
namespace A {
    namespace B {
        public class C {
            public void M() {}
        }
    }
}
"#;
        let symbols = plugin()
            .extract_symbols(Path::new("Nested.cs"), source)
            .unwrap();
        let method = symbols.iter().find(|s| s.name == "M").unwrap();
        assert_eq!(
            method.qualified_name.as_deref(),
            Some("A.B.C.M"),
            "got {:?}",
            method.qualified_name
        );
    }

    #[test]
    fn test_file_scoped_namespace_fqn() {
        let source = br#"
namespace A.B.C;

public class Type {
    public void M() {}
}
"#;
        let symbols = plugin()
            .extract_symbols(Path::new("FileScoped.cs"), source)
            .unwrap();
        let ty = symbols.iter().find(|s| s.name == "Type").unwrap();
        assert_eq!(ty.qualified_name.as_deref(), Some("A.B.C.Type"));
        let method = symbols.iter().find(|s| s.name == "M").unwrap();
        assert_eq!(method.qualified_name.as_deref(), Some("A.B.C.Type.M"));
    }

    #[test]
    fn test_annotated_with_http_get() {
        let source = br#"
public class Api {
    [HttpGet("x")]
    public void Get() {}
}
"#;
        let path = Path::new("Api.cs");
        let p = plugin();
        let symbols = p.extract_symbols(path, source).unwrap();
        let relations = p.extract_relations(path, source, &symbols).unwrap();
        let ann = relations
            .iter()
            .find(|r| r.relation_type == RelationType::AnnotatedWith && r.to == "HttpGet")
            .expect("AnnotatedWith HttpGet");
        assert!(
            ann.metadata.get("arguments").is_some(),
            "expected arguments metadata: {:?}",
            ann.metadata
        );
    }

    #[test]
    fn test_field_injection_call_hint() {
        let source = br#"
public interface IOrderRepository {
    void Save();
}

public class Svc {
    private readonly IOrderRepository _repo;
    public Svc(IOrderRepository repo) { _repo = repo; }
    public void Run() { _repo.Save(); }
}
"#;
        let path = Path::new("Svc.cs");
        let p = plugin();
        let symbols = p.extract_symbols(path, source).unwrap();
        let relations = p.extract_relations(path, source, &symbols).unwrap();
        let call = relations
            .iter()
            .find(|r| {
                r.relation_type == RelationType::Calls
                    && (r.to == "Save" || r.to.ends_with(".Save"))
            })
            .expect("Save call");
        assert_eq!(
            call.to_type_hint.as_deref(),
            Some("IOrderRepository"),
            "got {:?}",
            call
        );
    }

    #[test]
    fn test_static_call_using_hint() {
        let source = br#"
using System;

public class T {
    public void M() { Console.WriteLine("x"); }
}
"#;
        let path = Path::new("T.cs");
        let p = plugin();
        let symbols = p.extract_symbols(path, source).unwrap();
        let relations = p.extract_relations(path, source, &symbols).unwrap();
        let call = relations
            .iter()
            .find(|r| r.relation_type == RelationType::Calls && r.to.contains("WriteLine"))
            .expect("WriteLine call");
        assert!(
            call.to_qualified_hint
                .as_deref()
                .is_some_and(|h| h.contains("Console")),
            "got {:?}",
            call.to_qualified_hint
        );
    }

    #[test]
    fn test_instantiates_new() {
        let source = br#"
public class OrderDto {}
public class S {
    public void M() { var x = new OrderDto(); }
}
"#;
        let path = Path::new("S.cs");
        let p = plugin();
        let symbols = p.extract_symbols(path, source).unwrap();
        let relations = p.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Instantiates && r.to == "OrderDto"),
            "expected Instantiates: {relations:?}"
        );
    }

    #[test]
    fn test_record_and_complexity() {
        let source = br#"
public record R(int Id);

public class C {
    public int F(int x) {
        if (x > 0) return 1;
        return 0;
    }
}
"#;
        let path = Path::new("R.cs");
        let p = plugin();
        let symbols = p.extract_symbols(path, source).unwrap();
        let rec = symbols.iter().find(|s| s.name == "R").unwrap();
        assert_eq!(
            rec.metadata.get("is_record").and_then(|v| v.as_bool()),
            Some(true)
        );
        let method = symbols.iter().find(|s| s.name == "F").unwrap();
        let metrics = p.calculate_complexity(method, source).unwrap().unwrap();
        assert!(metrics.cyclomatic >= 2);
    }

    #[test]
    #[ignore = "requires example/roslyn corpus (ErrorFacts.cs deep AST)"]
    fn test_extract_deep_roslyn_errorfacts_without_stack_overflow() {
        let path = Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../example/roslyn/src/Compilers/CSharp/Portable/Errors/ErrorFacts.cs"
        ));
        if !path.is_file() {
            eprintln!("skip: {}", path.display());
            return;
        }
        let source = std::fs::read(path).unwrap();
        let p = plugin();
        let result = p.extract_all(path, &source).unwrap();
        assert!(
            result.symbols.iter().any(|s| s.name == "ErrorFacts"),
            "expected ErrorFacts type symbol"
        );
    }
}
