# Phase 17: Chef Support - Implementation Guide for Cursor

**Target**: Tier 1 Infrastructure-as-Code support for Chef  
**Timeline**: 3 weeks  
**Grade Target**: A (90%+)  
**Tests Required**: 35+  

## 🎯 Goals

Add comprehensive Chef cookbook analysis following existing Tier 1 architecture patterns. **No architecture changes allowed** - integrate seamlessly with existing graph backend, query system, and MCP server. Leverage existing Ruby parser from Phase 7.

## 📋 Prerequisites

Before starting, familiarize yourself with:
- `src/extraction/ruby.rs` - Existing Ruby tree-sitter parser (reuse this!)
- `src/extraction/ansible.rs` - Similar IaC pattern for reference
- `src/graph/schema.rs` - Where to add new NodeType/EdgeType enums
- `src/analysis/` - Existing analysis modules
- `src/mcp/tools.rs` - How to add new MCP tools

## 📁 File Structure

Create these new files:
```
src/extraction/chef.rs              # Chef DSL parser (500+ lines)
src/analysis/chef_cookbooks.rs      # Cookbook dependency analyzer (300+ lines)
src/security/chef.rs                # Security scanner (250+ lines)
src/cli/chef.rs                     # CLI commands (150+ lines)
tests/phase17_chef.rs               # Integration tests (400+ lines)
tests/fixtures/chef/                # Test cookbooks, recipes, etc.
docs/chef_support.md                # User documentation
```

Update these existing files:
```
src/graph/schema.rs                 # Add Chef node/edge types
src/extraction/mod.rs               # Register ChefParser
src/mcp/tools.rs                    # Add Chef MCP tools
src/cli/mod.rs                      # Add chef subcommand
src/lib.rs                          # Export new modules
README.md                           # Document Chef support
```

---

# Week 1: Parser & Core Extraction

## Task 17.1.1: Chef DSL Parser (Days 1-3)

### Step 1: Create Module Structure

**File**: `src/extraction/chef.rs`

```rust
//! Chef cookbook, recipe, and resource extraction.
//!
//! Parses Chef Ruby DSL and extracts:
//! - Cookbooks (recipes, resources, attributes, templates)
//! - Resources (package, service, file, template, etc.)
//! - Attributes (defaults, overrides, automatic)
//! - Dependencies (from metadata.rb)

use crate::error::Result;
use crate::extraction::ruby::RubyParser;
use crate::graph::schema::{Node, Edge, NodeType, EdgeType};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use regex::Regex;

/// Chef file type detector
#[derive(Debug, Clone, PartialEq)]
pub enum ChefFileType {
    Recipe,        // recipes/*.rb
    Attribute,     // attributes/*.rb
    Metadata,      // metadata.rb
    Resource,      // resources/*.rb (custom resources)
    Library,       // libraries/*.rb
}

/// Main Chef parser (wraps Ruby parser with Chef-specific logic)
pub struct ChefParser {
    ruby_parser: RubyParser,
    resource_regex: Regex,
    include_recipe_regex: Regex,
}

impl Default for ChefParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ChefParser {
    pub fn new() -> Self {
        Self {
            ruby_parser: RubyParser::new(),
            // Match Chef resource declarations: package 'nginx' do ... end
            resource_regex: Regex::new(
                r#"(?m)^(\w+)\s+['"]([^'"]+)['"](?:\s+do)?"#
            ).unwrap(),
            // Match include_recipe calls
            include_recipe_regex: Regex::new(
                r#"include_recipe\s+['"]([^'"]+)['"]"#
            ).unwrap(),
        }
    }
    
    /// Detect Chef file type from path
    pub fn detect_file_type(&self, path: &Path) -> Option<ChefFileType> {
        let path_str = path.to_string_lossy();
        
        if path_str.contains("/recipes/") && path_str.ends_with(".rb") {
            Some(ChefFileType::Recipe)
        } else if path_str.contains("/attributes/") && path_str.ends_with(".rb") {
            Some(ChefFileType::Attribute)
        } else if path_str.ends_with("metadata.rb") {
            Some(ChefFileType::Metadata)
        } else if path_str.contains("/resources/") && path_str.ends_with(".rb") {
            Some(ChefFileType::Resource)
        } else if path_str.contains("/libraries/") && path_str.ends_with(".rb") {
            Some(ChefFileType::Library)
        } else {
            None
        }
    }
}

/// Chef cookbook structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChefCookbook {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub dependencies: Vec<CookbookDependency>,
    pub recipes: Vec<Recipe>,
    pub attributes: HashMap<String, AttributeValue>,
    pub templates: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
    pub custom_resources: Vec<CustomResource>,
}

/// Cookbook dependency from metadata.rb
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookbookDependency {
    pub name: String,
    pub version_constraint: Option<String>,
}

/// Chef recipe
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recipe {
    pub id: Uuid,
    pub name: String,
    pub path: PathBuf,
    pub resources: Vec<ResourceDeclaration>,
    pub included_recipes: Vec<String>,
}

/// Resource declaration (package, service, file, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDeclaration {
    pub id: Uuid,
    pub resource_type: String,  // package, service, file, template, etc.
    pub name: String,
    pub properties: HashMap<String, String>,
    pub action: Vec<String>,    // [:install, :start, :create]
    pub notifies: Vec<String>,  // notification targets
    pub subscribes: Vec<String>,
}

/// Attribute value (can be nested)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    String(String),
    Number(i64),
    Boolean(bool),
    Array(Vec<AttributeValue>),
    Hash(HashMap<String, AttributeValue>),
}

/// Custom resource (LWRP/HWRP)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomResource {
    pub name: String,
    pub properties: Vec<String>,
    pub actions: Vec<String>,
}

impl ChefParser {
    /// Parse a Chef cookbook directory
    pub fn parse_cookbook(&self, cookbook_path: &Path) -> Result<ChefCookbook> {
        let name = cookbook_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        
        let metadata = self.parse_metadata(&cookbook_path.join("metadata.rb"))?;
        let recipes = self.parse_recipes_dir(&cookbook_path.join("recipes"))?;
        let attributes = self.parse_attributes_dir(&cookbook_path.join("attributes"))?;
        let templates = self.discover_files(&cookbook_path.join("templates"), &["erb"])?;
        let files = self.discover_files(&cookbook_path.join("files"), &[])?;
        let custom_resources = self.parse_resources_dir(&cookbook_path.join("resources"))?;
        
        Ok(ChefCookbook {
            name: metadata.0.unwrap_or(name),
            version: metadata.1.unwrap_or_else(|| "0.0.0".to_string()),
            path: cookbook_path.to_path_buf(),
            dependencies: metadata.2,
            recipes,
            attributes,
            templates,
            files,
            custom_resources,
        })
    }
    
    /// Parse metadata.rb
    fn parse_metadata(&self, metadata_path: &Path) -> Result<(Option<String>, Option<String>, Vec<CookbookDependency>)> {
        if !metadata_path.exists() {
            return Ok((None, None, Vec::new()));
        }
        
        let content = std::fs::read_to_string(metadata_path)?;
        
        let name_regex = Regex::new(r#"name\s+['"]([^'"]+)['"]"#).unwrap();
        let version_regex = Regex::new(r#"version\s+['"]([^'"]+)['"]"#).unwrap();
        let depends_regex = Regex::new(r#"depends\s+['"]([^'"]+)['"](?:,\s+['"]([^'"]+)['"])?"#).unwrap();
        
        let name = name_regex.captures(&content)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string());
        
        let version = version_regex.captures(&content)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string());
        
        let mut dependencies = Vec::new();
        for cap in depends_regex.captures_iter(&content) {
            let dep_name = cap.get(1).unwrap().as_str().to_string();
            let version_constraint = cap.get(2).map(|m| m.as_str().to_string());
            
            dependencies.push(CookbookDependency {
                name: dep_name,
                version_constraint,
            });
        }
        
        Ok((name, version, dependencies))
    }
    
    /// Parse recipes directory
    fn parse_recipes_dir(&self, recipes_path: &Path) -> Result<Vec<Recipe>> {
        let mut recipes = Vec::new();
        
        if !recipes_path.exists() {
            return Ok(recipes);
        }
        
        for entry in std::fs::read_dir(recipes_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|e| e.to_str()) == Some("rb") {
                if let Ok(recipe) = self.parse_recipe(&path) {
                    recipes.push(recipe);
                }
            }
        }
        
        Ok(recipes)
    }
    
    /// Parse a single recipe file
    pub fn parse_recipe(&self, recipe_path: &Path) -> Result<Recipe> {
        let content = std::fs::read_to_string(recipe_path)?;
        
        let name = recipe_path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_string();
        
        let resources = self.extract_resources(&content)?;
        let included_recipes = self.extract_included_recipes(&content);
        
        Ok(Recipe {
            id: Uuid::new_v4(),
            name,
            path: recipe_path.to_path_buf(),
            resources,
            included_recipes,
        })
    }
    
    /// Extract Chef resource declarations from recipe content
    fn extract_resources(&self, content: &str) -> Result<Vec<ResourceDeclaration>> {
        let mut resources = Vec::new();
        
        // Split into blocks (simple heuristic: find resource declarations)
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;
        
        while i < lines.len() {
            let line = lines[i].trim();
            
            // Check if this is a resource declaration
            if let Some(cap) = self.resource_regex.captures(line) {
                let resource_type = cap.get(1).unwrap().as_str();
                let resource_name = cap.get(2).unwrap().as_str();
                
                // Known Chef resources
                if self.is_chef_resource(resource_type) {
                    // Extract properties from the block
                    let mut properties = HashMap::new();
                    let mut action = Vec::new();
                    let mut notifies = Vec::new();
                    let mut subscribes = Vec::new();
                    
                    // Simple block parsing (look for 'do' ... 'end')
                    i += 1;
                    while i < lines.len() {
                        let block_line = lines[i].trim();
                        
                        if block_line == "end" {
                            break;
                        }
                        
                        // Parse property lines: key value
                        if let Some((key, value)) = self.parse_property_line(block_line) {
                            match key.as_str() {
                                "action" => {
                                    action.push(value.trim_matches(|c| c == ':' || c == '[' || c == ']').to_string());
                                }
                                "notifies" => {
                                    notifies.push(value);
                                }
                                "subscribes" => {
                                    subscribes.push(value);
                                }
                                _ => {
                                    properties.insert(key, value);
                                }
                            }
                        }
                        
                        i += 1;
                    }
                    
                    resources.push(ResourceDeclaration {
                        id: Uuid::new_v4(),
                        resource_type: resource_type.to_string(),
                        name: resource_name.to_string(),
                        properties,
                        action,
                        notifies,
                        subscribes,
                    });
                }
            }
            
            i += 1;
        }
        
        Ok(resources)
    }
    
    fn is_chef_resource(&self, name: &str) -> bool {
        matches!(
            name,
            "package" | "service" | "file" | "template" | "directory" |
            "execute" | "bash" | "script" | "user" | "group" |
            "apt_package" | "yum_package" | "cookbook_file" | "remote_file" |
            "cron" | "mount" | "link" | "git" | "subversion"
        )
    }
    
    fn parse_property_line(&self, line: &str) -> Option<(String, String)> {
        // Parse lines like: ensure :installed, owner 'root', mode '0644'
        let parts: Vec<&str> = line.splitn(2, ' ').collect();
        if parts.len() == 2 {
            let key = parts[0].trim();
            let value = parts[1].trim().trim_matches(|c| c == '\'' || c == '"');
            Some((key.to_string(), value.to_string()))
        } else {
            None
        }
    }
    
    fn extract_included_recipes(&self, content: &str) -> Vec<String> {
        self.include_recipe_regex
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }
    
    /// Parse attributes directory
    fn parse_attributes_dir(&self, attributes_path: &Path) -> Result<HashMap<String, AttributeValue>> {
        let mut attributes = HashMap::new();
        
        if !attributes_path.exists() {
            return Ok(attributes);
        }
        
        for entry in std::fs::read_dir(attributes_path)? {
            let entry = entry?;
            let path = entry.path();
            
            if path.extension().and_then(|e| e.to_str()) == Some("rb") {
                if let Ok(attrs) = self.parse_attribute_file(&path) {
                    attributes.extend(attrs);
                }
            }
        }
        
        Ok(attributes)
    }
    
    fn parse_attribute_file(&self, attr_path: &Path) -> Result<HashMap<String, AttributeValue>> {
        let content = std::fs::read_to_string(attr_path)?;
        let mut attributes = HashMap::new();
        
        // Parse attribute declarations: default['nginx']['port'] = 80
        let attr_regex = Regex::new(r#"(default|override|normal)\[['"]([^'"]+)['"]\](?:\[['"]([^'"]+)['"]\])?\s*=\s*(.+)"#).unwrap();
        
        for cap in attr_regex.captures_iter(&content) {
            let attr_name = cap.get(2).unwrap().as_str();
            let value_str = cap.get(cap.len() - 1).unwrap().as_str().trim();
            
            if let Some(value) = self.parse_attribute_value(value_str) {
                attributes.insert(attr_name.to_string(), value);
            }
        }
        
        Ok(attributes)
    }
    
    fn parse_attribute_value(&self, value_str: &str) -> Option<AttributeValue> {
        let trimmed = value_str.trim();
        
        // String
        if (trimmed.starts_with('\'') && trimmed.ends_with('\'')) ||
           (trimmed.starts_with('"') && trimmed.ends_with('"')) {
            return Some(AttributeValue::String(
                trimmed.trim_matches(|c| c == '\'' || c == '"').to_string()
            ));
        }
        
        // Number
        if let Ok(num) = trimmed.parse::<i64>() {
            return Some(AttributeValue::Number(num));
        }
        
        // Boolean
        if trimmed == "true" {
            return Some(AttributeValue::Boolean(true));
        }
        if trimmed == "false" {
            return Some(AttributeValue::Boolean(false));
        }
        
        // Default: String
        Some(AttributeValue::String(trimmed.to_string()))
    }
    
    fn parse_resources_dir(&self, resources_path: &Path) -> Result<Vec<CustomResource>> {
        // Custom resources parsing (simplified)
        Ok(Vec::new())
    }
    
    fn discover_files(&self, dir: &Path, extensions: &[&str]) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        
        if !dir.exists() {
            return Ok(files);
        }
        
        for entry in walkdir::WalkDir::new(dir) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let path = entry.path();
                if extensions.is_empty() || extensions.iter().any(|ext| {
                    path.extension().and_then(|e| e.to_str()) == Some(ext)
                }) {
                    files.push(path.to_path_buf());
                }
            }
        }
        
        Ok(files)
    }
}
```

### Step 2: Write Unit Tests

**File**: `tests/phase17_chef.rs`

```rust
use rbuilder::extraction::chef::*;

#[test]
fn test_parse_simple_recipe() {
    let recipe = r#"
package 'nginx' do
  action :install
end

service 'nginx' do
  action [:enable, :start]
  supports restart: true
end

template '/etc/nginx/nginx.conf' do
  source 'nginx.conf.erb'
  owner 'root'
  mode '0644'
  notifies :restart, 'service[nginx]'
end
"#;
    
    let parser = ChefParser::new();
    let temp_file = std::env::temp_dir().join("test_recipe.rb");
    std::fs::write(&temp_file, recipe).unwrap();
    
    let parsed = parser.parse_recipe(&temp_file).unwrap();
    
    assert_eq!(parsed.resources.len(), 3);
    
    let package = &parsed.resources[0];
    assert_eq!(package.resource_type, "package");
    assert_eq!(package.name, "nginx");
    assert!(package.action.contains(&"install".to_string()));
    
    let template = &parsed.resources[2];
    assert_eq!(template.resource_type, "template");
    assert_eq!(template.properties.get("source").map(|s| s.as_str()), Some("nginx.conf.erb"));
    assert!(!template.notifies.is_empty());
}

#[test]
fn test_parse_metadata_rb() {
    let metadata = r#"
name 'nginx'
version '1.2.0'
depends 'apt'
depends 'build-essential', '>= 8.0'
"#;
    
    let temp_file = std::env::temp_dir().join("metadata.rb");
    std::fs::write(&temp_file, metadata).unwrap();
    
    let parser = ChefParser::new();
    let (name, version, deps) = parser.parse_metadata(&temp_file).unwrap();
    
    assert_eq!(name, Some("nginx".to_string()));
    assert_eq!(version, Some("1.2.0".to_string()));
    assert_eq!(deps.len(), 2);
    assert_eq!(deps[0].name, "apt");
    assert_eq!(deps[1].version_constraint, Some(">= 8.0".to_string()));
}

#[test]
fn test_detect_file_type() {
    let parser = ChefParser::new();
    
    assert_eq!(
        parser.detect_file_type(Path::new("cookbooks/nginx/recipes/default.rb")),
        Some(ChefFileType::Recipe)
    );
    
    assert_eq!(
        parser.detect_file_type(Path::new("cookbooks/nginx/metadata.rb")),
        Some(ChefFileType::Metadata)
    );
    
    assert_eq!(
        parser.detect_file_type(Path::new("cookbooks/nginx/attributes/default.rb")),
        Some(ChefFileType::Attribute)
    );
}

#[test]
fn test_extract_included_recipes() {
    let recipe = r#"
include_recipe 'apt::default'
include_recipe 'nginx::install'

package 'nginx' do
  action :install
end
"#;
    
    let parser = ChefParser::new();
    let included = parser.extract_included_recipes(recipe);
    
    assert_eq!(included.len(), 2);
    assert!(included.contains(&"apt::default".to_string()));
    assert!(included.contains(&"nginx::install".to_string()));
}

#[test]
fn test_parse_attributes() {
    let attrs = r#"
default['nginx']['port'] = 80
default['nginx']['worker_processes'] = 4
override['nginx']['enable_ssl'] = true
"#;
    
    let temp_file = std::env::temp_dir().join("default.rb");
    std::fs::write(&temp_file, attrs).unwrap();
    
    let parser = ChefParser::new();
    let parsed = parser.parse_attribute_file(&temp_file).unwrap();
    
    assert_eq!(parsed.len(), 3);
    assert!(matches!(parsed.get("nginx"), Some(AttributeValue::String(_))));
}
```

**Acceptance Criteria**:
- [ ] Can parse Chef recipes (Ruby DSL)
- [ ] Extracts resources, properties, actions
- [ ] Parses metadata.rb for dependencies
- [ ] Handles include_recipe calls
- [ ] 10+ unit tests passing

---

## Task 17.1.2: Cookbook Dependency Analysis (Days 4-5)

**File**: `src/analysis/chef_cookbooks.rs`

```rust
//! Chef cookbook dependency analysis.

use crate::error::{Error, Result};
use crate::extraction::chef::{ChefCookbook, ChefParser};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use serde::{Serialize, Deserialize};

/// Cookbook dependency graph
#[derive(Debug, Clone)]
pub struct CookbookDependencyGraph {
    pub cookbooks: HashMap<String, CookbookNode>,
}

/// Cookbook node in dependency graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookbookNode {
    pub name: String,
    pub version: String,
    pub path: std::path::PathBuf,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
}

impl CookbookDependencyGraph {
    pub fn new() -> Self {
        Self {
            cookbooks: HashMap::new(),
        }
    }
    
    pub fn add_cookbook(&mut self, cookbook: ChefCookbook) {
        let dependencies: Vec<String> = cookbook.dependencies
            .iter()
            .map(|dep| dep.name.clone())
            .collect();
        
        let node = CookbookNode {
            name: cookbook.name.clone(),
            version: cookbook.version.clone(),
            path: cookbook.path.clone(),
            dependencies: dependencies.clone(),
            dependents: Vec::new(),
        };
        
        self.cookbooks.insert(cookbook.name.clone(), node);
        
        // Update dependents
        for dep_name in dependencies {
            if let Some(dep_node) = self.cookbooks.get_mut(&dep_name) {
                if !dep_node.dependents.contains(&cookbook.name) {
                    dep_node.dependents.push(cookbook.name.clone());
                }
            }
        }
    }
    
    pub fn get_dependencies(&self, cookbook_name: &str) -> Option<Vec<String>> {
        self.cookbooks.get(cookbook_name).map(|node| node.dependencies.clone())
    }
    
    pub fn validate_no_cycles(&self) -> Result<()> {
        for cookbook_name in self.cookbooks.keys() {
            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            if self.has_cycle(cookbook_name, &mut visited, &mut stack)? {
                return Err(Error::Analysis(format!(
                    "Circular dependency detected involving cookbook: {}",
                    cookbook_name
                )));
            }
        }
        Ok(())
    }
    
    fn has_cycle(
        &self,
        cookbook_name: &str,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
    ) -> Result<bool> {
        if stack.contains(cookbook_name) {
            return Ok(true);
        }
        if visited.contains(cookbook_name) {
            return Ok(false);
        }
        
        visited.insert(cookbook_name.to_string());
        stack.insert(cookbook_name.to_string());
        
        if let Some(node) = self.cookbooks.get(cookbook_name) {
            for dep in &node.dependencies {
                if self.has_cycle(dep, visited, stack)? {
                    return Ok(true);
                }
            }
        }
        
        stack.remove(cookbook_name);
        Ok(false)
    }
    
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();
        
        for cookbook_name in self.cookbooks.keys() {
            in_degree.insert(cookbook_name.clone(), 0);
        }
        
        for node in self.cookbooks.values() {
            for dep in &node.dependencies {
                *in_degree.entry(dep.clone()).or_insert(0) += 1;
            }
        }
        
        for (cookbook_name, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(cookbook_name.clone());
            }
        }
        
        while let Some(cookbook_name) = queue.pop_front() {
            result.push(cookbook_name.clone());
            
            if let Some(node) = self.cookbooks.get(&cookbook_name) {
                for dep in &node.dependencies {
                    if let Some(degree) = in_degree.get_mut(dep) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(dep.clone());
                        }
                    }
                }
            }
        }
        
        if result.len() != self.cookbooks.len() {
            return Err(Error::Analysis("Circular dependency detected in cookbooks".into()));
        }
        
        Ok(result)
    }
}

pub struct CookbookDependencyAnalyzer;

impl CookbookDependencyAnalyzer {
    pub fn new() -> Self {
        Self
    }
    
    pub fn analyze_cookbooks_dir(&self, cookbooks_path: &Path) -> Result<CookbookDependencyGraph> {
        let mut graph = CookbookDependencyGraph::new();
        let parser = ChefParser::new();
        
        if !cookbooks_path.exists() {
            return Ok(graph);
        }
        
        for entry in std::fs::read_dir(cookbooks_path)? {
            let entry = entry?;
            let cookbook_path = entry.path();
            
            if cookbook_path.is_dir() {
                match parser.parse_cookbook(&cookbook_path) {
                    Ok(cookbook) => graph.add_cookbook(cookbook),
                    Err(e) => eprintln!("Warning: Failed to parse cookbook {:?}: {}", cookbook_path, e),
                }
            }
        }
        
        graph.validate_no_cycles()?;
        
        Ok(graph)
    }
}
```

Add tests in `tests/phase17_chef.rs`:

```rust
#[test]
fn test_cookbook_dependency_graph() {
    // Create test fixtures
    // Test dependency detection
    // Test topological sort
}
```

---

# Week 2: Graph Integration & Security

## Task 17.2.1: Chef Node Types & Edges (Day 6)

**File**: `src/graph/schema.rs`

Add to existing enums:

```rust
pub enum NodeType {
    // ... existing types ...
    
    // Chef-specific (Phase 17)
    ChefCookbook,
    ChefRecipe,
    ChefResource,
    ChefAttribute,
    ChefTemplate,
    ChefCustomResource,
}

pub enum EdgeType {
    // ... existing types ...
    
    // Chef-specific (Phase 17)
    DependsOnCookbook,   // cookbook -> cookbook
    IncludesRecipe,      // recipe -> recipe
    DeclaresResource,    // recipe -> resource
    UsesTemplate,        // resource -> template
    DefinesAttribute,    // cookbook -> attribute
    NotifiesResource,    // resource -> resource
}
```

## Task 17.2.2: Chef Graph Construction (Days 7-9)

Add to `src/extraction/chef.rs`:

```rust
use crate::graph::backend::GraphBackend;

impl ChefParser {
    pub fn build_graph(
        &self,
        cookbook: &ChefCookbook,
        backend: &mut dyn GraphBackend,
    ) -> Result<Uuid> {
        let cookbook_node = Node::new(NodeType::ChefCookbook, cookbook.name.clone())
            .with_property("version", cookbook.version.clone())
            .with_property("path", cookbook.path.to_string_lossy().to_string());
        
        let cookbook_id = backend.insert_node(cookbook_node)?;
        
        // Add cookbook dependencies
        for dep in &cookbook.dependencies {
            let dep_node = Node::new(NodeType::ChefCookbook, dep.name.clone());
            let dep_id = backend.insert_node(dep_node)?;
            
            backend.insert_edge(Edge::new(
                cookbook_id,
                dep_id,
                EdgeType::DependsOnCookbook,
            ))?;
        }
        
        // Add recipes
        for recipe in &cookbook.recipes {
            let recipe_id = self.build_recipe_graph(recipe, backend)?;
            backend.insert_edge(Edge::new(
                cookbook_id,
                recipe_id,
                EdgeType::Contains,
            ))?;
        }
        
        Ok(cookbook_id)
    }
    
    fn build_recipe_graph(
        &self,
        recipe: &Recipe,
        backend: &mut dyn GraphBackend,
    ) -> Result<Uuid> {
        let recipe_node = Node::new(NodeType::ChefRecipe, recipe.name.clone());
        let recipe_id = backend.insert_node(recipe_node)?;
        
        // Add resources
        for resource in &recipe.resources {
            let resource_id = self.build_resource_graph(resource, backend)?;
            backend.insert_edge(Edge::new(
                recipe_id,
                resource_id,
                EdgeType::DeclaresResource,
            ))?;
        }
        
        // Link included recipes
        for included in &recipe.included_recipes {
            let included_node = Node::new(NodeType::ChefRecipe, included.clone());
            let included_id = backend.insert_node(included_node)?;
            
            backend.insert_edge(Edge::new(
                recipe_id,
                included_id,
                EdgeType::IncludesRecipe,
            ))?;
        }
        
        Ok(recipe_id)
    }
    
    fn build_resource_graph(
        &self,
        resource: &ResourceDeclaration,
        backend: &mut dyn GraphBackend,
    ) -> Result<Uuid> {
        let mut resource_node = Node::new(NodeType::ChefResource, resource.name.clone())
            .with_property("type", resource.resource_type.clone());
        
        if !resource.action.is_empty() {
            resource_node = resource_node.with_property("action", resource.action.join(","));
        }
        
        let resource_id = backend.insert_node(resource_node)?;
        
        // Link notifies/subscribes (simplified - would need to resolve references)
        
        Ok(resource_id)
    }
}
```

## Task 17.3.2: Chef Security Analysis (Days 10-12)

**File**: `src/security/chef.rs`

```rust
//! Chef security scanning for common vulnerabilities.

use crate::extraction::chef::{ChefCookbook, ResourceDeclaration};
use crate::security::{SecurityFinding, Severity};
use std::collections::HashSet;

pub struct ChefSecurityScanner {
    dangerous_resources: HashSet<String>,
}

impl Default for ChefSecurityScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl ChefSecurityScanner {
    pub fn new() -> Self {
        let dangerous_resources: HashSet<String> = [
            "execute", "bash", "script", "ruby_block",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        
        Self {
            dangerous_resources,
        }
    }
    
    pub fn scan_cookbook(&self, cookbook: &ChefCookbook) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        
        for recipe in &cookbook.recipes {
            for resource in &recipe.resources {
                findings.extend(self.scan_resource(resource, &recipe.name));
            }
        }
        
        findings
    }
    
    fn scan_resource(&self, resource: &ResourceDeclaration, recipe_name: &str) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        
        // Check for hardcoded secrets
        if let Some(finding) = self.check_hardcoded_secrets(resource, recipe_name) {
            findings.push(finding);
        }
        
        // Check for command injection in execute/bash
        if self.dangerous_resources.contains(&resource.resource_type) {
            if let Some(finding) = self.check_command_injection(resource, recipe_name) {
                findings.push(finding);
            }
        }
        
        // Check for insecure file permissions
        if resource.resource_type == "file" || resource.resource_type == "template" {
            if let Some(finding) = self.check_file_permissions(resource, recipe_name) {
                findings.push(finding);
            }
        }
        
        findings
    }
    
    fn check_hardcoded_secrets(&self, resource: &ResourceDeclaration, recipe_name: &str) -> Option<SecurityFinding> {
        let resource_str = format!("{:?}", resource.properties);
        
        let secret_patterns = ["password", "secret", "token", "api_key", "private_key"];
        
        for pattern in &secret_patterns {
            if resource_str.to_lowercase().contains(pattern) {
                if !resource_str.contains("node[") && !resource_str.contains("#{") {
                    return Some(SecurityFinding {
                        severity: Severity::High,
                        message: format!(
                            "Potential hardcoded secret in resource '{}' in recipe '{}'",
                            resource.name, recipe_name
                        ),
                        location: format!("{}:{}", recipe_name, resource.name),
                        cwe: Some("CWE-798".to_string()),
                        remediation: Some("Use Chef encrypted data bags or attributes instead".to_string()),
                    });
                }
            }
        }
        
        None
    }
    
    fn check_command_injection(&self, resource: &ResourceDeclaration, recipe_name: &str) -> Option<SecurityFinding> {
        if let Some(command) = resource.properties.get("command").or_else(|| resource.properties.get("code")) {
            if command.contains("#{") && !command.contains("Shellwords.escape") {
                return Some(SecurityFinding {
                    severity: Severity::Critical,
                    message: format!(
                        "Potential command injection in {} resource '{}' in recipe '{}'",
                        resource.resource_type, resource.name, recipe_name
                    ),
                    location: format!("{}:{}", recipe_name, resource.name),
                    cwe: Some("CWE-78".to_string()),
                    remediation: Some("Use Shellwords.escape for variable interpolation in commands".to_string()),
                });
            }
        }
        
        None
    }
    
    fn check_file_permissions(&self, resource: &ResourceDeclaration, recipe_name: &str) -> Option<SecurityFinding> {
        if let Some(mode) = resource.properties.get("mode") {
            // Check for world-writable permissions (e.g., 0777, 0666)
            if mode.contains("777") || mode.contains("666") {
                return Some(SecurityFinding {
                    severity: Severity::Medium,
                    message: format!(
                        "Insecure file permissions ({}) in resource '{}' in recipe '{}'",
                        mode, resource.name, recipe_name
                    ),
                    location: format!("{}:{}", recipe_name, resource.name),
                    cwe: Some("CWE-732".to_string()),
                    remediation: Some("Avoid world-writable permissions; use restrictive modes".to_string()),
                });
            }
        }
        
        None
    }
}
```

---

# Week 3: CLI, MCP, Testing & Documentation

## Task 17.4.1: CLI Commands (Days 13-14)

**File**: `src/cli/chef.rs`

```rust
//! Chef-specific CLI commands.

use crate::extraction::chef::ChefParser;
use crate::analysis::chef_cookbooks::CookbookDependencyAnalyzer;
use crate::security::chef::ChefSecurityScanner;
use crate::error::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct ChefArgs {
    #[command(subcommand)]
    pub command: ChefCommand,
}

#[derive(Debug, Subcommand)]
pub enum ChefCommand {
    /// Analyze Chef cookbooks and show dependencies
    Cookbooks {
        #[arg(default_value = "./cookbooks")]
        path: PathBuf,
        
        #[arg(long)]
        show_deps: bool,
        
        #[arg(long, default_value = "text")]
        format: String,
    },
    
    /// Validate Chef recipes
    Validate {
        path: PathBuf,
    },
    
    /// Run security scan on cookbooks
    SecurityScan {
        path: PathBuf,
        
        #[arg(long, default_value = "medium")]
        min_severity: String,
        
        #[arg(long, default_value = "text")]
        format: String,
    },
}

pub fn run_chef_command(args: ChefArgs) -> Result<()> {
    match args.command {
        ChefCommand::Cookbooks { path, show_deps, format } => {
            let analyzer = CookbookDependencyAnalyzer::new();
            let graph = analyzer.analyze_cookbooks_dir(&path)?;
            
            match format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&graph.cookbooks)?);
                }
                "mermaid" => {
                    print_cookbooks_mermaid(&graph);
                }
                _ => {
                    print_cookbooks_text(&graph, show_deps);
                }
            }
            
            Ok(())
        }
        
        ChefCommand::Validate { path } => {
            validate_chef_path(&path)
        }
        
        ChefCommand::SecurityScan { path, min_severity, format } => {
            run_security_scan(&path, &min_severity, &format)
        }
    }
}

fn print_cookbooks_text(graph: &crate::analysis::chef_cookbooks::CookbookDependencyGraph, show_deps: bool) {
    println!("Chef Cookbooks: {}", graph.cookbooks.len());
    println!();
    
    for (name, node) in &graph.cookbooks {
        println!("Cookbook: {} (v{})", name, node.version);
        
        if show_deps {
            if !node.dependencies.is_empty() {
                println!("  Dependencies:");
                for dep in &node.dependencies {
                    println!("    - {}", dep);
                }
            }
        }
        
        println!();
    }
}

fn print_cookbooks_mermaid(graph: &crate::analysis::chef_cookbooks::CookbookDependencyGraph) {
    println!("graph TD");
    
    for (name, node) in &graph.cookbooks {
        for dep in &node.dependencies {
            println!("    {}[{}] --> {}[{}]", name, name, dep, dep);
        }
    }
}

fn validate_chef_path(path: &PathBuf) -> Result<()> {
    let parser = ChefParser::new();
    
    if path.is_file() {
        match parser.parse_recipe(path) {
            Ok(recipe) => {
                println!("✓ Valid recipe: {}", recipe.name);
                println!("  Resources: {}", recipe.resources.len());
            }
            Err(e) => {
                eprintln!("✗ Invalid recipe: {}", e);
                return Err(e);
            }
        }
    } else if path.is_dir() {
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let file_path = entry.path();
                if file_path.extension().and_then(|e| e.to_str()) == Some("rb") {
                    if file_path.to_string_lossy().contains("/recipes/") {
                        match parser.parse_recipe(file_path) {
                            Ok(recipe) => {
                                println!("✓ {}: {} resources", file_path.display(), recipe.resources.len());
                            }
                            Err(e) => {
                                eprintln!("✗ {}: {}", file_path.display(), e);
                            }
                        }
                    }
                }
            }
        }
    }
    
    Ok(())
}

fn run_security_scan(path: &PathBuf, min_severity: &str, format: &str) -> Result<()> {
    let parser = ChefParser::new();
    let scanner = ChefSecurityScanner::new();
    let mut all_findings = Vec::new();
    
    let cookbooks = if path.is_dir() && path.join("metadata.rb").exists() {
        vec![parser.parse_cookbook(path)?]
    } else if path.is_dir() {
        let mut cookbooks = Vec::new();
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let cookbook_path = entry.path();
            if cookbook_path.is_dir() {
                if let Ok(cookbook) = parser.parse_cookbook(&cookbook_path) {
                    cookbooks.push(cookbook);
                }
            }
        }
        cookbooks
    } else {
        vec![]
    };
    
    for cookbook in &cookbooks {
        let findings = scanner.scan_cookbook(cookbook);
        all_findings.extend(findings);
    }
    
    // Filter and output
    let min_sev = parse_severity(min_severity);
    all_findings.retain(|f| f.severity >= min_sev);
    
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&all_findings)?);
        }
        _ => {
            if all_findings.is_empty() {
                println!("✓ No security issues found");
            } else {
                println!("Security Findings: {}", all_findings.len());
                for finding in &all_findings {
                    println!("[{:?}] {}", finding.severity, finding.message);
                    println!("  Location: {}", finding.location);
                    if let Some(rem) = &finding.remediation {
                        println!("  Remediation: {}", rem);
                    }
                    println!();
                }
            }
        }
    }
    
    Ok(())
}

fn parse_severity(s: &str) -> crate::security::Severity {
    match s.to_lowercase().as_str() {
        "critical" => crate::security::Severity::Critical,
        "high" => crate::security::Severity::High,
        "medium" => crate::security::Severity::Medium,
        _ => crate::security::Severity::Low,
    }
}
```

## Task 17.4.2: MCP Tools (Day 15)

Update `src/mcp/tools.rs`:

```rust
// Add Chef MCP tools

tools.push(Tool {
    name: "analyze_chef_cookbook".to_string(),
    description: "Analyze Chef cookbook structure, recipes, resources, and dependencies".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "cookbook_path": {
                "type": "string",
                "description": "Path to the Chef cookbook directory"
            }
        },
        "required": ["cookbook_path"]
    }),
});

tools.push(Tool {
    name: "find_chef_recipes".to_string(),
    description: "Find and analyze Chef recipes with resource details".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "cookbook_path": {
                "type": "string",
                "description": "Path to cookbook directory"
            }
        },
        "required": ["cookbook_path"]
    }),
});

tools.push(Tool {
    name: "chef_security_scan".to_string(),
    description: "Scan Chef cookbooks for security vulnerabilities".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "cookbooks_path": {
                "type": "string",
                "description": "Path to cookbooks directory"
            },
            "min_severity": {
                "type": "string",
                "default": "medium"
            }
        },
        "required": ["cookbooks_path"]
    }),
});
```

## Task 17.5.2: Documentation (Days 19-21)

**File**: `docs/chef_support.md`

```markdown
# Chef Support

rBuilder provides comprehensive support for analyzing Chef cookbooks, recipes, and resources.

## Features

- **Cookbook Parsing**: Extract recipes, resources, attributes from Chef cookbooks
- **Dependency Analysis**: Analyze cookbook dependencies and detect circular dependencies
- **Resource Tracking**: Track Chef resource declarations and relationships
- **Security Scanning**: Detect hardcoded secrets, command injection, insecure permissions
- **Graph Integration**: Build dependency graphs of your Chef infrastructure

## Supported Chef Versions

- Chef 14+
- Chef Infra Client 15+

## CLI Usage

### Analyze Cookbooks

```bash
rbuilder chef cookbooks --show-deps
rbuilder chef cookbooks --format mermaid > cookbooks.mmd
```

### Validate Recipes

```bash
rbuilder chef validate cookbooks/nginx
```

### Security Scan

```bash
rbuilder chef security-scan cookbooks/
rbuilder chef security-scan . --min-severity high --format json
```

## Query Examples

```bash
rbuilder query "type:ChefCookbook"
rbuilder query "type:ChefResource resource_type:package"
rbuilder analyze blast-radius "cookbooks/nginx"
```

## Security Checks

1. **CWE-798**: Hardcoded Secrets
2. **CWE-78**: Command Injection (execute/bash resources)
3. **CWE-732**: Insecure File Permissions

## Limitations

- Custom resources (LWRPs/HWRPs) have basic support
- Dynamic attributes (lazy evaluation) not fully resolved
- Chef InSpec integration not included
```

---

# Final Checklist

## Week 1 Deliverables
- [ ] `src/extraction/chef.rs` (500+ lines)
- [ ] `src/analysis/chef_cookbooks.rs` (300+ lines)
- [ ] 15+ unit tests

## Week 2 Deliverables
- [ ] NodeType/EdgeType updated
- [ ] Graph construction complete
- [ ] `src/security/chef.rs` (250+ lines)
- [ ] 10+ integration tests

## Week 3 Deliverables
- [ ] `src/cli/chef.rs` (150+ lines)
- [ ] MCP tools added
- [ ] `docs/chef_support.md`
- [ ] README.md updated

## Overall
- [ ] 35+ tests total
- [ ] Grade A (90%+)
- [ ] No architecture changes
- [ ] All features working
