//! Puppet manifest DSL parsing — modules, classes, defined types, resources.

use rbuilder_plugin_api::*;
use regex::Regex;
use serde_json::json;
use std::collections::HashSet;

const PUPPET_RESOURCES: &[&str] = &[
    "file",
    "package",
    "service",
    "exec",
    "user",
    "group",
    "cron",
    "mount",
    "host",
    "notify",
    "firewall",
    "yumrepo",
    "ssh_authorized_key",
    "sshkey",
    "augeas",
    "concat",
    "archive",
    "vcsrepo",
    "apt::source",
    "systemd::unit",
];

fn compile_pattern(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

struct ClassBlockContext<'a> {
    file: &'a str,
    line: usize,
    class_name: &'a str,
    params_str: &'a str,
    inherits: Option<&'a str>,
    body: &'a str,
    module_id: &'a str,
}

struct ParseOutput<'a> {
    symbols: &'a mut Vec<Symbol>,
    relations: &'a mut Vec<Relation>,
}

/// Puppet DSL parser producing plugin symbols and relations.
pub struct PuppetParser {
    class_regex: Regex,
    define_regex: Regex,
    resource_regex: Regex,
    include_regex: Regex,
    variable_regex: Regex,
    fact_regex: Regex,
    param_regex: Regex,
}

impl Default for PuppetParser {
    fn default() -> Self {
        Self::new()
    }
}

impl PuppetParser {
    /// Create a new Puppet parser.
    pub fn new() -> Self {
        Self {
            class_regex: compile_pattern(
                r"(?ms)class\s+([a-zA-Z0-9_:]+)\s*(?:\((.*?)\))?\s*(?:inherits\s+([a-zA-Z0-9_:]+))?\s*\{",
            ),
            define_regex: compile_pattern(r"(?ms)define\s+([a-zA-Z0-9_:]+)\s*(?:\((.*?)\))?\s*\{"),
            resource_regex: compile_pattern(r#"(?m)^(\w+(?:::\w+)*)\s*\{\s*['"]([^'"]+)['"]:"#),
            include_regex: compile_pattern(r"(?m)^\s*include\s+(?:::)?([a-zA-Z0-9_:]+)"),
            variable_regex: compile_pattern(r"\$([a-zA-Z0-9_]+)\s*=\s*(.+)"),
            fact_regex: compile_pattern(r#"\$facts\[['"]([^'"]+)['"]\]|\$::([a-zA-Z0-9_]+)"#),
            param_regex: compile_pattern(r"(?:(\w+)\s+)?\$([a-zA-Z0-9_]+)\s*(?:=\s*([^,]+))?"),
        }
    }

    /// Whether a file path should be handled by the Puppet plugin.
    pub fn is_puppet_path(path_str: &str) -> bool {
        let p = path_str.replace('\\', "/");
        if p.ends_with(".pp") {
            return p.contains("/manifests/")
                || p.starts_with("manifests/")
                || p.ends_with("site.pp")
                || p.contains("/modules/")
                || p.starts_with("modules/")
                || p.contains("/environments/");
        }
        if p.ends_with("metadata.json") {
            return p.contains("/modules/")
                || p.starts_with("modules/")
                || (p.contains("/environments/") && p.contains("/modules/"));
        }
        false
    }

    /// Parse a Puppet file into symbols and relations.
    pub fn parse(&self, file: &str, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
        if file.ends_with("metadata.json") {
            return self.parse_metadata(file, source);
        }
        if file.ends_with(".pp") {
            return self.parse_manifest(file, source);
        }
        (vec![], vec![])
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

    fn module_name_from_path(file: &str) -> String {
        let p = file.replace('\\', "/");
        if let Some(idx) = p.find("/modules/") {
            let rest = &p[idx + 9..];
            if let Some(end) = rest.find('/') {
                return rest[..end].to_string();
            }
        }
        if let Some(stripped) = p.strip_prefix("modules/") {
            if let Some(end) = stripped.find('/') {
                return stripped[..end].to_string();
            }
        }
        if let Some(idx) = p.find("/manifests/") {
            let before = &p[..idx];
            if let Some(name) = before.rsplit('/').next() {
                if name != "modules" && !name.is_empty() {
                    return name.to_string();
                }
            }
        }
        if p.ends_with("metadata.json") {
            if let Some(parent) = std::path::Path::new(&p)
                .parent()
                .and_then(|x| x.file_name())
            {
                return parent.to_string_lossy().to_string();
            }
        }
        "module".to_string()
    }

    fn normalize_module_dep(name: &str) -> String {
        if let Some((_vendor, rest)) = name.split_once('-') {
            if !rest.is_empty() {
                return rest.to_string();
            }
        }
        name.to_string()
    }

    fn class_id(class_name: &str) -> String {
        format!("class::{class_name}")
    }

    fn module_id(module_name: &str) -> String {
        format!("module::{module_name}")
    }

    fn resource_id(owner: &str, resource_type: &str, title: &str) -> String {
        format!("{owner}::resource::{resource_type}::{title}")
    }

    fn defined_type_id(name: &str) -> String {
        format!("defined::{name}")
    }

    fn parse_metadata(&self, file: &str, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let json: serde_json::Value = match serde_json::from_str(source) {
            Ok(v) => v,
            Err(_) => return (symbols, relations),
        };

        let mod_name = json
            .get("name")
            .and_then(|v| v.as_str())
            .map(String::from)
            .unwrap_or_else(|| Self::module_name_from_path(file));
        let version = json
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("0.0.0")
            .to_string();

        let module_id = Self::module_id(&mod_name);
        symbols.push(Symbol {
            name: module_id.clone(),
            symbol_type: SymbolType::PuppetModule,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({
                "version": version,
                "module": mod_name,
            }),
        });

        if let Some(deps) = json.get("dependencies").and_then(|v| v.as_array()) {
            for dep in deps {
                let Some(dep_name) = dep.get("name").and_then(|v| v.as_str()) else {
                    continue;
                };
                let normalized = Self::normalize_module_dep(dep_name);
                let dep_id = Self::module_id(&normalized);
                symbols.push(Symbol {
                    name: dep_id.clone(),
                    symbol_type: SymbolType::PuppetModule,
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
                    from: module_id.clone(),
                    to: dep_id,
                    relation_type: RelationType::DependsOnModule,
                    location: Self::loc(file, 1),
                    metadata: json!({}),
                });
            }
        }

        (symbols, relations)
    }

    fn parse_manifest(&self, file: &str, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let mod_name = Self::module_name_from_path(file);
        let module_id = Self::module_id(&mod_name);

        symbols.push(Symbol {
            name: module_id.clone(),
            symbol_type: SymbolType::PuppetModule,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "referenced": true, "module": mod_name }),
        });

        for cap in self.class_regex.captures_iter(source) {
            let Some(class_name) = cap.get(1).map(|m| m.as_str()) else {
                continue;
            };
            let params_str = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let inherits = cap.get(3).map(|m| m.as_str().to_string());
            let Some(full) = cap.get(0) else {
                continue;
            };
            let class_start = full.end();
            let class_body = self.extract_brace_body(&source[class_start..]);
            let line = source[..full.start()].lines().count().max(1);

            self.parse_class_block(
                ClassBlockContext {
                    file,
                    line,
                    class_name,
                    params_str,
                    inherits: inherits.as_deref(),
                    body: &class_body,
                    module_id: &module_id,
                },
                &mut ParseOutput {
                    symbols: &mut symbols,
                    relations: &mut relations,
                },
            );
        }

        for cap in self.define_regex.captures_iter(source) {
            let Some(type_name) = cap.get(1).map(|m| m.as_str()) else {
                continue;
            };
            let params_str = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let Some(full) = cap.get(0) else {
                continue;
            };
            let define_start = full.end();
            let body = self.extract_brace_body(&source[define_start..]);
            let line = source[..full.start()].lines().count().max(1);
            let dt_id = Self::defined_type_id(type_name);

            symbols.push(Symbol {
                name: dt_id.clone(),
                symbol_type: SymbolType::PuppetDefinedType,
                qualified_name: None,
                location: Self::loc(file, line),
                signature: None,
                return_type: None,
                parameters: self.parse_parameters(params_str),
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({ "module": mod_name }),
            });
            relations.push(Relation {
                from: module_id.clone(),
                to: dt_id.clone(),
                relation_type: RelationType::Defines,
                location: Self::loc(file, line),
                metadata: json!({}),
            });

            self.extract_resources_from_body(
                file,
                line,
                &dt_id,
                &body,
                &mut symbols,
                &mut relations,
            );
        }

        (symbols, relations)
    }

    fn parse_class_block(&self, ctx: ClassBlockContext<'_>, out: &mut ParseOutput<'_>) {
        let ClassBlockContext {
            file,
            line,
            class_name,
            params_str,
            inherits,
            body,
            module_id,
        } = ctx;
        let symbols = &mut out.symbols;
        let relations = &mut out.relations;
        let class_id = Self::class_id(class_name);
        symbols.push(Symbol {
            name: class_id.clone(),
            symbol_type: SymbolType::PuppetClass,
            qualified_name: None,
            location: Self::loc(file, line),
            signature: None,
            return_type: None,
            parameters: self.parse_parameters(params_str),
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "class": class_name }),
        });
        relations.push(Relation {
            from: module_id.to_string(),
            to: class_id.clone(),
            relation_type: RelationType::Defines,
            location: Self::loc(file, line),
            metadata: json!({}),
        });

        if let Some(parent) = inherits {
            let parent_id = Self::class_id(parent);
            symbols.push(Symbol {
                name: parent_id.clone(),
                symbol_type: SymbolType::PuppetClass,
                qualified_name: None,
                location: Self::loc(file, line),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({ "referenced": true }),
            });
            relations.push(Relation {
                from: class_id.clone(),
                to: parent_id,
                relation_type: RelationType::InheritsClass,
                location: Self::loc(file, line),
                metadata: json!({}),
            });
        }

        for included in self.extract_includes(body) {
            let inc_id = Self::class_id(&included);
            symbols.push(Symbol {
                name: inc_id.clone(),
                symbol_type: SymbolType::PuppetClass,
                qualified_name: None,
                location: Self::loc(file, line),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({ "referenced": true }),
            });
            relations.push(Relation {
                from: class_id.clone(),
                to: inc_id,
                relation_type: RelationType::IncludesClass,
                location: Self::loc(file, line),
                metadata: json!({}),
            });
        }

        for cap in self.variable_regex.captures_iter(body) {
            let Some(var_match) = cap.get(1) else {
                continue;
            };
            let var_name = var_match.as_str();
            let var_id = format!("var::{class_name}::{var_name}");
            symbols.push(Symbol {
                name: var_id,
                symbol_type: SymbolType::PuppetVariable,
                qualified_name: None,
                location: Self::loc(file, line),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({ "value": cap.get(2).map(|m| m.as_str().trim()) }),
            });
        }

        self.extract_facts(file, line, &class_id, body, symbols, relations);
        self.extract_resources_from_body(file, line, &class_id, body, symbols, relations);
    }

    fn extract_resources_from_body(
        &self,
        file: &str,
        base_line: usize,
        owner_id: &str,
        body: &str,
        symbols: &mut Vec<Symbol>,
        relations: &mut Vec<Relation>,
    ) {
        let lines: Vec<&str> = body.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();
            if let Some(cap) = self.resource_regex.captures(line) {
                let (Some(type_match), Some(title_match)) = (cap.get(1), cap.get(2)) else {
                    i += 1;
                    continue;
                };
                let resource_type = type_match.as_str();
                let title = title_match.as_str();
                if self.is_puppet_resource(resource_type) {
                    let mut attrs = Vec::new();
                    let mut notify = Vec::new();
                    let mut require = Vec::new();
                    i += 1;
                    while i < lines.len() {
                        let attr_line = lines[i].trim();
                        if attr_line == "}" {
                            break;
                        }
                        if let Some((key, value)) = self.parse_attribute_line(attr_line) {
                            match key.as_str() {
                                "notify" => notify.push(value),
                                "require" => require.push(value),
                                _ => attrs.push(format!("{key}={value}")),
                            }
                        }
                        i += 1;
                    }
                    let res_id = Self::resource_id(owner_id, resource_type, title);
                    symbols.push(Symbol {
                        name: res_id.clone(),
                        symbol_type: SymbolType::PuppetResource,
                        qualified_name: None,
                        location: Self::loc(file, base_line + i),
                        signature: Some(attrs.join("; ")),
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: json!({
                            "resource_type": resource_type,
                            "title": title,
                        }),
                    });
                    relations.push(Relation {
                        from: owner_id.to_string(),
                        to: res_id.clone(),
                        relation_type: RelationType::DeclaresResource,
                        location: Self::loc(file, base_line + i),
                        metadata: json!({}),
                    });
                    for n in notify {
                        if let Some(target) = self.parse_resource_ref(&n, owner_id) {
                            relations.push(Relation {
                                from: res_id.clone(),
                                to: target,
                                relation_type: RelationType::NotifiesResource,
                                location: Self::loc(file, base_line + i),
                                metadata: json!({}),
                            });
                        }
                    }
                    for r in require {
                        if let Some(target) = self.parse_resource_ref(&r, owner_id) {
                            relations.push(Relation {
                                from: res_id.clone(),
                                to: target,
                                relation_type: RelationType::RequiresResource,
                                location: Self::loc(file, base_line + i),
                                metadata: json!({}),
                            });
                        }
                    }
                }
            }
            i += 1;
        }
    }

    fn extract_facts(
        &self,
        file: &str,
        line: usize,
        owner_id: &str,
        body: &str,
        symbols: &mut Vec<Symbol>,
        relations: &mut Vec<Relation>,
    ) {
        let mut seen = HashSet::new();
        for cap in self.fact_regex.captures_iter(body) {
            let fact_name = cap
                .get(1)
                .or_else(|| cap.get(2))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
            if fact_name.is_empty() || !seen.insert(fact_name.clone()) {
                continue;
            }
            let fact_id = format!("fact::{fact_name}");
            symbols.push(Symbol {
                name: fact_id.clone(),
                symbol_type: SymbolType::PuppetFact,
                qualified_name: None,
                location: Self::loc(file, line),
                signature: None,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({}),
            });
            relations.push(Relation {
                from: owner_id.to_string(),
                to: fact_id,
                relation_type: RelationType::UsesFact,
                location: Self::loc(file, line),
                metadata: json!({}),
            });
        }
    }

    fn parse_resource_ref(&self, value: &str, owner_id: &str) -> Option<String> {
        static RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
        let re =
            RE.get_or_init(|| compile_pattern(r#"(\w+(?:::\w+)*)\s*\[\s*['"]([^'"]+)['"]\s*\]"#));
        let cap = re.captures(value.trim())?;
        let resource_type = cap.get(1)?.as_str().to_ascii_lowercase();
        let title = cap.get(2)?.as_str();
        Some(Self::resource_id(owner_id, &resource_type, title))
    }

    fn extract_brace_body(&self, content: &str) -> String {
        let mut brace_count = 1;
        let mut body = String::new();
        for ch in content.chars() {
            if ch == '{' {
                brace_count += 1;
            } else if ch == '}' {
                brace_count -= 1;
                if brace_count == 0 {
                    break;
                }
            }
            body.push(ch);
        }
        body
    }

    fn parse_parameters(&self, params_str: &str) -> Vec<Parameter> {
        if params_str.is_empty() {
            return vec![];
        }
        self.param_regex
            .captures_iter(params_str)
            .filter_map(|cap| {
                let name = cap.get(2)?.as_str().to_string();
                Some(Parameter {
                    name,
                    param_type: cap.get(1).map(|m| m.as_str().to_string()),
                    default_value: cap.get(3).map(|m| m.as_str().trim().to_string()),
                })
            })
            .collect()
    }

    fn extract_includes(&self, content: &str) -> Vec<String> {
        self.include_regex
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }

    fn is_puppet_resource(&self, name: &str) -> bool {
        PUPPET_RESOURCES.contains(&name)
    }

    fn parse_attribute_line(&self, line: &str) -> Option<(String, String)> {
        let parts: Vec<&str> = line.splitn(2, "=>").collect();
        if parts.len() == 2 {
            let key = parts[0].trim().to_string();
            let value = parts[1]
                .trim()
                .trim_end_matches(',')
                .trim_matches(|c| c == '\'' || c == '"')
                .to_string();
            Some((key, value))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_puppet_path_detection() {
        assert!(PuppetParser::is_puppet_path(
            "modules/nginx/manifests/init.pp"
        ));
        assert!(PuppetParser::is_puppet_path("modules/nginx/metadata.json"));
        assert!(!PuppetParser::is_puppet_path("lib/helper.rb"));
    }

    #[test]
    fn test_include_regex_only() {
        let body = "\n  include common\n";
        let re = Regex::new(r"(?m)^\s*include\s+(?:::)?([a-zA-Z0-9_:]+)").unwrap();
        let caps: Vec<_> = re.captures_iter(body).collect();
        assert_eq!(caps.len(), 1);
        assert_eq!(caps[0].get(1).expect("regex group 1").as_str(), "common");
    }

    #[test]
    fn test_class_extraction() {
        let parser = PuppetParser::new();
        let source = r#"
class nginx ($port = 80) {
  package { 'nginx':
    ensure => installed,
  }
}
"#;
        let (symbols, relations) = parser.parse("modules/nginx/manifests/init.pp", source);
        assert!(symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::PuppetClass));
        assert!(symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::PuppetResource));
        assert!(relations
            .iter()
            .any(|r| r.relation_type == RelationType::DeclaresResource));
    }

    #[test]
    fn test_minimal_include() {
        let parser = PuppetParser::new();
        let source = "class nginx {\n  include common\n}\n";
        let (_, relations) = parser.parse("modules/nginx/manifests/init.pp", source);
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::IncludesClass),
            "{:?}",
            relations
        );
    }

    #[test]
    fn test_fixture_includes_class() {
        let parser = PuppetParser::new();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .expect("workspace root")
            .join("tests/fixtures/puppet/modules/nginx/manifests/init.pp");
        let source = std::fs::read_to_string(&path).unwrap();
        let (_, relations) = parser.parse("modules/nginx/manifests/init.pp", &source);
        assert!(
            relations
                .iter()
                .any(|r| r.relation_type == RelationType::IncludesClass),
            "missing IncludesClass in {:?}",
            relations
                .iter()
                .map(|r| r.relation_type)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_malformed_input_doesnt_panic() {
        let parser = PuppetParser::new();
        let malformed = "not valid puppet code }{}{";
        let (_symbols, _relations) = parser.parse("test.pp", malformed);
    }
}
