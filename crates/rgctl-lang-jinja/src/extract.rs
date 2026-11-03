//! Iterative template extraction with explicit lexical scopes.

use crate::expression::{children, literal_string, peel, root_identifier, text};
use crate::{InvocationKind, ParsedTemplate, SyntaxProfile, expression_facts, parse_template};
use rgctl_plugin_api::{
    ExtractAllResult, Parameter, Relation, RelationType, Result, ScopeFrame, ScopeKind, ScopeStack,
    SourceLocation, SourceReference, SourceReferenceContext, SourceReferenceKind, Symbol,
    SymbolType, dispatch_lookup_tracked,
};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const MAX_DEPTH: usize = 2048;

struct Frame {
    variables: HashMap<String, String>,
    imports: HashMap<String, (String, Option<String>)>,
    ordinals: HashMap<&'static str, usize>,
    barrier: bool,
}

impl Frame {
    fn new(barrier: bool) -> Self {
        Self {
            variables: HashMap::new(),
            imports: HashMap::new(),
            ordinals: HashMap::new(),
            barrier,
        }
    }
}

enum Visit<'tree> {
    Node(tree_sitter::Node<'tree>, usize),
    Leave,
    Bind(tree_sitter::Node<'tree>, &'static str),
}

struct Extract<'a> {
    path: &'a Path,
    parsed: &'a ParsedTemplate,
    root: String,
    result: ExtractAllResult,
    scopes: ScopeStack,
    frames: Vec<Frame>,
    evidence: HashMap<String, Vec<Value>>,
    diagnostics: Vec<Value>,
    named_ordinals: HashMap<String, usize>,
}

fn symbol(
    name: String,
    symbol_type: SymbolType,
    qualified_name: String,
    location: SourceLocation,
    kind: &str,
) -> Symbol {
    Symbol {
        name,
        symbol_type,
        qualified_name: Some(qualified_name),
        location,
        signature: None,
        return_type: None,
        parameters: Vec::new(),
        fields: Vec::new(),
        modifiers: Vec::new(),
        documentation: None,
        metadata: json!({"language": "jinja", "kind": kind}),
    }
}

impl<'a> Extract<'a> {
    fn caller(&self) -> &str {
        self.scopes
            .current_function()
            .map_or(self.root.as_str(), |scope| scope.qualified_prefix.as_str())
    }

    fn prefix(&self) -> &str {
        self.scopes
            .current()
            .map_or(self.root.as_str(), |scope| scope.qualified_prefix.as_str())
    }

    fn push(&mut self, kind: ScopeKind, qn: String, node: tree_sitter::Node<'_>, barrier: bool) {
        let depth = self.scopes.depth();
        self.scopes.push(ScopeFrame::new(
            kind,
            None,
            qn,
            node.start_byte(),
            node.end_byte(),
            depth,
        ));
        self.frames.push(Frame::new(barrier));
    }

    fn lookup(&self, name: &str) -> Option<&str> {
        for (index, frame) in self.frames.iter().enumerate().rev() {
            if let Some(qn) = frame.variables.get(name) {
                return Some(qn);
            }
            if frame.barrier && index >= 2 {
                return self
                    .frames
                    .iter()
                    .take(2)
                    .rev()
                    .find_map(|frame| frame.variables.get(name).map(String::as_str));
            }
        }
        None
    }

    fn imported(&self, name: &str) -> Option<&(String, Option<String>)> {
        for (index, frame) in self.frames.iter().enumerate().rev() {
            if let Some(binding) = frame.imports.get(name) {
                return Some(binding);
            }
            if frame.barrier && index >= 2 {
                return self
                    .frames
                    .iter()
                    .take(2)
                    .rev()
                    .find_map(|frame| frame.imports.get(name));
            }
        }
        None
    }

    fn relation(&mut self, from: String, to: String, kind: RelationType, location: SourceLocation) {
        self.result.relations.push(Relation {
            from,
            to: to.clone(),
            relation_type: kind,
            location,
            metadata: json!({"language": "jinja"}),
            to_qualified_hint: Some(to),
            to_type_hint: None,
        });
    }

    fn declaration(
        &mut self,
        node: tree_sitter::Node<'_>,
        name: &str,
        kind: &str,
        ty: SymbolType,
    ) -> String {
        let qn = format!("{}::binding:{name}", self.prefix());
        if self
            .frames
            .last()
            .is_some_and(|frame| frame.variables.contains_key(name))
        {
            return qn;
        }
        let location = self.parsed.location(node, self.path);
        self.result.symbols.push(symbol(
            name.to_owned(),
            ty,
            qn.clone(),
            location.clone(),
            kind,
        ));
        self.relation(
            self.root.clone(),
            qn.clone(),
            RelationType::Defines,
            location,
        );
        if let Some(frame) = self.frames.last_mut() {
            frame.variables.insert(name.to_owned(), qn.clone());
        }
        qn
    }

    fn context(&mut self, name: &str, location: &SourceLocation) -> String {
        if let Some(qn) = self.lookup(name) {
            return qn.to_owned();
        }
        let qn = format!("{}::context:{name}", self.root);
        if let Some(existing) = self
            .frames
            .first()
            .and_then(|frame| frame.variables.get(name))
        {
            return existing.clone();
        }
        self.result.symbols.push(symbol(
            name.to_owned(),
            SymbolType::Variable,
            qn.clone(),
            location.clone(),
            "jinja_context_input",
        ));
        self.relation(
            self.root.clone(),
            qn.clone(),
            RelationType::Defines,
            location.clone(),
        );
        if let Some(frame) = self.frames.first_mut() {
            frame.variables.insert(name.to_owned(), qn.clone());
        }
        qn
    }

    fn reference(
        &mut self,
        kind: SourceReferenceKind,
        target: String,
        node: tree_sitter::Node<'_>,
    ) -> SourceReference {
        SourceReference {
            kind,
            source: self.caller().to_owned(),
            target,
            candidates: Vec::new(),
            dynamic: false,
            location: self.parsed.location(node, self.path),
            context: SourceReferenceContext::default(),
            binding: None,
            member: None,
            properties: BTreeMap::new(),
        }
    }

    fn expressions(&mut self, node: tree_sitter::Node<'_>) {
        let facts = expression_facts(self.parsed, node);
        self.expression_evidence(node, facts);
    }

    fn expression_evidence(&mut self, node: tree_sitter::Node<'_>, facts: crate::ExpressionFacts) {
        let caller = self.caller().to_owned();
        for access in &facts.variables {
            let location = self.parsed.location_for_range(
                access.range.clone(),
                self.path.to_string_lossy().into_owned(),
            );
            let target = self.context(&access.name, &location);
            let kind = if access.name == "ansible_facts" {
                RelationType::UsesFact
            } else {
                RelationType::References
            };
            self.relation(caller.clone(), target, kind, location);
        }
        for invocation in &facts.invocations {
            if invocation.kind != InvocationKind::Call {
                continue;
            }
            if matches!(
                invocation.name.as_str(),
                "range"
                    | "dict"
                    | "namespace"
                    | "cycler"
                    | "joiner"
                    | "lipsum"
                    | "super"
                    | "caller"
            ) {
                continue;
            }
            let mut reference = self.reference(
                SourceReferenceKind::MacroCall,
                self.path.to_string_lossy().into_owned(),
                node,
            );
            reference
                .properties
                .insert("lexical_scope".into(), self.prefix().to_owned());
            reference.location = self.parsed.location_for_range(
                invocation.range.clone(),
                self.path.to_string_lossy().into_owned(),
            );
            reference
                .properties
                .insert("invocation".into(), invocation.name.clone());
            let (binding, member) = invocation
                .name
                .split_once('.')
                .map_or((invocation.name.as_str(), None), |(binding, member)| {
                    (binding, Some(member))
                });
            if let Some((template, imported_member)) = self.imported(binding) {
                reference.target = template.clone();
                reference.binding = Some(binding.to_owned());
                reference.member = member
                    .map(str::to_owned)
                    .or_else(|| imported_member.clone());
            } else {
                reference.member = Some(invocation.name.clone());
                reference
                    .properties
                    .insert("local_template".into(), "true".into());
            }
            self.result.source_references.push(reference);
        }
        self.evidence.entry(caller).or_default().push(json!(facts));
    }

    fn local_name(&mut self, kind: &'static str) -> String {
        let prefix = self.prefix().to_owned();
        let index = if let Some(frame) = self.frames.last_mut() {
            let count = frame.ordinals.entry(kind).or_default();
            let index = *count;
            *count += 1;
            index
        } else {
            0
        };
        format!("{prefix}::{kind}:{index}")
    }

    fn bind_target(&mut self, node: tree_sitter::Node<'_>, kind: &'static str) {
        let mut pending = vec![node];
        while let Some(target) = pending.pop() {
            let target = peel(target);
            if let Some(root) = root_identifier(target) {
                if target.child_by_field_name("object").is_some() {
                    self.expressions(target);
                    if let Some(qn) = self.lookup(text(self.parsed, root)).map(str::to_owned) {
                        self.relation(
                            self.caller().to_owned(),
                            qn,
                            RelationType::Modifies,
                            self.parsed.location(target, self.path),
                        );
                    }
                } else {
                    self.declaration(root, text(self.parsed, root), kind, SymbolType::Variable);
                }
            } else {
                pending.extend(children(target));
            }
        }
    }

    fn template_reference(&mut self, node: tree_sitter::Node<'_>, kind: SourceReferenceKind) {
        let target = children(node).into_iter().find(|child| {
            matches!(
                child.kind(),
                "string_literal" | "expression" | "identifier" | "list_literal" | "tuple_literal"
            )
        });
        let Some(target) = target else { return };
        let mut reference = self.reference(kind, text(self.parsed, target).to_owned(), node);
        reference.source = self.root.clone();
        if let Some(literal) = literal_string(self.parsed, target) {
            reference.target = literal.clone();
            reference.candidates.push(literal);
        } else if matches!(target.kind(), "list_literal" | "tuple_literal") {
            for child in children(target) {
                if let Some(literal) = literal_string(self.parsed, child) {
                    reference.candidates.push(literal);
                } else {
                    reference.dynamic = true;
                    self.expressions(child);
                }
            }
        } else {
            reference.dynamic = true;
            self.expressions(target);
        }
        for child in children(node) {
            if child.kind() == "ternary_expression" {
                reference.dynamic = true;
                self.expressions(child);
            }
            if matches!(child.kind(), "include_attribute" | "import_attribute") {
                reference
                    .properties
                    .insert(child.kind().to_owned(), text(self.parsed, child).to_owned());
            }
        }
        self.result.source_references.push(reference);
    }

    fn import(&mut self, node: tree_sitter::Node<'_>) {
        let from = children(node)
            .into_iter()
            .find(|child| child.kind() == "import_from");
        let literal = from.and_then(|from| from.named_child(0)).or_else(|| {
            children(node)
                .into_iter()
                .find(|child| child.kind() == "string_literal")
        });
        let Some(literal) = literal else { return };
        let Some(template) = literal_string(self.parsed, literal) else {
            return;
        };
        let mut bindings = Vec::new();
        if from.is_some() {
            for entry in children(node)
                .into_iter()
                .filter(|child| child.kind() == "import_binding")
            {
                let Some(name) = entry.child_by_field_name("name") else {
                    continue;
                };
                let alias = entry.child_by_field_name("alias").unwrap_or(name);
                bindings.push((alias, Some(text(self.parsed, name).to_owned())));
            }
        } else if let Some(alias) = children(node)
            .into_iter()
            .find(|child| child.kind() == "import_as")
            .and_then(|alias| alias.named_child(0))
        {
            bindings.push((alias, None));
        }
        for (alias_node, member) in bindings {
            let alias = text(self.parsed, alias_node).to_owned();
            self.declaration(alias_node, &alias, "jinja_import", SymbolType::Import);
            if let Some(frame) = self.frames.last_mut() {
                frame
                    .imports
                    .insert(alias.clone(), (template.clone(), member.clone()));
            }
            let mut reference =
                self.reference(SourceReferenceKind::JinjaImport, template.clone(), node);
            reference.source = self.root.clone();
            reference.candidates.push(template.clone());
            reference.binding = Some(alias);
            reference.member = member;
            for child in children(node)
                .into_iter()
                .filter(|child| child.kind() == "import_attribute")
            {
                reference
                    .properties
                    .insert("context".into(), text(self.parsed, child).to_owned());
            }
            self.result.source_references.push(reference);
        }
    }

    fn walk(&mut self) {
        let mut pending = vec![Visit::Node(self.parsed.tree.root_node(), 0)];
        while let Some(visit) = pending.pop() {
            match visit {
                Visit::Leave => {
                    self.scopes.pop();
                    self.frames.pop();
                    continue;
                }
                Visit::Bind(node, kind) => {
                    self.bind_target(node, kind);
                    continue;
                }
                Visit::Node(node, depth) => {
                    let _mask =
                        dispatch_lookup_tracked(crate::coverage::dispatch(), node.kind_id());
                    if node.is_error() || node.is_missing() || depth > MAX_DEPTH {
                        self.diagnostics.push(json!({
                            "reason": if depth > MAX_DEPTH { "ast_depth_limit" } else { "unsupported_or_malformed_syntax" },
                            "location": self.parsed.location(node, self.path),
                        }));
                        continue;
                    }
                    match node.kind() {
                        "comment" | "content" | "raw_block" | "raw_body" => continue,
                        "macro_block" | "block_block" => {
                            let Some(header) = node.named_child(0) else {
                                continue;
                            };
                            if header.has_error() {
                                self.diagnostics.push(json!({"reason":"malformed_declaration", "location":self.parsed.location(header, self.path)}));
                                continue;
                            }
                            let macro_definition = node.kind() == "macro_block";
                            let signature = if macro_definition {
                                header.named_child(0).unwrap_or(header)
                            } else {
                                header
                            };
                            let Some(name_node) = signature.named_child(0) else {
                                continue;
                            };
                            let name = text(self.parsed, name_node).to_owned();
                            let kind = if macro_definition {
                                "jinja_macro"
                            } else {
                                "jinja_block"
                            };
                            let parent = if macro_definition && self.frames.len() > 2 {
                                self.prefix()
                            } else {
                                self.root.as_str()
                            };
                            let base = format!("{parent}::{kind}:{name}");
                            let ordinal = self.named_ordinals.entry(base.clone()).or_default();
                            let qn = if *ordinal == 0 {
                                base
                            } else {
                                format!("{base}:duplicate:{ordinal}")
                            };
                            if *ordinal > 0 {
                                self.diagnostics.push(json!({"reason":"duplicate_named_definition","name":name,"location":self.parsed.location(node,self.path)}));
                            }
                            *ordinal += 1;
                            let location = self.parsed.location(node, self.path);
                            let mut definition = symbol(
                                name.clone(),
                                SymbolType::Function,
                                qn.clone(),
                                location.clone(),
                                kind,
                            );
                            definition.metadata["template_owner"] = json!(self.root);
                            definition.metadata["lexical_scope"] =
                                json!(if self.frames.len() == 2 {
                                    self.root.as_str()
                                } else {
                                    self.prefix()
                                });
                            definition.metadata["exported"] = json!(
                                macro_definition
                                    && self.frames.len() == 2
                                    && !name.starts_with('_')
                            );
                            if !macro_definition {
                                definition.metadata["scoped"] = json!(
                                    text(self.parsed, header)
                                        .split_whitespace()
                                        .any(|word| word == "scoped")
                                );
                                definition.metadata["required"] = json!(
                                    text(self.parsed, header)
                                        .split_whitespace()
                                        .any(|word| word == "required")
                                );
                            }
                            let args: Vec<_> = children(signature)
                                .into_iter()
                                .filter(|child| child.kind() == "arg")
                                .collect();
                            for arg in &args {
                                if let Some(parameter) =
                                    root_identifier(arg.named_child(0).unwrap_or(*arg))
                                {
                                    definition.parameters.push(Parameter {
                                        name: text(self.parsed, parameter).to_owned(),
                                        param_type: None,
                                        default_value: (arg.named_child_count() > 1)
                                            .then(|| arg.named_child(arg.named_child_count() - 1))
                                            .flatten()
                                            .map(|value| text(self.parsed, value).to_owned()),
                                    });
                                }
                            }
                            self.result.symbols.push(definition);
                            self.relation(
                                self.root.clone(),
                                qn.clone(),
                                RelationType::Defines,
                                location,
                            );
                            if macro_definition && let Some(frame) = self.frames.last_mut() {
                                frame.variables.insert(name, qn.clone());
                            }
                            self.push(
                                ScopeKind::Function,
                                qn,
                                node,
                                !macro_definition
                                    && !text(self.parsed, header)
                                        .split_whitespace()
                                        .any(|word| word == "scoped"),
                            );
                            for arg in args {
                                if let Some(parameter) =
                                    root_identifier(arg.named_child(0).unwrap_or(arg))
                                {
                                    self.declaration(
                                        parameter,
                                        text(self.parsed, parameter),
                                        "jinja_parameter",
                                        SymbolType::Variable,
                                    );
                                }
                                if arg.named_child_count() > 1
                                    && let Some(default) =
                                        arg.named_child(arg.named_child_count() - 1)
                                {
                                    self.expressions(default);
                                }
                            }
                            pending.push(Visit::Leave);
                            for child in children(node).into_iter().rev().filter(|child| {
                                child.id() != header.id() && !child.kind().starts_with("end")
                            }) {
                                pending.push(Visit::Node(child, depth + 1));
                            }
                            continue;
                        }
                        "for_block" => {
                            let Some(header) = node.named_child(0) else {
                                continue;
                            };
                            let Some(binding) = header.named_child(0) else {
                                continue;
                            };
                            let inputs = children(binding);
                            let mut after_in = false;
                            for child in &inputs {
                                if child.kind() == "binary_operator" {
                                    after_in = true;
                                } else if after_in {
                                    self.expressions(*child);
                                }
                            }
                            let qn = self.local_name("for");
                            self.push(ScopeKind::GuardBlock, qn, node, false);
                            for child in inputs
                                .into_iter()
                                .take_while(|child| child.kind() != "binary_operator")
                            {
                                self.bind_target(child, "jinja_loop_binding");
                            }
                            self.declaration(
                                header,
                                "loop",
                                "jinja_loop_context",
                                SymbolType::Variable,
                            );
                            for filter in children(header).into_iter().skip(1) {
                                self.expressions(filter);
                            }
                            if let Some(otherwise) = children(node)
                                .into_iter()
                                .find(|child| child.kind() == "else_block")
                            {
                                pending.push(Visit::Node(otherwise, depth + 1));
                            }
                            pending.push(Visit::Leave);
                            for child in children(node).into_iter().rev().filter(|child| {
                                child.id() != header.id()
                                    && child.kind() != "else_block"
                                    && !child.kind().starts_with("end")
                            }) {
                                pending.push(Visit::Node(child, depth + 1));
                            }
                            continue;
                        }
                        "with_block" => {
                            let Some(header) = node.named_child(0) else {
                                continue;
                            };
                            let assignments: Vec<_> = children(header)
                                .into_iter()
                                .filter(|child| child.kind() == "assignment_expression")
                                .collect();
                            for assignment in &assignments {
                                if let Some(value) = assignment
                                    .named_child(assignment.named_child_count().saturating_sub(1))
                                {
                                    self.expressions(value);
                                }
                            }
                            let qn = self.local_name("with");
                            self.push(ScopeKind::GuardBlock, qn, node, false);
                            for assignment in assignments {
                                if let Some(name) = assignment.named_child(0) {
                                    self.bind_target(name, "jinja_with_binding");
                                }
                            }
                            pending.push(Visit::Leave);
                            for child in children(node).into_iter().rev().filter(|child| {
                                child.id() != header.id() && !child.kind().starts_with("end")
                            }) {
                                pending.push(Visit::Node(child, depth + 1));
                            }
                            continue;
                        }
                        "call_block" => {
                            let Some(header) = node.named_child(0) else {
                                continue;
                            };
                            let parts = children(header);
                            if let Some(call) = header.child_by_field_name("call") {
                                self.expressions(call);
                            }
                            let qn = self.local_name("call");
                            let mut callable = symbol(
                                "<caller>".into(),
                                SymbolType::Function,
                                qn.clone(),
                                self.parsed.location(node, self.path),
                                "jinja_call_body",
                            );
                            callable.metadata["synthetic"] = json!(true);
                            self.result.symbols.push(callable);
                            self.push(ScopeKind::Lambda, qn, node, false);
                            for parameter in parts
                                .into_iter()
                                .filter(|child| child.kind() == "identifier")
                            {
                                self.declaration(
                                    parameter,
                                    text(self.parsed, parameter),
                                    "jinja_parameter",
                                    SymbolType::Variable,
                                );
                            }
                            pending.push(Visit::Leave);
                            for child in children(node).into_iter().rev().filter(|child| {
                                child.id() != header.id() && !child.kind().starts_with("end")
                            }) {
                                pending.push(Visit::Node(child, depth + 1));
                            }
                            continue;
                        }
                        "set_statement" => {
                            let named = children(node);
                            let split = named
                                .iter()
                                .position(|child| child.kind() == "binary_operator");
                            if let Some(split) = split {
                                for value in &named[split + 1..] {
                                    self.expressions(*value);
                                }
                                for target in &named[..split] {
                                    self.bind_target(*target, "jinja_assignment");
                                }
                            }
                            continue;
                        }
                        "set_block" => {
                            let Some(header) = node.named_child(0) else {
                                continue;
                            };
                            if let Some(name) = header.named_child(0) {
                                pending.push(Visit::Bind(name, "jinja_assignment"));
                            }
                            for child in children(node).into_iter().rev().filter(|child| {
                                child.id() != header.id() && !child.kind().starts_with("end")
                            }) {
                                pending.push(Visit::Node(child, depth + 1));
                            }
                            continue;
                        }
                        "import_statement" => {
                            self.import(node);
                            continue;
                        }
                        "include_statement" => {
                            self.template_reference(node, SourceReferenceKind::JinjaInclude);
                            continue;
                        }
                        "extends_statement" => {
                            self.template_reference(node, SourceReferenceKind::JinjaExtends);
                            continue;
                        }
                        "render_expression"
                        | "if_statement"
                        | "elif_statement"
                        | "do_statement"
                        | "autoescape_statement" => {
                            self.expressions(node);
                            continue;
                        }
                        "filter_statement" => {
                            // A block filter has an implicit input, so analyze its
                            // explicit arguments without declaring the filter name.
                            if let Some(filter) = node.named_child(0) {
                                let facts =
                                    crate::expression::block_filter_facts(self.parsed, filter);
                                self.expression_evidence(filter, facts);
                            }
                            continue;
                        }
                        _ => {}
                    }
                    for child in children(node).into_iter().rev() {
                        pending.push(Visit::Node(child, depth + 1));
                    }
                }
            }
        }
    }
}

pub(crate) fn extract(
    path: &Path,
    source: &[u8],
    syntax: &SyntaxProfile,
) -> Result<ExtractAllResult> {
    let root_name = format!(
        "jinja:{}::@template",
        path.to_string_lossy().replace('\\', "/")
    );
    let vaulted = source.starts_with(b"$ANSIBLE_VAULT;");
    let parsed = if vaulted {
        Err("encrypted Ansible Vault template cannot be statically decrypted".to_owned())
    } else {
        parse_template(path, source, syntax).map_err(|error| error.to_string())
    };
    let parsed = match parsed {
        Ok(parsed) => parsed,
        Err(error) => {
            let mut root = symbol(
                path.file_name()
                    .map_or("template", |name| name.to_str().unwrap_or("template"))
                    .to_owned(),
                SymbolType::Module,
                root_name,
                SourceLocation {
                    file: path.to_string_lossy().into_owned(),
                    start_line: 1,
                    end_line: 1,
                    start_column: 0,
                    end_column: 0,
                },
                "jinja_template",
            );
            root.metadata["parse_status"] = json!("error");
            root.metadata["source_diagnostics_json"] = json!(
                json!([{"reason":if vaulted {"encrypted_vault_content"} else {"invalid_syntax_profile"},"message":error}])
                    .to_string()
            );
            return Ok(ExtractAllResult::from_parts(vec![root], Vec::new()));
        }
    };
    extract_parsed_template(path, &path.to_string_lossy().replace('\\', "/"), &parsed)
}

/// Extract a parsed fragment without parsing it again.
///
/// The identity is supplied by the containing source language and must be
/// stable under whitespace/comment insertion. Locations address the original
/// fragment bytes; a containing language maps those through its scalar map.
///
/// # Errors
/// Returns an error when serializing source evidence fails.
pub fn extract_parsed_template(
    path: &Path,
    identity: &str,
    parsed: &ParsedTemplate,
) -> Result<ExtractAllResult> {
    let root_name = format!("jinja:{identity}::@template");
    let node = parsed.tree.root_node();
    let location = parsed.location(node, path);
    let root = symbol(
        path.file_name()
            .map_or("template", |name| name.to_str().unwrap_or("template"))
            .to_owned(),
        SymbolType::Module,
        root_name.clone(),
        location.clone(),
        "jinja_template",
    );
    let render_name = format!("{root_name}::@render");
    let mut render = symbol(
        "<render>".into(),
        SymbolType::Function,
        render_name.clone(),
        location,
        "jinja_render",
    );
    render.metadata["synthetic"] = json!(true);
    let mut extract = Extract {
        path,
        parsed,
        root: root_name.clone(),
        result: ExtractAllResult::from_parts(vec![root, render], Vec::new()),
        scopes: ScopeStack::new(),
        frames: Vec::new(),
        evidence: HashMap::new(),
        diagnostics: Vec::new(),
        named_ordinals: HashMap::new(),
    };
    extract.push(ScopeKind::Module, root_name, node, false);
    extract.push(ScopeKind::Function, render_name, node, false);
    extract.relation(
        extract.root.clone(),
        extract.caller().to_owned(),
        RelationType::Defines,
        parsed.location(node, path),
    );
    extract.walk();
    let mut refs_by_source: HashMap<&str, Vec<&SourceReference>> = HashMap::new();
    for reference in &extract.result.source_references {
        refs_by_source
            .entry(&reference.source)
            .or_default()
            .push(reference);
    }
    for symbol in &mut extract.result.symbols {
        if let Some(qn) = symbol.qualified_name.as_deref() {
            if let Some(references) = refs_by_source.get(qn) {
                symbol.metadata["source_references_json"] =
                    json!(serde_json::to_string(references)?);
            }
            if let Some(evidence) = extract.evidence.get(qn) {
                symbol.metadata["expression_facts_json"] = json!(serde_json::to_string(evidence)?);
            }
        }
    }
    if let Some(root) = extract.result.symbols.first_mut() {
        root.metadata["parse_status"] = json!(if parsed.tree.root_node().has_error()
            || !extract.diagnostics.is_empty()
        {
            "partial"
        } else {
            "complete"
        });
        root.metadata["syntax_profile_json"] = json!(serde_json::to_string(&parsed.syntax)?);
        if !extract.diagnostics.is_empty() {
            root.metadata["source_diagnostics_json"] =
                json!(serde_json::to_string(&extract.diagnostics)?);
        }
    }
    Ok(extract.result)
}
