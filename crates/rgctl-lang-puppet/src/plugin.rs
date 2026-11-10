//! Puppet `LanguagePlugin` — symbols, relations, complexity.

use crate::single_pass::{extract_all_single_pass, puppet_ast_handlers};
use rgctl_plugin_api::{
    AstHandlerTable, ComplexityMetrics, Error, ExtractAllResult, Field, LanguagePlugin, Parameter,
    Relation, RelationType, Result, SourceLocation, Symbol, SymbolType, complexity_from_metadata,
    parse_with_pool,
};
use rgctl_plugin_helpers::{ComplexityCalculator, record_symbol_spans};
use std::path::Path;
use tree_sitter::{Node, Parser, Tree};

pub(crate) const PUPPET_BRANCH_KINDS: &[&str] = &[
    "if_statement",
    "unless_statement",
    "case_statement",
    "selector",
    "iterator_statement",
    "elsif_statement",
];

const BRANCH_KINDS: &[&str] = PUPPET_BRANCH_KINDS;

const NESTING_KINDS: &[&str] = &[
    "if_statement",
    "unless_statement",
    "case_statement",
    "block",
    "iterator_statement",
];

/// Puppet Tier 1 language plugin.
pub struct PuppetPlugin {
    _parser: Parser,
}

impl PuppetPlugin {
    pub fn new() -> Result<Self> {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_puppet::LANGUAGE.into())
            .map_err(|e| Error::PluginError(format!("Failed to set Puppet grammar: {e}")))?;
        Ok(Self { _parser: parser })
    }

    pub(crate) fn parse(&self, file_path: &Path, source: &[u8]) -> Result<Tree> {
        parse_with_pool("puppet", tree_sitter_puppet::LANGUAGE.into(), source).map_err(|e| {
            Error::ParseError {
                file: file_path.to_path_buf(),
                line: 0,
                message: e,
            }
        })
    }

    /// Emit symbols for a single node (no child walk) — used by single-pass DFS.
    pub(crate) fn collect_symbol_at(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) {
        // Reuse walk_symbols match arm by temporarily walking only this node:
        // walk_symbols always recurses; instead duplicate the match via a helper flag.
        self.walk_symbols_node_only(node, source, file_path, symbols);
    }

    pub(crate) fn collect_relation_at(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        relations: &mut Vec<Relation>,
    ) {
        self.walk_relations_node_only(node, source, file_path, relations);
    }

    fn loc(node: Node, file_path: &str) -> SourceLocation {
        SourceLocation {
            file: file_path.to_string(),
            start_line: node.start_position().row + 1,
            end_line: node.end_position().row + 1,
            start_column: node.start_position().column,
            end_column: node.end_position().column,
        }
    }

    fn text(node: Node, source: &[u8]) -> Option<String> {
        node.utf8_text(source).ok().map(|s| s.to_string())
    }

    /// Identifier / class_identifier / string content.
    fn ident_text(node: Node, source: &[u8]) -> Option<String> {
        match node.kind() {
            "identifier" | "class_identifier" | "node_name" => Self::text(node, source),
            "string" => {
                let raw = Self::text(node, source)?;
                Some(raw.trim_matches('\'').trim_matches('"').to_string())
            }
            "variable" => Self::text(node, source),
            _ => {
                // Prefer nested identifier
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if matches!(
                        child.kind(),
                        "identifier" | "class_identifier" | "string" | "node_name"
                    ) && let Some(t) = Self::ident_text(child, source)
                    {
                        return Some(t);
                    }
                }
                Self::text(node, source)
            }
        }
    }

    fn first_child_ident(node: Node, source: &[u8]) -> Option<String> {
        Self::first_child_ident_node(node).and_then(|n| Self::ident_text(n, source))
    }

    fn first_child_ident_node(node: Node<'_>) -> Option<Node<'_>> {
        let mut c = node.walk();
        for child in node.children(&mut c) {
            if matches!(
                child.kind(),
                "identifier" | "class_identifier" | "string" | "node_name" | "default"
            ) {
                return Some(child);
            }
        }
        None
    }

    fn extract_parameters(params: Node, source: &[u8]) -> Vec<Parameter> {
        let mut out = Vec::new();
        let mut c = params.walk();
        for child in params.children(&mut c) {
            if child.kind() != "parameter" {
                continue;
            }
            let mut name = None;
            let mut param_type = None;
            let mut pc = child.walk();
            for part in child.children(&mut pc) {
                match part.kind() {
                    "variable" => name = Self::text(part, source),
                    "type" | "builtin_type" | "array_type" | "composite_type"
                    | "attribute_type"
                        if param_type.is_none() =>
                    {
                        param_type = Self::text(part, source);
                    }
                    _ => {}
                }
            }
            if let Some(n) = name {
                let clean = n.trim_start_matches('$').to_string();
                out.push(Parameter {
                    name: clean,
                    param_type,
                    default_value: None,
                });
            }
        }
        out
    }

    fn params_as_fields(params: &[Parameter]) -> Vec<Field> {
        params
            .iter()
            .map(|p| Field {
                name: p.name.clone(),
                field_type: p.param_type.clone(),
                visibility: None,
            })
            .collect()
    }

    fn enclosing_host_name(node: Node, source: &[u8]) -> Option<String> {
        let mut cur = node;
        while let Some(parent) = cur.parent() {
            match parent.kind() {
                "class_definition" | "defined_resource_type" | "function_declaration" => {
                    return Self::first_child_ident(parent, source);
                }
                "node_definition" => {
                    return Self::first_child_ident(parent, source).map(|n| format!("node:{n}"));
                }
                _ => cur = parent,
            }
        }
        None
    }

    #[allow(clippy::too_many_arguments)]
    fn push_cfg_host_function(
        symbols: &mut Vec<Symbol>,
        name: String,
        qualified_name: String,
        node: Node,
        file_path: &str,
        source: &[u8],
        parameters: Vec<Parameter>,
        puppet_kind: &str,
    ) {
        // Discover CFG indexes `NodeType::Function` only; Puppet class/define/node
        // bodies are the CFG hosts, so emit a parallel Function symbol.
        let name_node = Self::first_child_ident_node(node);
        let mut sym = Symbol {
            name: name.clone(),
            symbol_type: SymbolType::Function,
            qualified_name: Some(qualified_name),
            location: Self::loc(node, file_path),
            signature: Self::text(node, source)
                .and_then(|s| s.lines().next().map(|l| l.trim().to_string())),
            return_type: None,
            parameters,
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: serde_json::json!({
                "language": "puppet",
                "cfg_host": true,
                "puppet_kind": puppet_kind
            }),
            ..Default::default()
        };
        record_symbol_spans(&mut sym, node, name_node);
        sym = rgctl_plugin_helpers::callable_identity::symbol(
            sym,
            node,
            source,
            rgctl_plugin_helpers::callable_identity::Policy::Puppet,
        );
        symbols.push(sym);
    }

    fn walk_symbols(&self, node: Node, source: &[u8], file_path: &str, symbols: &mut Vec<Symbol>) {
        self.walk_symbols_node_only(node, source, file_path, symbols);
        let mut c = node.walk();
        for child in node.children(&mut c).collect::<Vec<_>>() {
            self.walk_symbols(child, source, file_path, symbols);
        }
    }

    pub(crate) fn walk_symbols_node_only(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        symbols: &mut Vec<Symbol>,
    ) {
        match node.kind() {
            "class_definition" => {
                if let Some(name) = Self::first_child_ident(node, source) {
                    let params = node
                        .children(&mut node.walk())
                        .find(|c| c.kind() == "parameter_list")
                        .map(|p| Self::extract_parameters(p, source))
                        .unwrap_or_default();
                    let fields = Self::params_as_fields(&params);
                    let mut sym = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::PuppetClass,
                        qualified_name: Some(name.clone()),
                        location: Self::loc(node, file_path),
                        signature: Self::text(node, source)
                            .and_then(|s| s.lines().next().map(|l| l.trim().to_string())),
                        return_type: None,
                        parameters: params.clone(),
                        fields,
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "language": "puppet" }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut sym, node, Self::first_child_ident_node(node));
                    symbols.push(sym);
                    // Distinct qn so Function is not deduped against PuppetClass.
                    Self::push_cfg_host_function(
                        symbols,
                        name.clone(),
                        format!("cfg:{name}"),
                        node,
                        file_path,
                        source,
                        params,
                        "class",
                    );
                }
            }
            "defined_resource_type" => {
                if let Some(name) = Self::first_child_ident(node, source) {
                    let params = node
                        .children(&mut node.walk())
                        .find(|c| c.kind() == "parameter_list")
                        .map(|p| Self::extract_parameters(p, source))
                        .unwrap_or_default();
                    let fields = Self::params_as_fields(&params);
                    let mut sym = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::PuppetDefinedType,
                        qualified_name: Some(name.clone()),
                        location: Self::loc(node, file_path),
                        signature: Self::text(node, source)
                            .and_then(|s| s.lines().next().map(|l| l.trim().to_string())),
                        return_type: None,
                        parameters: params.clone(),
                        fields,
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "language": "puppet" }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut sym, node, Self::first_child_ident_node(node));
                    symbols.push(sym);
                    Self::push_cfg_host_function(
                        symbols,
                        name.clone(),
                        format!("cfg:{name}"),
                        node,
                        file_path,
                        source,
                        params,
                        "defined_type",
                    );
                }
            }
            "node_definition" => {
                let mut names = Vec::new();
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.kind() == "node_name"
                        && let Some(n) = Self::ident_text(child, source)
                    {
                        names.push((n, child));
                    }
                }
                for (name, name_node) in names {
                    let q = format!("node:{name}");
                    let mut sym = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::PuppetNode,
                        qualified_name: Some(q.clone()),
                        location: Self::loc(node, file_path),
                        signature: Some(format!("node {name}")),
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "language": "puppet", "kind": "node" }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut sym, node, Some(name_node));
                    symbols.push(sym);
                    // CFG `callable_name_for_cfg` returns `node:{name}`.
                    Self::push_cfg_host_function(
                        symbols,
                        q.clone(),
                        format!("cfg:{q}"),
                        node,
                        file_path,
                        source,
                        vec![],
                        "node",
                    );
                }
            }
            "function_declaration" => {
                if let Some(name) = Self::first_child_ident(node, source) {
                    let params = node
                        .children(&mut node.walk())
                        .find(|c| c.kind() == "parameter_list")
                        .map(|p| Self::extract_parameters(p, source))
                        .unwrap_or_default();
                    let stem = Path::new(file_path)
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .unwrap_or("puppet");
                    let qn = if name.contains("::") {
                        name.clone()
                    } else {
                        format!("{stem}::{name}")
                    };
                    let mut sym = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::Function,
                        qualified_name: Some(qn),
                        location: Self::loc(node, file_path),
                        signature: Self::text(node, source)
                            .and_then(|s| s.lines().next().map(|l| l.trim().to_string())),
                        return_type: None,
                        parameters: params,
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "language": "puppet" }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut sym, node, Self::first_child_ident_node(node));
                    sym = rgctl_plugin_helpers::callable_identity::symbol(
                        sym,
                        node,
                        source,
                        rgctl_plugin_helpers::callable_identity::Policy::Puppet,
                    );
                    symbols.push(sym);
                }
            }
            "type_declaration" => {
                if let Some(name) = Self::first_child_ident(node, source) {
                    let mut sym = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::TypeAlias,
                        qualified_name: Some(name),
                        location: Self::loc(node, file_path),
                        signature: Self::text(node, source)
                            .and_then(|s| s.lines().next().map(|l| l.trim().to_string())),
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({
                            "language": "puppet",
                            "kind": "type_alias"
                        }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut sym, node, Self::first_child_ident_node(node));
                    symbols.push(sym);
                }
            }
            "resource_declaration" => {
                let type_name = node
                    .child_by_field_name("type")
                    .and_then(|n| Self::ident_text(n, source));
                let title_node = node.child_by_field_name("title");
                let title = title_node.and_then(|n| Self::ident_text(n, source));
                if let (Some(ty), Some(title)) = (type_name, title) {
                    let name = format!("{ty}[{title}]");
                    let mut sym = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::PuppetResource,
                        qualified_name: Some(name),
                        location: Self::loc(node, file_path),
                        signature: Some(format!("{ty} {{ '{title}': ... }}")),
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({
                            "language": "puppet",
                            "resource_type": ty,
                            "title": title
                        }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut sym, node, title_node);
                    symbols.push(sym);
                }
            }
            "assignment" => {
                // Emit variable on LHS
                let mut c = node.walk();
                if let Some(var) = node.children(&mut c).find(|ch| ch.kind() == "variable")
                    && let Some(raw) = Self::text(var, source)
                {
                    let name = raw.trim_start_matches('$').to_string();
                    let mut sym = Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::PuppetVariable,
                        qualified_name: Some(format!("${name}")),
                        location: Self::loc(var, file_path),
                        signature: None,
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "language": "puppet" }),
                        ..Default::default()
                    };
                    record_symbol_spans(&mut sym, node, Some(var));
                    symbols.push(sym);
                }
            }
            "lambda" => {
                // Synthetic anonymous function at line
                let line = node.start_position().row + 1;
                let name = format!("anonymous@L{line}");
                let mut sym = Symbol {
                    name: name.clone(),
                    symbol_type: SymbolType::Function,
                    qualified_name: Some(name),
                    location: Self::loc(node, file_path),
                    signature: Some("|...| { ... }".to_string()),
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: serde_json::json!({
                        "language": "puppet",
                        "is_lambda": true
                    }),
                    ..Default::default()
                };
                record_symbol_spans(&mut sym, node, None);
                sym = rgctl_plugin_helpers::callable_identity::symbol(
                    sym,
                    node,
                    source,
                    rgctl_plugin_helpers::callable_identity::Policy::Puppet,
                );
                symbols.push(sym);
            }
            _ => {}
        }
    }

    fn walk_relations(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        relations: &mut Vec<Relation>,
    ) {
        self.walk_relations_node_only(node, source, file_path, relations);
        let mut c = node.walk();
        for child in node.children(&mut c).collect::<Vec<_>>() {
            self.walk_relations(child, source, file_path, relations);
        }
    }

    pub(crate) fn walk_relations_node_only(
        &self,
        node: Node,
        source: &[u8],
        file_path: &str,
        relations: &mut Vec<Relation>,
    ) {
        let from = Self::enclosing_host_name(node, source)
            .unwrap_or_else(|| Path::new(file_path).display().to_string());

        match node.kind() {
            "include_statement" => {
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if matches!(
                        child.kind(),
                        "identifier" | "class_identifier" | "string" | "variable"
                    ) && let Some(to) = Self::ident_text(child, source)
                    {
                        relations.push(Relation {
                            from: from.clone(),
                            to,
                            relation_type: RelationType::IncludesClass,
                            location: Self::loc(node, file_path),
                            metadata: serde_json::json!({ "language": "puppet" }),
                            to_qualified_hint: None,
                            to_type_hint: Some("puppetclass".to_string()),
                            ..Default::default()
                        });
                    }
                }
            }
            "require_statement" => {
                if let Some(to) = Self::first_child_ident(node, source) {
                    relations.push(Relation {
                        from: from.clone(),
                        to,
                        relation_type: RelationType::RequiresResource,
                        location: Self::loc(node, file_path),
                        metadata: serde_json::json!({
                            "language": "puppet",
                            "kind": "require_statement"
                        }),
                        to_qualified_hint: None,
                        to_type_hint: None,
                        ..Default::default()
                    });
                }
            }
            "class_inherits" => {
                if let Some(to) = Self::first_child_ident(node, source) {
                    // Parent of class_inherits is class_definition
                    let class_from = node
                        .parent()
                        .and_then(|p| Self::first_child_ident(p, source))
                        .unwrap_or(from.clone());
                    relations.push(Relation {
                        from: class_from,
                        to,
                        relation_type: RelationType::InheritsClass,
                        location: Self::loc(node, file_path),
                        metadata: serde_json::json!({ "language": "puppet" }),
                        to_qualified_hint: None,
                        to_type_hint: Some("puppetclass".to_string()),
                        ..Default::default()
                    });
                }
            }
            "relation" => {
                // statement -> statement (resource refs)
                let stmts: Vec<_> = node
                    .children(&mut node.walk())
                    .filter(|c| {
                        c.kind() == "statement"
                            || c.kind() == "resource_reference"
                            || c.kind() == "resource_declaration"
                    })
                    .collect();
                // Children may be resource_reference directly
                let mut refs = Vec::new();
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if child.kind() == "resource_reference" {
                        if let Some(t) = Self::text(child, source) {
                            refs.push(t.replace(' ', ""));
                        }
                    } else if child.kind() == "resource_declaration"
                        && let (Some(ty), Some(title)) = (
                            child
                                .child_by_field_name("type")
                                .and_then(|n| Self::ident_text(n, source)),
                            child
                                .child_by_field_name("title")
                                .and_then(|n| Self::ident_text(n, source)),
                        )
                    {
                        refs.push(format!("{ty}[{title}]"));
                    }
                }
                // Also scan nested resource_reference under statement children
                if refs.len() < 2 {
                    let mut stack = vec![node];
                    refs.clear();
                    while let Some(n) = stack.pop() {
                        if n.kind() == "resource_reference"
                            && let Some(t) = Self::text(n, source)
                        {
                            refs.push(t.replace(' ', ""));
                        }
                        let mut cc = n.walk();
                        for ch in n.children(&mut cc) {
                            stack.push(ch);
                        }
                    }
                }
                if refs.len() >= 2 {
                    for w in refs.windows(2) {
                        relations.push(Relation {
                            from: w[0].clone(),
                            to: w[1].clone(),
                            relation_type: RelationType::RequiresResource,
                            location: Self::loc(node, file_path),
                            metadata: serde_json::json!({
                                "language": "puppet",
                                "kind": "relation"
                            }),
                            to_qualified_hint: None,
                            to_type_hint: Some("puppetresource".to_string()),
                            ..Default::default()
                        });
                    }
                }
                let _ = stmts;
            }
            "function_call" => {
                // First identifier-like child is callee
                let mut callee = None;
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if matches!(child.kind(), "identifier" | "class_identifier") {
                        callee = Self::ident_text(child, source);
                        break;
                    }
                }
                if let Some(to) = callee {
                    let mut meta = serde_json::json!({ "language": "puppet" });
                    if to == "lookup" || to == "hiera" || to == "hiera_hash" {
                        meta["unresolved"] = serde_json::Value::Bool(true);
                    }
                    relations.push(Relation {
                        from: from.clone(),
                        to: to.clone(),
                        relation_type: RelationType::Calls,
                        location: Self::loc(node, file_path),
                        metadata: meta,
                        to_qualified_hint: Some(to.clone()),
                        to_type_hint: Some("function".to_string()),
                        ..Default::default()
                    });
                }
            }
            "variable" => {
                if let Some(raw) = Self::text(node, source)
                    && (raw.starts_with("$facts") || raw.starts_with("$::facts"))
                {
                    let fact = raw.trim_start_matches('$').to_string();
                    relations.push(Relation {
                        from: from.clone(),
                        to: fact.clone(),
                        relation_type: RelationType::UsesFact,
                        location: Self::loc(node, file_path),
                        metadata: serde_json::json!({ "language": "puppet" }),
                        to_qualified_hint: None,
                        to_type_hint: Some("puppetfact".to_string()),
                        ..Default::default()
                    });
                }
            }
            "resource_reference" => {
                if let Some(to) = Self::text(node, source) {
                    relations.push(Relation {
                        from: from.clone(),
                        to: to.replace(' ', ""),
                        relation_type: RelationType::References,
                        location: Self::loc(node, file_path),
                        metadata: serde_json::json!({ "language": "puppet" }),
                        to_qualified_hint: None,
                        to_type_hint: Some("puppetresource".to_string()),
                        ..Default::default()
                    });
                }
            }
            "resource_collector" => {
                // `File <| title == 'x' |>` — relate host to collected resource type.
                if let Some(to) = Self::first_child_ident(node, source) {
                    relations.push(Relation {
                        from: from.clone(),
                        to,
                        relation_type: RelationType::References,
                        location: Self::loc(node, file_path),
                        metadata: serde_json::json!({
                            "language": "puppet",
                            "kind": "resource_collector"
                        }),
                        to_qualified_hint: None,
                        to_type_hint: Some("puppetresource".to_string()),
                        ..Default::default()
                    });
                }
            }
            "resource_default" => {
                if let Some(to) = node
                    .child_by_field_name("type")
                    .and_then(|n| Self::ident_text(n, source))
                    .or_else(|| Self::first_child_ident(node, source))
                {
                    relations.push(Relation {
                        from: from.clone(),
                        to,
                        relation_type: RelationType::References,
                        location: Self::loc(node, file_path),
                        metadata: serde_json::json!({
                            "language": "puppet",
                            "kind": "resource_default"
                        }),
                        to_qualified_hint: None,
                        to_type_hint: Some("puppetresource".to_string()),
                        ..Default::default()
                    });
                }
            }
            "search_expression" => {
                // `title == 'foo'` inside a collector — attribute filter reference.
                if let Some(to) = Self::first_child_ident(node, source) {
                    relations.push(Relation {
                        from: from.clone(),
                        to,
                        relation_type: RelationType::References,
                        location: Self::loc(node, file_path),
                        metadata: serde_json::json!({
                            "language": "puppet",
                            "kind": "search_expression"
                        }),
                        to_qualified_hint: None,
                        to_type_hint: None,
                        ..Default::default()
                    });
                }
            }
            "tag_statement" => {
                let mut c = node.walk();
                for child in node.children(&mut c) {
                    if matches!(
                        child.kind(),
                        "identifier" | "class_identifier" | "string" | "variable"
                    ) && let Some(to) = Self::ident_text(child, source)
                    {
                        relations.push(Relation {
                            from: from.clone(),
                            to,
                            relation_type: RelationType::References,
                            location: Self::loc(node, file_path),
                            metadata: serde_json::json!({
                                "language": "puppet",
                                "kind": "tag_statement"
                            }),
                            to_qualified_hint: None,
                            to_type_hint: None,
                            ..Default::default()
                        });
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn maybe_module_from_metadata(
        &self,
        file_path: &Path,
        symbols: &mut Vec<Symbol>,
        relations: &mut Vec<Relation>,
    ) {
        let mut dir = file_path.parent().map(Path::to_path_buf);
        for _ in 0..4 {
            let Some(d) = dir.clone() else { break };
            let meta = d.join("metadata.json");
            if meta.is_file() {
                if let Ok(bytes) = std::fs::read(&meta)
                    && let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
                {
                    let name = v
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("unknown")
                        .to_string();
                    let loc = SourceLocation {
                        file: meta.display().to_string(),
                        start_line: 1,
                        end_line: 1,
                        start_column: 0,
                        end_column: 0,
                    };
                    symbols.push(Symbol {
                        name: name.clone(),
                        symbol_type: SymbolType::PuppetModule,
                        qualified_name: Some(name.clone()),
                        location: loc.clone(),
                        signature: None,
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: serde_json::json!({ "language": "puppet" }),
                        ..Default::default()
                    });
                    if let Some(deps) = v.get("dependencies").and_then(|d| d.as_array()) {
                        for dep in deps {
                            let dep_name = dep
                                .get("name")
                                .and_then(|n| n.as_str())
                                .unwrap_or("")
                                .to_string();
                            if dep_name.is_empty() {
                                continue;
                            }
                            relations.push(Relation {
                                from: name.clone(),
                                to: dep_name,
                                relation_type: RelationType::DependsOnModule,
                                location: loc.clone(),
                                metadata: serde_json::json!({ "language": "puppet" }),
                                to_qualified_hint: None,
                                to_type_hint: Some("puppetmodule".to_string()),
                                ..Default::default()
                            });
                        }
                    }
                }
                break;
            }
            dir = d.parent().map(Path::to_path_buf);
        }
    }
}

impl LanguagePlugin for PuppetPlugin {
    fn language_id(&self) -> &str {
        "puppet"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec!["pp"]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(tree_sitter_puppet::LANGUAGE.into())
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let tree = self.parse(file_path, source)?;
        let path_str = file_path.to_string_lossy();
        let mut symbols = Vec::new();
        self.walk_symbols(tree.root_node(), source, &path_str, &mut symbols);
        let mut _rels = Vec::new();
        self.maybe_module_from_metadata(file_path, &mut symbols, &mut _rels);
        Ok(symbols)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        let tree = self.parse(file_path, source)?;
        let path_str = file_path.to_string_lossy();
        let mut relations = Vec::new();
        self.walk_relations(tree.root_node(), source, &path_str, &mut relations);
        let mut _syms = Vec::new();
        self.maybe_module_from_metadata(file_path, &mut _syms, &mut relations);
        Ok(relations)
    }

    fn extract_all(&self, file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
        extract_all_single_pass(self, file_path, source)
    }

    fn declared_ast_handlers(&self) -> Option<&AstHandlerTable> {
        Some(puppet_ast_handlers())
    }

    fn calculate_complexity(
        &self,
        symbol: &Symbol,
        source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        if let Some(metrics) = complexity_from_metadata(symbol) {
            return Ok(Some(metrics));
        }
        let file_path = Path::new(&symbol.location.file);
        let tree = self.parse(file_path, source)?;
        let mut stack = vec![tree.root_node()];
        let mut target = None;
        while let Some(n) = stack.pop() {
            let start = n.start_position().row + 1;
            let end = n.end_position().row + 1;
            if start == symbol.location.start_line
                && end == symbol.location.end_line
                && matches!(
                    n.kind(),
                    "class_definition"
                        | "defined_resource_type"
                        | "function_declaration"
                        | "node_definition"
                )
            {
                target = Some(n);
                break;
            }
            let mut c = n.walk();
            for ch in n.children(&mut c) {
                stack.push(ch);
            }
        }
        let Some(node) = target else {
            return Ok(Some(ComplexityMetrics {
                cyclomatic: 1,
                cognitive: 0,
                loc: symbol
                    .location
                    .end_line
                    .saturating_sub(symbol.location.start_line)
                    + 1,
                parameters: symbol.parameters.len(),
                nesting_depth: 0,
                returns: 0,
            }));
        };
        let cyclomatic = ComplexityCalculator::cyclomatic(node, BRANCH_KINDS);
        let cognitive = ComplexityCalculator::cognitive(node, BRANCH_KINDS);
        let nesting_depth = ComplexityCalculator::nesting_depth(node, NESTING_KINDS);
        Ok(Some(ComplexityMetrics {
            cyclomatic,
            cognitive,
            loc: symbol
                .location
                .end_line
                .saturating_sub(symbol.location.start_line)
                + 1,
            parameters: symbol.parameters.len(),
            nesting_depth,
            returns: 0,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin() -> PuppetPlugin {
        PuppetPlugin::new().expect("plugin")
    }

    #[test]
    fn extracts_class_resource_include_inherit() {
        let src = br#"
class profile::nginx inherits profile::base (
  String $package_name = 'nginx',
) {
  include stdlib
  package { 'nginx':
    ensure => installed,
  }
  Package['nginx'] -> Service['nginx']
}
"#;
        let p = plugin();
        let path = Path::new("modules/profile/manifests/nginx.pp");
        let symbols = p.extract_symbols(path, src).expect("symbols");
        assert!(
            symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::PuppetClass && s.name == "profile::nginx"),
            "class missing: {:?}",
            symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::PuppetResource)
        );
        let class = symbols
            .iter()
            .find(|s| s.name == "profile::nginx")
            .expect("class");
        assert!(
            class.fields.iter().any(|f| f.name == "package_name"),
            "expected parameter field"
        );
        assert_eq!(
            class
                .fields
                .iter()
                .find(|f| f.name == "package_name")
                .and_then(|f| f.field_type.as_deref()),
            Some("String")
        );

        let rels = p.extract_relations(path, src, &symbols).expect("rels");
        assert!(
            rels.iter()
                .any(|r| r.relation_type == RelationType::IncludesClass && r.to.contains("stdlib")),
            "include missing: {rels:?}"
        );
        assert!(
            rels.iter()
                .any(|r| r.relation_type == RelationType::InheritsClass && r.to.contains("base")),
            "inherit missing: {rels:?}"
        );
    }

    #[test]
    fn extracts_node_function_typealias() {
        let src = br#"
type Profile::Port = Integer[1, 65535]
function profile::helpers::normalize($value) {
  $value
}
node 'web01' {
  include role::web
}
"#;
        let p = plugin();
        let path = Path::new("manifests/site.pp");
        let symbols = p.extract_symbols(path, src).expect("symbols");
        assert!(
            symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::PuppetNode)
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::Function)
        );
        assert!(
            symbols
                .iter()
                .any(|s| s.symbol_type == SymbolType::TypeAlias)
        );
    }

    #[test]
    fn registry_extensions() {
        let p = plugin();
        assert_eq!(p.language_id(), "puppet");
        assert!(p.file_extensions().contains(&"pp"));
        assert!(p.grammar().is_some());
    }
}
