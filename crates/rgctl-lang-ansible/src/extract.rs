//! Static Ansible structure and embedded Jinja, without runtime evaluation.

use crate::yaml::{Scalar, Yaml, named, sequence, value};
use rgctl_lang_jinja::{
    ExpressionFacts, extract_parsed_template, parse_expression, parse_fragment,
};
use rgctl_plugin_api::{
    ExtractAllResult, Relation, RelationType, Result, SourceLocation, SourceReference,
    SourceReferenceContext, SourceReferenceKind, Symbol, SymbolType,
};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use tree_sitter::Node;

type Pairs<'source, 'tree> = Vec<(Cow<'source, str>, Node<'tree>)>;
const MAX_DEPTH: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Playbook,
    Tasks,
    Handlers,
    Meta,
}

/// Role root and role-relative source path, determined without filesystem reads.
pub(crate) fn role_path(path: &Path) -> Option<(String, String)> {
    let path = path.to_string_lossy().replace('\\', "/");
    let components: Vec<_> = path.split('/').collect();
    let index = components.iter().rposition(|part| *part == "roles")?;
    if components.len() <= index + 2 {
        return None;
    }
    Some((
        components[..index + 2].join("/"),
        components[index + 2..].join("/"),
    ))
}

fn path_kind(path: &Path) -> Option<Kind> {
    let (_, relative) = role_path(path)?;
    if relative.starts_with("tasks/") {
        Some(Kind::Tasks)
    } else if relative.starts_with("handlers/") {
        Some(Kind::Handlers)
    } else if matches!(
        relative.as_str(),
        "meta/main.yml" | "meta/main.yaml" | "meta/main"
    ) {
        Some(Kind::Meta)
    } else {
        None
    }
}

pub(crate) fn matches_path(path: &Path) -> bool {
    if !path
        .components()
        .any(|component| component.as_os_str() == "roles")
    {
        return false;
    }
    path_kind(path).is_some()
        && match path.extension().and_then(|extension| extension.to_str()) {
            Some("yml" | "yaml") => true,
            None => path.file_name().is_some_and(|name| name == "main"),
            _ => false,
        }
}

fn get<'tree>(pairs: &Pairs<'_, 'tree>, key: &str) -> Option<Node<'tree>> {
    pairs
        .iter()
        .find(|(name, _)| name == key)
        .map(|(_, node)| *node)
}

fn name_value(yaml: &Yaml<'_>, node: Option<Node<'_>>) -> Option<String> {
    node.and_then(|node| yaml.scalar(node).ok())
        .map(|scalar| scalar.value)
}

fn builtin(module: &str) -> &str {
    module.strip_prefix("ansible.builtin.").unwrap_or(module)
}

fn reserved(key: &str) -> bool {
    key.starts_with("with_")
        || key.starts_with("become")
        || matches!(
            key,
            "name"
                | "args"
                | "action"
                | "local_action"
                | "when"
                | "failed_when"
                | "changed_when"
                | "until"
                | "vars"
                | "register"
                | "loop"
                | "loop_control"
                | "notify"
                | "listen"
                | "tags"
                | "block"
                | "rescue"
                | "always"
                | "environment"
                | "delegate_to"
                | "delegate_facts"
                | "ignore_errors"
                | "ignore_unreachable"
                | "run_once"
                | "retries"
                | "delay"
                | "check_mode"
                | "diff"
                | "async"
                | "poll"
                | "connection"
                | "collections"
                | "any_errors_fatal"
                | "debugger"
                | "throttle"
                | "timeout"
                | "no_log"
                | "module_defaults"
        )
}

fn symbol(
    name: String,
    ty: SymbolType,
    qn: String,
    location: SourceLocation,
    kind: &str,
) -> Symbol {
    Symbol {
        name,
        symbol_type: ty,
        qualified_name: Some(qn),
        location,
        signature: None,
        return_type: None,
        parameters: Vec::new(),
        fields: Vec::new(),
        modifiers: Vec::new(),
        documentation: None,
        metadata: json!({"language":"ansible", "kind":kind}),
    }
}

struct Frame {
    qn: String,
    global: bool,
    variables: HashMap<String, Vec<String>>,
}

enum Visit<'tree> {
    Task {
        node: Node<'tree>,
        parent: String,
        handler: bool,
        section: &'static str,
        index: usize,
        depth: usize,
    },
    Leave,
}

struct Extract<'a, 'source> {
    yaml: &'a Yaml<'source>,
    path: &'a Path,
    file: String,
    root: String,
    role: Option<String>,
    playbook: Option<String>,
    result: ExtractAllResult,
    frames: Vec<Frame>,
    counts: HashMap<(String, String), usize>,
    diagnostics: Vec<Value>,
    expressions: HashMap<String, Vec<Value>>,
    inputs: HashMap<String, HashMap<String, String>>,
}

impl<'a, 'source> Extract<'a, 'source> {
    fn location(&self, node: Node<'_>) -> SourceLocation {
        self.yaml.node_location(node, &self.file)
    }

    fn diagnostic(&mut self, node: Node<'_>, reason: &str) {
        self.diagnostics
            .push(json!({"reason":reason,"location":self.location(node)}));
    }

    fn relation(&mut self, from: String, to: String, kind: RelationType, location: SourceLocation) {
        self.result.relations.push(Relation {
            from,
            to: to.clone(),
            relation_type: kind,
            location,
            metadata: json!({"language":"ansible"}),
            to_qualified_hint: Some(to),
            to_type_hint: None,
        });
    }

    fn unique(&mut self, parent: &str, category: &str, name: &str, index: usize) -> String {
        let key = (parent.to_owned(), format!("{category}:{name}"));
        let count = self.counts.entry(key).or_default();
        let id = if name.is_empty() {
            format!("{parent}::{category}:anonymous:{index}")
        } else {
            format!("{parent}::{category}:{name}:{}", *count)
        };
        *count += 1;
        id
    }

    fn reference(
        &mut self,
        source: &str,
        kind: SourceReferenceKind,
        scalar: &Scalar,
        node: Node<'_>,
    ) -> SourceReference {
        let dynamic =
            !scalar.literal && (scalar.value.contains("{{") || scalar.value.contains("{%"));
        SourceReference {
            kind,
            source: source.to_owned(),
            target: scalar.value.clone(),
            candidates: if dynamic {
                Vec::new()
            } else {
                vec![scalar.value.clone()]
            },
            dynamic,
            location: self.location(node),
            context: SourceReferenceContext {
                role_root: self.role.clone(),
                playbook: self.playbook.clone(),
                template_roots: Vec::new(),
            },
            binding: None,
            member: None,
            properties: BTreeMap::new(),
        }
    }

    fn add_reference(&mut self, source: &str, kind: SourceReferenceKind, node: Node<'_>) {
        match self.yaml.scalar(node) {
            Ok(scalar) => {
                let reference = self.reference(source, kind, &scalar, node);
                self.result.source_references.push(reference);
            }
            Err(_) => self.diagnostic(node, "unresolved_scalar_reference"),
        }
    }

    fn declare(
        &mut self,
        owner: &str,
        name: &str,
        kind: &str,
        node: Node<'_>,
        global: bool,
    ) -> String {
        let qn = format!("{owner}::{kind}:{name}");
        if name.is_empty()
            || !name
                .as_bytes()
                .first()
                .is_some_and(|byte| byte.is_ascii_alphabetic() || *byte == b'_')
            || name
                .bytes()
                .any(|byte| !byte.is_ascii_alphanumeric() && byte != b'_')
        {
            self.diagnostic(node, "dynamic_or_invalid_variable_name");
            return qn;
        }
        let mut declaration = symbol(
            name.to_owned(),
            SymbolType::AnsibleVariable,
            qn.clone(),
            self.location(node),
            kind,
        );
        declaration.metadata["declaring_scope"] = json!(if global {
            self.frames
                .iter()
                .rev()
                .find(|frame| frame.global)
                .map_or(self.root.as_str(), |frame| frame.qn.as_str())
        } else {
            owner
        });
        declaration.metadata["binding_mode"] = json!("source_declaration_candidate");
        if global {
            declaration.metadata["runtime_value_unresolved"] = json!(true);
            declaration.metadata["binding_mode"] = json!("source_declaration_candidate");
        }
        self.result.symbols.push(declaration);
        self.relation(
            owner.to_owned(),
            qn.clone(),
            RelationType::Defines,
            self.location(node),
        );
        let frame = if global {
            self.frames.iter_mut().rev().find(|frame| frame.global)
        } else {
            self.frames.last_mut()
        };
        if let Some(frame) = frame {
            frame
                .variables
                .entry(name.to_owned())
                .or_default()
                .push(qn.clone());
        }
        qn
    }

    fn input(&mut self, owner: &str, name: &str, location: SourceLocation) -> String {
        for frame in self.frames.iter().rev() {
            if let Some(candidates) = frame.variables.get(name)
                && candidates.len() == 1
            {
                return candidates[0].clone();
            }
            if frame.variables.contains_key(name) {
                break;
            }
        }
        if let Some(target) = self.inputs.get(owner).and_then(|inputs| inputs.get(name)) {
            return target.clone();
        }
        let qn = format!("{owner}::context:{name}");
        {
            let mut input = symbol(
                name.to_owned(),
                SymbolType::AnsibleVariable,
                qn.clone(),
                location,
                "ansible_context_input",
            );
            input.metadata["runtime_value_unresolved"] = json!(true);
            if matches!(
                name,
                "hostvars"
                    | "groups"
                    | "group_names"
                    | "inventory_hostname"
                    | "inventory_hostname_short"
                    | "ansible_facts"
                    | "ansible_check_mode"
            ) {
                input.metadata["ansible_magic"] = json!(true);
            }
            let candidates: Vec<_> = self
                .frames
                .iter()
                .rev()
                .filter_map(|frame| frame.variables.get(name))
                .flatten()
                .collect();
            if !candidates.is_empty() {
                input.metadata["candidate_definitions_json"] =
                    json!(serde_json::to_string(&candidates).unwrap_or_default());
            }
            self.result.symbols.push(input);
        }
        self.inputs
            .entry(owner.to_owned())
            .or_default()
            .insert(name.to_owned(), qn.clone());
        qn
    }

    fn inline_vars(&mut self, owner: &str, node: Option<Node<'_>>) {
        let Some(node) = node else { return };
        let pairs = self.yaml.pairs(node);
        for (key, value) in &pairs {
            self.declare(owner, key, "ansible_variable", *value, false);
        }
        for (key, value) in pairs {
            self.scan_values(owner, value, format!("vars.{key}"), false);
        }
    }

    fn embed(&mut self, owner: &str, node: Node<'_>, field: &str, bare: bool) {
        let scalar_node = value(node);
        if bare
            && scalar_node.kind() == "plain_scalar"
            && named(scalar_node).any(|child| {
                matches!(
                    child.kind(),
                    "boolean_scalar" | "integer_scalar" | "float_scalar" | "null_scalar"
                )
            })
        {
            return;
        }
        let scalar = match self.yaml.scalar(node) {
            Ok(scalar) => scalar,
            Err(_) => {
                self.diagnostic(node, "unresolved_embedded_scalar");
                return;
            }
        };
        // Legacy Ansible conditions may still be explicitly templated. Parse
        // those as templates rather than nesting a second expression wrapper.
        let bare = bare
            && !["{{", "{%"]
                .iter()
                .any(|marker| scalar.value.trim_start().starts_with(marker));
        if !bare
            && !["{{", "{%", "{#"]
                .iter()
                .any(|marker| scalar.value.contains(marker))
        {
            return;
        }
        let parsed = if bare {
            parse_expression(self.path, scalar.value.as_bytes())
        } else {
            parse_fragment(self.path, scalar.value.as_bytes())
        };
        let parsed = match parsed {
            Ok(parsed) => parsed,
            Err(_) => {
                self.diagnostic(node, "invalid_embedded_jinja");
                return;
            }
        };
        let identity = format!("{owner}::field:{field}");
        let fragment = match extract_parsed_template(self.path, &identity, &parsed) {
            Ok(fragment) => fragment,
            Err(_) => {
                self.diagnostic(node, "invalid_embedded_jinja");
                return;
            }
        };
        let mut redirects = HashMap::new();
        let preserve_root = fragment
            .source_references
            .iter()
            .any(|reference| reference.kind == SourceReferenceKind::JinjaExtends);
        for symbol in &fragment.symbols {
            let Some(qn) = symbol.qualified_name.as_ref() else {
                continue;
            };
            match symbol.metadata["kind"].as_str() {
                Some("jinja_template") if preserve_root => {
                    self.relation(
                        owner.to_owned(),
                        qn.clone(),
                        RelationType::Defines,
                        self.location(node),
                    );
                }
                Some("jinja_template" | "jinja_render") => {
                    redirects.insert(qn.clone(), owner.to_owned());
                }
                Some("jinja_context_input") => {
                    let location = scalar.location(self.yaml, &symbol.location);
                    let target = self.input(owner, &symbol.name, location);
                    redirects.insert(qn.clone(), target);
                }
                _ => {}
            }
        }
        for mut symbol in fragment.symbols {
            if let Some(raw) = symbol.metadata["expression_facts_json"].as_str()
                && let Ok(mut groups) = serde_json::from_str::<Vec<ExpressionFacts>>(raw)
            {
                for facts in &mut groups {
                    for access in &mut facts.variables {
                        access.range = scalar.range(access.range.clone());
                    }
                    for invocation in &mut facts.invocations {
                        invocation.range = scalar.range(invocation.range.clone());
                        if matches!(
                            invocation.plugin.as_deref(),
                            Some("template" | "ansible.builtin.template")
                        ) && let Some(argument) = invocation.arguments.get(1)
                        {
                            let literal =
                                invocation.literal_arguments.get(1).and_then(Option::as_ref);
                            let target = literal.cloned().unwrap_or_else(|| argument.clone());
                            self.result.source_references.push(SourceReference {
                                kind: SourceReferenceKind::TemplateRender,
                                source: owner.to_owned(),
                                target: target.clone(),
                                candidates: literal.map(|_| vec![target]).unwrap_or_default(),
                                dynamic: literal.is_none(),
                                location: self
                                    .yaml
                                    .location(invocation.range.clone(), self.file.clone()),
                                context: SourceReferenceContext {
                                    role_root: self.role.clone(),
                                    playbook: self.playbook.clone(),
                                    template_roots: Vec::new(),
                                },
                                binding: None,
                                member: None,
                                properties: BTreeMap::from([(
                                    "source_form".into(),
                                    "template_lookup".into(),
                                )]),
                            });
                        }
                    }
                }
                self.expressions
                    .entry(owner.to_owned())
                    .or_default()
                    .push(json!({"field":field,"facts":groups}));
            }
            if symbol.metadata["parse_status"] == "partial"
                || symbol.metadata["parse_status"] == "error"
            {
                self.diagnostic(node, "partial_embedded_jinja");
            }
            if symbol
                .qualified_name
                .as_ref()
                .is_some_and(|qn| redirects.contains_key(qn))
            {
                continue;
            }
            symbol.location = scalar.location(self.yaml, &symbol.location);
            symbol.metadata["language"] = json!("ansible");
            symbol.metadata["embedded_language"] = json!("jinja");
            symbol.metadata["embedded_in"] = json!(owner);
            symbol.metadata["exported"] = json!(false);
            if let Some(metadata) = symbol.metadata.as_object_mut() {
                metadata.remove("source_references_json");
                metadata.remove("expression_facts_json");
            }
            self.result.symbols.push(symbol);
        }
        for mut relation in fragment.relations {
            if relation.relation_type == RelationType::Defines
                && redirects.contains_key(&relation.to)
            {
                continue;
            }
            if let Some(redirect) = redirects.get(&relation.from) {
                relation.from = redirect.clone();
            }
            if let Some(redirect) = redirects.get(&relation.to) {
                relation.to = redirect.clone();
                relation.to_qualified_hint = Some(redirect.clone());
            }
            if relation.from == relation.to {
                continue;
            }
            relation.location = scalar.location(self.yaml, &relation.location);
            relation.metadata["language"] = json!("ansible");
            relation.metadata["embedded_in"] = json!(owner);
            self.result.relations.push(relation);
        }
        for mut reference in fragment.source_references {
            if let Some(redirect) = redirects.get(&reference.source) {
                reference.source = redirect.clone();
            }
            reference.location = scalar.location(self.yaml, &reference.location);
            reference.context.role_root = self.role.clone();
            reference.context.playbook = self.playbook.clone();
            self.result.source_references.push(reference);
        }
    }

    fn scan_values(&mut self, owner: &str, root: Node<'_>, field: String, bare: bool) {
        let mut pending = vec![(root, field, 0)];
        while let Some((node, field, depth)) = pending.pop() {
            if depth > MAX_DEPTH || node.has_error() {
                self.diagnostic(node, "unsupported_yaml_region");
                continue;
            }
            if let Some(tag) = self.yaml.tag(node) {
                if tag == "!unsafe" {
                    continue;
                }
                if tag == "!vault" {
                    self.diagnostic(node, "encrypted_vault_content");
                    continue;
                }
                if !matches!(tag, "!!str" | "!<tag:yaml.org,2002:str>") {
                    self.diagnostic(node, "unsupported_yaml_tag");
                    continue;
                }
            }
            let node = value(node);
            if matches!(node.kind(), "block_mapping" | "flow_mapping") {
                for (key, value) in self.yaml.pairs(node).into_iter().rev() {
                    self.scan_candidate(&mut pending, value, format!("{field}.{key}"), depth + 1);
                }
            } else if matches!(node.kind(), "block_sequence" | "flow_sequence") {
                for (index, item) in sequence(node).into_iter().enumerate().rev() {
                    self.scan_candidate(&mut pending, item, format!("{field}[{index}]"), depth + 1);
                }
            } else {
                self.embed(owner, node, &field, bare);
            }
        }
    }

    fn scan_candidate<'tree>(
        &self,
        pending: &mut Vec<(Node<'tree>, String, usize)>,
        value: Node<'tree>,
        field: String,
        depth: usize,
    ) {
        pending.push((value, field, depth));
    }

    fn role_references(&mut self, owner: &str, node: Node<'_>, dependency: bool) {
        for (index, item) in sequence(node).into_iter().enumerate() {
            let pairs = self.yaml.pairs(item);
            let name = get(&pairs, "role")
                .or_else(|| get(&pairs, "name"))
                .unwrap_or(item);
            self.add_reference(
                owner,
                if dependency {
                    SourceReferenceKind::RoleDependency
                } else {
                    SourceReferenceKind::AnsibleRole
                },
                name,
            );
            self.scan_values(owner, item, format!("roles[{index}]"), false);
        }
    }

    fn task<'tree>(&mut self, visit: Visit<'tree>, pending: &mut Vec<Visit<'tree>>) {
        let Visit::Task {
            node,
            parent,
            handler,
            section,
            index,
            depth,
        } = visit
        else {
            return;
        };
        if depth > MAX_DEPTH || node.has_error() {
            self.diagnostic(node, "unsupported_task_region");
            return;
        }
        let pairs = self.yaml.pairs(node);
        if pairs.is_empty() {
            self.diagnostic(node, "invalid_task_mapping");
            return;
        }
        let name = name_value(self.yaml, get(&pairs, "name")).unwrap_or_default();
        let list_scope = format!("{parent}::section:{section}");
        let qn = self.unique(
            &list_scope,
            if handler { "handler" } else { "task" },
            &name,
            index,
        );
        let mut task = symbol(
            if name.is_empty() {
                "<unnamed>".into()
            } else {
                name
            },
            if handler {
                SymbolType::AnsibleHandler
            } else {
                SymbolType::AnsibleTask
            },
            qn.clone(),
            self.location(node),
            if handler {
                "ansible_handler"
            } else {
                "ansible_task"
            },
        );
        task.metadata["section"] = json!(section);
        task.metadata["parent_qualified_name"] = json!(parent);
        if get(&pairs, "block").is_some() {
            task.metadata["kind"] = json!("ansible_block");
        }
        if let Some(role) = &self.role {
            task.metadata["role_root"] = json!(role);
        }
        let task_index = self.result.symbols.len();
        self.result.symbols.push(task);
        self.relation(
            parent.clone(),
            qn.clone(),
            RelationType::Defines,
            self.location(node),
        );
        if !handler
            && self.playbook.is_some()
            && self
                .frames
                .last()
                .is_some_and(|frame| frame.global && frame.qn == parent)
        {
            self.relation(
                parent,
                qn.clone(),
                RelationType::ExecutesTask,
                self.location(node),
            );
        }
        self.frames.push(Frame {
            qn: qn.clone(),
            global: false,
            variables: HashMap::new(),
        });
        let reference_start = self.result.source_references.len();
        self.inline_vars(&qn, get(&pairs, "vars"));
        // Loop expressions use the outer context; bindings apply to the body.
        for (key, value) in &pairs {
            if key == "loop" || key.starts_with("with_") {
                self.scan_values(&qn, *value, key.to_string(), false);
            }
        }
        if get(&pairs, "loop").is_some() || pairs.iter().any(|(key, _)| key.starts_with("with_")) {
            let loop_control = get(&pairs, "loop_control")
                .map(|node| self.yaml.pairs(node))
                .unwrap_or_default();
            let loop_var = name_value(self.yaml, get(&loop_control, "loop_var"))
                .unwrap_or_else(|| "item".into());
            self.declare(&qn, &loop_var, "ansible_loop_binding", node, false);
            if let Some(index_var) = name_value(self.yaml, get(&loop_control, "index_var")) {
                self.declare(&qn, &index_var, "ansible_loop_binding", node, false);
            }
        }
        let mut module = None;
        let mut parameters = Vec::new();
        if let Some(action) = get(&pairs, "action").or_else(|| get(&pairs, "local_action")) {
            let action_pairs = self.yaml.pairs(action);
            if let Some(name) = name_value(self.yaml, get(&action_pairs, "module")) {
                module = Some(name);
                parameters = action_pairs
                    .into_iter()
                    .filter(|(key, _)| key != "module")
                    .collect();
            } else if let Some(raw) = name_value(self.yaml, Some(action)) {
                // Preserve free-form argument spelling. Only a plain module
                // token is classified; free-form key=value parsing is not guessed.
                module = raw.split_whitespace().next().map(str::to_owned);
                self.result.symbols[task_index].metadata["action_arguments"] = json!(raw);
            }
        } else {
            let actions: Vec<_> = pairs.iter().filter(|(key, _)| !reserved(key)).collect();
            if actions.len() == 1 {
                module = Some(actions[0].0.to_string());
                parameters = self.yaml.pairs(actions[0].1);
            } else if actions.len() > 1 {
                self.diagnostic(node, "ambiguous_task_action");
            }
        }
        if let Some(args) = get(&pairs, "args") {
            parameters.extend(self.yaml.pairs(args));
        }
        if let Some(module) = &module {
            self.result.symbols[task_index].metadata["module"] = json!(module);
            match builtin(module) {
                "template" => {
                    if let Some(src) = get(&parameters, "src")
                        && let Ok(scalar) = self.yaml.scalar(src)
                    {
                        let mut reference =
                            self.reference(&qn, SourceReferenceKind::TemplateRender, &scalar, src);
                        for (key, value) in &parameters {
                            if matches!(
                                key.as_ref(),
                                "variable_start_string"
                                    | "variable_end_string"
                                    | "block_start_string"
                                    | "block_end_string"
                                    | "comment_start_string"
                                    | "comment_end_string"
                                    | "trim_blocks"
                                    | "lstrip_blocks"
                            ) && let Ok(setting) = self.yaml.scalar(*value)
                            {
                                reference.properties.insert(key.to_string(), setting.value);
                            }
                        }
                        self.result.source_references.push(reference);
                    }
                    if let Some(dest) = name_value(self.yaml, get(&parameters, "dest")) {
                        self.result.symbols[task_index].metadata["template_dest"] = json!(dest);
                    }
                }
                "include_role" | "import_role" => {
                    if let Some(name) = get(&parameters, "name") {
                        self.add_reference(&qn, SourceReferenceKind::AnsibleRole, name);
                        if let (Some(tasks_from), Ok(role)) =
                            (get(&parameters, "tasks_from"), self.yaml.scalar(name))
                            && let Ok(target) = self.yaml.scalar(tasks_from)
                        {
                            let mut reference = self.reference(
                                &qn,
                                SourceReferenceKind::TaskInclude,
                                &target,
                                tasks_from,
                            );
                            reference.dynamic |=
                                role.value.contains("{{") || role.value.contains("{%");
                            reference.properties.insert("role_name".into(), role.value);
                            reference
                                .properties
                                .insert("source_form".into(), "role_tasks_from".into());
                            self.result.symbols[task_index].metadata["tasks_from"] =
                                json!(target.value);
                            self.result.source_references.push(reference);
                        }
                    }
                }
                "include_tasks" | "import_tasks" | "include" | "import_playbook" => {
                    let target = get(&parameters, "file").or_else(|| {
                        pairs
                            .iter()
                            .find(|(key, _)| builtin(key) == builtin(module))
                            .map(|(_, value)| *value)
                    });
                    if let Some(target) = target {
                        self.add_reference(
                            &qn,
                            if builtin(module) == "import_playbook" {
                                SourceReferenceKind::PlaybookInclude
                            } else {
                                SourceReferenceKind::TaskInclude
                            },
                            target,
                        );
                    }
                }
                "set_fact" => {
                    for (key, value) in &parameters {
                        if key != "cacheable" {
                            self.declare(&qn, key, "ansible_set_fact", *value, true);
                        }
                    }
                }
                _ => {}
            }
        }
        if let Some(register) = get(&pairs, "register")
            && let Some(name) = name_value(self.yaml, Some(register))
        {
            self.declare(&qn, &name, "ansible_register", register, true);
        }
        if let Some(notify) = get(&pairs, "notify") {
            let items = sequence(notify);
            if items.is_empty() {
                self.add_reference(&qn, SourceReferenceKind::HandlerNotify, notify);
            } else {
                for item in items {
                    self.add_reference(&qn, SourceReferenceKind::HandlerNotify, item);
                }
            }
        }
        if handler && let Some(listen) = get(&pairs, "listen") {
            let items = sequence(listen);
            let topics: Vec<_> = if items.is_empty() {
                name_value(self.yaml, Some(listen)).into_iter().collect()
            } else {
                items
                    .into_iter()
                    .filter_map(|item| name_value(self.yaml, Some(item)))
                    .collect()
            };
            self.result.symbols[task_index].metadata["listen_topics_json"] =
                json!(serde_json::to_string(&topics).unwrap_or_default());
        }
        for (key, value) in &pairs {
            if matches!(
                key.as_ref(),
                "vars" | "block" | "rescue" | "always" | "loop"
            ) || key.starts_with("with_")
            {
                continue;
            }
            self.scan_values(
                &qn,
                *value,
                key.to_string(),
                matches!(
                    key.as_ref(),
                    "when" | "failed_when" | "changed_when" | "until"
                ),
            );
        }
        for reference in self.result.source_references[reference_start..]
            .iter_mut()
            .filter(|reference| reference.source == qn)
        {
            if let Some(module) = &module {
                reference
                    .properties
                    .entry("action".into())
                    .or_insert_with(|| module.clone());
            }
        }
        pending.push(Visit::Leave);
        for section in ["always", "rescue", "block"] {
            if let Some(children) = get(&pairs, section) {
                for (index, node) in sequence(children).into_iter().enumerate().rev() {
                    pending.push(Visit::Task {
                        node,
                        parent: qn.clone(),
                        handler,
                        section,
                        index,
                        depth: depth + 1,
                    });
                }
            }
        }
    }

    fn tasks(&mut self, root: Node<'_>, parent: &str, handler: bool, section: &'static str) {
        let mut pending = Vec::new();
        for (index, node) in sequence(root).into_iter().enumerate().rev() {
            pending.push(Visit::Task {
                node,
                parent: parent.to_owned(),
                handler,
                section,
                index,
                depth: 0,
            });
        }
        while let Some(visit) = pending.pop() {
            match visit {
                visit @ Visit::Task { .. } => self.task(visit, &mut pending),
                Visit::Leave => {
                    self.frames.pop();
                }
            }
        }
    }

    fn playbook(&mut self, root: Node<'_>) {
        for (index, node) in sequence(root).into_iter().enumerate() {
            let pairs = self.yaml.pairs(node);
            if let Some(import) = get(&pairs, "import_playbook")
                .or_else(|| get(&pairs, "ansible.builtin.import_playbook"))
            {
                self.add_reference(
                    &self.root.clone(),
                    SourceReferenceKind::PlaybookInclude,
                    import,
                );
                self.scan_values(
                    &self.root.clone(),
                    node,
                    format!("import_playbook[{index}]"),
                    false,
                );
                continue;
            }
            if get(&pairs, "hosts").is_none() {
                self.diagnostic(node, "play_missing_hosts");
                continue;
            }
            let name = name_value(self.yaml, get(&pairs, "name")).unwrap_or_default();
            let qn = self.unique(&self.root.clone(), "play", &name, index);
            let mut play = symbol(
                if name.is_empty() {
                    "<unnamed play>".into()
                } else {
                    name
                },
                SymbolType::AnsiblePlay,
                qn.clone(),
                self.location(node),
                "ansible_play",
            );
            if let Some(hosts) = name_value(self.yaml, get(&pairs, "hosts")) {
                play.metadata["hosts_expression"] = json!(hosts);
            }
            self.result.symbols.push(play);
            self.relation(
                self.root.clone(),
                qn.clone(),
                RelationType::Defines,
                self.location(node),
            );
            self.frames.push(Frame {
                qn: qn.clone(),
                global: true,
                variables: HashMap::new(),
            });
            self.inline_vars(&qn, get(&pairs, "vars"));
            if let Some(roles) = get(&pairs, "roles") {
                self.role_references(&qn, roles, false);
            }
            for section in ["pre_tasks", "tasks", "post_tasks", "handlers"] {
                if let Some(tasks) = get(&pairs, section) {
                    self.tasks(tasks, &qn, section == "handlers", section);
                }
            }
            for (key, value) in &pairs {
                if !matches!(
                    key.as_ref(),
                    "vars" | "roles" | "pre_tasks" | "tasks" | "post_tasks" | "handlers"
                ) {
                    self.scan_values(&qn, *value, key.to_string(), false);
                }
            }
            self.frames.pop();
        }
    }

    fn finish(mut self) -> Result<ExtractAllResult> {
        let mut grouped: HashMap<&str, Vec<&SourceReference>> = HashMap::new();
        for reference in &self.result.source_references {
            grouped
                .entry(&reference.source)
                .or_default()
                .push(reference);
        }
        for symbol in &mut self.result.symbols {
            if let Some(qn) = symbol.qualified_name.as_deref() {
                if let Some(references) = grouped.get(qn) {
                    symbol.metadata["source_references_json"] =
                        json!(serde_json::to_string(references)?);
                }
                if let Some(expressions) = self.expressions.get(qn) {
                    symbol.metadata["expression_facts_json"] =
                        json!(serde_json::to_string(expressions)?);
                }
            }
        }
        if let Some(root) = self.result.symbols.first_mut() {
            root.metadata["parse_status"] = json!(if self.diagnostics.is_empty() {
                "complete"
            } else {
                "partial"
            });
            if !self.diagnostics.is_empty() {
                root.metadata["source_diagnostics_json"] =
                    json!(serde_json::to_string(&self.diagnostics)?);
            }
        }
        Ok(self.result)
    }
}

pub(crate) fn extract(path: &Path, source: &[u8], force: bool) -> Result<Option<ExtractAllResult>> {
    if !force
        && path_kind(path).is_none()
        && (path.components().any(|component| {
            matches!(
                component.as_os_str().to_str(),
                Some("group_vars" | "host_vars" | "inventory" | "inventories")
            )
        }) || role_path(path).is_some_and(|(_, relative)| {
            relative.starts_with("defaults/") || relative.starts_with("vars/")
        }))
    {
        return Ok(None);
    }
    if !force
        && !matches_path(path)
        && !path
            .extension()
            .is_some_and(|extension| extension == "yml" || extension == "yaml")
    {
        return Ok(None);
    }
    if !force
        && path_kind(path).is_none()
        && ![b"hosts".as_slice(), b"import_playbook".as_slice()]
            .iter()
            .any(|needle| source.windows(needle.len()).any(|part| part == *needle))
    {
        return Ok(None);
    }
    let file = path.to_string_lossy().replace('\\', "/");
    if source.starts_with(b"$ANSIBLE_VAULT;") {
        let mut root = symbol(
            file.clone(),
            SymbolType::Module,
            format!("ansible:{file}::@source"),
            SourceLocation {
                file: file.clone(),
                start_line: 1,
                end_line: 1,
                start_column: 0,
                end_column: 0,
            },
            "ansible_source",
        );
        root.metadata["parse_status"] = json!("error");
        root.metadata["source_diagnostics_json"] =
            json!(r#"[{"reason":"encrypted_vault_content"}]"#);
        return Ok(Some(ExtractAllResult::from_parts(vec![root], Vec::new())));
    }
    let yaml = Yaml::parse(path, source)?;
    let roots: Vec<_> = named(yaml.tree.root_node())
        .filter(|node| node.kind() == "document")
        .map(value)
        .collect();
    let path_kind = path_kind(path);
    let playbook = roots.iter().any(|root| {
        sequence(*root).iter().any(|node| {
            let pairs = yaml.pairs(*node);
            get(&pairs, "hosts").is_some()
                || get(&pairs, "import_playbook").is_some()
                || get(&pairs, "ansible.builtin.import_playbook").is_some()
        })
    });
    let kind = if playbook {
        Kind::Playbook
    } else if let Some(kind) = path_kind {
        kind
    } else if force {
        Kind::Tasks
    } else {
        return Ok(None);
    };
    let root_qn = format!(
        "ansible:{file}::@{}",
        if kind == Kind::Playbook {
            "playbook"
        } else {
            "source"
        }
    );
    let mut root = symbol(
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("ansible")
            .to_owned(),
        if kind == Kind::Playbook {
            SymbolType::AnsiblePlaybook
        } else {
            SymbolType::Module
        },
        root_qn.clone(),
        yaml.node_location(yaml.tree.root_node(), &file),
        if kind == Kind::Playbook {
            "ansible_playbook"
        } else {
            "ansible_source"
        },
    );
    let role = role_path(path).map(|(root, _)| root);
    if let Some(role) = &role {
        root.metadata["role_root"] = json!(role);
    }
    root.metadata["source_kind"] = json!(match kind {
        Kind::Playbook => "playbook",
        Kind::Tasks => "tasks",
        Kind::Handlers => "handlers",
        Kind::Meta => "role_meta",
    });
    let mut extract = Extract {
        yaml: &yaml,
        path,
        file: file.clone(),
        root: root_qn.clone(),
        role,
        playbook: (kind == Kind::Playbook).then_some(file),
        result: ExtractAllResult::from_parts(vec![root], Vec::new()),
        frames: vec![Frame {
            qn: root_qn.clone(),
            global: true,
            variables: HashMap::new(),
        }],
        counts: HashMap::new(),
        diagnostics: Vec::new(),
        expressions: HashMap::new(),
        inputs: HashMap::new(),
    };
    if yaml.tree.root_node().has_error() {
        extract.diagnostic(yaml.tree.root_node(), "malformed_yaml");
        return extract.finish().map(Some);
    }
    for root in roots {
        match kind {
            Kind::Playbook => extract.playbook(root),
            Kind::Tasks | Kind::Handlers => {
                if matches!(value(root).kind(), "block_sequence" | "flow_sequence") {
                    extract.tasks(
                        root,
                        &root_qn,
                        kind == Kind::Handlers,
                        if kind == Kind::Handlers {
                            "handlers"
                        } else {
                            "tasks"
                        },
                    );
                } else {
                    extract.diagnostic(root, "invalid_task_list");
                }
            }
            Kind::Meta => {
                if let Some(dependencies) = get(&yaml.pairs(root), "dependencies") {
                    extract.role_references(&root_qn, dependencies, true);
                }
            }
        }
    }
    extract.finish().map(Some)
}
