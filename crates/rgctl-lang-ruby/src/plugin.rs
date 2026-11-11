//! Ruby `LanguagePlugin` — symbols, relations, complexity.

use crate::single_pass::{extract_all_single_pass, ruby_ast_handlers};
use rgctl_plugin_api::{
    AstHandlerTable, ComplexityMetrics, Error, ExtractAllResult, Field, LanguagePlugin, Parameter,
    RUBY_CALL_KINDS, Relation, RelationType, Result, SourceLocation, Symbol, SymbolType,
    callee_name, complexity_from_metadata, containing_function, parse_with_pool, ruby_call_callee,
    walk_calls,
};
use rgctl_plugin_helpers::{ComplexityCalculator, record_symbol_spans};
use std::path::Path;
use tree_sitter::{Node, Parser, Tree};

pub(crate) const RUBY_FUNCTION_KINDS: &[&str] = &["method", "singleton_method"];

pub(crate) const RUBY_COMPLEXITY_BRANCH_KINDS: &[&str] = &[
    "if",
    "unless",
    "while",
    "until",
    "for",
    "case",
    "rescue",
    "rescue_modifier",
];

/// Ruby Tier 1 language plugin.
pub struct RubyPlugin {
    _parser: Parser,
}

impl RubyPlugin {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_ruby::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Ruby grammar: {e}")))?;
        Ok(Self { _parser: parser })
    }

    pub(crate) fn parse(&self, file_path: &Path, source: &[u8]) -> Result<Tree> {
        parse_with_pool("ruby", tree_sitter_ruby::LANGUAGE.into(), source).map_err(|e| {
            Error::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message: e,
            }
        })
    }

    pub(crate) fn loc(node: Node, file_path: &str) -> SourceLocation {
        SourceLocation {
            file: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_column: node.start_position().column,
            end_column: node.end_position().column,
        }
    }

    pub(crate) fn constant_text(node: Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "constant" | "identifier" | "simple_symbol" => node
                .utf8_text(source)
                .ok()
                .map(|s| s.trim_start_matches(':').to_string()),
            "scope_resolution" => {
                let mut parts = Vec::new();
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.kind() == "constant" || child.kind() == "identifier" {
                        if let Ok(t) = child.utf8_text(source) {
                            parts.push(t.to_string());
                        }
                    }
                }
                if parts.is_empty() {
                    node.utf8_text(source).ok().map(str::to_string)
                } else {
                    Some(parts.join("::"))
                }
            }
            _ => callee_name(node, source),
        }
    }

    pub(crate) fn method_name(node: Node, source: &[u8]) -> Option<String> {
        node.child_by_field_name("name")
            .and_then(|n| Self::constant_text(n, source))
            .or_else(|| {
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.kind() == "identifier" || child.kind() == "constant" {
                        return child.utf8_text(source).ok().map(str::to_string);
                    }
                }
                None
            })
    }

    pub(crate) fn join_qn(prefix: &[String], name: &str) -> String {
        if prefix.is_empty() {
            name.to_string()
        } else {
            format!("{}::{name}", prefix.join("::"))
        }
    }

    fn instance_method_qn(type_qn: &str, method: &str) -> String {
        format!("{type_qn}#{method}")
    }

    fn class_method_qn(type_qn: &str, method: &str) -> String {
        format!("{type_qn}.{method}")
    }

    fn extract_parameters(params: Node, source: &[u8]) -> Result<Vec<Parameter>> {
        let mut out = Vec::new();
        let mut c = params.walk();
        for child in params.children(&mut c) {
            match child.kind() {
                "identifier" | "optional_parameter" | "keyword_parameter" => {
                    let name_node = child.child_by_field_name("name").unwrap_or(child);
                    if let Ok(name) = name_node.utf8_text(source) {
                        out.push(Parameter {
                            name: name.to_string(),
                            param_type: None,
                            default_value: None,
                        });
                    }
                }
                "block_parameter"
                | "forward_parameter"
                | "splat_parameter"
                | "hash_splat_parameter" => {
                    if let Ok(name) = child.utf8_text(source) {
                        out.push(Parameter {
                            name: name
                                .trim_start_matches('&')
                                .trim_start_matches('*')
                                .to_string(),
                            param_type: None,
                            default_value: None,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }

    pub(crate) fn extract_method(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        scope: &[String],
        owner_type_qn: Option<&str>,
        singleton: bool,
    ) -> Result<Symbol> {
        let name = Self::method_name(node, source).ok_or_else(|| Error::ParseError {
            file: file_path.into(),
            line: node.start_position().row + 1,
            message: "Method missing name".to_string(),
        })?;

        let parameters = node
            .child_by_field_name("parameters")
            .map(|p| Self::extract_parameters(p, source))
            .transpose()?
            .unwrap_or_default();

        let (qualified_name, metadata) = if name == "initialize" {
            if let Some(tq) = owner_type_qn {
                (
                    Some(format!("{tq}.<init>")),
                    serde_json::json!({
                        "language": "ruby",
                        "is_constructor": true,
                    }),
                )
            } else {
                (
                    None,
                    serde_json::json!({ "language": "ruby", "is_constructor": true }),
                )
            }
        } else if let Some(tq) = owner_type_qn {
            let qn = if singleton {
                Self::class_method_qn(tq, &name)
            } else {
                Self::instance_method_qn(tq, &name)
            };
            (
                Some(qn),
                serde_json::json!({
                    "language": "ruby",
                    "singleton": singleton,
                }),
            )
        } else {
            (
                Some(Self::join_qn(scope, &name)),
                serde_json::json!({ "language": "ruby" }),
            )
        };

        let mut sym = Symbol {
            name,
            symbol_type: SymbolType::Function,
            qualified_name,
            location: Self::loc(node, file_path),
            signature: Some(
                node.utf8_text(source)
                    .unwrap_or("")
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
        record_symbol_spans(&mut sym, node, node.child_by_field_name("name"));
        sym = rgctl_plugin_helpers::callable_identity::symbol(
            sym,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Ruby,
        );
        Ok(sym)
    }

    pub(crate) fn extract_class_or_module(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        scope: &[String],
        is_module: bool,
    ) -> Result<(Symbol, String)> {
        let name = node
            .child_by_field_name("name")
            .and_then(|n| Self::constant_text(n, source))
            .ok_or_else(|| Error::ParseError {
                file: file_path.into(),
                line: node.start_position().row + 1,
                message: "Type missing name".to_string(),
            })?;

        let type_qn = Self::join_qn(scope, &name);
        let mut sym = Symbol {
            name: name.clone(),
            symbol_type: if is_module {
                SymbolType::Module
            } else {
                SymbolType::Class
            },
            qualified_name: Some(type_qn.clone()),
            location: Self::loc(node, file_path),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: self.extract_type_fields(node, source)?,
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "ruby",
                "kind": if is_module { "module" } else { "class" },
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut sym, node, node.child_by_field_name("name"));
        Ok((sym, type_qn))
    }

    fn extract_type_fields(&self, type_node: Node, source: &[u8]) -> Result<Vec<Field>> {
        let mut fields = Vec::new();
        let mut stack = vec![type_node];
        while let Some(node) = stack.pop() {
            if node.kind() == "call" {
                if let Some(method) = ruby_call_callee(node, source) {
                    if matches!(
                        method.as_str(),
                        "attr_reader" | "attr_writer" | "attr_accessor"
                    ) {
                        let mut c = node.walk();
                        for child in node.children(&mut c) {
                            if child.kind() == "argument_list" || child.kind() == "simple_symbol" {
                                let mut c2 = child.walk();
                                for arg in child.children(&mut c2) {
                                    if let Some(n) = Self::constant_text(arg, source) {
                                        fields.push(Field {
                                            source_span: rgctl_plugin_helpers::field_source_span(
                                                arg, source, &n,
                                            ),
                                            name: n,
                                            field_type: None,
                                            visibility: None,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
            if node.kind() == "method" {
                if Self::method_name(node, source).as_deref() == Some("initialize") {
                    if let Some(body) = node.child_by_field_name("body") {
                        self.collect_ivar_fields(body, source, &mut fields);
                    }
                }
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.is_named() {
                    stack.push(child);
                }
            }
        }
        fields.sort_by(|a, b| a.name.cmp(&b.name));
        fields.dedup_by(|a, b| a.name == b.name);
        Ok(fields)
    }

    fn collect_ivar_fields(&self, body: Node, source: &[u8], fields: &mut Vec<Field>) {
        let mut stack = vec![body];
        while let Some(node) = stack.pop() {
            if node.kind() == "assignment" {
                if let Some(left) = node.child_by_field_name("left") {
                    if left.kind() == "instance_variable" {
                        if let Ok(name) = left.utf8_text(source) {
                            fields.push(Field {
                                source_span: rgctl_plugin_helpers::field_source_span(
                                    node,
                                    source,
                                    name.trim_start_matches('@'),
                                ),
                                name: name.trim_start_matches('@').to_string(),
                                field_type: None,
                                visibility: None,
                            });
                        }
                    }
                }
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.is_named() {
                    stack.push(child);
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn traverse_symbols(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        scope: &mut Vec<String>,
        owner_type_qn: Option<String>,
        singleton: bool,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        match node.kind() {
            "class" => {
                let (sym, type_qn) =
                    self.extract_class_or_module(node, source, file_path, scope, false)?;
                symbols.push(sym);
                scope.push(
                    node.child_by_field_name("name")
                        .and_then(|n| Self::constant_text(n, source))
                        .unwrap_or_default(),
                );
                self.walk_type_body(node, source, file_path, scope, Some(type_qn), symbols)?;
                scope.pop();
            }
            "module" => {
                let (sym, type_qn) =
                    self.extract_class_or_module(node, source, file_path, scope, true)?;
                symbols.push(sym);
                scope.push(
                    node.child_by_field_name("name")
                        .and_then(|n| Self::constant_text(n, source))
                        .unwrap_or_default(),
                );
                self.walk_type_body(node, source, file_path, scope, Some(type_qn), symbols)?;
                scope.pop();
            }
            "singleton_class" => {
                let outer = owner_type_qn
                    .clone()
                    .unwrap_or_else(|| Self::join_qn(scope, "Object"));
                self.walk_type_body(node, source, file_path, scope, Some(outer), symbols)?;
            }
            "method" => {
                symbols.push(self.extract_method(
                    node,
                    source,
                    file_path,
                    scope,
                    owner_type_qn.as_deref(),
                    singleton,
                )?);
            }
            "singleton_method" => {
                symbols.push(self.extract_method(
                    node,
                    source,
                    file_path,
                    scope,
                    owner_type_qn.as_deref(),
                    true,
                )?);
            }
            _ => {
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.is_named() {
                        self.traverse_symbols(
                            child,
                            source,
                            file_path,
                            scope,
                            owner_type_qn.clone(),
                            singleton,
                            symbols,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn walk_type_body(
        &self,
        type_node: Node,
        source: &[u8],
        file_path: &str,
        scope: &mut Vec<String>,
        owner_type_qn: Option<String>,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        let mut c = type_node.walk();
        for child in type_node.children(&mut c) {
            if child.is_named() {
                self.walk_body_content(
                    child,
                    source,
                    file_path,
                    scope,
                    owner_type_qn.clone(),
                    symbols,
                )?;
            }
        }
        Ok(())
    }

    fn walk_body_content(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        scope: &mut Vec<String>,
        owner_type_qn: Option<String>,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        match node.kind() {
            "body_statement" | "program" => {
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.is_named() {
                        self.walk_body_content(
                            child,
                            source,
                            file_path,
                            scope,
                            owner_type_qn.clone(),
                            symbols,
                        )?;
                    }
                }
            }
            "class" | "module" | "singleton_class" => {
                self.traverse_symbols(
                    node,
                    source,
                    file_path,
                    scope,
                    owner_type_qn,
                    false,
                    symbols,
                )?;
            }
            "method" | "singleton_method" => {
                symbols.push(self.extract_method(
                    node,
                    source,
                    file_path,
                    scope,
                    owner_type_qn.as_deref(),
                    node.kind() == "singleton_method",
                )?);
            }
            _ => {
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.is_named() {
                        self.walk_body_content(
                            child,
                            source,
                            file_path,
                            scope,
                            owner_type_qn.clone(),
                            symbols,
                        )?;
                    }
                }
            }
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
        let mut scope = Vec::new();
        let file_path_str = file_path.to_string_lossy();
        self.traverse_symbols(
            root,
            source,
            &file_path_str,
            &mut scope,
            None,
            false,
            &mut symbols,
        )?;
        self.extract_top_level_requires(root, source, &file_path_str, &mut symbols)?;
        Ok(symbols)
    }

    fn extract_top_level_requires(
        &self,
        root: Node,
        source: &[u8],
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) -> Result<()> {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            if node.kind() == "call" {
                if let Some(callee) = ruby_call_callee(node, source) {
                    if matches!(callee.as_str(), "require" | "require_relative") {
                        if let Some(path) = self.require_path_arg(node, source) {
                            let mut sym = Symbol {
                                name: path.clone(),
                                symbol_type: SymbolType::Import,
                                qualified_name: Some(path),
                                location: Self::loc(node, file_path),
                                signature: None,
                                return_type: None,
                                parameters: vec![],
                                fields: vec![],
                                modifiers: vec![],
                                documentation: None,
                                metadata: serde_json::json!({
                                    "language": "ruby",
                                    "kind": callee,
                                }),
                                ..Default::default()
                            };
                            record_symbol_spans(&mut sym, node, None);
                            symbols.push(sym);
                        }
                    }
                }
            }
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if child.is_named() {
                    stack.push(child);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn require_path_arg(&self, call: Node, source: &[u8]) -> Option<String> {
        let mut c = call.walk();
        for child in call.children(&mut c) {
            if child.kind() == "argument_list" || child.kind() == "string" {
                let mut c2 = child.walk();
                for arg in child.children(&mut c2) {
                    if arg.kind() == "string" || arg.kind() == "string_content" {
                        if let Ok(t) = arg.utf8_text(source) {
                            return Some(t.trim_matches(['"', '\'']).to_string());
                        }
                    }
                }
            }
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
        let file_path_str = file_path.to_string_lossy();
        walk_calls(
            root,
            source,
            file_path,
            symbols,
            RUBY_CALL_KINDS,
            "ruby",
            &mut relations,
        );
        self.extract_mixin_and_new_edges(root, source, &file_path_str, symbols, &mut relations)?;
        Ok(relations)
    }

    pub(crate) fn extract_mixin_and_new_edges(
        &self,
        root: Node,
        source: &[u8],
        file_path: &str,
        symbols: &[Symbol],
        relations: &mut Vec<Relation>,
    ) -> Result<()> {
        let function_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| s.symbol_type == SymbolType::Function)
            .collect();

        let mut scope: Vec<String> = Vec::new();
        self.walk_relation_calls(
            root,
            source,
            file_path,
            symbols,
            &function_symbols,
            &mut scope,
            None,
            relations,
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments, clippy::only_used_in_recursion)]
    fn walk_relation_calls(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbols: &[Symbol],
        function_symbols: &[&Symbol],
        scope: &mut Vec<String>,
        owner_type_qn: Option<String>,
        relations: &mut Vec<Relation>,
    ) {
        let mut next_owner = owner_type_qn.clone();
        let mut pushed_scope = false;
        match node.kind() {
            "class" | "module" => {
                if let Some(name) = node
                    .child_by_field_name("name")
                    .and_then(|n| Self::constant_text(n, source))
                {
                    next_owner = Some(Self::join_qn(scope, &name));
                    scope.push(name);
                    pushed_scope = true;
                }
            }
            "call" => {
                let from = containing_function(node, function_symbols)
                    .and_then(|f| f.qualified_name.clone())
                    .or_else(|| next_owner.clone())
                    .or_else(|| scope.last().map(|n| Self::join_qn(scope, n)));
                if let Some(from) = from {
                    if let Some(callee) = ruby_call_callee(node, source) {
                        match callee.as_str() {
                            "include" | "prepend" => {
                                if let Some(mod_name) = self.first_constant_arg(node, source) {
                                    relations.push(Relation {
                                        from: from.clone(),
                                        to: mod_name.clone(),
                                        relation_type: RelationType::Extends,
                                        location: Self::loc(node, file_path),
                                        metadata: serde_json::json!({
                                            "language": "ruby",
                                            "mixin": callee,
                                        }),
                                        to_qualified_hint: Some(mod_name),
                                        to_type_hint: None,
                                        ..Default::default()
                                    });
                                }
                            }
                            "extend" => {
                                if let Some(mod_name) = self.first_constant_arg(node, source) {
                                    relations.push(Relation {
                                        from: from.clone(),
                                        to: mod_name.clone(),
                                        relation_type: RelationType::Uses,
                                        location: Self::loc(node, file_path),
                                        metadata: serde_json::json!({
                                            "language": "ruby",
                                            "mixin": "extend",
                                        }),
                                        to_qualified_hint: Some(mod_name),
                                        to_type_hint: None,
                                        ..Default::default()
                                    });
                                }
                            }
                            "new" => {
                                if let Some(recv) = node.child_by_field_name("receiver") {
                                    if let Some(ty) = Self::constant_text(recv, source) {
                                        relations.push(Relation {
                                            from: from.clone(),
                                            to: ty.clone(),
                                            relation_type: RelationType::Instantiates,
                                            location: Self::loc(node, file_path),
                                            metadata: serde_json::json!({ "language": "ruby" }),
                                            to_qualified_hint: Some(ty),
                                            to_type_hint: None,
                                            ..Default::default()
                                        });
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            _ => {}
        }

        let mut c = node.walk();
        for child in node.children(&mut c) {
            if child.is_named() {
                self.walk_relation_calls(
                    child,
                    source,
                    file_path,
                    symbols,
                    function_symbols,
                    scope,
                    next_owner.clone(),
                    relations,
                );
            }
        }

        if pushed_scope {
            scope.pop();
        }
    }

    fn first_constant_arg(&self, call: Node, source: &[u8]) -> Option<String> {
        let mut c = call.walk();
        for child in call.children(&mut c) {
            if child.kind() == "argument_list" {
                let mut c2 = child.walk();
                for arg in child.children(&mut c2) {
                    if arg.is_named() {
                        return Self::constant_text(arg, source);
                    }
                }
            }
        }
        None
    }

    fn find_method_node<'a>(
        node: Node<'a>,
        target_line: usize,
        name: &str,
        source: &[u8],
    ) -> Option<Node<'a>> {
        if RUBY_FUNCTION_KINDS.contains(&node.kind()) {
            if node.start_position().row == target_line {
                if Self::method_name(node, source).as_deref() == Some(name) {
                    return Some(node);
                }
            }
        }
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if let Some(found) = Self::find_method_node(child, target_line, name, source) {
                return Some(found);
            }
        }
        None
    }
}

impl LanguagePlugin for RubyPlugin {
    fn language_id(&self) -> &str {
        "ruby"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["rb"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_ruby::LANGUAGE.into())
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
        Some(ruby_ast_handlers())
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
        let Some(func_node) =
            Self::find_method_node(tree.root_node(), target_line, &symbol.name, source)
        else {
            return Ok(None);
        };
        Ok(Some(ComplexityMetrics {
            cyclomatic: ComplexityCalculator::cyclomatic(func_node, RUBY_COMPLEXITY_BRANCH_KINDS),
            cognitive: ComplexityCalculator::cognitive(func_node, RUBY_COMPLEXITY_BRANCH_KINDS),
            loc: ComplexityCalculator::loc(func_node),
            parameters: symbol.parameters.len(),
            nesting_depth: ComplexityCalculator::nesting_depth(
                func_node,
                &["body_statement", "block"],
            ),
            returns: ComplexityCalculator::return_count(func_node, "return"),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_class_and_instance_method() {
        let source = br#"
module Shop
  class Order
    def total
      1
    end
  end
end
"#;
        let plugin = RubyPlugin::new().unwrap();
        let path = Path::new("order.rb");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        assert!(
            symbols
                .iter()
                .any(|s| s.name == "Order" && s.symbol_type == SymbolType::Class)
        );
        let method = symbols.iter().find(|s| s.name == "total").expect("total");
        assert_eq!(method.qualified_name.as_deref(), Some("Shop::Order#total"));
    }

    #[test]
    fn test_initialize_constructor() {
        let source = br#"
class Order
  def initialize(customer)
    @customer = customer
  end
end
"#;
        let plugin = RubyPlugin::new().unwrap();
        let symbols = plugin.extract_symbols(Path::new("o.rb"), source).unwrap();
        let init = symbols
            .iter()
            .find(|s| s.name == "initialize")
            .expect("init");
        assert!(
            init.metadata
                .get("is_constructor")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        );
        assert_eq!(init.qualified_name.as_deref(), Some("Order.<init>"));
        let class = symbols.iter().find(|s| s.name == "Order").expect("class");
        assert!(class.fields.iter().any(|f| f.name == "customer"));
    }

    #[test]
    fn test_calls_and_require() {
        let source = br#"
require_relative 'models/user'

class Auth
  def login
    User.find(1)
    puts "ok"
  end
end
"#;
        let plugin = RubyPlugin::new().unwrap();
        let path = Path::new("auth.rb");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        assert!(symbols.iter().any(|s| s.symbol_type == SymbolType::Import));
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Calls)
        );
    }

    #[test]
    fn test_ecommerce_fixture_calls_and_extends() {
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let path = manifest_dir.join("../../rgctl-tests/ecommerce-ruby/lib/order_dto.rb");
        let source = std::fs::read(&path).expect("read fixture");
        let plugin = RubyPlugin::new().unwrap();
        let extracted = plugin.extract_all(&path, &source).unwrap();
        assert!(
            extracted
                .relations
                .iter()
                .any(|r| r.relation_type == RelationType::Extends),
            "include Trackable -> Extends"
        );
        let extend = extracted
            .relations
            .iter()
            .find(|r| r.relation_type == RelationType::Extends)
            .expect("extends");
        assert_eq!(extend.from, "OrderDTO", "Extends from should be class QN");
        let path2 =
            manifest_dir.join("../../rgctl-tests/ecommerce-ruby/app/services/order_service.rb");
        let source2 = std::fs::read(&path2).expect("read service");
        let extracted2 = plugin.extract_all(&path2, &source2).unwrap();
        assert!(
            extracted2
                .relations
                .iter()
                .any(|r| r.relation_type == RelationType::Calls),
            "method calls in OrderService: {:?}",
            extracted2
                .relations
                .iter()
                .map(|r| (r.relation_type, r.from.clone(), r.to.clone()))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_include_mixin() {
        let source = br#"
module Timestampable
end

class User
  include Timestampable
end
"#;
        let plugin = RubyPlugin::new().unwrap();
        let path = Path::new("user.rb");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::Extends)
        );
    }
}
