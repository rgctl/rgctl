//! JavaScript language plugin
//!
//! Extracts symbols, relationships, and complexity metrics from JavaScript source code
//! using TreeSitter.

use crate::single_pass::{extract_all_single_pass, javascript_ast_handlers};
use rgctl_plugin_api::{
    AstHandlerTable, ComplexityMetrics, Error, ExtractAllResult, Field, JS_CALL_KINDS,
    LanguagePlugin, Parameter, Relation, RelationType, Result, SourceLocation, Symbol, SymbolType,
    callee_name, complexity_from_metadata, containing_function, parse_with_pool,
};
use rgctl_plugin_helpers::{
    bound_function_expression_name, extract_cjs_require_symbols, extract_class_extends_relations,
    extract_import_symbols, is_ecmascript_function_node, is_function_expression_kind,
    simple_type_name, type_name_from_node,
};
use rgctl_semantic::type_inference::TypeInferencer;
use std::path::Path;
use tree_sitter::Node;

/// Cap AST walk depth (matches CFG expression walk limit in `rgctl-analysis`).
const MAX_TREE_DEPTH: usize = 2048;

fn push_tree_children<'a>(stack: &mut Vec<(Node<'a>, usize)>, node: Node<'a>, depth: usize) {
    let mut cursor = node.walk();
    let children: Vec<Node<'a>> = node.children(&mut cursor).collect();
    for child in children.into_iter().rev() {
        stack.push((child, depth + 1));
    }
}

/// JavaScript language plugin
pub struct JavaScriptPlugin;

impl JavaScriptPlugin {
    /// Create a new JavaScript plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    fn find_containing_class_name(&self, node: Node, source: &[u8]) -> Option<String> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "class_declaration" {
                let mut cursor = parent.walk();
                for child in parent.children(&mut cursor) {
                    if child.kind() == "identifier" {
                        return child.utf8_text(source).ok().map(str::to_string);
                    }
                }
            }
            current = parent;
        }
        None
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

        // Skip regex-based inference when there are no params (common for arrows /
        // keyword-filtered nodes). Compiling/running inference per function was a
        // Node-corpus hot-path cost (AGENTS.md clone / CPU discipline).
        if !parameters.is_empty() {
            let function_source = node.utf8_text(source).unwrap_or("");
            let inferred_types = TypeInferencer::new().infer_javascript(function_source);
            for param in &mut parameters {
                if param.param_type.is_none() {
                    if let Some(inference) = inferred_types.get(&param.name) {
                        param.param_type = Some(format!("{:?}", inference.inferred));
                    }
                }
            }
        }

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
                serde_json::json!({ "language": "javascript", "is_constructor": true }),
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
                serde_json::json!({ "language": "javascript" }),
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
            return_type: None,
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

    fn extract_parameters(&self, params_node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut parameters = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            if child.kind() == "identifier" {
                parameters.push(Parameter {
                    name: child.utf8_text(source)?.to_string(),
                    param_type: None,
                    default_value: None,
                });
            } else if child.kind() == "assignment_pattern" {
                let mut assign_cursor = child.walk();
                let mut name = None;
                let mut default = None;

                for assign_child in child.children(&mut assign_cursor) {
                    if assign_child.kind() == "identifier" {
                        name = Some(assign_child.utf8_text(source)?.to_string());
                    } else if name.is_some() {
                        default = Some(assign_child.utf8_text(source)?.to_string());
                    }
                }

                if let Some(name) = name {
                    parameters.push(Parameter {
                        name,
                        param_type: None,
                        default_value: default,
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
                "identifier" => {
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

        let name = name.unwrap_or_else(|| "AnonymousClass".to_string());

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
            metadata: serde_json::json!({ "language": "javascript" }),
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
        let mut seen = std::collections::HashSet::new();
        let mut cursor = class_body.walk();

        for child in class_body.children(&mut cursor) {
            match child.kind() {
                "field_definition" | "public_field_definition" => {
                    let name = child
                        .child_by_field_name("property")
                        .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
                        .or_else(|| {
                            let mut c = child.walk();
                            for n in child.children(&mut c) {
                                if n.kind() == "property_identifier" {
                                    return n.utf8_text(source).ok().map(str::to_string);
                                }
                            }
                            None
                        });
                    if let Some(name) = name {
                        if seen.insert(name.clone()) {
                            fields.push(Field {
                                name,
                                field_type: None,
                                visibility: None,
                            });
                        }
                    }
                }
                "method_definition" => {
                    let method_name = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
                        .or_else(|| {
                            let mut c = child.walk();
                            for n in child.children(&mut c) {
                                if matches!(n.kind(), "property_identifier" | "identifier") {
                                    return n.utf8_text(source).ok().map(str::to_string);
                                }
                            }
                            None
                        });
                    if method_name.as_deref() == Some("constructor") {
                        self.collect_this_assignments(child, source, &mut fields, &mut seen)?;
                    }
                }
                _ => {}
            }
        }

        Ok(fields)
    }

    fn collect_this_assignments(
        &self,
        node: Node,
        source: &[u8],
        fields: &mut Vec<Field>,
        seen: &mut std::collections::HashSet<String>,
    ) -> Result<()> {
        let mut stack = vec![(node, 0usize)];
        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }
            if node.kind() == "assignment_expression" {
                if let Some(left) = node.child_by_field_name("left") {
                    if left.kind() == "member_expression" {
                        let object = left.child_by_field_name("object");
                        let property = left.child_by_field_name("property");
                        let is_this = object
                            .and_then(|o| o.utf8_text(source).ok())
                            .is_some_and(|t| t == "this");
                        if is_this {
                            if let Some(name) = property
                                .and_then(|p| p.utf8_text(source).ok())
                                .map(str::to_string)
                            {
                                if seen.insert(name.clone()) {
                                    fields.push(Field {
                                        name,
                                        field_type: None,
                                        visibility: None,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            push_tree_children(&mut stack, node, depth);
        }
        Ok(())
    }

    /// Stable export identity: binding names, never the declaration body.
    fn export_binding_identity(node: Node, source: &[u8]) -> String {
        let mut names = Vec::new();
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "function_declaration"
                | "generator_function_declaration"
                | "generator_function"
                | "function"
                | "class_declaration"
                | "class" => {
                    if let Some(name) = child
                        .child_by_field_name("name")
                        .and_then(|n| n.utf8_text(source).ok())
                    {
                        names.push(name.to_string());
                    } else {
                        names.push("default".to_string());
                    }
                }
                "lexical_declaration" | "variable_declaration" => {
                    let mut decl_cursor = child.walk();
                    for decl in child.children(&mut decl_cursor) {
                        if decl.kind() == "variable_declarator"
                            && let Some(name) = decl
                                .child_by_field_name("name")
                                .and_then(|n| n.utf8_text(source).ok())
                        {
                            names.push(name.to_string());
                        }
                    }
                }
                "export_clause" => {
                    let mut clause = child.walk();
                    for spec in child.children(&mut clause) {
                        if spec.kind() != "export_specifier" {
                            continue;
                        }
                        let name_node = spec
                            .child_by_field_name("alias")
                            .or_else(|| spec.child_by_field_name("name"));
                        if let Some(name) = name_node.and_then(|n| n.utf8_text(source).ok()) {
                            names.push(name.to_string());
                        }
                    }
                }
                "*" | "namespace_export" => names.push("*".to_string()),
                _ => {}
            }
        }
        if names.is_empty() {
            let fallback = node
                .utf8_text(source)
                .ok()
                .filter(|text| text.contains("default"))
                .map(|_| "default")
                .unwrap_or("clause");
            names.push(fallback.to_string());
        }
        format!("export:{}", names.join(","))
    }

    pub(crate) fn extract_export_symbol(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Option<Symbol> {
        let raw = source
            .get(node.start_byte()..node.end_byte())?
            .trim_ascii_start();
        if !raw.starts_with(b"export") {
            return None;
        }
        // Identity is the exported binding, not the statement text. Inline
        // `export function` bodies must not churn the stable key on an edit.
        let name = Self::export_binding_identity(node, source);
        let mut symbol = Symbol {
            name,
            symbol_type: SymbolType::Import,
            qualified_name: None,
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "javascript",
                "direction": "export",
            }),
            ..Default::default()
        };
        rgctl_plugin_helpers::record_symbol_spans(&mut symbol, node, None);
        Some(symbol)
    }

    fn emit_symbol_for_node(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        if is_ecmascript_function_node(node) {
            symbols.push(self.extract_function(node, source, file_path)?);
        }
        match node.kind() {
            "class_declaration" => {
                symbols.push(self.extract_class(node, source, file_path)?);
            }
            "import_statement" => {
                symbols.extend(extract_import_symbols(
                    node,
                    source,
                    file_path,
                    "javascript",
                ));
            }
            "export_statement" => {
                if let Some(sym) = self.extract_export_symbol(node, source, file_path) {
                    symbols.push(sym);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn symbols_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
    ) -> Result<Vec<Symbol>> {
        let mut symbols = Vec::new();
        let file_path_str = file_path.to_string_lossy();
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }
            self.emit_symbol_for_node(node, source, &file_path_str, &mut symbols)?;
            push_tree_children(&mut stack, node, depth);
        }

        symbols.extend(extract_cjs_require_symbols(
            root,
            source,
            &file_path_str,
            "javascript",
        ));
        Ok(symbols)
    }

    fn walk_js_calls(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) {
        let function_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .collect();
        let file_path_str = file_path.to_string_lossy();
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }

            if JS_CALL_KINDS.contains(&node.kind()) {
                if let Some(from_fn) = containing_function(node, &function_symbols) {
                    let func = node.child_by_field_name("function");
                    let unresolved = func
                        .map(|f| self.is_unresolved_callee(f, source))
                        .unwrap_or(false)
                        || func.is_some_and(|f| f.kind() == "subscript_expression");
                    let callee = func
                        .and_then(|n| callee_name(n, source))
                        .or_else(|| callee_name(node, source));
                    let callee = if unresolved && callee.as_deref().unwrap_or("").is_empty() {
                        Some("dynamic".to_string())
                    } else {
                        callee
                    };
                    if let Some(callee) = callee.filter(|c| !c.is_empty()) {
                        let from = from_fn
                            .qualified_name
                            .clone()
                            .unwrap_or_else(|| from_fn.name.clone());
                        let mut meta = serde_json::json!({ "language": "javascript" });
                        if unresolved {
                            meta["unresolved"] = serde_json::Value::Bool(true);
                        }
                        let same_file_matches: Vec<_> = symbols
                            .iter()
                            .filter(|s| {
                                s.name == callee
                                    && s.symbol_type == SymbolType::Function
                                    && s.location.file == file_path_str
                            })
                            .collect();
                        let local_target = match same_file_matches.as_slice() {
                            [only] => only
                                .qualified_name
                                .clone()
                                .unwrap_or_else(|| callee.clone()),
                            _ => callee.clone(),
                        };
                        relations.push(Relation {
                            from,
                            to: local_target,
                            relation_type: RelationType::Calls,
                            location: source_location(node, &file_path_str),
                            metadata: meta,
                            to_qualified_hint: None,
                            to_type_hint: None,
                            ..Default::default()
                        });
                    }
                }
            }

            push_tree_children(&mut stack, node, depth);
        }
    }

    pub(crate) fn is_unresolved_callee(&self, func: Node, source: &[u8]) -> bool {
        if func.kind() == "subscript_expression" {
            return true;
        }
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

    /// Emit a JS `Calls` relation with known enclosing function (single-pass flush).
    pub(crate) fn emit_js_call(
        &self,
        node: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        from_fn: &Symbol,
        relations: &mut Vec<Relation>,
    ) {
        let file_path_str = file_path.to_string_lossy();
        let func = node.child_by_field_name("function");
        let unresolved = func
            .map(|f| self.is_unresolved_callee(f, source))
            .unwrap_or(false)
            || func.is_some_and(|f| f.kind() == "subscript_expression");
        let callee = func
            .and_then(|n| callee_name(n, source))
            .or_else(|| callee_name(node, source));
        let callee = if unresolved && callee.as_deref().unwrap_or("").is_empty() {
            Some("dynamic".to_string())
        } else {
            callee
        };
        let Some(callee) = callee.filter(|c| !c.is_empty()) else {
            return;
        };
        let from = from_fn
            .qualified_name
            .clone()
            .unwrap_or_else(|| from_fn.name.clone());
        let mut meta = serde_json::json!({ "language": "javascript" });
        if unresolved {
            meta["unresolved"] = serde_json::Value::Bool(true);
        }
        let same_file_matches: Vec<_> = symbols
            .iter()
            .filter(|s| {
                s.name == callee
                    && s.symbol_type == SymbolType::Function
                    && s.location.file == file_path_str
            })
            .collect();
        let local_target = match same_file_matches.as_slice() {
            [only] => only
                .qualified_name
                .clone()
                .unwrap_or_else(|| callee.clone()),
            _ => callee.clone(),
        };
        relations.push(Relation {
            from,
            to: local_target,
            relation_type: RelationType::Calls,
            location: source_location(node, &file_path_str),
            metadata: meta,
            to_qualified_hint: None,
            to_type_hint: None,
            ..Default::default()
        });
    }

    fn extract_heritage(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) {
        let mut stack = vec![(root, 0usize)];
        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }
            if node.kind() == "class_declaration" {
                if let Some(name) = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                {
                    extract_class_extends_relations(
                        node,
                        source,
                        file_path,
                        name,
                        "javascript",
                        relations,
                    );
                }
            }
            push_tree_children(&mut stack, node, depth);
        }
    }

    fn extract_instantiations(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) {
        let function_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .collect();
        let file_path_str = file_path.to_string_lossy();
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }
            if node.kind() == "new_expression" {
                if let Some(from_fn) = containing_function(node, &function_symbols) {
                    let from = from_fn
                        .qualified_name
                        .clone()
                        .unwrap_or_else(|| from_fn.name.clone());
                    if let Some(ctor) = node.child_by_field_name("constructor") {
                        if let Some(target) = type_name_from_node(ctor, source) {
                            relations.push(Relation {
                                from,
                                to: simple_type_name(&target),
                                relation_type: RelationType::Instantiates,
                                location: source_location(node, &file_path_str),
                                metadata: serde_json::json!({ "language": "javascript" }),
                                to_qualified_hint: Some(target),
                                to_type_hint: None,
                                ..Default::default()
                            });
                        }
                    }
                }
            }
            push_tree_children(&mut stack, node, depth);
        }
    }

    fn relations_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let mut relations = Vec::new();
        self.walk_js_calls(root, source, file_path, symbols, &mut relations);
        self.extract_heritage(root, source, file_path, &mut relations);
        self.extract_instantiations(root, source, file_path, symbols, &mut relations);
        Ok(relations)
    }

    fn find_function_at_line<'a>(&self, root: Node<'a>, line: usize) -> Option<Node<'a>> {
        let mut stack = vec![(root, 0usize)];
        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_TREE_DEPTH {
                continue;
            }
            if matches!(
                node.kind(),
                "function_declaration" | "method_definition" | "arrow_function"
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
                    "if_statement" | "switch_statement" | "while_statement" | "for_statement"
                    | "catch_clause" | "ternary_expression" | "case" => {
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
                    "if_statement" | "while_statement" | "for_statement" => {
                        cognitive += 1 + nesting;
                    }
                    "switch_statement" | "catch_clause" => {
                        cognitive += 1 + nesting;
                    }
                    _ => {}
                }
            }
            for child in children.into_iter().rev() {
                let child_nesting = match child.kind() {
                    "if_statement" | "while_statement" | "for_statement" => nesting + 1,
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
                    "if_statement" | "while_statement" | "for_statement" | "statement_block"
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

    pub(crate) fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        parse_with_pool(
            "javascript",
            tree_sitter_javascript::LANGUAGE.into(),
            source,
        )
        .map_err(|e| Error::ParseError {
            file: file_path.to_path_buf(),
            line: 0,
            message: e,
        })
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
            "class_declaration" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string),
            "method_definition" => {
                let method = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok())
                    .map(str::to_string);
                method.and_then(|m| {
                    self.find_containing_class_name(node, source)
                        .map(|owner| format!("{owner}.{m}"))
                })
            }
            _ => None,
        };
        if let Some(from) = from {
            for (decorator_name, args) in js_decorators_for_node(node, source) {
                let mut meta = serde_json::json!({ "language": "javascript" });
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

    /// `export { foo as bar } from './m'` → Uses per specifier.
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
            "language": "javascript",
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

fn js_collect_decorators(node: Node, source: &[u8]) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "decorator" {
            if let Some((name, args)) = js_decorator_name_and_args(child, source) {
                out.push((name, args));
            }
        }
    }
    out
}

fn js_decorators_for_node(node: Node, source: &[u8]) -> Vec<(String, Option<String>)> {
    let mut out = js_collect_decorators(node, source);
    if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            out.extend(js_collect_decorators(parent, source));
        }
    }
    out
}

fn js_decorator_name_and_args(decorator: Node, source: &[u8]) -> Option<(String, Option<String>)> {
    let inner = decorator.named_child(0)?;
    match inner.kind() {
        "identifier" => {
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

impl Default for JavaScriptPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create JavaScriptPlugin")
    }
}

impl LanguagePlugin for JavaScriptPlugin {
    fn language_id(&self) -> &str {
        "javascript"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["js", "jsx", "mjs", "cjs"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_javascript::LANGUAGE.into())
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
            "javascript",
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
        Some(javascript_ast_handlers())
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
            "javascript",
            tree_sitter_javascript::LANGUAGE.into(),
            source,
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_javascript_plugin_language_id() {
        let plugin = JavaScriptPlugin::new().unwrap();
        assert_eq!(plugin.language_id(), "javascript");
    }

    #[test]
    fn test_javascript_plugin_file_extensions() {
        let plugin = JavaScriptPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["js", "jsx", "mjs", "cjs"]);
    }

    #[test]
    fn test_extract_function() {
        let plugin = JavaScriptPlugin::new().unwrap();
        let source = b"function add(a, b) { return a + b; }";
        let symbols = plugin
            .extract_symbols(Path::new("test.js"), source)
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
        let plugin = JavaScriptPlugin::new().unwrap();
        let source = b"const multiply = (x, y) => x * y;";
        let symbols = plugin
            .extract_symbols(Path::new("test.js"), source)
            .unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].symbol_type, SymbolType::Function);
        assert_eq!(symbols[0].name, "multiply");
    }

    #[test]
    fn test_extract_named_arrows_distinct() {
        let plugin = JavaScriptPlugin::new().unwrap();
        let source = br#"
function declaredAdd(a, b) { return a + b; }
const arrowAdd = (a, b) => a + b;
const arrowHelper = () => 1;
const api = { fetchAll: async () => 0 };
[1].map(x => x + 1);
"#;
        let symbols = plugin
            .extract_symbols(Path::new("arrows.js"), source)
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
        assert!(
            names.iter().any(|n| n.starts_with("anonymous@L")),
            "callback should stay span-disambiguated anonymous: {names:?}"
        );
        assert_eq!(
            names
                .iter()
                .filter(|n| n.starts_with("anonymous@L"))
                .count(),
            1,
            "only the map callback should be anonymous, got {names:?}"
        );
    }

    #[test]
    fn test_function_declaration_has_no_anonymous_duplicate() {
        let plugin = JavaScriptPlugin::new().unwrap();
        let source = b"export function gamma(n) { return n + 1; }";
        let symbols = plugin
            .extract_symbols(Path::new("gamma.js"), source)
            .unwrap();
        let fns: Vec<_> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(fns, vec!["gamma"], "unexpected functions: {fns:?}");
        let exports: Vec<_> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Import)
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(exports, vec!["export:gamma"]);
        assert!(
            !exports.iter().any(|name| name.contains("return")),
            "export identity must not include the function body: {exports:?}"
        );
    }

    #[test]
    fn test_extract_class() {
        let plugin = JavaScriptPlugin::new().unwrap();
        let source = b"class User { constructor(name) { this.name = name; } }";
        let symbols = plugin
            .extract_symbols(Path::new("test.js"), source)
            .unwrap();

        assert!(!symbols.is_empty());
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].symbol_type, SymbolType::Class);
    }

    #[test]
    fn test_extract_fields_and_constructor() {
        let source = br#"
class User {
  role = "user";
  constructor(name) {
    this.name = name;
  }
}
"#;
        let plugin = JavaScriptPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("User.js"), source)
            .unwrap();
        let class = symbols
            .iter()
            .find(|s| s.name == "User" && s.symbol_type == SymbolType::Class)
            .expect("class");
        assert!(class.fields.iter().any(|f| f.name == "role"));
        assert!(class.fields.iter().any(|f| f.name == "name"));
        let ctor = symbols
            .iter()
            .find(|s| {
                s.symbol_type == SymbolType::Function
                    && s.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true)
            })
            .expect("constructor");
        assert_eq!(ctor.name, "User");
        assert_eq!(ctor.qualified_name.as_deref(), Some("User.<init>"));
        assert_eq!(ctor.parameters.len(), 1);
        assert_eq!(ctor.parameters[0].name, "name");
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
function caller() {
    helper();
}

function helper() {}
"#;
        let plugin = JavaScriptPlugin::new().unwrap();
        let path = Path::new("test.js");
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
    fn test_named_and_default_import() {
        let source = br#"
import express from 'express';
import { foo, bar as Baz } from 'lodash';
"#;
        let plugin = JavaScriptPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("a.js"), source).unwrap();
        assert!(symbols.iter().any(|s| s.symbol_type == SymbolType::Import));
        assert!(symbols.len() >= 2);
    }

    #[test]
    fn test_class_extends_error() {
        let source = br#"class AppError extends Error {}"#;
        let plugin = JavaScriptPlugin::new().unwrap();
        let path = Path::new("err.js");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Extends && r.to == "Error")
        );
    }

    #[test]
    fn test_method_qualified_name_and_instantiates() {
        let source = br#"
class OrderService {
  checkout() { return new OrderDto(); }
}
class OrderDto {}
"#;
        let plugin = JavaScriptPlugin::new().unwrap();
        let path = Path::new("svc.js");
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
                .any(|r| r.relation_type == RelationType::Instantiates && r.to == "OrderDto")
        );
    }

    #[test]
    fn test_unresolved_dynamic_call_metadata() {
        let source = br#"
function f(obj, key) { return obj[key](); }
"#;
        let plugin = JavaScriptPlugin::new().unwrap();
        let path = Path::new("dyn.js");
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
    fn test_cjs_require_import() {
        let source = br#"const fs = require('fs');"#;
        let plugin = JavaScriptPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("cjs.js"), source).unwrap();
        assert!(symbols.iter().any(|s| {
            s.symbol_type == SymbolType::Import
                && s.metadata.get("module_system").and_then(|v| v.as_str()) == Some("cjs")
        }));
    }
}
