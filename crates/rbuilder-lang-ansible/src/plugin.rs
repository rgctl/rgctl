//! Ansible playbook and role extraction plugin (Phase 16).

use crate::parser::AnsibleParser;
use rbuilder_plugin_api::Result;
use rbuilder_plugin_api::*;
use serde_yaml::Value;
use std::path::Path;

/// Ansible IaC plugin — playbooks, roles, variables, templates.
pub struct AnsiblePlugin {
    parser: AnsibleParser,
}

impl AnsiblePlugin {
    /// Create a new Ansible plugin instance.
    pub fn new() -> Result<Self> {
        Ok(Self {
            parser: AnsibleParser::new(),
        })
    }

    fn parse_file(&self, file_path: &Path, source: &[u8]) -> (Vec<Symbol>, Vec<Relation>) {
        let file = file_path.to_string_lossy();
        if !AnsibleParser::is_ansible_path(&file) {
            return (vec![], vec![]);
        }
        let text = std::str::from_utf8(source).unwrap_or("");
        if file.ends_with(".j2") {
            return self.parser.parse(&file, &Value::Null, text);
        }
        let value: Value = serde_yaml::from_str(text).unwrap_or(Value::Null);
        self.parser.parse(&file, &value, text)
    }
}

impl LanguagePlugin for AnsiblePlugin {
    fn language_id(&self) -> &str {
        "ansible"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec![]
    }

    fn grammar(&self) -> Option<tree_sitter::Language> {
        None
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        Ok(self.parse_file(file_path, source).0)
    }

    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        Ok(self.parse_file(file_path, source).1)
    }

    fn calculate_complexity(
        &self,
        _symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        Ok(None)
    }

    fn matches_path(&self, path: &str) -> bool {
        AnsibleParser::is_ansible_path(path)
    }
}

/// Returns true when `path` looks like an Ansible playbook, role, or template file.
pub fn matches_path(path: &str) -> bool {
    AnsibleParser::is_ansible_path(path)
}

/// Parse Ansible YAML/Jinja content into symbols and relations.
pub fn parse_content(
    file: &str,
    value: &serde_yaml::Value,
    text: &str,
) -> (Vec<Symbol>, Vec<Relation>) {
    AnsibleParser::new().parse(file, value, text)
}

/// Extract role names from a role `meta/main.yml` file body.
pub fn role_dependencies_from_meta(meta_path: &str, content: &str) -> Vec<String> {
    let value: serde_yaml::Value = serde_yaml::from_str(content).unwrap_or(serde_yaml::Value::Null);
    let (_, relations) = AnsibleParser::new().parse(meta_path, &value, content);
    relations
        .into_iter()
        .filter(|rel| rel.relation_type == RelationType::DependsOnRole)
        .map(|rel| rel.to)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ansible_plugin_playbook() {
        let plugin = AnsiblePlugin::new().unwrap();
        let source = br#"
- name: deploy
  hosts: all
  tasks:
    - name: ping
      ping:
"#;
        let path = Path::new("playbooks/deploy.yml");
        let symbols = plugin.extract_symbols(path, source).unwrap();
        assert!(symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::AnsiblePlaybook));
        let relations = plugin.extract_relations(path, source, &symbols).unwrap();
        assert!(relations
            .iter()
            .any(|r| r.relation_type == RelationType::ExecutesTask));
    }
}
