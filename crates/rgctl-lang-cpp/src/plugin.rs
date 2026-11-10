//! C++ language plugin using Tree-sitter.

use crate::single_pass::{cpp_ast_handlers, extract_all_single_pass};
use rgctl_plugin_api::*;
use rgctl_plugin_helpers::record_symbol_spans;
use std::path::Path;
use tree_sitter::Node;

/// C++ language plugin.
pub struct CppPlugin;

impl CppPlugin {
    /// Create a new C++ plugin.
    pub fn new() -> Result<Self> {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&tree_sitter_cpp::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set C++ grammar: {e}")))?;
        Ok(Self)
    }

    pub(crate) fn parse(&self, file_path: &Path, source: &[u8]) -> Result<tree_sitter::Tree> {
        parse_with_pool("cpp", tree_sitter_cpp::LANGUAGE.into(), source).map_err(|e| {
            Error::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message: e,
            }
        })
    }

    pub(crate) fn extract_function(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        scope: Option<&str>,
    ) -> Result<Option<Symbol>> {
        let Some(name) = function_name_from_node(node, source) else {
            return Ok(None);
        };

        // Skip destructors for constructor detection (`~Foo`).
        let is_destructor = name.starts_with('~');
        let is_constructor = !is_destructor && scope.is_some_and(|s| s == name) && !name.is_empty();

        let parameters = extract_parameters(node, source)?;

        let qualified_name = if is_constructor {
            scope.map(|s| format!("{s}::<init>"))
        } else {
            scope.map(|s| format!("{s}::{name}"))
        };

        let metadata = if is_constructor {
            serde_json::json!({ "language": "cpp", "is_constructor": true })
        } else {
            serde_json::json!({ "language": "cpp" })
        };

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name,
            location: source_location(node, file_path),
            signature: Some(first_line(node, source)),
            return_type: None,
            parameters,
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata,
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, function_name_node(node));
        symbol = rgctl_plugin_helpers::callable_identity::symbol(
            symbol,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Cpp,
        );
        Ok(Some(symbol))
    }

    pub(crate) fn extract_class(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbol_type: SymbolType,
    ) -> Result<Symbol> {
        let name = type_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Type missing name".to_string(),
        })?;

        // Class/struct: field_declaration; enum: enumerator list.
        let fields = if matches!(symbol_type, SymbolType::Class | SymbolType::Struct) {
            extract_type_fields(node, source)?
        } else if symbol_type == SymbolType::Enum {
            crate::depth::extract_enumerator_fields(node, source)
        } else {
            vec![]
        };

        let mut symbol = Symbol {
            name: name.clone(),
            symbol_type,
            qualified_name: None,
            location: source_location(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields,
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({ "language": "cpp" }),
            ..Default::default()
        };
        record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
        symbol = rgctl_plugin_helpers::callable_identity::symbol(
            symbol,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Cpp,
        );
        Ok(symbol)
    }

    fn symbols_from_tree(
        &self,
        root: Node,
        source: &[u8],
        file_path: &Path,
    ) -> Result<Vec<Symbol>> {
        let mut symbols = Vec::new();
        self.traverse(
            root,
            source,
            &file_path.to_string_lossy(),
            &mut symbols,
            None,
        )?;
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
        walk_cpp_calls(root, source, file_path, symbols, &mut relations, None);
        self.extract_inheritance(root, source, file_path, &mut relations)?;
        self.extract_instantiations(root, source, file_path, symbols, &mut relations)?;
        Ok(relations)
    }

    /// Iterative tree traversal using an explicit stack to prevent stack overflows on deep ASTs.
    fn traverse(
        &self,
        root: Node,
        source: &[u8],
        file_path: &str,
        symbols: &mut Vec<Symbol>,
        initial_scope: Option<String>,
    ) -> Result<()> {
        const MAX_DEPTH: usize = 2048;
        let mut stack = vec![(root, 0usize, initial_scope)];

        while let Some((node, depth, scope)) = stack.pop() {
            if depth > MAX_DEPTH {
                tracing::warn!(
                    file = %file_path,
                    depth = depth,
                    "AST depth limit exceeded during C++ traversal; skipping deep branches"
                );
                continue;
            }

            let next_scope = match node.kind() {
                "namespace_definition" => node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
                    .map(|name| {
                        scope
                            .as_ref()
                            .map(|s| format!("{s}::{name}"))
                            .unwrap_or(name)
                    }),
                "class_specifier" | "struct_specifier" => type_name(node, source),
                _ => None,
            };
            let child_scope = next_scope.clone().or(scope);
            let active_scope = child_scope.as_deref();

            match node.kind() {
                "function_definition" | "declaration" => {
                    let is_prototype =
                        node.kind() == "declaration" && node.child_by_field_name("body").is_none();
                    if !is_prototype {
                        if let Some(mut sym) =
                            self.extract_function(node, source, file_path, active_scope)?
                        {
                            sym.modifiers = method_modifiers(node, source);
                            symbols.push(sym);
                        }
                    }
                }
                "template_declaration" => {
                    if let Some(mut sym) =
                        self.extract_function(node, source, file_path, active_scope)?
                    {
                        sym.modifiers = method_modifiers(node, source);
                        sym.metadata["is_template"] = serde_json::Value::Bool(true);
                        symbols.push(sym);
                    } else if let Some(child) = template_enclosed_type(node) {
                        if type_name(child, source).is_some() {
                            let st = if child.kind() == "class_specifier" {
                                SymbolType::Class
                            } else {
                                SymbolType::Struct
                            };
                            let mut sym = self.extract_class(child, source, file_path, st)?;
                            sym.metadata["is_template"] = serde_json::Value::Bool(true);
                            symbols.push(sym);
                        }
                    }
                }
                "class_specifier" => {
                    if type_name(node, source).is_some() {
                        symbols.push(self.extract_class(
                            node,
                            source,
                            file_path,
                            SymbolType::Class,
                        )?);
                    }
                }
                "struct_specifier" => {
                    if type_name(node, source).is_some() {
                        symbols.push(self.extract_class(
                            node,
                            source,
                            file_path,
                            SymbolType::Struct,
                        )?);
                    }
                }
                "enum_specifier" => {
                    if type_name(node, source).is_some() {
                        symbols.push(self.extract_class(
                            node,
                            source,
                            file_path,
                            SymbolType::Enum,
                        )?);
                    }
                }
                "type_definition" => {
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if matches!(child.kind(), "class_specifier" | "struct_specifier")
                            && type_name(child, source).is_some()
                        {
                            let st = if child.kind() == "class_specifier" {
                                SymbolType::Class
                            } else {
                                SymbolType::Struct
                            };
                            symbols.push(self.extract_class(child, source, file_path, st)?);
                        }
                    }
                }
                "preproc_include" => {
                    let text = node.utf8_text(source)?.trim().to_string();
                    let mut symbol = Symbol {
                        name: text,
                        symbol_type: SymbolType::Import,
                        qualified_name: None,
                        location: source_location(node, file_path),
                        signature: None,
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "language": "cpp" }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
                    symbols.push(symbol);
                }
                "using_declaration" => {
                    let (name, qualified) = using_import_names(node, source);
                    let mut symbol = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::Import,
                        qualified_name: qualified,
                        location: source_location(node, file_path),
                        signature: Some(node.utf8_text(source)?.trim().to_string()),
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "language": "cpp", "import_kind": "using" }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut symbol, node, node.child_by_field_name("name"));
                    symbols.push(symbol);
                }
                _ => {}
            }

            let mut cursor = node.walk();
            let children: Vec<Node> = node.children(&mut cursor).collect();
            for child in children.into_iter().rev() {
                stack.push((child, depth + 1, child_scope.clone()));
            }
        }
        Ok(())
    }

    /// Iterative inheritance extraction (heap stack; bounded depth).
    fn extract_inheritance(
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
                tracing::warn!(
                    file = %file_path.display(),
                    depth = depth,
                    "AST depth limit exceeded during C++ inheritance walk; skipping deep branches"
                );
                continue;
            }

            if matches!(node.kind(), "class_specifier" | "struct_specifier") {
                let class_name = type_name(node, source).unwrap_or_default();
                if !class_name.is_empty() {
                    let mut cursor = node.walk();
                    for child in node.children(&mut cursor) {
                        if child.kind() == "base_class_clause" {
                            collect_base_relations(
                                &class_name,
                                child,
                                source,
                                file_path,
                                relations,
                            );
                        }
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

    /// `new` / `new[]` → `Instantiates` from the enclosing function.
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

            if node.kind() == "new_expression" {
                if let Some(from_fn) = containing_function(node, &function_symbols) {
                    let from = from_fn
                        .qualified_name
                        .clone()
                        .unwrap_or_else(|| from_fn.name.clone());
                    if let Some(type_node) = node.child_by_field_name("type") {
                        if let Some(target) = type_name_from_type_node(type_node, source) {
                            let scoped =
                                scoped_type_name(type_node, source).unwrap_or(target.clone());
                            relations.push(Relation {
                                from,
                                to: target,
                                relation_type: RelationType::Instantiates,
                                location: source_location(node, &file_path.to_string_lossy()),
                                metadata: serde_json::json!({
                                    "language": "cpp",
                                    "is_array": node.child_by_field_name("declarator").is_some(),
                                }),
                                to_qualified_hint: Some(scoped),
                                to_type_hint: None,
                                ..Default::default()
                            });
                        }
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
}

impl LanguagePlugin for CppPlugin {
    fn language_id(&self) -> &str {
        "cpp"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["cpp", "cc", "cxx", "hpp", "hh", "hxx"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_cpp::LANGUAGE.into())
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
        Some(cpp_ast_handlers())
    }

    fn calculate_complexity(
        &self,
        symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        if let Some(metrics) = complexity_from_metadata(symbol) {
            return Ok(Some(metrics));
        }
        Ok(None)
    }
}

pub(crate) fn collect_base_relations(
    from: &str,
    base_clause: Node,
    source: &[u8],
    file_path: &Path,
    relations: &mut Vec<Relation>,
) {
    let mut access = "public";
    let mut cursor = base_clause.walk();
    for child in base_clause.children(&mut cursor) {
        match child.kind() {
            "access_specifier" => {
                access = access_label(child, source);
            }
            "type_identifier"
            | "qualified_identifier"
            | "qualified_type_identifier"
            | "template_type" => {
                if let Some(base) = base_type_name(child, source) {
                    let mut rel = relation(from, &base, RelationType::Extends, child, file_path);
                    rel.metadata["access"] = serde_json::Value::String(access.to_string());
                    if let Some(hint) = scoped_type_name(child, source) {
                        rel.to_qualified_hint = Some(hint);
                    }
                    relations.push(rel);
                }
            }
            _ => {}
        }
    }
}

fn access_label(node: Node, source: &[u8]) -> &'static str {
    let Ok(text) = node.utf8_text(source) else {
        return "public";
    };
    if text.contains("protected") {
        "protected"
    } else if text.contains("private") {
        "private"
    } else {
        "public"
    }
}

fn base_type_name(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" => node
            .utf8_text(source)
            .ok()
            .map(str::trim)
            .map(str::to_string),
        "qualified_identifier" | "qualified_type_identifier" => {
            last_named_child_by_field(node, "name")
                .and_then(|n| base_type_name(n, source))
                .or_else(|| {
                    node.utf8_text(source).ok().and_then(|text| {
                        text.rsplit("::").next().map(str::trim).map(str::to_string)
                    })
                })
        }
        "template_type" => node
            .child_by_field_name("name")
            .and_then(|n| base_type_name(n, source)),
        _ => None,
    }
}

pub(crate) fn scoped_type_name(node: Node, source: &[u8]) -> Option<String> {
    match node.kind() {
        "type_identifier" => node
            .utf8_text(source)
            .ok()
            .map(str::trim)
            .map(str::to_string),
        "qualified_identifier" | "qualified_type_identifier" => {
            let scope = node
                .child_by_field_name("scope")
                .and_then(|s| scoped_type_name(s, source));
            let name = last_named_child_by_field(node, "name")
                .and_then(|n| scoped_type_name(n, source))?;
            scope.map(|s| format!("{s}::{name}")).or(Some(name))
        }
        "template_type" => {
            let name = node
                .child_by_field_name("name")
                .and_then(|n| scoped_type_name(n, source))?;
            Some(name)
        }
        "type_specifier" | "primitive_type" => node
            .named_child(0)
            .and_then(|n| scoped_type_name(n, source)),
        _ => node
            .utf8_text(source)
            .ok()
            .map(str::trim)
            .map(str::to_string),
    }
}

pub(crate) fn type_name_from_type_node(node: Node, source: &[u8]) -> Option<String> {
    if node.kind() == "type_specifier" {
        return node.named_child(0).and_then(|n| base_type_name(n, source));
    }
    base_type_name(node, source)
}

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

pub(crate) fn using_import_names(node: Node, source: &[u8]) -> (String, Option<String>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "identifier" => {
                let name = child.utf8_text(source).unwrap_or("").trim().to_string();
                return (name, None);
            }
            "qualified_identifier" => {
                let qualified = scoped_type_name(child, source).unwrap_or_default();
                let alias = last_named_child_by_field(child, "name")
                    .and_then(|n| n.utf8_text(source).ok().map(str::trim).map(str::to_string))
                    .unwrap_or_else(|| qualified.clone());
                return (alias, Some(qualified));
            }
            _ => {}
        }
    }
    let text = node.utf8_text(source).unwrap_or("").trim().to_string();
    (text, None)
}

pub(crate) fn template_enclosed_type(node: Node) -> Option<Node> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if matches!(child.kind(), "class_specifier" | "struct_specifier") {
            return Some(child);
        }
    }
    None
}

pub(crate) fn method_modifiers(node: Node, source: &[u8]) -> Vec<String> {
    let mut mods = Vec::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        match child.kind() {
            "virtual" if child.utf8_text(source).ok() == Some("virtual") => {
                mods.push("virtual".to_string());
            }
            "override" if child.utf8_text(source).ok() == Some("override") => {
                mods.push("override".to_string());
            }
            _ => {}
        }
    }
    mods
}

/// Scope-aware call walk: applies namespace/class scope to unqualified callee hints.
fn walk_cpp_calls(
    root: Node,
    source: &[u8],
    file_path: &Path,
    symbols: &[Symbol],
    relations: &mut Vec<Relation>,
    initial_scope: Option<String>,
) {
    const MAX_DEPTH: usize = 2048;
    let function_symbols: Vec<&Symbol> = symbols
        .iter()
        .filter(|s| s.symbol_type == SymbolType::Function)
        .collect();
    let mut stack = vec![(root, 0usize, initial_scope)];

    while let Some((node, depth, scope)) = stack.pop() {
        if depth > MAX_DEPTH {
            tracing::warn!(
                file = ?file_path,
                depth = depth,
                "AST depth limit exceeded during C++ call walk; skipping deep branches"
            );
            continue;
        }

        let next_scope = match node.kind() {
            "namespace_definition" => node
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
                .map(|name| {
                    scope
                        .as_ref()
                        .map(|s| format!("{s}::{name}"))
                        .unwrap_or(name)
                }),
            "class_specifier" | "struct_specifier" => type_name(node, source),
            _ => None,
        };
        let child_scope = next_scope.or(scope.clone());

        if CPP_CALL_KINDS.contains(&node.kind()) {
            push_call_relation(
                node,
                source,
                file_path,
                symbols,
                &function_symbols,
                CPP_CALL_KINDS,
                "cpp",
                relations,
            );
            if let Some(last) = relations.last_mut() {
                if let Some(scope) = child_scope.as_deref() {
                    if let Some(func) = node.child_by_field_name("function") {
                        if last.to_qualified_hint.is_none() && func.kind() == "identifier" {
                            let callee = func.utf8_text(source).ok().unwrap_or("").trim();
                            if !callee.is_empty() {
                                last.to_qualified_hint = Some(format!("{scope}::{callee}"));
                            }
                        } else if let Some(hint) = last.to_qualified_hint.clone() {
                            if let Some(enriched) = enrich_cpp_namespace_hint(&hint, scope) {
                                last.to_qualified_hint = Some(enriched);
                            }
                        }
                    }
                }
            }
        }

        let mut cursor = node.walk();
        let children: Vec<Node> = node.children(&mut cursor).collect();
        for child in children.into_iter().rev() {
            stack.push((child, depth + 1, child_scope.clone()));
        }
    }
}

/// Prefix single-segment qualified calls with the enclosing namespace parent
/// (`repositories::foo` inside `ecommerce::services` → `ecommerce::repositories::foo`).
pub(crate) fn enrich_cpp_namespace_hint(hint: &str, active_scope: &str) -> Option<String> {
    const SKIP_QUALS: &[&str] = &["std", "boost", "absl", "fmt", "detail"];
    let (qual, name) = hint.rsplit_once("::")?;
    if qual.contains("::") {
        return None;
    }
    if SKIP_QUALS.contains(&qual) {
        return None;
    }
    let parent = active_scope.rsplit_once("::").map(|(p, _)| p)?;
    Some(format!("{parent}::{qual}::{name}"))
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
        metadata: serde_json::json!({ "language": "cpp" }),
        to_qualified_hint: None,
        to_type_hint: None,
        ..Default::default()
    }
}

fn function_name_from_node(node: Node, source: &[u8]) -> Option<String> {
    if matches!(node.kind(), "function_definition" | "declaration") {
        if let Some(decl) = node.child_by_field_name("declarator") {
            return name_from_declarator(decl, source);
        }
    }
    None
}

fn find_parameter_list(node: Node) -> Option<Node> {
    let mut current = node.child_by_field_name("declarator")?;
    const MAX_DEPTH: usize = 512;
    for _ in 0..MAX_DEPTH {
        if current.kind() == "function_declarator" {
            return current.child_by_field_name("parameters");
        }
        current = current.child_by_field_name("declarator")?;
    }
    None
}

/// F3: typed parameters from `parameter_list` / `parameter_declaration`.
fn extract_parameters(node: Node, source: &[u8]) -> Result<Vec<Parameter>> {
    let mut parameters = Vec::new();
    let Some(params_node) = find_parameter_list(node) else {
        return Ok(parameters);
    };

    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        if !matches!(
            child.kind(),
            "parameter_declaration" | "optional_parameter_declaration"
        ) {
            continue;
        }
        let param_type = child
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string));
        let name = child
            .child_by_field_name("declarator")
            .and_then(|d| name_from_declarator(d, source));
        let default_value = child
            .child_by_field_name("default_value")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string));
        if let Some(name) = name {
            parameters.push(Parameter {
                name,
                param_type,
                default_value,
            });
        }
    }
    Ok(parameters)
}

pub(crate) fn extract_type_fields(type_node: Node, source: &[u8]) -> Result<Vec<Field>> {
    let mut fields = Vec::new();
    let Some(body) = type_node.child_by_field_name("body") else {
        return Ok(fields);
    };
    if body.kind() != "field_declaration_list" {
        return Ok(fields);
    }

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "field_declaration" {
            continue;
        }
        let field_type = child
            .child_by_field_name("type")
            .and_then(|n| n.utf8_text(source).ok().map(str::to_string));

        for i in 0..child.child_count() {
            if child.field_name_for_child(i as u32) != Some("declarator") {
                continue;
            }
            let Some(declarator) = child.child(i) else {
                continue;
            };
            if let Some(name) = name_from_declarator(declarator, source) {
                fields.push(Field {
                    name,
                    field_type: field_type.clone(),
                    visibility: None,
                });
            }
        }
    }
    Ok(fields)
}

fn name_node_from_declarator(root: Node) -> Option<Node> {
    const MAX_DEPTH: usize = 512;
    let mut stack = vec![(root, 0usize)];

    while let Some((node, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            continue;
        }

        match node.kind() {
            "identifier" | "type_identifier" | "field_identifier" | "destructor_name" => {
                return Some(node);
            }
            "qualified_identifier" => {
                return node.child_by_field_name("name").or(Some(node));
            }
            "function_declarator"
            | "pointer_declarator"
            | "reference_declarator"
            | "parenthesized_declarator"
            | "array_declarator" => {
                if let Some(inner) = node.child_by_field_name("declarator") {
                    stack.push((inner, depth + 1));
                } else {
                    let mut cursor = node.walk();
                    let children: Vec<Node> = node.children(&mut cursor).collect();
                    for child in children.into_iter().rev() {
                        if child.is_named() {
                            stack.push((child, depth + 1));
                        }
                    }
                }
            }
            _ => {
                let mut cursor = node.walk();
                let children: Vec<Node> = node.children(&mut cursor).collect();
                for child in children.into_iter().rev() {
                    if child.is_named() {
                        stack.push((child, depth + 1));
                    }
                }
            }
        }
    }
    None
}

fn name_from_declarator(root: Node, source: &[u8]) -> Option<String> {
    name_node_from_declarator(root).and_then(|n| n.utf8_text(source).ok().map(str::to_string))
}

fn function_name_node(node: Node) -> Option<Node> {
    if matches!(node.kind(), "function_definition" | "declaration") {
        let decl = node.child_by_field_name("declarator")?;
        return name_node_from_declarator(decl);
    }
    None
}

pub(crate) fn type_name(node: Node, source: &[u8]) -> Option<String> {
    node.child_by_field_name("name")
        .and_then(|n| n.utf8_text(source).ok().map(str::to_string))
}

/// Typedef alias from `type_definition` (`typedef union { ... } Alias` → `Alias`).
pub(crate) fn type_definition_typedef_name(node: Node, source: &[u8]) -> Option<String> {
    if node.kind() != "type_definition" {
        return None;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "type_identifier" {
            return child.utf8_text(source).ok().map(str::to_string);
        }
    }
    None
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_extract_cpp_class_and_method() {
        let source = br#"
#include <string>

class UserService {
public:
    std::string authenticate(const std::string& token) {
        return token;
    }
};
"#;
        let plugin = CppPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("UserService.cpp"), source)
            .unwrap();
        assert!(symbols.iter().any(|s| s.name == "UserService"));
        assert!(symbols.iter().any(|s| s.name == "authenticate"));
    }

    #[test]
    fn test_extract_cpp_fields_ctor_and_typed_params() {
        let source = br#"
class Order {
public:
    int orderId;

    Order(int id) : orderId(id) {}

    int get(int x) {
        return x;
    }
};
"#;
        let plugin = CppPlugin::new().unwrap();
        let symbols = plugin
            .extract_symbols(Path::new("Order.cpp"), source)
            .unwrap();

        let class = symbols
            .iter()
            .find(|s| s.name == "Order" && s.symbol_type == SymbolType::Class)
            .expect("Order class");
        assert!(
            class
                .fields
                .iter()
                .any(|f| f.name == "orderId" && f.field_type.as_deref() == Some("int")),
            "fields: {:?}",
            class.fields
        );

        let ctor = symbols
            .iter()
            .find(|s| {
                s.symbol_type == SymbolType::Function
                    && s.metadata.get("is_constructor").and_then(|v| v.as_bool()) == Some(true)
            })
            .expect("constructor");
        assert_eq!(ctor.qualified_name.as_deref(), Some("Order::<init>"));
        assert_eq!(ctor.parameters.len(), 1);
        assert_eq!(ctor.parameters[0].name, "id");
        assert_eq!(ctor.parameters[0].param_type.as_deref(), Some("int"));

        let get = symbols.iter().find(|s| s.name == "get").expect("get");
        assert_eq!(get.parameters.len(), 1);
        assert_eq!(get.parameters[0].name, "x");
        assert_eq!(get.parameters[0].param_type.as_deref(), Some("int"));
    }

    #[test]
    fn test_extract_relations_calls() {
        let source = br#"
void foo() {
    bar();
    baz(1);
}

void bar() {}
int baz(int x) { return x; }
"#;
        let plugin = CppPlugin::new().unwrap();
        let path = Path::new("example.cpp");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls)),
            "expected Calls relations, got {relations:?}"
        );
    }

    #[test]
    fn test_extract_relations_inheritance() {
        let source = br#"class Derived : public Base { };"#;
        let plugin = CppPlugin::new().unwrap();
        let path = Path::new("Service.cpp");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        let extends: Vec<_> = relations
            .iter()
            .filter(|r| matches!(r.relation_type, RelationType::Extends))
            .collect();
        assert_eq!(extends.len(), 1, "expected one Extends: {relations:?}");
        assert_eq!(extends[0].to, "Base");
        assert_eq!(
            extends[0].metadata.get("access").and_then(|v| v.as_str()),
            Some("public")
        );
    }

    #[test]
    fn test_extract_relations_multiple_inheritance() {
        let source = br#"
class Multi : public BaseService, protected OtherBase, private HiddenBase {};
"#;
        let plugin = CppPlugin::new().unwrap();
        let path = Path::new("Multi.cpp");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        let extends: Vec<_> = relations
            .iter()
            .filter(|r| matches!(r.relation_type, RelationType::Extends))
            .collect();
        assert_eq!(extends.len(), 3, "expected three Extends: {relations:?}");
        assert!(extends.iter().any(|r| r.to == "BaseService"));
        assert!(extends.iter().any(|r| r.to == "OtherBase"));
        assert!(extends.iter().any(|r| r.to == "HiddenBase"));
    }

    #[test]
    fn test_extract_relations_qualified_calls() {
        let source = br#"
namespace coolstore {
void init() {}
void caller() {
    coolstore::init();
    obj.method();
}
}
"#;
        let plugin = CppPlugin::new().unwrap();
        let path = Path::new("calls.cpp");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        let calls: Vec<_> = relations
            .iter()
            .filter(|r| matches!(r.relation_type, RelationType::Calls))
            .collect();
        assert!(
            calls.iter().any(|r| {
                r.to_qualified_hint
                    .as_deref()
                    .is_some_and(|h| h.contains("coolstore::init"))
            }),
            "expected qualified hint for namespaced call: {relations:?}"
        );
    }

    #[test]
    fn test_extract_relations_template_call() {
        let source = br#"
template<typename T>
void caller() {
    foo<T>();
}
"#;
        let plugin = CppPlugin::new().unwrap();
        let path = Path::new("template_call.cpp");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| matches!(r.relation_type, RelationType::Calls) && r.to == "foo"),
            "expected template call extraction: {relations:?}"
        );
    }

    #[test]
    fn test_extract_relations_namespace_sibling_hint() {
        let service_source = br#"
namespace ecommerce::services {
int checkout(sqlite3* db, int user_id) {
    return repositories::order_create(db, nullptr);
}
}
"#;
        let repo_source = br#"
namespace ecommerce::repositories {
int order_create(sqlite3* db, void* out) { return 0; }
}
"#;
        let plugin = CppPlugin::new().unwrap();
        let service_path = Path::new("order_service.cpp");
        let service_symbols = plugin
            .extract_symbols(service_path, service_source)
            .unwrap();
        let relations = plugin
            .extract_relations(service_path, service_source, &service_symbols)
            .unwrap();
        let call = relations
            .iter()
            .find(|r| matches!(r.relation_type, RelationType::Calls))
            .unwrap_or_else(|| panic!("missing call: {relations:?}"));
        assert_eq!(
            call.to_qualified_hint.as_deref(),
            Some("ecommerce::repositories::order_create")
        );
        assert!(
            call.to.contains("order_create"),
            "unexpected to field: {}",
            call.to
        );
        let _ = plugin.extract_symbols(Path::new("order_repository.cpp"), repo_source);
    }

    #[test]
    fn test_extract_relations_instantiates() {
        let source = br#"
void make() {
    auto p = new Order();
    auto arr = new Order[4];
}
"#;
        let plugin = CppPlugin::new().unwrap();
        let path = Path::new("new.cpp");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        let inst: Vec<_> = relations
            .iter()
            .filter(|r| matches!(r.relation_type, RelationType::Instantiates))
            .collect();
        assert!(
            inst.iter().any(|r| r.to == "Order"),
            "expected Instantiates to Order: {relations:?}"
        );
        assert!(inst.len() >= 2, "expected heap + array new: {relations:?}");
    }
}
