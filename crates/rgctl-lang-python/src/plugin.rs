//! Python language plugin
//!
//! Extracts symbols, relationships, and complexity metrics from Python source code
//! using TreeSitter.

use crate::single_pass::{extract_all_single_pass, python_ast_handlers};
use rgctl_plugin_api::{
    AstHandlerTable, ComplexityMetrics, Error, ExtractAllResult, Field, LanguagePlugin, Parameter,
    Relation, RelationType, Result, SourceLocation, Symbol, SymbolType, callee_name,
    complexity_from_metadata, containing_function, infer_python_method_target, parse_with_pool,
};
use rgctl_plugin_helpers::{
    containing_class_name, extract_decorator_relations, extract_python_class_extends_relations,
    extract_python_import_symbols, python_simple_type_name, record_symbol_spans,
};
use rgctl_semantic::type_inference::TypeInferencer;
use std::path::Path;
use tree_sitter::Node;

/// Python language plugin
pub struct PythonPlugin;

impl PythonPlugin {
    /// Create a new Python plugin
    pub fn new() -> Result<Self> {
        Ok(Self)
    }

    /// Extract function definition
    pub(crate) fn extract_function(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
    ) -> Result<Symbol> {
        let mut cursor = node.walk();
        let mut name = None;
        let mut parameters = Vec::new();
        let mut modifiers = Vec::new();
        let mut documentation = None;
        let mut return_type = None;

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "parameters" => {
                    parameters = self.extract_parameters(child, source)?;
                }
                "type" | "type_identifier" => {
                    // return annotation after `->`
                    return_type = Some(child.utf8_text(source)?.to_string());
                }
                "block" => {
                    // Check for docstring
                    let mut block_cursor = child.walk();
                    for block_child in child.children(&mut block_cursor) {
                        if block_child.kind() == "expression_statement" {
                            let mut expr_cursor = block_child.walk();
                            for expr_child in block_child.children(&mut expr_cursor) {
                                if expr_child.kind() == "string" {
                                    let doc = expr_child.utf8_text(source)?;
                                    documentation = Some(
                                        doc.trim_matches(|c| c == '"' || c == '\'').to_string(),
                                    );
                                    break;
                                }
                            }
                            break;
                        }
                    }
                }
                "decorator" => {
                    modifiers.push(child.utf8_text(source)?.to_string());
                }
                _ => {}
            }
        }

        let raw_name = name.ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Function missing name".to_string(),
        })?;

        // Infer types for parameters without explicit type hints
        let function_source = node.utf8_text(source).unwrap_or("");
        let inferencer = TypeInferencer::new();
        let inferred_types = inferencer.infer_python(function_source);

        // Update parameters with inferred types (do not override annotations)
        for param in &mut parameters {
            if param.param_type.is_none() {
                if let Some(inference) = inferred_types.get(&param.name) {
                    param.param_type = Some(format!("{:?}", inference.inferred));
                }
            }
        }

        let is_constructor = raw_name == "__init__";
        let class_name = if is_constructor {
            self.find_containing_class_name(node, source)
        } else {
            None
        };
        let (name, qualified_name, metadata) = if is_constructor {
            let class_name = class_name.unwrap_or_else(|| "object".to_string());
            (
                raw_name,
                Some(format!("{class_name}.<init>")),
                serde_json::json!({ "language": "python", "is_constructor": true }),
            )
        } else if let Some(class_name) = self.find_containing_class_name(node, source) {
            (
                raw_name.clone(),
                Some(format!("{class_name}.{raw_name}")),
                serde_json::json!({ "language": "python" }),
            )
        } else {
            (raw_name, None, serde_json::json!({ "language": "python" }))
        };

        let mut sym = Symbol {
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
            documentation,
            metadata,
            ..Default::default()
        };
        record_symbol_spans(&mut sym, node, node.child_by_field_name("name"));
        Ok(sym)
    }

    fn find_containing_class_name(&self, node: Node, source: &[u8]) -> Option<String> {
        containing_class_name(node, source)
    }

    /// Extract function parameters
    fn extract_parameters(&self, params_node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut parameters = Vec::new();
        let mut cursor = params_node.walk();

        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "identifier" => {
                    parameters.push(Parameter {
                        name: child.utf8_text(source)?.to_string(),
                        param_type: None,
                        default_value: None,
                    });
                }
                "typed_parameter" => {
                    if let Some(param) = self.extract_typed_parameter(child, source, None)? {
                        parameters.push(param);
                    }
                }
                "typed_default_parameter" => {
                    let default = child
                        .child_by_field_name("value")
                        .and_then(|n| n.utf8_text(source).ok())
                        .map(str::to_string);
                    if let Some(param) = self.extract_typed_parameter(child, source, default)? {
                        parameters.push(param);
                    }
                }
                "default_parameter" => {
                    let mut param_cursor = child.walk();
                    let mut name = None;
                    let mut default = None;
                    let mut param_type = None;

                    for param_child in child.children(&mut param_cursor) {
                        match param_child.kind() {
                            "identifier" if name.is_none() => {
                                name = Some(param_child.utf8_text(source)?.to_string());
                            }
                            "typed_parameter" => {
                                if let Some(p) =
                                    self.extract_typed_parameter(param_child, source, None)?
                                {
                                    name = Some(p.name);
                                    param_type = p.param_type;
                                }
                            }
                            "type" | "type_identifier" => {
                                param_type = Some(param_child.utf8_text(source)?.to_string());
                            }
                            "=" => {}
                            _ if name.is_some() && default.is_none() => {
                                default = Some(param_child.utf8_text(source)?.to_string());
                            }
                            _ => {}
                        }
                    }

                    if let Some(name) = name {
                        parameters.push(Parameter {
                            name,
                            param_type,
                            default_value: default,
                        });
                    }
                }
                _ => {}
            }
        }

        Ok(parameters)
    }

    fn extract_typed_parameter(
        &self,
        node: Node,
        source: &[u8],
        default_value: Option<String>,
    ) -> Result<Option<Parameter>> {
        let mut name = None;
        let mut param_type = None;
        let mut cursor = node.walk();

        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier" if name.is_none() => {
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "type" | "type_identifier" => {
                    param_type = Some(child.utf8_text(source)?.to_string());
                }
                _ => {
                    // Nested type nodes (e.g. generic `list[str]`) often appear as `type`
                    // children already handled; also accept field name "type".
                }
            }
        }

        if param_type.is_none() {
            param_type = node
                .child_by_field_name("type")
                .and_then(|n| n.utf8_text(source).ok())
                .map(str::to_string);
        }

        Ok(name.map(|name| Parameter {
            name,
            param_type,
            default_value,
        }))
    }

    /// Extract class definition
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
                    name = Some(child.utf8_text(source)?.to_string());
                }
                "block" => {
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

        let mut sym = Symbol {
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
            metadata: serde_json::json!({ "language": "python" }),
            ..Default::default()
        };
        record_symbol_spans(&mut sym, node, node.child_by_field_name("name"));
        Ok(sym)
    }

    /// Extract class fields: annotated attrs, body assigns, lambdas, `self`/`cls` in methods.
    pub(crate) fn extract_class_fields(
        &self,
        block_node: Node,
        source: &[u8],
    ) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let mut cursor = block_node.walk();

        for child in block_node.children(&mut cursor) {
            match child.kind() {
                "expression_statement" => {
                    let mut expr_cursor = child.walk();
                    for expr_child in child.children(&mut expr_cursor) {
                        if expr_child.kind() != "assignment" {
                            continue;
                        }
                        if expr_child.child_by_field_name("type").is_some() {
                            if let Some((name, ty)) =
                                crate::depth::annotated_assignment_field(expr_child, source)
                            {
                                crate::depth::push_field(&mut fields, &mut seen, name, ty);
                                continue;
                            }
                        }
                        if let Some(name) =
                            crate::depth::class_lambda_binding_name(expr_child, source)
                        {
                            crate::depth::push_field(
                                &mut fields,
                                &mut seen,
                                name,
                                Some("Callable".to_string()),
                            );
                            continue;
                        }
                        // Plain class attr / enum member (`PENDING = "x"`).
                        if let Some(left) = expr_child.child_by_field_name("left") {
                            if left.kind() == "identifier" {
                                let name = left.utf8_text(source)?.to_string();
                                crate::depth::push_field(&mut fields, &mut seen, name, None);
                            }
                        }
                    }
                }
                "function_definition" => {
                    self.harvest_method_fields(child, source, &mut fields, &mut seen)?;
                }
                "decorated_definition" => {
                    // `@classmethod` / `@staticmethod` wrap the real function_definition.
                    let mut dc = child.walk();
                    for inner in child.children(&mut dc) {
                        if inner.kind() == "function_definition" {
                            self.harvest_method_fields(inner, source, &mut fields, &mut seen)?;
                        }
                    }
                }
                _ => {}
            }
        }

        Ok(fields)
    }

    fn harvest_method_fields(
        &self,
        fn_node: Node,
        source: &[u8],
        fields: &mut Vec<Field>,
        seen: &mut std::collections::HashSet<String>,
    ) -> Result<()> {
        // All methods: harvest `self.x` / `cls.x` (not only `__init__`).
        self.collect_self_assignments(fn_node, source, fields, seen)?;
        let fn_name = fn_node
            .child_by_field_name("name")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string));
        if fn_name.as_deref() == Some("__init__") {
            self.infer_init_param_field_types(fn_node, source, fields)?;
        }
        Ok(())
    }

    fn infer_init_param_field_types(
        &self,
        init_node: Node,
        source: &[u8],
        fields: &mut [Field],
    ) -> Result<()> {
        let params = init_node
            .child_by_field_name("parameters")
            .map(|p| self.extract_parameters(p, source))
            .transpose()?
            .unwrap_or_default();
        for param in params {
            if param.name == "self" {
                continue;
            }
            let Some(param_type) = param.param_type else {
                continue;
            };
            if let Some(field) = fields.iter_mut().find(|f| f.name == param.name) {
                if field.field_type.is_none() {
                    field.field_type = Some(param_type);
                }
            }
        }
        Ok(())
    }

    fn collect_self_assignments(
        &self,
        node: Node,
        source: &[u8],
        fields: &mut Vec<Field>,
        seen: &mut std::collections::HashSet<String>,
    ) -> Result<()> {
        if node.kind() == "assignment" {
            if let Some(left) = node.child_by_field_name("left") {
                if let Some(name) = crate::depth::self_or_cls_attr_name(left, source) {
                    let field_type = node
                        .child_by_field_name("right")
                        .and_then(|r| infer_field_type_from_rhs(r, source));
                    crate::depth::push_field(fields, seen, name, field_type);
                }
            } else {
                // Fallback without field names: attribute whose first identifier is self/cls
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "attribute" {
                        let mut attr_cursor = child.walk();
                        let parts: Vec<_> = child
                            .children(&mut attr_cursor)
                            .filter(|c| c.kind() == "identifier")
                            .collect();
                        if parts.len() >= 2 {
                            let obj = parts[0].utf8_text(source)?;
                            if obj == "self" || obj == "cls" {
                                let name = parts[1].utf8_text(source)?.to_string();
                                crate::depth::push_field(fields, seen, name, None);
                            }
                        }
                        break;
                    }
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            self.collect_self_assignments(child, source, fields, seen)?;
        }
        Ok(())
    }

    /// Calculate cyclomatic complexity
    fn calculate_cyclomatic(&self, node: Node) -> usize {
        let mut complexity = 1;

        fn traverse(node: Node, complexity: &mut usize) {
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "if_statement" | "elif_clause" | "while_statement" | "for_statement"
                    | "except_clause" => {
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

    /// Calculate cognitive complexity
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
                    "elif_clause" | "except_clause" => {
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
                    "if_statement" | "while_statement" | "for_statement" | "block"
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

    fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        parse_with_pool("python", tree_sitter_python::LANGUAGE.into(), source).map_err(|e| {
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

        fn traverse_for_symbols(
            node: Node,
            source: &[u8],
            file_path: &str,
            symbols: &mut Vec<Symbol>,
            plugin: &PythonPlugin,
        ) -> Result<()> {
            match node.kind() {
                "function_definition" => {
                    symbols.push(plugin.extract_function(node, source, file_path)?);
                }
                "class_definition" => {
                    symbols.push(plugin.extract_class(node, source, file_path)?);
                }
                "import_statement" | "import_from_statement" => {
                    symbols.extend(extract_python_import_symbols(
                        node, source, file_path, "python",
                    ));
                }
                "type_alias_statement" => {
                    if let Some(sym) =
                        crate::depth::extract_type_alias_statement(node, source, file_path)
                    {
                        symbols.push(sym);
                    }
                }
                "as_pattern_target" => {
                    if let Some(sym) =
                        crate::depth::extract_as_pattern_target(node, source, file_path)
                    {
                        symbols.push(sym);
                    }
                }
                "named_expression" => {
                    if let Some(sym) =
                        crate::depth::extract_named_expression(node, source, file_path)
                    {
                        symbols.push(sym);
                    }
                }
                "lambda" => {
                    let class_qn = containing_class_name(node, source);
                    if let Some(sym) = crate::depth::extract_lambda_symbol(
                        node,
                        source,
                        file_path,
                        class_qn.as_deref(),
                    ) {
                        symbols.push(sym);
                    }
                }
                "decorated_definition" => {}
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

    fn extract_heritage(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        const MAX_DEPTH: usize = 2048;
        let mut stack = vec![(root, 0usize)];
        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            if node.kind() == "class_definition" {
                relations.extend(extract_python_class_extends_relations(
                    node,
                    source,
                    &file_path.to_string_lossy(),
                    "python",
                ));
            }
            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push((child, depth + 1));
            }
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
        let class_names: std::collections::HashSet<String> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Class)
            .map(|s| s.name.clone())
            .collect();
        let file_path_str = file_path.to_string_lossy().to_string();
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }
            if node.kind() == "call" {
                if let Some(from_fn) = containing_function(node, &function_symbols) {
                    let from = from_fn
                        .qualified_name
                        .clone()
                        .unwrap_or_else(|| from_fn.name.clone());
                    if let Some(target) = instantiation_target(node, source, &class_names) {
                        relations.push(Relation {
                            from,
                            to: target.clone(),
                            relation_type: RelationType::Instantiates,
                            location: rgctl_plugin_helpers::python_source_location(
                                node,
                                &file_path_str,
                            ),
                            metadata: serde_json::json!({ "language": "python" }),
                            to_qualified_hint: Some(target),
                            to_type_hint: None,
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
        self.walk_python_calls(root, source, file_path, symbols, &mut relations);
        self.extract_heritage(root, source, file_path, &mut relations)?;
        extract_decorator_relations(
            root,
            source,
            &file_path.to_string_lossy(),
            "python",
            &mut relations,
        );
        self.extract_instantiations(root, source, file_path, symbols, &mut relations)?;
        Ok(relations)
    }

    fn walk_python_calls(
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
        let mut stack = vec![(root, 0usize)];

        while let Some((node, depth)) = stack.pop() {
            if depth > MAX_DEPTH {
                continue;
            }

            if node.kind() == "call" {
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
                        let (to_qualified_hint, to_type_hint) =
                            infer_python_method_target(node, source, symbols, from_fn);
                        let mut meta = serde_json::json!({ "language": "python" });
                        if python_call_unresolved_local(node, source) {
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
                            location: rgctl_plugin_helpers::python_source_location(
                                node,
                                &file_path.to_string_lossy(),
                            ),
                            metadata: meta,
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
}

fn infer_field_type_from_rhs(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "identifier" | "attribute" => python_simple_type_name(node, source),
        "call" => node
            .child_by_field_name("function")
            .and_then(|f| python_simple_type_name(f, source)),
        _ => None,
    }
}

pub(crate) fn instantiation_target(
    call: Node,
    source: &[u8],
    class_names: &std::collections::HashSet<String>,
) -> Option<String> {
    let func = call.child_by_field_name("function")?;
    match func.kind() {
        "identifier" => {
            let name = func.utf8_text(source).ok()?.to_string();
            if class_names.contains(&name) {
                Some(name)
            } else {
                None
            }
        }
        "attribute" => {
            let attr = func.child_by_field_name("attribute")?;
            let method = attr.utf8_text(source).ok()?;
            if method == "model_validate" {
                func.child_by_field_name("object")
                    .and_then(|o| python_simple_type_name(o, source))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn python_call_unresolved_local(call: Node, source: &[u8]) -> bool {
    let Some(func) = call.child_by_field_name("function") else {
        return false;
    };
    if func.kind() == "subscript" {
        return true;
    }
    if func.kind() != "attribute" {
        return false;
    }
    let attr = func.child_by_field_name("attribute");
    match attr.map(|a| a.kind()) {
        Some("identifier") => false,
        Some(kind) => {
            kind != "identifier"
                || attr
                    .and_then(|a| a.utf8_text(source).ok())
                    .is_some_and(|t| t.starts_with('['))
        }
        None => true,
    }
}

impl Default for PythonPlugin {
    fn default() -> Self {
        Self::new().expect("Failed to create PythonPlugin")
    }
}

impl LanguagePlugin for PythonPlugin {
    fn language_id(&self) -> &str {
        "python"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["py", "pyw"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_python::LANGUAGE.into())
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
        Some(python_ast_handlers())
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

        let tree = parse_with_pool("python", tree_sitter_python::LANGUAGE.into(), source).map_err(
            |e| Error::ParseError {
                file: symbol.location.file.clone().into(),
                line: symbol.location.start_line,
                message: e,
            },
        )?;

        let root = tree.root_node();
        let target_line = symbol.location.start_line - 1;

        fn find_function_at_line(node: Node, line: usize) -> Option<Node> {
            if node.kind() == "function_definition" && node.start_position().row == line {
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
    fn test_python_plugin_language_id() {
        let plugin = PythonPlugin::new().unwrap();
        assert_eq!(plugin.language_id(), "python");
    }

    #[test]
    fn test_python_plugin_file_extensions() {
        let plugin = PythonPlugin::new().unwrap();
        assert_eq!(plugin.file_extensions(), vec!["py", "pyw"]);
    }

    #[test]
    fn test_extract_simple_function() {
        let plugin = PythonPlugin::new().unwrap();
        let source = b"def add(a, b):\n    return a + b";
        let symbols = plugin
            .extract_symbols(Path::new("test.py"), source)
            .unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "add");
        assert_eq!(symbols[0].symbol_type, SymbolType::Function);
        assert_eq!(symbols[0].parameters.len(), 2);
    }

    #[test]
    fn test_extract_function_with_defaults() {
        let plugin = PythonPlugin::new().unwrap();
        let source = b"def greet(name, greeting='Hello'):\n    return f'{greeting} {name}'";
        let symbols = plugin
            .extract_symbols(Path::new("test.py"), source)
            .unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].parameters.len(), 2);
        assert_eq!(
            symbols[0].parameters[1].default_value,
            Some("'Hello'".to_string())
        );
    }

    #[test]
    fn test_extract_class() {
        let plugin = PythonPlugin::new().unwrap();
        let source = b"class User:\n    def __init__(self):\n        self.name = 'test'";
        let symbols = plugin
            .extract_symbols(Path::new("test.py"), source)
            .unwrap();

        assert_eq!(symbols.len(), 2); // Class + __init__ method
        assert_eq!(symbols[0].name, "User");
        assert_eq!(symbols[0].symbol_type, SymbolType::Class);
    }

    #[test]
    fn test_extract_fields_and_constructor() {
        let source = br#"
class User:
    kind = "person"

    def __init__(self, name: str, age: int):
        self.name = name
        self.age = age
"#;
        let plugin = PythonPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("user.py"), source)
            .unwrap();
        let class = symbols
            .iter()
            .find(|s| s.name == "User" && s.symbol_type == SymbolType::Class)
            .expect("class");
        assert!(class.fields.iter().any(|f| f.name == "kind"));
        assert!(class.fields.iter().any(|f| f.name == "name"));
        assert!(class.fields.iter().any(|f| f.name == "age"));
        let ctor = symbols
            .iter()
            .find(|s| {
                s.symbol_type == SymbolType::Function
                    && s.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true)
            })
            .expect("constructor");
        assert_eq!(ctor.name, "__init__");
        assert_eq!(ctor.qualified_name.as_deref(), Some("User.<init>"));
        let name_param = ctor
            .parameters
            .iter()
            .find(|p| p.name == "name")
            .expect("name param");
        assert_eq!(name_param.param_type.as_deref(), Some("str"));
        let age_param = ctor
            .parameters
            .iter()
            .find(|p| p.name == "age")
            .expect("age param");
        assert_eq!(age_param.param_type.as_deref(), Some("int"));
    }

    #[test]
    fn test_calculate_complexity() {
        let plugin = PythonPlugin::new().unwrap();
        let source = b"def check(x):\n    if x > 0:\n        if x < 100:\n            return True\n    return False";
        let symbols = plugin
            .extract_symbols(Path::new("test.py"), source)
            .unwrap();

        assert_eq!(symbols.len(), 1);
        let complexity = plugin.calculate_complexity(&symbols[0], source).unwrap();
        assert!(complexity.is_some());
        let metrics = complexity.unwrap();
        assert_eq!(metrics.cyclomatic, 3);
    }

    #[test]
    fn test_type_inference() {
        let plugin = PythonPlugin::new().unwrap();
        let source = b"def process(name, count):\n    return name.upper() + str(count + 1)";
        let symbols = plugin
            .extract_symbols(Path::new("test.py"), source)
            .unwrap();

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].parameters.len(), 2);

        // name should be inferred as String (uses .upper())
        let name_param = &symbols[0].parameters[0];
        assert_eq!(name_param.name, "name");
        assert!(name_param.param_type.is_some());
        assert!(name_param.param_type.as_ref().unwrap().contains("String"));

        // count should be inferred as Numeric (uses +)
        let count_param = &symbols[0].parameters[1];
        assert_eq!(count_param.name, "count");
        assert!(count_param.param_type.is_some());
        assert!(count_param.param_type.as_ref().unwrap().contains("Numeric"));
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
def caller():
    helper()

def helper():
    pass
"#;
        let plugin = PythonPlugin::new().unwrap();
        let path = Path::new("test.py");
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
    fn test_extraction_depth_import_extends_decorators() {
        let source = br#"
from app.repositories.order import OrderRepository
from pydantic import BaseModel

class OrderService(BaseRepository):
    def checkout(self):
        pass

@router.get("")
def list_orders():
    pass
"#;
        let plugin = PythonPlugin::new().unwrap();
        let path = Path::new("svc.py");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        assert!(symbols.iter().any(|s| s.symbol_type == SymbolType::Import));
        let checkout = symbols.iter().find(|s| s.name == "checkout").unwrap();
        assert_eq!(
            checkout.qualified_name.as_deref(),
            Some("OrderService.checkout")
        );
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Extends)
        );
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::AnnotatedWith)
        );
    }

    #[test]
    fn test_extract_all_single_pass_forward_calls_and_complexity() {
        let source = br#"
def caller():
    helper()
    if True:
        return

def helper():
    pass
"#;
        let plugin = PythonPlugin::new().unwrap();
        let path = Path::new("test.py");
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
    fn test_instantiates_and_typed_call() {
        let source = br#"
class Order:
    def __init__(self, id: str):
        self.id = id

class OrderRepository:
    def list_for_user(self, user_id: str):
        pass

class OrderService:
    def __init__(self):
        self.orders = OrderRepository()

    def checkout(self, user_id: str):
        self.orders.list_for_user(user_id)
        return Order(id="1")
"#;
        let plugin = PythonPlugin::new().unwrap();
        let path = Path::new("svc.py");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Instantiates && r.to == "Order"),
            "{relations:?}"
        );
        assert!(
            relations.iter().any(|r| {
                r.relation_type == RelationType::Calls
                    && r.to_type_hint.as_deref() == Some("OrderRepository")
            }),
            "{relations:?}"
        );
    }

    #[test]
    fn fastapi_extract_all_emits_exposes_route_and_injects() {
        let source = br#"
from fastapi import Depends

@app.get("/users/{id}")
def get_user(id: str, db = Depends(get_db)):
    return db
"#;
        let plugin = PythonPlugin::new().unwrap();
        let result = plugin.extract_all(Path::new("users.py"), source).unwrap();
        let route = result
            .relations
            .iter()
            .find(|r| r.relation_type == RelationType::ExposesRoute)
            .expect("ExposesRoute");
        assert_eq!(route.to, "GET /users/{id}");
        assert!(
            result
                .relations
                .iter()
                .any(|r| { r.relation_type == RelationType::Injects && r.to == "get_db" }),
            "Injects get_db: {:?}",
            result.relations
        );
        let handler = result
            .symbols
            .iter()
            .find(|s| s.name == "get_user")
            .expect("get_user");
        let tags = handler
            .metadata
            .get("semantic_tags")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            tags.contains("endpoint") && tags.contains("dependency"),
            "semantic_tags={tags:?}"
        );
    }
}
