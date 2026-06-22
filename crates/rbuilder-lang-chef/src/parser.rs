//! Chef cookbook DSL parsing — recipes, resources, attributes, templates.

use rbuilder_plugin_api::*;
use regex::Regex;
use serde_json::json;
use std::collections::HashSet;

const CHEF_RESOURCES: &[&str] = &[
    "package",
    "service",
    "file",
    "template",
    "directory",
    "execute",
    "bash",
    "script",
    "user",
    "group",
    "apt_package",
    "yum_package",
    "cookbook_file",
    "remote_file",
    "cron",
    "mount",
    "link",
    "git",
    "subversion",
    "ruby_block",
    "powershell_script",
    "windows_service",
];

type ParsedResource = (
    String,
    String,
    std::collections::HashMap<String, String>,
    String,
    Vec<String>,
);

fn compile_pattern(pattern: &str) -> Regex {
    Regex::new(pattern).unwrap()
}

/// Chef Ruby DSL parser producing plugin symbols and relations.
pub struct ChefParser {
    resource_regex: Regex,
    include_recipe_regex: Regex,
    depends_regex: Regex,
    name_regex: Regex,
    version_regex: Regex,
    attribute_regex: Regex,
    erb_var_regex: Regex,
}

impl Default for ChefParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ChefParser {
    /// Create a new Chef parser.
    pub fn new() -> Self {
        Self {
            resource_regex: compile_pattern(r#"(?m)^(\w+)\s+['"]([^'"]+)['"](?:\s+do)?"#),
            include_recipe_regex: compile_pattern(r#"include_recipe\s+['"]([^'"]+)['"]"#),
            depends_regex: compile_pattern(
                r#"depends\s+['"]([^'"]+)['"](?:,\s+['"]([^'"]+)['"])?"#,
            ),
            name_regex: compile_pattern(r#"name\s+['"]([^'"]+)['"]"#),
            version_regex: compile_pattern(r#"version\s+['"]([^'"]+)['"]"#),
            attribute_regex: compile_pattern(r#"default\['([^']+)'\]\s*=\s*(.+)"#),
            erb_var_regex: compile_pattern(r#"(?:#\{([^}]+)\}|<%=?\s*@?node\[['"]([^'"]+)['"]\])"#),
        }
    }

    /// Whether a file path should be handled by the Chef plugin.
    pub fn is_chef_path(path_str: &str) -> bool {
        let p = path_str.replace('\\', "/");
        if p.ends_with(".erb") {
            return p.contains("/templates/")
                || p.starts_with("templates/")
                || p.contains("/cookbooks/")
                || p.starts_with("cookbooks/");
        }
        if !p.ends_with(".rb") {
            return false;
        }
        p.ends_with("metadata.rb")
            || p.contains("/recipes/")
            || p.starts_with("recipes/")
            || p.contains("/attributes/")
            || p.starts_with("attributes/")
            || p.contains("/resources/")
            || p.starts_with("resources/")
            || (p.contains("/cookbooks/") || p.starts_with("cookbooks/"))
    }

    /// Parse a Chef file into symbols and relations.
    pub fn parse(&self, file: &str, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
        if file.ends_with(".erb") {
            return self.parse_template(file, source);
        }
        if file.ends_with("metadata.rb") {
            return self.parse_metadata(file, source);
        }
        if file.contains("/recipes/") || file.starts_with("recipes/") {
            return self.parse_recipe(file, source);
        }
        if file.contains("/attributes/") || file.starts_with("attributes/") {
            return self.parse_attributes(file, source);
        }
        if file.contains("/resources/") || file.starts_with("resources/") {
            return self.parse_custom_resource(file, source);
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

    fn cookbook_name_from_path(file: &str) -> String {
        let p = file.replace('\\', "/");
        if let Some(idx) = p.find("/cookbooks/") {
            let rest = &p[idx + 11..];
            if let Some(end) = rest.find('/') {
                return rest[..end].to_string();
            }
        }
        if let Some(stripped) = p.strip_prefix("cookbooks/") {
            if let Some(end) = stripped.find('/') {
                return stripped[..end].to_string();
            }
        }
        for segment in ["recipes", "attributes", "templates", "resources"] {
            if let Some(idx) = p.find(&format!("/{segment}/")) {
                let before = &p[..idx];
                if let Some(name) = before.rsplit('/').next() {
                    if name != "cookbooks" && !name.is_empty() {
                        return name.to_string();
                    }
                }
            }
        }
        if p.ends_with("metadata.rb") {
            if let Some(parent) = std::path::Path::new(&p)
                .parent()
                .and_then(|x| x.file_name())
            {
                return parent.to_string_lossy().to_string();
            }
        }
        "cookbook".to_string()
    }

    fn parse_metadata(&self, file: &str, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let cb_name = self
            .name_regex
            .captures(source)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| Self::cookbook_name_from_path(file));
        let version = self
            .version_regex
            .captures(source)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string())
            .unwrap_or_else(|| "0.0.0".to_string());

        let cookbook_id = format!("cookbook::{cb_name}");
        symbols.push(Symbol {
            name: cookbook_id.clone(),
            symbol_type: SymbolType::ChefCookbook,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "version": version, "cookbook": cb_name }),
        });

        for cap in self.depends_regex.captures_iter(source) {
            let Some(dep_match) = cap.get(1) else {
                continue;
            };
            let dep = dep_match.as_str().to_string();
            let dep_id = format!("cookbook::{dep}");
            symbols.push(Symbol {
                name: dep_id.clone(),
                symbol_type: SymbolType::ChefCookbook,
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
                from: cookbook_id.clone(),
                to: dep_id,
                relation_type: RelationType::DependsOnCookbook,
                location: Self::loc(file, 1),
                metadata: json!({}),
            });
        }

        (symbols, relations)
    }

    fn parse_recipe(&self, file: &str, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let cb_name = Self::cookbook_name_from_path(file);
        let recipe_name = std::path::Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("default");
        let cookbook_id = format!("cookbook::{cb_name}");
        let recipe_id = format!("{cookbook_id}::recipe::{recipe_name}");

        symbols.push(Symbol {
            name: cookbook_id.clone(),
            symbol_type: SymbolType::ChefCookbook,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "cookbook": cb_name }),
        });

        symbols.push(Symbol {
            name: recipe_id.clone(),
            symbol_type: SymbolType::ChefRecipe,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "cookbook": cb_name }),
        });

        relations.push(Relation {
            from: cookbook_id,
            to: recipe_id.clone(),
            relation_type: RelationType::Defines,
            location: Self::loc(file, 1),
            metadata: json!({}),
        });

        for included in self.extract_included_recipes(source) {
            let target = if included.contains("::") {
                format!("cookbook::{included}")
            } else {
                format!("cookbook::{cb_name}::recipe::{included}")
            };
            symbols.push(Symbol {
                name: target.clone(),
                symbol_type: SymbolType::ChefRecipe,
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
                from: recipe_id.clone(),
                to: target,
                relation_type: RelationType::IncludesRecipe,
                location: Self::loc(file, 1),
                metadata: json!({}),
            });
        }

        let resources = self.extract_resources(source);
        for (idx, res) in resources.into_iter().enumerate() {
            let resource_id = format!("{recipe_id}::resource::{}::{}::{idx}", res.0, res.1);
            let props_json = serde_json::to_string(&res.2).unwrap_or_default();
            symbols.push(Symbol {
                name: resource_id.clone(),
                symbol_type: SymbolType::ChefResource,
                qualified_name: None,
                location: Self::loc(file, 1),
                signature: Some(props_json),
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({
                    "resource_type": res.0,
                    "action": res.3,
                    "cookbook": cb_name,
                    "recipe": recipe_name,
                }),
            });
            relations.push(Relation {
                from: recipe_id.clone(),
                to: resource_id.clone(),
                relation_type: RelationType::DeclaresResource,
                location: Self::loc(file, 1),
                metadata: json!({}),
            });

            if res.0 == "template" {
                if let Some(src) = res.2.get("source") {
                    let template_id = format!("template::{cb_name}::{src}");
                    symbols.push(Symbol {
                        name: template_id.clone(),
                        symbol_type: SymbolType::ChefTemplate,
                        qualified_name: None,
                        location: Self::loc(file, 1),
                        signature: None,
                        return_type: None,
                        parameters: vec![],
                        fields: vec![],
                        modifiers: vec![],
                        documentation: None,
                        metadata: json!({ "referenced": true, "cookbook": cb_name }),
                    });
                    relations.push(Relation {
                        from: resource_id.clone(),
                        to: template_id,
                        relation_type: RelationType::UsesTemplate,
                        location: Self::loc(file, 1),
                        metadata: json!({}),
                    });
                }
            }

            for notify in &res.4 {
                relations.push(Relation {
                    from: resource_id.clone(),
                    to: format!("{recipe_id}::notify::{notify}"),
                    relation_type: RelationType::NotifiesResource,
                    location: Self::loc(file, 1),
                    metadata: json!({ "notify": notify }),
                });
            }
        }

        (symbols, relations)
    }

    fn parse_attributes(&self, file: &str, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let cb_name = Self::cookbook_name_from_path(file);
        let cookbook_id = format!("cookbook::{cb_name}");

        symbols.push(Symbol {
            name: cookbook_id.clone(),
            symbol_type: SymbolType::ChefCookbook,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "cookbook": cb_name }),
        });

        for cap in self.attribute_regex.captures_iter(source) {
            let Some(attr_match) = cap.get(1) else {
                continue;
            };
            let attr_name = attr_match.as_str().to_string();
            let value = cap.get(2).map(|m| m.as_str().trim().to_string());
            let attr_id = format!("attribute::{cb_name}::{attr_name}");
            symbols.push(Symbol {
                name: attr_id.clone(),
                symbol_type: SymbolType::ChefAttribute,
                qualified_name: None,
                location: Self::loc(file, 1),
                signature: value,
                return_type: None,
                parameters: vec![],
                fields: vec![],
                modifiers: vec![],
                documentation: None,
                metadata: json!({ "cookbook": cb_name, "attribute": attr_name }),
            });
            relations.push(Relation {
                from: cookbook_id.clone(),
                to: attr_id,
                relation_type: RelationType::DefinesAttribute,
                location: Self::loc(file, 1),
                metadata: json!({}),
            });
        }

        (symbols, relations)
    }

    fn parse_custom_resource(&self, file: &str, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let cb_name = Self::cookbook_name_from_path(file);
        let resource_name = std::path::Path::new(file)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("resource");
        let id = format!("cookbook::{cb_name}::custom_resource::{resource_name}");

        symbols.push(Symbol {
            name: id,
            symbol_type: SymbolType::ChefCustomResource,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: Some(source.lines().take(5).collect::<Vec<_>>().join("\n")),
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "cookbook": cb_name }),
        });

        (symbols, vec![])
    }

    fn parse_template(&self, file: &str, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
        let mut symbols = Vec::new();
        let mut relations = Vec::new();
        let name = std::path::Path::new(file)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("template.erb")
            .to_string();
        let cb_name = Self::cookbook_name_from_path(file);
        let template_id = format!("template::{cb_name}::{name}");

        symbols.push(Symbol {
            name: template_id.clone(),
            symbol_type: SymbolType::ChefTemplate,
            qualified_name: None,
            location: Self::loc(file, 1),
            signature: None,
            return_type: None,
            parameters: vec![],
            fields: vec![],
            modifiers: vec![],
            documentation: None,
            metadata: json!({ "cookbook": cb_name, "template": true }),
        });

        for var in self.extract_erb_vars(source) {
            let attr_id = format!("attribute::{cb_name}::{var}");
            if !symbols.iter().any(|s| s.name == attr_id) {
                symbols.push(Symbol {
                    name: attr_id.clone(),
                    symbol_type: SymbolType::ChefAttribute,
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
                from: template_id.clone(),
                to: attr_id,
                relation_type: RelationType::References,
                location: Self::loc(file, 1),
                metadata: json!({}),
            });
        }

        (symbols, relations)
    }

    fn extract_included_recipes(&self, source: &str) -> Vec<String> {
        self.include_recipe_regex
            .captures_iter(source)
            .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
            .collect()
    }

    fn extract_erb_vars(&self, source: &str) -> Vec<String> {
        let mut vars = HashSet::new();
        for cap in self.erb_var_regex.captures_iter(source) {
            if let Some(m) = cap.get(1) {
                vars.insert(m.as_str().trim().to_string());
            }
            if let Some(m) = cap.get(2) {
                vars.insert(m.as_str().to_string());
            }
        }
        vars.into_iter().collect()
    }

    fn is_chef_resource(&self, name: &str) -> bool {
        CHEF_RESOURCES.contains(&name)
    }

    fn extract_resources(&self, source: &str) -> Vec<ParsedResource> {
        let mut resources = Vec::new();
        let lines: Vec<&str> = source.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();
            if let Some(cap) = self.resource_regex.captures(line) {
                let (Some(type_match), Some(name_match)) = (cap.get(1), cap.get(2)) else {
                    i += 1;
                    continue;
                };
                let resource_type = type_match.as_str();
                let resource_name = name_match.as_str();
                if self.is_chef_resource(resource_type) {
                    let mut properties = std::collections::HashMap::new();
                    let mut action = String::new();
                    let mut notifies = Vec::new();
                    i += 1;
                    while i < lines.len() {
                        let block_line = lines[i].trim();
                        if block_line == "end" {
                            break;
                        }
                        if let Some((key, value)) = self.parse_property_line(block_line) {
                            match key.as_str() {
                                "action" => {
                                    action = value
                                        .trim_matches(|c: char| c == ':' || c == '[' || c == ']')
                                        .to_string();
                                }
                                "notifies" => notifies.push(value),
                                "subscribes" => {}
                                _ => {
                                    properties.insert(key, value);
                                }
                            }
                        }
                        i += 1;
                    }
                    resources.push((
                        resource_type.to_string(),
                        resource_name.to_string(),
                        properties,
                        action,
                        notifies,
                    ));
                }
            }
            i += 1;
        }
        resources
    }

    fn parse_property_line(&self, line: &str) -> Option<(String, String)> {
        let line = line.trim().trim_end_matches(',');
        if let Some((key, value)) = line.split_once(' ') {
            let key = key.trim().to_string();
            let value = value
                .trim()
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
    fn test_chef_path_detection() {
        assert!(ChefParser::is_chef_path(
            "cookbooks/nginx/recipes/default.rb"
        ));
        assert!(ChefParser::is_chef_path("cookbooks/nginx/metadata.rb"));
        assert!(ChefParser::is_chef_path(
            "cookbooks/nginx/templates/app.erb"
        ));
        assert!(!ChefParser::is_chef_path("lib/helper.rb"));
    }

    #[test]
    fn test_recipe_resource_extraction() {
        let parser = ChefParser::new();
        let source = r#"
package 'nginx' do
  action :install
end

service 'nginx' do
  action [:enable, :start]
  notifies :restart, 'service[nginx]'
end
"#;
        let (symbols, relations) = parser.parse("cookbooks/nginx/recipes/default.rb", source);
        assert!(symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::ChefRecipe));
        assert!(symbols
            .iter()
            .any(|s| s.symbol_type == SymbolType::ChefResource));
        assert!(relations
            .iter()
            .any(|r| r.relation_type == RelationType::DeclaresResource));
    }

    #[test]
    fn test_metadata_dependencies() {
        let parser = ChefParser::new();
        let source = r#"
name 'nginx'
version '1.0.0'
depends 'apt'
depends 'build-essential', '~> 5.0'
"#;
        let (symbols, relations) = parser.parse("cookbooks/nginx/metadata.rb", source);
        assert!(symbols.iter().any(|s| s.name == "cookbook::nginx"));
        assert!(relations
            .iter()
            .any(|r| r.relation_type == RelationType::DependsOnCookbook));
        assert_eq!(relations.len(), 2);
    }

    #[test]
    fn test_malformed_input_doesnt_panic() {
        let parser = ChefParser::new();
        let malformed = "not valid chef code }{}{";
        let (symbols, _) = parser.parse("test.rb", malformed);
        assert!(symbols.is_empty());
    }
}
