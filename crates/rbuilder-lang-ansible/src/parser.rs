//! Ansible YAML parsing — playbooks, roles, variables, and templates.

use rbuilder_plugin_api::*;
use regex::Regex;
use serde_json::json;
use serde_yaml::Value;
use std::collections::HashSet;

const CONTROL_KEYS: &[&str] = &[
    "name",
    "when",
    "loop",
    "with_items",
    "with_dict",
    "with_fileglob",
    "tags",
    "notify",
    "become",
    "become_user",
    "become_method",
    "vars",
    "register",
    "changed_when",
    "failed_when",
    "block",
    "rescue",
    "always",
    "import_tasks",
    "include_tasks",
    "import_playbook",
    "include",
    "meta",
    "delegate_to",
    "ignore_errors",
    "no_log",
    "async",
    "poll",
    "until",
    "retries",
    "delay",
    "listen",
    "include_role",
    "import_role",
];

/// Ansible YAML parser producing plugin symbols and relations.
pub struct AnsibleParser {
    jinja_var_regex: Regex,
}

struct TaskEntryContext<'a> {
    file: &'a str,
    parent_id: &'a str,
    task_val: &'a Value,
    task_type: SymbolType,
    idx: usize,
}

struct ParseOutput<'a> {
    symbols: &'a mut Vec<Symbol>,
    relations: &'a mut Vec<Relation>,
}

fn compile_pattern(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

impl Default for AnsibleParser {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsibleParser {
    /// Create a new parser with Jinja2 variable extraction.
    pub fn new() -> Self {
        Self {
            jinja_var_regex: compile_pattern(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_\.'\[\]]*)\s*\}\}"),
        }
    }

    /// Whether a file path should be handled by the Ansible plugin.
    pub fn is_ansible_path(path_str: &str) -> bool {
        let p = path_str.replace('\\', "/");
        if p.ends_with(".j2") {
            return true;
        }
        if !(p.ends_with(".yml") || p.ends_with(".yaml")) {
            return false;
        }
        if p.contains(".github/workflows/") || p.contains("gitlab-ci") {
            return false;
        }
        p.contains("/roles/")
            || p.starts_with("roles/")
            || p.contains("/group_vars/")
            || p.starts_with("group_vars/")
            || p.contains("/host_vars/")
            || p.starts_with("host_vars/")
            || p.contains("/playbooks/")
            || p.starts_with("playbooks/")
            || p.contains("/inventory/")
            || p.starts_with("inventory/")
            || p.contains("/templates/")
            || p.starts_with("templates/")
            || p.contains("/ansible/")
            || p.starts_with("ansible/")
            || p.ends_with("/site.yml")
            || p.ends_with("/site.yaml")
            || p.ends_with("/playbook.yml")
            || p.ends_with("/playbook.yaml")
            || p.ends_with("site.yml")
            || p.ends_with("site.yaml")
    }

    /// Extract `{{ variable }}` references from text.
    pub fn extract_jinja_vars(&self, text: &str) -> Vec<String> {
        self.jinja_var_regex
            .captures_iter(text)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    /// Parse an Ansible file into symbols and relations.
    pub fn parse(
        &self,
        file: &str,
        value: &Value,
        source_text: &str,
    ) -> (Vec<Symbol>, Vec<Relation>) {
        if file.ends_with(".j2") {
            return self.parse_template(file, source_text);
        }
        let is_role_path = file.contains("/roles/") || file.starts_with("roles/");
        if is_role_path && file.contains("/meta/") {
            return self.parse_role_meta(file, value);
        }
        if is_role_path && (file.contains("/tasks/") || file.contains("/handlers/")) {
            return self.parse_role_tasks(file, value, false);
        }
        if file.contains("group_vars/") || file.contains("host_vars/") {
            return self.parse_vars_file(file, value);
        }
        self.parse_playbook(file, value)
    }

    fn loc(file: &str, line: usize) -> SourceLocation {
        SourceLocation {
            file: file.to_string(),
            start_line: line,
            end_line: line,
            start_column: 0,
            end_column: 0,
        }
    }

    fn playbook_name(file: &str) -> String {
        std::path::Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("playbook")
            .to_string()
    }

    fn role_name_from_path(file: &str) -> Option<String> {
        let p = file.replace('\\', "/");
        let rest = if let Some(idx) = p.find("/roles/") {
            &p[idx + 7..]
        } else {
            p.strip_prefix("roles/")?
        };
        let end = rest.find('/')?;
        Some(rest[..end].to_string())
    }

    fn parse_playbook(&self, file: &str, value: &Value) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let pb_name = Self::playbook_name(file);

        symbols.push(Symbol {
            name: pb_name.clone(),
            symbol_type: SymbolType::AnsiblePlaybook,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "ansible": true }),
        });

        if let Some(seq) = value.as_sequence() {
            for item in seq {
                if let Some(imported) = item.get("import_playbook").and_then(|v| v.as_str()) {
                    relations.push(Relation {
                        from: pb_name.clone(),
                        to: imported.to_string(),
                        relation_type: RelationType::IncludesPlaybook,
                        location: Self::loc(file, 1),
                        metadata: json!({}),
                    });
                    continue;
                }
                if item.get("hosts").is_some() || item.get("tasks").is_some() {
                    self.parse_play(file, &pb_name, item, &mut symbols, &mut relations);
                }
            }
        } else if value.get("hosts").is_some() || value.get("tasks").is_some() {
            self.parse_play(file, &pb_name, value, &mut symbols, &mut relations);
        }

        (symbols, relations)
    }

    fn parse_play(
        &self,
        file: &str,
        pb_name: &str,
        play_value: &Value,
        symbols: &mut Vec<Symbol>,
        relations: &mut Vec<Relation>,
    ) {
        let play_name = play_value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed play");
        let play_id = format!("{pb_name}::{play_name}");
        let hosts = play_value
            .get("hosts")
            .and_then(|v| v.as_str())
            .unwrap_or("all");

        symbols.push(Symbol {
            name: play_id.clone(),
            symbol_type: SymbolType::AnsiblePlay,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: Some(format!("hosts: {hosts}")),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "hosts": hosts, "playbook": pb_name }),
        });

        if let Some(roles) = play_value.get("roles") {
            self.parse_role_list(file, &play_id, roles, symbols, relations);
        }

        for (key, handler) in [
            ("tasks", SymbolType::AnsibleTask),
            ("pre_tasks", SymbolType::AnsibleTask),
            ("post_tasks", SymbolType::AnsibleTask),
        ] {
            if let Some(tasks) = play_value.get(key) {
                self.parse_task_list(file, &play_id, tasks, handler, symbols, relations);
            }
        }

        if let Some(handlers) = play_value.get("handlers") {
            self.parse_task_list(
                file,
                &play_id,
                handlers,
                SymbolType::AnsibleHandler,
                symbols,
                relations,
            );
        }
    }

    fn parse_role_list(
        &self,
        file: &str,
        play_id: &str,
        roles: &Value,
        symbols: &mut Vec<Symbol>,
        relations: &mut Vec<Relation>,
    ) {
        let Some(seq) = roles.as_sequence() else {
            return;
        };
        for role_val in seq {
            let role_name = match role_val {
                Value::String(s) => s.clone(),
                Value::Mapping(m) => m
                    .get(Value::String("role".into()))
                    .or_else(|| m.get(Value::String("name".into())))
                    .and_then(|v| v.as_str())
                    .unwrap_or("role")
                    .to_string(),
                _ => continue,
            };
            symbols.push(Symbol {
                name: role_name.clone(),
                symbol_type: SymbolType::AnsibleRole,
                qualified_name: None,
                location: Self::loc(file, 1),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({ "referenced": true }),
            });
            relations.push(Relation {
                from: play_id.to_string(),
                to: role_name,
                relation_type: RelationType::IncludesRole,
                location: Self::loc(file, 1),
                metadata: json!({}),
            });
        }
    }

    fn parse_task_list(
        &self,
        file: &str,
        parent_id: &str,
        tasks: &Value,
        task_type: SymbolType,
        symbols: &mut Vec<Symbol>,
        relations: &mut Vec<Relation>,
    ) {
        let Some(seq) = tasks.as_sequence() else {
            return;
        };
        for (idx, task_val) in seq.iter().enumerate() {
            self.parse_task_entry(
                TaskEntryContext {
                    file,
                    parent_id,
                    task_val,
                    task_type,
                    idx,
                },
                &mut ParseOutput { symbols, relations },
            );
        }
    }

    fn parse_task_entry(&self, ctx: TaskEntryContext<'_>, out: &mut ParseOutput<'_>) {
        let TaskEntryContext {
            file,
            parent_id,
            task_val,
            task_type,
            idx,
        } = ctx;
        let symbols = &mut out.symbols;
        let relations = &mut out.relations;
        if let Some(block) = task_val.get("block") {
            self.parse_task_list(file, parent_id, block, task_type, symbols, relations);
            if let Some(rescue) = task_val.get("rescue") {
                self.parse_task_list(file, parent_id, rescue, task_type, symbols, relations);
            }
            if let Some(always) = task_val.get("always") {
                self.parse_task_list(file, parent_id, always, task_type, symbols, relations);
            }
            return;
        }

        if let Some(role_inc) = task_val.get("include_role").or(task_val.get("import_role")) {
            if let Some(name) = role_inc
                .get("name")
                .and_then(|v| v.as_str())
                .or_else(|| role_inc.as_str())
            {
                symbols.push(Symbol {
                    name: name.to_string(),
                    symbol_type: SymbolType::AnsibleRole,
                    qualified_name: None,
                    location: Self::loc(file, 1),
                    signature: None,
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: json!({ "referenced": true }),
                });
                relations.push(Relation {
                    from: parent_id.to_string(),
                    to: name.to_string(),
                    relation_type: RelationType::IncludesRole,
                    location: Self::loc(file, 1),
                    metadata: json!({}),
                });
            }
            return;
        }

        let name = task_val
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed task");
        let task_id = if task_type == SymbolType::AnsibleHandler {
            format!("{parent_id}::handler::{name}")
        } else {
            format!("{parent_id}::{name}::{idx}")
        };
        let (module, args_text) = task_module_and_args(task_val);
        let become_flag = task_val
            .get("become")
            .and_then(|v| v.as_bool())
            .map(|b| b.to_string());

        symbols.push(Symbol {
            name: task_id.clone(),
            symbol_type: task_type,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: Some(args_text.clone()),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({
                "module": module,
                "become": become_flag,
                "parent": parent_id,
            }),
        });

        relations.push(Relation {
            from: parent_id.to_string(),
            to: task_id.clone(),
            relation_type: RelationType::ExecutesTask,
            location: Self::loc(file, 1),
            metadata: json!({}),
        });

        let notify_targets = collect_notify(task_val);
        for handler in notify_targets {
            relations.push(Relation {
                from: task_id.clone(),
                to: format!("{parent_id}::handler::{handler}"),
                relation_type: RelationType::NotifiesHandler,
                location: Self::loc(file, 1),
                metadata: json!({ "handler": handler }),
            });
        }

        let task_yaml = serde_yaml::to_string(task_val).unwrap_or_default();
        for var in self.extract_jinja_vars(&task_yaml) {
            let var_id = format!("var::{var}");
            if !symbols.iter().any(|s| s.name == var_id) {
                symbols.push(Symbol {
                    name: var_id.clone(),
                    symbol_type: SymbolType::AnsibleVariable,
                    qualified_name: None,
                    location: Self::loc(file, 1),
                    signature: None,
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: json!({ "variable": var }),
                });
            }
            relations.push(Relation {
                from: task_id.clone(),
                to: var_id,
                relation_type: RelationType::UsesVariable,
                location: Self::loc(file, 1),
                metadata: json!({}),
            });
        }

        if module == "template" {
            if let Some(src) = task_val
                .get("template")
                .and_then(|t| t.get("src").or(Some(t)))
                .and_then(|v| v.as_str())
            {
                relations.push(Relation {
                    from: task_id,
                    to: src.to_string(),
                    relation_type: RelationType::RendersTemplate,
                    location: Self::loc(file, 1),
                    metadata: json!({}),
                });
            }
        }
    }

    fn parse_role_meta(&self, file: &str, value: &Value) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let role_name = Self::role_name_from_path(file).unwrap_or_else(|| {
            value
                .get("galaxy_info")
                .and_then(|g| g.get("role_name"))
                .and_then(|v| v.as_str())
                .unwrap_or("role")
                .to_string()
        });

        symbols.push(Symbol {
            name: role_name.clone(),
            symbol_type: SymbolType::AnsibleRole,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "role_path": file }),
        });

        if let Some(deps) = value.get("dependencies").and_then(|d| d.as_sequence()) {
            for dep in deps {
                let dep_name = match dep {
                    Value::String(s) => s.clone(),
                    Value::Mapping(m) => m
                        .get(Value::String("role".into()))
                        .or_else(|| m.get(Value::String("name".into())))
                        .and_then(|v| v.as_str())
                        .unwrap_or("dependency")
                        .to_string(),
                    _ => continue,
                };
                symbols.push(Symbol {
                    name: dep_name.clone(),
                    symbol_type: SymbolType::AnsibleRole,
                    qualified_name: None,
                    location: Self::loc(file, 1),
                    signature: None,
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: json!({ "dependency": true }),
                });
                relations.push(Relation {
                    from: role_name.clone(),
                    to: dep_name,
                    relation_type: RelationType::DependsOnRole,
                    location: Self::loc(file, 1),
                    metadata: json!({}),
                });
            }
        }

        (symbols, relations)
    }

    fn parse_role_tasks(
        &self,
        file: &str,
        value: &Value,
        _from_handlers: bool,
    ) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let role_name = Self::role_name_from_path(file).unwrap_or_else(|| "role".to_string());
        let is_handler = file.contains("/handlers/");
        let task_type = if is_handler {
            SymbolType::AnsibleHandler
        } else {
            SymbolType::AnsibleTask
        };

        symbols.push(Symbol {
            name: role_name.clone(),
            symbol_type: SymbolType::AnsibleRole,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "role_path": file }),
        });

        self.parse_task_list(
            file,
            &role_name,
            value,
            task_type,
            &mut symbols,
            &mut relations,
        );

        (symbols, relations)
    }

    fn parse_vars_file(&self, file: &str, value: &Value) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let scope = if file.contains("group_vars/") {
            "group_vars"
        } else {
            "host_vars"
        };
        let mut keys = HashSet::new();
        collect_yaml_keys(value, &mut keys);
        for key in keys {
            let var_id = format!("var::{key}");
            symbols.push(Symbol {
                name: var_id.clone(),
                symbol_type: SymbolType::AnsibleVariable,
                qualified_name: None,
                location: Self::loc(file, 1),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({ "scope": scope, "variable": key }),
            });
            relations.push(Relation {
                from: scope.to_string(),
                to: var_id,
                relation_type: RelationType::Defines,
                location: Self::loc(file, 1),
                metadata: json!({}),
            });
        }
        (symbols, relations)
    }

    fn parse_template(&self, file: &str, source_text: &str) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let name = std::path::Path::new(file)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("template")
            .to_string();

        symbols.push(Symbol {
            name: name.clone(),
            symbol_type: SymbolType::AnsibleTemplate,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "template": true }),
        });

        for var in self.extract_jinja_vars(source_text) {
            let var_id = format!("var::{var}");
            if !symbols.iter().any(|s| s.name == var_id) {
                symbols.push(Symbol {
                    name: var_id.clone(),
                    symbol_type: SymbolType::AnsibleVariable,
                    qualified_name: None,
                    location: Self::loc(file, 1),
                    signature: None,
                    return_type: None,
                    parameters: vec![],
                    fields: vec![],
                    modifiers: vec![],
                    documentation: None,
                    metadata: json!({ "variable": var }),
                });
            }
            relations.push(Relation {
                from: name.clone(),
                to: var_id,
                relation_type: RelationType::UsesVariable,
                location: Self::loc(file, 1),
                metadata: json!({}),
            });
        }

        (symbols, relations)
    }
}

fn task_module_and_args(task_val: &Value) -> (String, String) {
    let Some(obj) = task_val.as_mapping() else {
        return ("unknown".into(), String::new());
    };
    for (k, v) in obj {
        if let Some(key) = k.as_str() {
            if !CONTROL_KEYS.contains(&key) {
                let args = serde_yaml::to_string(v).unwrap_or_default();
                return (key.to_string(), args);
            }
        }
    }
    ("unknown".into(), String::new())
}

fn collect_notify(task_val: &Value) -> Vec<String> {
    let notify = task_val.get("notify");
    match notify {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Sequence(seq)) => seq
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => vec![],
    }
}

fn collect_yaml_keys(value: &Value, keys: &mut HashSet<String>) {
    match value {
        Value::Mapping(map) => {
            for (k, v) in map {
                if let Some(key) = k.as_str() {
                    keys.insert(key.to_string());
                }
                collect_yaml_keys(v, keys);
            }
        }
        Value::Sequence(seq) => {
            for item in seq {
                collect_yaml_keys(item, keys);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jinja_var_extraction() {
        let parser = AnsibleParser::new();
        let vars = parser.extract_jinja_vars("echo {{ app_name }} and {{ db.host }}");
        assert!(vars.contains(&"app_name".to_string()));
        assert!(vars.contains(&"db.host".to_string()));
    }

    #[test]
    fn test_playbook_parsing() {
        let parser = AnsibleParser::new();
        let yaml: Value = serde_yaml::from_str(
            r#"
- name: web tier
  hosts: web
  roles:
    - nginx
  tasks:
    - name: install nginx
      apt:
        name: nginx
      notify: restart nginx
  handlers:
    - name: restart nginx
      service:
        name: nginx
        state: restarted
"#,
        )
        .unwrap();
        let (symbols, relations) = parser.parse("playbooks/site.yml", &yaml, "");
        assert!(symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::AnsiblePlaybook));
        assert!(symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::AnsiblePlay));
        assert!(symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::AnsibleTask));
        assert!(relations
            .iter()
            .any(|r| r.relation_type == RelationType::IncludesRole));
        assert!(relations
            .iter()
            .any(|r| r.relation_type == RelationType::ExecutesTask));
    }

    #[test]
    fn test_role_meta_dependencies() {
        let parser = AnsibleParser::new();
        let yaml: Value = serde_yaml::from_str(
            r#"
dependencies:
  - common
  - role: geerlingguy.mysql
"#,
        )
        .unwrap();
        let (symbols, relations) = parser.parse("roles/nginx/meta/main.yml", &yaml, "");
        assert!(symbols.iter().any(|s| s.name == "nginx"));
        assert!(relations
            .iter()
            .any(|r| r.relation_type == RelationType::DependsOnRole));
        assert_eq!(relations.len(), 2);
    }
}
