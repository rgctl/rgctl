# Phase 18: Puppet Support - Implementation Guide for Cursor

**Target**: Tier 1 Infrastructure-as-Code support for Puppet  
**Timeline**: 3 weeks  
**Grade Target**: A (90%+)  
**Tests Required**: 35+  

## 🎯 Goals

Add comprehensive Puppet manifest analysis following existing Tier 1 architecture patterns. **No architecture changes allowed** - integrate seamlessly with existing graph backend, query system, and MCP server. Custom regex-based parser (no tree-sitter grammar available).

## 📋 Prerequisites

Before starting, familiarize yourself with:
- `src/extraction/ansible.rs` - Similar IaC pattern for reference
- `src/extraction/chef.rs` - Another IaC parser with custom parsing
- `src/graph/schema.rs` - Where to add new NodeType/EdgeType enums
- `src/analysis/` - Existing analysis modules
- Puppet DSL syntax (classes, resources, defined types)

## 📁 File Structure

Create these new files:
```
src/extraction/puppet.rs            # Puppet DSL parser (600+ lines)
src/analysis/puppet_modules.rs      # Module dependency analyzer (300+ lines)
src/security/puppet.rs              # Security scanner (250+ lines)
src/cli/puppet.rs                   # CLI commands (150+ lines)
tests/phase18_puppet.rs             # Integration tests (400+ lines)
tests/fixtures/puppet/              # Test modules, manifests, etc.
docs/puppet_support.md              # User documentation
```

Update these existing files:
```
src/graph/schema.rs                 # Add Puppet node/edge types
src/extraction/mod.rs               # Register PuppetParser
src/mcp/tools.rs                    # Add Puppet MCP tools
src/cli/mod.rs                      # Add puppet subcommand
src/lib.rs                          # Export new modules
README.md                           # Document Puppet support
```

---

# Week 1: Parser & Core Extraction

## Task 18.1.1: Puppet DSL Parser (Days 1-4)

### Step 1: Create Module Structure

**File**: `src/extraction/puppet.rs`

```rust
//! Puppet manifest, module, and resource extraction.
//!
//! Parses Puppet DSL (.pp files) and extracts:
//! - Modules (classes, defined types, resources)
//! - Classes (parameters, inheritance, resources)
//! - Resources (file, package, service, exec, etc.)
//! - Variables and facts

use crate::error::Result;
use crate::graph::schema::{Node, Edge, NodeType, EdgeType};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use regex::Regex;

/// Puppet file type detector
#[derive(Debug, Clone, PartialEq)]
pub enum PuppetFileType {
    Manifest,      // .pp files
    Metadata,      // metadata.json
}

/// Main Puppet parser (regex-based, no tree-sitter available)
pub struct PuppetParser {
    class_regex: Regex,
    resource_regex: Regex,
    include_regex: Regex,
    variable_regex: Regex,
}

impl Default for PuppetParser {
    fn default() -> Self {
        Self::new()
    }
}

impl PuppetParser {
    pub fn new() -> Self {
        Self {
            // Match: class name (params) inherits parent { ... }
            class_regex: Regex::new(
                r"(?ms)class\s+([a-zA-Z0-9_:]+)\s*(?:\((.*?)\))?\s*(?:inherits\s+([a-zA-Z0-9_:]+))?\s*\{"
            ).unwrap(),
            
            // Match: resource_type { 'title': ... }
            resource_regex: Regex::new(
                r#"(?m)^(\w+)\s*\{\s*['"]([^'"]+)['"]:"#
            ).unwrap(),
            
            // Match: include ::class_name
            include_regex: Regex::new(
                r"(?:include|require|contain)\s+::?([a-zA-Z0-9_:]+)"
            ).unwrap(),
            
            // Match: $variable = value
            variable_regex: Regex::new(
                r"\$([a-zA-Z0-9_]+)\s*=\s*(.+)"
            ).unwrap(),
        }
    }
    
    /// Detect Puppet file type from path
    pub fn detect_file_type(&self, path: &Path) -> Option<PuppetFileType> {
        if path.extension().and_then(|e| e.to_str()) == Some("pp") {
            Some(PuppetFileType::Manifest)
        } else if path.file_name().and_then(|n| n.to_str()) == Some("metadata.json") {
            Some(PuppetFileType::Metadata)
        } else {
            None
        }
    }
}

/// Puppet module structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuppetModule {
    pub name: String,
    pub version: String,
    pub path: PathBuf,
    pub dependencies: Vec<ModuleDependency>,
    pub classes: Vec<PuppetClass>,
    pub defined_types: Vec<DefinedType>,
    pub manifests: Vec<PathBuf>,
}

/// Module dependency from metadata.json
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDependency {
    pub name: String,
    pub version_requirement: Option<String>,
}

/// Puppet class
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PuppetClass {
    pub id: Uuid,
    pub name: String,
    pub params: Vec<Parameter>,
    pub resources: Vec<ResourceDeclaration>,
    pub included_classes: Vec<String>,
    pub inherits: Option<String>,
}

/// Class parameter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub name: String,
    pub type_annotation: Option<String>,
    pub default_value: Option<String>,
}

/// Defined type (custom resource type)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefinedType {
    pub id: Uuid,
    pub name: String,
    pub params: Vec<Parameter>,
    pub resources: Vec<ResourceDeclaration>,
}

/// Resource declaration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceDeclaration {
    pub id: Uuid,
    pub resource_type: String,  // file, package, service, exec, etc.
    pub title: String,
    pub attributes: HashMap<String, String>,
    pub notify: Vec<String>,
    pub require: Vec<String>,
    pub subscribe: Vec<String>,
}

impl PuppetParser {
    /// Parse a Puppet module directory
    pub fn parse_module(&self, module_path: &Path) -> Result<PuppetModule> {
        let name = module_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        
        let metadata = self.parse_metadata(&module_path.join("metadata.json"))?;
        let manifests = self.discover_manifests(&module_path.join("manifests"))?;
        let classes = self.parse_manifests(&manifests)?;
        
        Ok(PuppetModule {
            name: metadata.0.unwrap_or(name),
            version: metadata.1.unwrap_or_else(|| "0.0.0".to_string()),
            path: module_path.to_path_buf(),
            dependencies: metadata.2,
            classes,
            defined_types: Vec::new(), // TODO: parse defined types
            manifests,
        })
    }
    
    /// Parse metadata.json
    fn parse_metadata(&self, metadata_path: &Path) -> Result<(Option<String>, Option<String>, Vec<ModuleDependency>)> {
        if !metadata_path.exists() {
            return Ok((None, None, Vec::new()));
        }
        
        let content = std::fs::read_to_string(metadata_path)?;
        let json: JsonValue = serde_json::from_str(&content)?;
        
        let name = json.get("name")
            .and_then(|v| v.as_str())
            .map(String::from);
        
        let version = json.get("version")
            .and_then(|v| v.as_str())
            .map(String::from);
        
        let mut dependencies = Vec::new();
        if let Some(deps_array) = json.get("dependencies").and_then(|v| v.as_array()) {
            for dep in deps_array {
                if let Some(dep_name) = dep.get("name").and_then(|v| v.as_str()) {
                    let version_req = dep.get("version_requirement")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    
                    dependencies.push(ModuleDependency {
                        name: dep_name.to_string(),
                        version_requirement: version_req,
                    });
                }
            }
        }
        
        Ok((name, version, dependencies))
    }
    
    fn discover_manifests(&self, manifests_path: &Path) -> Result<Vec<PathBuf>> {
        let mut manifests = Vec::new();
        
        if !manifests_path.exists() {
            return Ok(manifests);
        }
        
        for entry in walkdir::WalkDir::new(manifests_path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("pp") {
                    manifests.push(path.to_path_buf());
                }
            }
        }
        
        Ok(manifests)
    }
    
    fn parse_manifests(&self, manifest_paths: &[PathBuf]) -> Result<Vec<PuppetClass>> {
        let mut all_classes = Vec::new();
        
        for manifest_path in manifest_paths {
            match self.parse_manifest(manifest_path) {
                Ok(mut classes) => all_classes.append(&mut classes),
                Err(e) => eprintln!("Warning: Failed to parse manifest {:?}: {}", manifest_path, e),
            }
        }
        
        Ok(all_classes)
    }
    
    /// Parse a single manifest file
    pub fn parse_manifest(&self, manifest_path: &Path) -> Result<Vec<PuppetClass>> {
        let content = std::fs::read_to_string(manifest_path)?;
        self.extract_classes(&content)
    }
    
    fn extract_classes(&self, content: &str) -> Result<Vec<PuppetClass>> {
        let mut classes = Vec::new();
        
        for cap in self.class_regex.captures_iter(content) {
            let class_name = cap.get(1).unwrap().as_str().to_string();
            let params_str = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let inherits = cap.get(3).map(|m| m.as_str().to_string());
            
            // Extract class body (find matching braces)
            let class_start = cap.get(0).unwrap().end();
            let class_body = self.extract_class_body(&content[class_start..])?;
            
            let params = self.parse_parameters(params_str);
            let resources = self.extract_resources(&class_body)?;
            let included_classes = self.extract_includes(&class_body);
            
            classes.push(PuppetClass {
                id: Uuid::new_v4(),
                name: class_name,
                params,
                resources,
                included_classes,
                inherits,
            });
        }
        
        Ok(classes)
    }
    
    fn extract_class_body(&self, content: &str) -> Result<String> {
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
        
        Ok(body)
    }
    
    fn parse_parameters(&self, params_str: &str) -> Vec<Parameter> {
        let mut params = Vec::new();
        
        if params_str.is_empty() {
            return params;
        }
        
        // Simple parameter parsing: $name = default, $type $name = default
        let param_regex = Regex::new(r"(?:(\w+)\s+)?\$([a-zA-Z0-9_]+)\s*(?:=\s*([^,]+))?").unwrap();
        
        for cap in param_regex.captures_iter(params_str) {
            let type_annotation = cap.get(1).map(|m| m.as_str().to_string());
            let name = cap.get(2).unwrap().as_str().to_string();
            let default_value = cap.get(3).map(|m| m.as_str().trim().to_string());
            
            params.push(Parameter {
                name,
                type_annotation,
                default_value,
            });
        }
        
        params
    }
    
    fn extract_resources(&self, content: &str) -> Result<Vec<ResourceDeclaration>> {
        let mut resources = Vec::new();
        let lines: Vec<&str> = content.lines().collect();
        let mut i = 0;
        
        while i < lines.len() {
            let line = lines[i].trim();
            
            if let Some(cap) = self.resource_regex.captures(line) {
                let resource_type = cap.get(1).unwrap().as_str();
                let title = cap.get(2).unwrap().as_str();
                
                // Known Puppet resources
                if self.is_puppet_resource(resource_type) {
                    // Extract attributes from the block
                    let mut attributes = HashMap::new();
                    let mut notify = Vec::new();
                    let mut require = Vec::new();
                    let mut subscribe = Vec::new();
                    
                    i += 1;
                    while i < lines.len() {
                        let attr_line = lines[i].trim();
                        
                        if attr_line == "}" {
                            break;
                        }
                        
                        // Parse attribute lines: key => value,
                        if let Some((key, value)) = self.parse_attribute_line(attr_line) {
                            match key.as_str() {
                                "notify" => notify.push(value),
                                "require" => require.push(value),
                                "subscribe" => subscribe.push(value),
                                _ => {
                                    attributes.insert(key, value);
                                }
                            }
                        }
                        
                        i += 1;
                    }
                    
                    resources.push(ResourceDeclaration {
                        id: Uuid::new_v4(),
                        resource_type: resource_type.to_string(),
                        title: title.to_string(),
                        attributes,
                        notify,
                        require,
                        subscribe,
                    });
                }
            }
            
            i += 1;
        }
        
        Ok(resources)
    }
    
    fn is_puppet_resource(&self, name: &str) -> bool {
        matches!(
            name,
            "file" | "package" | "service" | "exec" | "user" | "group" |
            "cron" | "mount" | "host" | "notify" | "firewall" | "yumrepo" |
            "apt::source" | "systemd::unit"
        )
    }
    
    fn parse_attribute_line(&self, line: &str) -> Option<(String, String)> {
        // Parse: ensure => present, owner => 'root', mode => '0644',
        let parts: Vec<&str> = line.splitn(2, "=>").collect();
        if parts.len() == 2 {
            let key = parts[0].trim().to_string();
            let value = parts[1].trim().trim_end_matches(',').trim_matches(|c| c == '\'' || c == '"').to_string();
            Some((key, value))
        } else {
            None
        }
    }
    
    fn extract_includes(&self, content: &str) -> Vec<String> {
        self.include_regex
            .captures_iter(content)
            .filter_map(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
            .collect()
    }
}
```

### Step 2: Write Unit Tests

**File**: `tests/phase18_puppet.rs`

```rust
use rbuilder::extraction::puppet::*;
use std::path::Path;

#[test]
fn test_parse_simple_manifest() {
    let manifest = r#"
class nginx (
  String $version = '1.18.0',
  Integer $port = 80,
) {
  package { 'nginx':
    ensure => $version,
  }
  
  service { 'nginx':
    ensure => running,
    enable => true,
    require => Package['nginx'],
  }
  
  file { '/etc/nginx/nginx.conf':
    ensure  => file,
    content => template('nginx/nginx.conf.erb'),
    owner   => 'root',
    mode    => '0644',
    notify  => Service['nginx'],
  }
}
"#;
    
    let parser = PuppetParser::new();
    let temp_file = std::env::temp_dir().join("test_manifest.pp");
    std::fs::write(&temp_file, manifest).unwrap();
    
    let classes = parser.parse_manifest(&temp_file).unwrap();
    
    assert_eq!(classes.len(), 1);
    
    let class = &classes[0];
    assert_eq!(class.name, "nginx");
    assert_eq!(class.params.len(), 2);
    assert_eq!(class.resources.len(), 3);
    
    let package = &class.resources[0];
    assert_eq!(package.resource_type, "package");
    assert_eq!(package.title, "nginx");
    
    let service = &class.resources[1];
    assert_eq!(service.resource_type, "service");
    assert!(!service.require.is_empty());
    
    let file = &class.resources[2];
    assert_eq!(file.resource_type, "file");
    assert!(!file.notify.is_empty());
}

#[test]
fn test_parse_metadata_json() {
    let metadata = r#"{
  "name": "puppetlabs-nginx",
  "version": "1.0.0",
  "dependencies": [
    {
      "name": "puppetlabs-stdlib",
      "version_requirement": ">= 4.0.0"
    },
    {
      "name": "puppetlabs-concat",
      "version_requirement": ">= 2.0.0"
    }
  ]
}"#;
    
    let temp_file = std::env::temp_dir().join("metadata.json");
    std::fs::write(&temp_file, metadata).unwrap();
    
    let parser = PuppetParser::new();
    let (name, version, deps) = parser.parse_metadata(&temp_file).unwrap();
    
    assert_eq!(name, Some("puppetlabs-nginx".to_string()));
    assert_eq!(version, Some("1.0.0".to_string()));
    assert_eq!(deps.len(), 2);
    assert_eq!(deps[0].name, "puppetlabs-stdlib");
}

#[test]
fn test_class_inheritance() {
    let manifest = r#"
class nginx::params {
  $package_name = 'nginx'
  $service_name = 'nginx'
}

class nginx inherits nginx::params {
  package { $package_name:
    ensure => present,
  }
}
"#;
    
    let parser = PuppetParser::new();
    let temp_file = std::env::temp_dir().join("inheritance.pp");
    std::fs::write(&temp_file, manifest).unwrap();
    
    let classes = parser.parse_manifest(&temp_file).unwrap();
    
    assert_eq!(classes.len(), 2);
    assert_eq!(classes[1].name, "nginx");
    assert_eq!(classes[1].inherits, Some("nginx::params".to_string()));
}

#[test]
fn test_extract_includes() {
    let manifest = r#"
class profile::web {
  include ::nginx
  require ::firewall
  contain ::selinux
}
"#;
    
    let parser = PuppetParser::new();
    let temp_file = std::env::temp_dir().join("includes.pp");
    std::fs::write(&temp_file, manifest).unwrap();
    
    let classes = parser.parse_manifest(&temp_file).unwrap();
    
    assert_eq!(classes[0].included_classes.len(), 3);
    assert!(classes[0].included_classes.contains(&"nginx".to_string()));
    assert!(classes[0].included_classes.contains(&"firewall".to_string()));
}
```

**Acceptance Criteria**:
- [ ] Can parse Puppet manifests (.pp files)
- [ ] Extracts classes, parameters, inheritance
- [ ] Extracts resources, attributes, relationships
- [ ] Parses metadata.json for dependencies
- [ ] 12+ unit tests passing

---

## Task 18.1.2: Module Dependency Analysis (Days 5)

**File**: `src/analysis/puppet_modules.rs`

```rust
//! Puppet module dependency analysis.

use crate::error::{Error, Result};
use crate::extraction::puppet::{PuppetModule, PuppetParser};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone)]
pub struct ModuleDependencyGraph {
    pub modules: HashMap<String, ModuleNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    pub name: String,
    pub version: String,
    pub path: std::path::PathBuf,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
}

impl ModuleDependencyGraph {
    pub fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }
    
    pub fn add_module(&mut self, module: PuppetModule) {
        let dependencies: Vec<String> = module.dependencies
            .iter()
            .map(|dep| dep.name.clone())
            .collect();
        
        let node = ModuleNode {
            name: module.name.clone(),
            version: module.version.clone(),
            path: module.path.clone(),
            dependencies: dependencies.clone(),
            dependents: Vec::new(),
        };
        
        self.modules.insert(module.name.clone(), node);
        
        for dep_name in dependencies {
            if let Some(dep_node) = self.modules.get_mut(&dep_name) {
                if !dep_node.dependents.contains(&module.name) {
                    dep_node.dependents.push(module.name.clone());
                }
            }
        }
    }
    
    pub fn get_dependencies(&self, module_name: &str) -> Option<Vec<String>> {
        self.modules.get(module_name).map(|node| node.dependencies.clone())
    }
    
    pub fn validate_no_cycles(&self) -> Result<()> {
        for module_name in self.modules.keys() {
            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            if self.has_cycle(module_name, &mut visited, &mut stack)? {
                return Err(Error::Analysis(format!(
                    "Circular dependency detected involving module: {}",
                    module_name
                )));
            }
        }
        Ok(())
    }
    
    fn has_cycle(
        &self,
        module_name: &str,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
    ) -> Result<bool> {
        if stack.contains(module_name) {
            return Ok(true);
        }
        if visited.contains(module_name) {
            return Ok(false);
        }
        
        visited.insert(module_name.to_string());
        stack.insert(module_name.to_string());
        
        if let Some(node) = self.modules.get(module_name) {
            for dep in &node.dependencies {
                if self.has_cycle(dep, visited, stack)? {
                    return Ok(true);
                }
            }
        }
        
        stack.remove(module_name);
        Ok(false)
    }
    
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();
        
        for module_name in self.modules.keys() {
            in_degree.insert(module_name.clone(), 0);
        }
        
        for node in self.modules.values() {
            for dep in &node.dependencies {
                *in_degree.entry(dep.clone()).or_insert(0) += 1;
            }
        }
        
        for (module_name, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(module_name.clone());
            }
        }
        
        while let Some(module_name) = queue.pop_front() {
            result.push(module_name.clone());
            
            if let Some(node) = self.modules.get(&module_name) {
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
        
        if result.len() != self.modules.len() {
            return Err(Error::Analysis("Circular dependency detected in modules".into()));
        }
        
        Ok(result)
    }
}

pub struct ModuleDependencyAnalyzer;

impl ModuleDependencyAnalyzer {
    pub fn new() -> Self {
        Self
    }
    
    pub fn analyze_modules_dir(&self, modules_path: &Path) -> Result<ModuleDependencyGraph> {
        let mut graph = ModuleDependencyGraph::new();
        let parser = PuppetParser::new();
        
        if !modules_path.exists() {
            return Ok(graph);
        }
        
        for entry in std::fs::read_dir(modules_path)? {
            let entry = entry?;
            let module_path = entry.path();
            
            if module_path.is_dir() {
                match parser.parse_module(&module_path) {
                    Ok(module) => graph.add_module(module),
                    Err(e) => eprintln!("Warning: Failed to parse module {:?}: {}", module_path, e),
                }
            }
        }
        
        graph.validate_no_cycles()?;
        
        Ok(graph)
    }
}
```

---

# Week 2: Graph Integration & Security

## Task 18.2.1: Puppet Node Types & Edges (Day 6)

**File**: `src/graph/schema.rs`

```rust
pub enum NodeType {
    // ... existing types ...
    
    // Puppet-specific (Phase 18)
    PuppetModule,
    PuppetClass,
    PuppetDefinedType,
    PuppetResource,
    PuppetVariable,
    PuppetFact,
}

pub enum EdgeType {
    // ... existing types ...
    
    // Puppet-specific (Phase 18)
    DependsOnModule,     // module -> module
    IncludesClass,       // class -> class
    InheritsClass,       // class -> class
    DeclaresResource,    // class -> resource
    NotifiesResource,    // resource -> resource
    RequiresResource,    // resource -> resource
    UsesFact,            // class/resource -> fact
}
```

## Task 18.2.2: Puppet Graph Construction (Days 7-9)

Add to `src/extraction/puppet.rs`:

```rust
use crate::graph::backend::GraphBackend;

impl PuppetParser {
    pub fn build_graph(
        &self,
        module: &PuppetModule,
        backend: &mut dyn GraphBackend,
    ) -> Result<Uuid> {
        let module_node = Node::new(NodeType::PuppetModule, module.name.clone())
            .with_property("version", module.version.clone());
        
        let module_id = backend.insert_node(module_node)?;
        
        // Add dependencies
        for dep in &module.dependencies {
            let dep_node = Node::new(NodeType::PuppetModule, dep.name.clone());
            let dep_id = backend.insert_node(dep_node)?;
            
            backend.insert_edge(Edge::new(
                module_id,
                dep_id,
                EdgeType::DependsOnModule,
            ))?;
        }
        
        // Add classes
        for class in &module.classes {
            let class_id = self.build_class_graph(class, backend)?;
            backend.insert_edge(Edge::new(
                module_id,
                class_id,
                EdgeType::Contains,
            ))?;
        }
        
        Ok(module_id)
    }
    
    fn build_class_graph(
        &self,
        class: &PuppetClass,
        backend: &mut dyn GraphBackend,
    ) -> Result<Uuid> {
        let class_node = Node::new(NodeType::PuppetClass, class.name.clone());
        let class_id = backend.insert_node(class_node)?;
        
        // Add inheritance
        if let Some(parent) = &class.inherits {
            let parent_node = Node::new(NodeType::PuppetClass, parent.clone());
            let parent_id = backend.insert_node(parent_node)?;
            
            backend.insert_edge(Edge::new(
                class_id,
                parent_id,
                EdgeType::InheritsClass,
            ))?;
        }
        
        // Add resources
        for resource in &class.resources {
            let resource_id = self.build_resource_graph(resource, backend)?;
            backend.insert_edge(Edge::new(
                class_id,
                resource_id,
                EdgeType::DeclaresResource,
            ))?;
        }
        
        // Add includes
        for included in &class.included_classes {
            let included_node = Node::new(NodeType::PuppetClass, included.clone());
            let included_id = backend.insert_node(included_node)?;
            
            backend.insert_edge(Edge::new(
                class_id,
                included_id,
                EdgeType::IncludesClass,
            ))?;
        }
        
        Ok(class_id)
    }
    
    fn build_resource_graph(
        &self,
        resource: &ResourceDeclaration,
        backend: &mut dyn GraphBackend,
    ) -> Result<Uuid> {
        let resource_node = Node::new(NodeType::PuppetResource, resource.title.clone())
            .with_property("type", resource.resource_type.clone());
        
        let resource_id = backend.insert_node(resource_node)?;
        
        Ok(resource_id)
    }
}
```

## Task 18.3.2: Puppet Security Analysis (Days 10-12)

**File**: `src/security/puppet.rs`

```rust
//! Puppet security scanning for common vulnerabilities.

use crate::extraction::puppet::{PuppetModule, ResourceDeclaration};
use crate::security::{SecurityFinding, Severity};
use std::collections::HashSet;

pub struct PuppetSecurityScanner {
    dangerous_resources: HashSet<String>,
}

impl Default for PuppetSecurityScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl PuppetSecurityScanner {
    pub fn new() -> Self {
        let dangerous_resources: HashSet<String> = [
            "exec",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        
        Self {
            dangerous_resources,
        }
    }
    
    pub fn scan_module(&self, module: &PuppetModule) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        
        for class in &module.classes {
            for resource in &class.resources {
                findings.extend(self.scan_resource(resource, &class.name));
            }
        }
        
        findings
    }
    
    fn scan_resource(&self, resource: &ResourceDeclaration, class_name: &str) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        
        // Check for hardcoded secrets
        if let Some(finding) = self.check_hardcoded_secrets(resource, class_name) {
            findings.push(finding);
        }
        
        // Check for exec command injection
        if resource.resource_type == "exec" {
            if let Some(finding) = self.check_exec_injection(resource, class_name) {
                findings.push(finding);
            }
        }
        
        // Check for insecure file permissions
        if resource.resource_type == "file" {
            if let Some(finding) = self.check_file_permissions(resource, class_name) {
                findings.push(finding);
            }
        }
        
        findings
    }
    
    fn check_hardcoded_secrets(&self, resource: &ResourceDeclaration, class_name: &str) -> Option<SecurityFinding> {
        let resource_str = format!("{:?}", resource.attributes);
        
        let secret_patterns = ["password", "secret", "token", "api_key"];
        
        for pattern in &secret_patterns {
            if resource_str.to_lowercase().contains(pattern) {
                if !resource_str.contains("$") && !resource_str.contains("lookup(") {
                    return Some(SecurityFinding {
                        severity: Severity::High,
                        message: format!(
                            "Potential hardcoded secret in resource '{}' in class '{}'",
                            resource.title, class_name
                        ),
                        location: format!("{}:{}", class_name, resource.title),
                        cwe: Some("CWE-798".to_string()),
                        remediation: Some("Use Hiera lookup() or encrypted data instead".to_string()),
                    });
                }
            }
        }
        
        None
    }
    
    fn check_exec_injection(&self, resource: &ResourceDeclaration, class_name: &str) -> Option<SecurityFinding> {
        if let Some(command) = resource.attributes.get("command") {
            if command.contains("$") && !command.contains("shellquote") {
                return Some(SecurityFinding {
                    severity: Severity::Critical,
                    message: format!(
                        "Potential command injection in exec resource '{}' in class '{}'",
                        resource.title, class_name
                    ),
                    location: format!("{}:{}", class_name, resource.title),
                    cwe: Some("CWE-78".to_string()),
                    remediation: Some("Use shellquote() function for variable interpolation in commands".to_string()),
                });
            }
        }
        
        None
    }
    
    fn check_file_permissions(&self, resource: &ResourceDeclaration, class_name: &str) -> Option<SecurityFinding> {
        if let Some(mode) = resource.attributes.get("mode") {
            if mode.contains("777") || mode.contains("666") {
                return Some(SecurityFinding {
                    severity: Severity::Medium,
                    message: format!(
                        "Insecure file permissions ({}) in resource '{}' in class '{}'",
                        mode, resource.title, class_name
                    ),
                    location: format!("{}:{}", class_name, resource.title),
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

(Similar structure to Ansible and Chef - CLI commands, MCP tools, comprehensive testing, documentation)

**Files to create**:
- `src/cli/puppet.rs` - CLI commands (Days 13-14)
- MCP tools in `src/mcp/tools.rs` (Day 15)
- Comprehensive tests (Days 16-18)
- `docs/puppet_support.md` (Days 19-21)

---

# Final Checklist

## Overall
- [ ] 35+ tests total
- [ ] Grade A (90%+)
- [ ] No architecture changes
- [ ] All features working
- [ ] Documentation complete
