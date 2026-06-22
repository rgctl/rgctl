# Phase 16: Ansible Support - Implementation Guide for Cursor

**Target**: Tier 1 Infrastructure-as-Code support for Ansible  
**Timeline**: 3 weeks  
**Grade Target**: A (90%+)  
**Tests Required**: 35+  

## 🎯 Goals

Add comprehensive Ansible playbook analysis following existing Tier 1 architecture patterns. **No architecture changes allowed** - integrate seamlessly with existing graph backend, query system, and MCP server.

## 📋 Prerequisites

Before starting, familiarize yourself with:
- `src/extraction/github_actions.rs` - Similar YAML + variable extraction pattern
- `src/extraction/gitlab_ci.rs` - Another CI/CD YAML parser reference
- `src/graph/schema.rs` - Where to add new NodeType/EdgeType enums
- `src/analysis/` - Existing analysis modules (community, centrality, taint)
- `src/mcp/tools.rs` - How to add new MCP tools

## 📁 File Structure

Create these new files:
```
src/extraction/ansible.rs          # Main parser (500+ lines)
src/analysis/ansible_roles.rs      # Role dependency analyzer (300+ lines)
src/security/ansible.rs             # Security scanner (250+ lines)
src/cli/ansible.rs                  # CLI commands (150+ lines)
tests/phase16_ansible.rs            # Integration tests (400+ lines)
tests/fixtures/ansible/             # Test playbooks, roles, etc.
docs/ansible_support.md             # User documentation
```

Update these existing files:
```
src/graph/schema.rs                 # Add Ansible node/edge types
src/extraction/mod.rs               # Register AnsibleParser
src/mcp/tools.rs                    # Add Ansible MCP tools
src/cli/mod.rs                      # Add ansible subcommand
src/lib.rs                          # Export new modules
README.md                           # Document Ansible support
```

---

# Week 1: Parser & Core Extraction

## Task 16.1.1: Ansible YAML Parser (Days 1-3)

### Step 1: Create Module Structure

**File**: `src/extraction/ansible.rs`

```rust
//! Ansible playbook, role, and variable extraction.
//!
//! Parses Ansible YAML files and extracts:
//! - Playbooks (plays, tasks, handlers)
//! - Roles (tasks, handlers, defaults, vars, meta)
//! - Variables (Jinja2 templates, group_vars, host_vars)
//! - Inventory files

use crate::error::Result;
use crate::graph::schema::{Node, Edge, NodeType, EdgeType};
use serde::{Deserialize, Serialize};
use serde_yaml::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;
use regex::Regex;

/// Ansible file type detector
#[derive(Debug, Clone, PartialEq)]
pub enum AnsibleFileType {
    Playbook,       // playbook.yml, site.yml
    Role,           // roles/*/tasks/main.yml
    Vars,           // group_vars/*, host_vars/*
    Inventory,      // inventory.yml, hosts
    TaskFile,       // included task files
    Handler,        // handlers/main.yml
}

/// Main Ansible parser
pub struct AnsibleParser {
    jinja_var_regex: Regex,
    ansible_facts_regex: Regex,
}

impl Default for AnsibleParser {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsibleParser {
    pub fn new() -> Self {
        Self {
            // Match {{ variable }}, {{ variable.attribute }}, {{ variable['key'] }}
            jinja_var_regex: Regex::new(r"\{\{\s*([a-zA-Z_][a-zA-Z0-9_\.'\[\]]*)\s*\}\}").unwrap(),
            // Match ansible_facts, ansible_user, etc.
            ansible_facts_regex: Regex::new(r"ansible_[a-zA-Z_][a-zA-Z0-9_]*").unwrap(),
        }
    }
    
    /// Detect Ansible file type from path and content
    pub fn detect_file_type(&self, path: &Path, yaml: &Value) -> Result<AnsibleFileType> {
        let path_str = path.to_string_lossy();
        
        // Check path patterns
        if path_str.contains("roles/") && path_str.contains("/tasks/") {
            return Ok(AnsibleFileType::Role);
        }
        if path_str.contains("group_vars/") || path_str.contains("host_vars/") {
            return Ok(AnsibleFileType::Vars);
        }
        if path_str.contains("handlers/") {
            return Ok(AnsibleFileType::Handler);
        }
        
        // Check content structure
        if let Some(array) = yaml.as_sequence() {
            if array.iter().any(|item| {
                item.get("hosts").is_some() || item.get("import_playbook").is_some()
            }) {
                return Ok(AnsibleFileType::Playbook);
            }
        }
        
        Ok(AnsibleFileType::TaskFile)
    }
    
    /// Extract Jinja2 variables from a string
    pub fn extract_jinja_vars(&self, text: &str) -> Vec<String> {
        self.jinja_var_regex
            .captures_iter(text)
            .map(|cap| cap[1].to_string())
            .collect()
    }
}

/// Ansible playbook structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnsiblePlaybook {
    pub name: String,
    pub path: PathBuf,
    pub plays: Vec<Play>,
    pub imported_playbooks: Vec<String>,
}

/// A single play in a playbook
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Play {
    pub name: String,
    pub hosts: String,
    pub become: bool,
    pub tasks: Vec<Task>,
    pub pre_tasks: Vec<Task>,
    pub post_tasks: Vec<Task>,
    pub roles: Vec<RoleReference>,
    pub handlers: Vec<Handler>,
    pub vars: HashMap<String, Value>,
}

/// Ansible task
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: Uuid,
    pub name: String,
    pub module: String,
    pub args: HashMap<String, Value>,
    pub when: Option<String>,
    pub loop_var: Option<String>,
    pub tags: Vec<String>,
    pub notify: Vec<String>,
    pub become: Option<bool>,
    pub vars: HashMap<String, Value>,
}

/// Role reference in a play
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleReference {
    pub name: String,
    pub vars: HashMap<String, Value>,
}

/// Handler (same as task but triggered by notify)
pub type Handler = Task;

impl AnsibleParser {
    /// Parse a playbook file
    pub fn parse_playbook(&self, path: &Path) -> Result<AnsiblePlaybook> {
        let content = std::fs::read_to_string(path)?;
        let yaml: Value = serde_yaml::from_str(&content)?;
        
        let plays = self.extract_plays(&yaml)?;
        let imported = self.extract_imported_playbooks(&yaml);
        
        Ok(AnsiblePlaybook {
            name: path.file_stem().unwrap().to_string_lossy().to_string(),
            path: path.to_path_buf(),
            plays,
            imported_playbooks: imported,
        })
    }
    
    fn extract_plays(&self, yaml: &Value) -> Result<Vec<Play>> {
        let mut plays = Vec::new();
        
        if let Some(array) = yaml.as_sequence() {
            for play_value in array {
                if let Some(play) = self.parse_play(play_value)? {
                    plays.push(play);
                }
            }
        }
        
        Ok(plays)
    }
    
    fn parse_play(&self, play_value: &Value) -> Result<Option<Play>> {
        // Skip import_playbook entries
        if play_value.get("import_playbook").is_some() {
            return Ok(None);
        }
        
        let name = play_value
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unnamed play")
            .to_string();
        
        let hosts = play_value
            .get("hosts")
            .and_then(|v| v.as_str())
            .unwrap_or("all")
            .to_string();
        
        let become = play_value
            .get("become")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        let tasks = self.extract_tasks(play_value.get("tasks"))?;
        let pre_tasks = self.extract_tasks(play_value.get("pre_tasks"))?;
        let post_tasks = self.extract_tasks(play_value.get("post_tasks"))?;
        let roles = self.extract_roles(play_value.get("roles"))?;
        let handlers = self.extract_tasks(play_value.get("handlers"))?;
        let vars = self.extract_vars(play_value.get("vars"));
        
        Ok(Some(Play {
            name,
            hosts,
            become,
            tasks,
            pre_tasks,
            post_tasks,
            roles,
            handlers,
            vars,
        }))
    }
    
    fn extract_tasks(&self, tasks_value: Option<&Value>) -> Result<Vec<Task>> {
        let mut tasks = Vec::new();
        
        if let Some(Value::Sequence(task_list)) = tasks_value {
            for task_value in task_list {
                if let Some(task) = self.parse_task(task_value)? {
                    tasks.push(task);
                }
            }
        }
        
        Ok(tasks)
    }
    
    fn parse_task(&self, task_value: &Value) -> Result<Option<Task>> {
        if let Some(obj) = task_value.as_mapping() {
            let name = task_value
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unnamed task")
                .to_string();
            
            // Find the module name (any key that's not a control key)
            let control_keys = [
                "name", "when", "loop", "with_items", "tags", "notify", 
                "become", "vars", "register", "changed_when", "failed_when"
            ];
            
            let (module, args) = obj
                .iter()
                .find(|(k, _)| {
                    if let Some(key_str) = k.as_str() {
                        !control_keys.contains(&key_str)
                    } else {
                        false
                    }
                })
                .map(|(k, v)| {
                    let module_name = k.as_str().unwrap().to_string();
                    let args = self.parse_module_args(v);
                    (module_name, args)
                })
                .unwrap_or_else(|| ("unknown".to_string(), HashMap::new()));
            
            let when_cond = task_value.get("when").and_then(|v| v.as_str()).map(String::from);
            let loop_var = task_value.get("loop").or_else(|| task_value.get("with_items"))
                .map(|_| "item".to_string());
            
            let tags = task_value
                .get("tags")
                .and_then(|v| v.as_sequence())
                .map(|seq| seq.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();
            
            let notify = task_value
                .get("notify")
                .and_then(|v| match v {
                    Value::String(s) => Some(vec![s.clone()]),
                    Value::Sequence(seq) => Some(seq.iter().filter_map(|v| v.as_str().map(String::from)).collect()),
                    _ => None,
                })
                .unwrap_or_default();
            
            let become = task_value.get("become").and_then(|v| v.as_bool());
            let vars = self.extract_vars(task_value.get("vars"));
            
            return Ok(Some(Task {
                id: Uuid::new_v4(),
                name,
                module,
                args,
                when: when_cond,
                loop_var,
                tags,
                notify,
                become,
                vars,
            }));
        }
        
        Ok(None)
    }
    
    fn parse_module_args(&self, value: &Value) -> HashMap<String, Value> {
        let mut args = HashMap::new();
        
        match value {
            Value::Mapping(map) => {
                for (k, v) in map {
                    if let Some(key) = k.as_str() {
                        args.insert(key.to_string(), v.clone());
                    }
                }
            }
            Value::String(s) => {
                args.insert("_raw".to_string(), Value::String(s.clone()));
            }
            _ => {}
        }
        
        args
    }
    
    fn extract_roles(&self, roles_value: Option<&Value>) -> Result<Vec<RoleReference>> {
        let mut roles = Vec::new();
        
        if let Some(Value::Sequence(role_list)) = roles_value {
            for role_value in role_list {
                match role_value {
                    Value::String(name) => {
                        roles.push(RoleReference {
                            name: name.clone(),
                            vars: HashMap::new(),
                        });
                    }
                    Value::Mapping(_) => {
                        if let Some(name) = role_value.get("role").or_else(|| role_value.get("name")) {
                            if let Some(name_str) = name.as_str() {
                                let vars = self.extract_vars(role_value.get("vars"));
                                roles.push(RoleReference {
                                    name: name_str.to_string(),
                                    vars,
                                });
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
        
        Ok(roles)
    }
    
    fn extract_vars(&self, vars_value: Option<&Value>) -> HashMap<String, Value> {
        if let Some(Value::Mapping(map)) = vars_value {
            map.iter()
                .filter_map(|(k, v)| k.as_str().map(|s| (s.to_string(), v.clone())))
                .collect()
        } else {
            HashMap::new()
        }
    }
    
    fn extract_imported_playbooks(&self, yaml: &Value) -> Vec<String> {
        let mut imports = Vec::new();
        
        if let Some(array) = yaml.as_sequence() {
            for item in array {
                if let Some(import) = item.get("import_playbook").and_then(|v| v.as_str()) {
                    imports.push(import.to_string());
                }
            }
        }
        
        imports
    }
}
```

### Step 2: Add Ansible Role Parser

Add to `src/extraction/ansible.rs`:

```rust
/// Ansible role structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnsibleRole {
    pub name: String,
    pub path: PathBuf,
    pub tasks: Vec<Task>,
    pub handlers: Vec<Handler>,
    pub defaults: HashMap<String, Value>,
    pub vars: HashMap<String, Value>,
    pub meta: RoleMeta,
    pub templates: Vec<PathBuf>,
    pub files: Vec<PathBuf>,
}

/// Role metadata (from meta/main.yml)
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RoleMeta {
    pub dependencies: Vec<RoleDependency>,
    pub galaxy_info: HashMap<String, Value>,
}

/// Role dependency
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleDependency {
    pub name: String,
    pub src: Option<String>,
    pub version: Option<String>,
    pub vars: HashMap<String, Value>,
}

impl AnsibleParser {
    /// Parse an Ansible role directory
    pub fn parse_role(&self, role_path: &Path) -> Result<AnsibleRole> {
        let name = role_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        
        let tasks = self.parse_role_tasks(&role_path.join("tasks/main.yml"))?;
        let handlers = self.parse_role_tasks(&role_path.join("handlers/main.yml"))?;
        let defaults = self.parse_vars_file(&role_path.join("defaults/main.yml"))?;
        let vars = self.parse_vars_file(&role_path.join("vars/main.yml"))?;
        let meta = self.parse_role_meta(&role_path.join("meta/main.yml"))?;
        let templates = self.discover_files(&role_path.join("templates"), &["j2", "yml"])?;
        let files = self.discover_files(&role_path.join("files"), &[])?;
        
        Ok(AnsibleRole {
            name,
            path: role_path.to_path_buf(),
            tasks,
            handlers,
            defaults,
            vars,
            meta,
            templates,
            files,
        })
    }
    
    fn parse_role_tasks(&self, task_file: &Path) -> Result<Vec<Task>> {
        if !task_file.exists() {
            return Ok(Vec::new());
        }
        
        let content = std::fs::read_to_string(task_file)?;
        let yaml: Value = serde_yaml::from_str(&content)?;
        self.extract_tasks(Some(&yaml))
    }
    
    fn parse_vars_file(&self, vars_file: &Path) -> Result<HashMap<String, Value>> {
        if !vars_file.exists() {
            return Ok(HashMap::new());
        }
        
        let content = std::fs::read_to_string(vars_file)?;
        let yaml: Value = serde_yaml::from_str(&content)?;
        Ok(self.extract_vars(Some(&yaml)))
    }
    
    fn parse_role_meta(&self, meta_file: &Path) -> Result<RoleMeta> {
        if !meta_file.exists() {
            return Ok(RoleMeta::default());
        }
        
        let content = std::fs::read_to_string(meta_file)?;
        let yaml: Value = serde_yaml::from_str(&content)?;
        
        let dependencies = yaml
            .get("dependencies")
            .and_then(|v| v.as_sequence())
            .map(|seq| self.parse_role_dependencies(seq))
            .transpose()?
            .unwrap_or_default();
        
        let galaxy_info = yaml
            .get("galaxy_info")
            .and_then(|v| v.as_mapping())
            .map(|map| {
                map.iter()
                    .filter_map(|(k, v)| k.as_str().map(|s| (s.to_string(), v.clone())))
                    .collect()
            })
            .unwrap_or_default();
        
        Ok(RoleMeta {
            dependencies,
            galaxy_info,
        })
    }
    
    fn parse_role_dependencies(&self, deps: &[Value]) -> Result<Vec<RoleDependency>> {
        let mut dependencies = Vec::new();
        
        for dep in deps {
            match dep {
                Value::String(name) => {
                    dependencies.push(RoleDependency {
                        name: name.clone(),
                        src: None,
                        version: None,
                        vars: HashMap::new(),
                    });
                }
                Value::Mapping(_) => {
                    if let Some(role_name) = dep.get("role").or_else(|| dep.get("name")) {
                        if let Some(name_str) = role_name.as_str() {
                            let src = dep.get("src").and_then(|v| v.as_str()).map(String::from);
                            let version = dep.get("version").and_then(|v| v.as_str()).map(String::from);
                            let vars = self.extract_vars(dep.get("vars"));
                            
                            dependencies.push(RoleDependency {
                                name: name_str.to_string(),
                                src,
                                version,
                                vars,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        
        Ok(dependencies)
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

### Step 3: Write Unit Tests

**File**: `tests/phase16_ansible.rs`

```rust
use rbuilder::extraction::ansible::*;

#[test]
fn test_parse_simple_playbook() {
    let yaml = r#"
---
- name: Configure web servers
  hosts: webservers
  become: yes
  
  tasks:
    - name: Install nginx
      apt:
        name: nginx
        state: present
      notify: restart nginx
    
    - name: Start nginx
      service:
        name: nginx
        state: started
  
  handlers:
    - name: restart nginx
      service:
        name: nginx
        state: restarted
"#;
    
    let parser = AnsibleParser::new();
    let temp_file = std::env::temp_dir().join("test_playbook.yml");
    std::fs::write(&temp_file, yaml).unwrap();
    
    let playbook = parser.parse_playbook(&temp_file).unwrap();
    
    assert_eq!(playbook.plays.len(), 1);
    assert_eq!(playbook.plays[0].name, "Configure web servers");
    assert_eq!(playbook.plays[0].hosts, "webservers");
    assert!(playbook.plays[0].become);
    assert_eq!(playbook.plays[0].tasks.len(), 2);
    assert_eq!(playbook.plays[0].handlers.len(), 1);
    
    let task = &playbook.plays[0].tasks[0];
    assert_eq!(task.name, "Install nginx");
    assert_eq!(task.module, "apt");
    assert_eq!(task.notify, vec!["restart nginx"]);
}

#[test]
fn test_extract_jinja_variables() {
    let parser = AnsibleParser::new();
    
    let text = "{{ ansible_user }}/{{ app_name }}/config.yml";
    let vars = parser.extract_jinja_vars(text);
    
    assert_eq!(vars.len(), 2);
    assert!(vars.contains(&"ansible_user".to_string()));
    assert!(vars.contains(&"app_name".to_string()));
}

#[test]
fn test_parse_role_with_dependencies() {
    let yaml = r#"
dependencies:
  - role: common
  - role: nginx
    vars:
      nginx_port: 8080
  - name: postgresql
    version: "1.2.3"
"#;
    
    let parser = AnsibleParser::new();
    // Test role meta parsing
    // (implementation detail - create temp file with proper role structure)
}

#[test]
fn test_detect_file_type_playbook() {
    let yaml = serde_yaml::from_str(r#"
- name: Test play
  hosts: all
  tasks:
    - debug: msg="hello"
"#).unwrap();
    
    let parser = AnsibleParser::new();
    let path = std::path::Path::new("site.yml");
    let file_type = parser.detect_file_type(path, &yaml).unwrap();
    
    assert_eq!(file_type, AnsibleFileType::Playbook);
}

#[test]
fn test_parse_task_with_loop() {
    let yaml = serde_yaml::from_str(r#"
name: Install packages
apt:
  name: "{{ item }}"
  state: present
loop:
  - nginx
  - postgresql
tags:
  - packages
"#).unwrap();
    
    let parser = AnsibleParser::new();
    let task = parser.parse_task(&yaml).unwrap().unwrap();
    
    assert_eq!(task.name, "Install packages");
    assert_eq!(task.module, "apt");
    assert_eq!(task.loop_var, Some("item".to_string()));
    assert_eq!(task.tags, vec!["packages"]);
}
```

**Acceptance Criteria**:
- [ ] Can parse playbook YAML files
- [ ] Extracts plays, tasks, handlers, roles
- [ ] Handles Jinja2 variable extraction
- [ ] Detects file types correctly
- [ ] 10+ unit tests passing

---

## Task 16.1.2: Role Dependency Analysis (Days 4-5)

**File**: `src/analysis/ansible_roles.rs`

```rust
//! Ansible role dependency analysis.

use crate::error::{Error, Result};
use crate::extraction::ansible::{AnsibleRole, RoleDependency};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use serde::{Serialize, Deserialize};

/// Role dependency graph
#[derive(Debug, Clone)]
pub struct RoleDependencyGraph {
    pub roles: HashMap<String, RoleNode>,
}

/// Role node in dependency graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleNode {
    pub name: String,
    pub path: std::path::PathBuf,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
}

impl RoleDependencyGraph {
    pub fn new() -> Self {
        Self {
            roles: HashMap::new(),
        }
    }
    
    pub fn add_role(&mut self, role: AnsibleRole) {
        let dependencies: Vec<String> = role.meta.dependencies
            .iter()
            .map(|dep| dep.name.clone())
            .collect();
        
        let node = RoleNode {
            name: role.name.clone(),
            path: role.path.clone(),
            dependencies: dependencies.clone(),
            dependents: Vec::new(),
        };
        
        self.roles.insert(role.name.clone(), node);
        
        // Update dependents
        for dep_name in dependencies {
            if let Some(dep_node) = self.roles.get_mut(&dep_name) {
                if !dep_node.dependents.contains(&role.name) {
                    dep_node.dependents.push(role.name.clone());
                }
            }
        }
    }
    
    /// Get direct dependencies of a role
    pub fn get_dependencies(&self, role_name: &str) -> Option<Vec<String>> {
        self.roles.get(role_name).map(|node| node.dependencies.clone())
    }
    
    /// Get all transitive dependencies (depth-first)
    pub fn get_all_dependencies(&self, role_name: &str) -> Result<Vec<String>> {
        let mut visited = HashSet::new();
        let mut result = Vec::new();
        self.collect_dependencies_recursive(role_name, &mut visited, &mut result)?;
        Ok(result)
    }
    
    fn collect_dependencies_recursive(
        &self,
        role_name: &str,
        visited: &mut HashSet<String>,
        result: &mut Vec<String>,
    ) -> Result<()> {
        if visited.contains(role_name) {
            return Ok(());
        }
        visited.insert(role_name.to_string());
        
        if let Some(node) = self.roles.get(role_name) {
            for dep in &node.dependencies {
                result.push(dep.clone());
                self.collect_dependencies_recursive(dep, visited, result)?;
            }
        }
        
        Ok(())
    }
    
    /// Validate no circular dependencies
    pub fn validate_no_cycles(&self) -> Result<()> {
        for role_name in self.roles.keys() {
            let mut visited = HashSet::new();
            let mut stack = HashSet::new();
            if self.has_cycle(role_name, &mut visited, &mut stack)? {
                return Err(Error::Analysis(format!(
                    "Circular dependency detected involving role: {}",
                    role_name
                )));
            }
        }
        Ok(())
    }
    
    fn has_cycle(
        &self,
        role_name: &str,
        visited: &mut HashSet<String>,
        stack: &mut HashSet<String>,
    ) -> Result<bool> {
        if stack.contains(role_name) {
            return Ok(true);
        }
        if visited.contains(role_name) {
            return Ok(false);
        }
        
        visited.insert(role_name.to_string());
        stack.insert(role_name.to_string());
        
        if let Some(node) = self.roles.get(role_name) {
            for dep in &node.dependencies {
                if self.has_cycle(dep, visited, stack)? {
                    return Ok(true);
                }
            }
        }
        
        stack.remove(role_name);
        Ok(false)
    }
    
    /// Topological sort of roles (dependency order)
    pub fn topological_sort(&self) -> Result<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut queue = VecDeque::new();
        let mut result = Vec::new();
        
        // Calculate in-degrees
        for role_name in self.roles.keys() {
            in_degree.insert(role_name.clone(), 0);
        }
        for node in self.roles.values() {
            for dep in &node.dependencies {
                *in_degree.entry(dep.clone()).or_insert(0) += 1;
            }
        }
        
        // Find nodes with in-degree 0
        for (role_name, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(role_name.clone());
            }
        }
        
        // Kahn's algorithm
        while let Some(role_name) = queue.pop_front() {
            result.push(role_name.clone());
            
            if let Some(node) = self.roles.get(&role_name) {
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
        
        if result.len() != self.roles.len() {
            return Err(Error::Analysis("Circular dependency detected in roles".into()));
        }
        
        Ok(result)
    }
}

/// Role dependency analyzer
pub struct RoleDependencyAnalyzer;

impl RoleDependencyAnalyzer {
    pub fn new() -> Self {
        Self
    }
    
    /// Analyze all roles in a directory
    pub fn analyze_roles_dir(&self, roles_path: &Path) -> Result<RoleDependencyGraph> {
        let mut graph = RoleDependencyGraph::new();
        let parser = crate::extraction::ansible::AnsibleParser::new();
        
        if !roles_path.exists() {
            return Ok(graph);
        }
        
        for entry in std::fs::read_dir(roles_path)? {
            let entry = entry?;
            let role_path = entry.path();
            
            if role_path.is_dir() {
                match parser.parse_role(&role_path) {
                    Ok(role) => graph.add_role(role),
                    Err(e) => eprintln!("Warning: Failed to parse role {:?}: {}", role_path, e),
                }
            }
        }
        
        graph.validate_no_cycles()?;
        
        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_circular_dependency() {
        // Create test fixture with circular deps
        // role_a -> role_b -> role_c -> role_a
    }
    
    #[test]
    fn test_topological_sort() {
        // Test dependency ordering
    }
}
```

Add tests in `tests/phase16_ansible.rs`:

```rust
#[test]
fn test_role_dependency_detection() {
    // Create test fixture directory structure
    let temp_dir = tempdir::TempDir::new("ansible_roles").unwrap();
    // ... create role directories with meta/main.yml
    
    let analyzer = RoleDependencyAnalyzer::new();
    let graph = analyzer.analyze_roles_dir(temp_dir.path()).unwrap();
    
    assert_eq!(graph.roles.len(), 3);
    assert_eq!(graph.get_dependencies("nginx").unwrap(), vec!["common"]);
}
```

---

# Week 2: Graph Integration & Security

## Task 16.2.1: Ansible Node Types & Edges (Day 6)

**File**: `src/graph/schema.rs`

Add to existing NodeType enum:

```rust
pub enum NodeType {
    // ... existing types ...
    
    // Ansible-specific (Phase 16)
    AnsiblePlaybook,
    AnsiblePlay,
    AnsibleTask,
    AnsibleRole,
    AnsibleHandler,
    AnsibleVariable,
    AnsibleTemplate,
}
```

Add to existing EdgeType enum:

```rust
pub enum EdgeType {
    // ... existing types ...
    
    // Ansible-specific (Phase 16)
    IncludesRole,       // playbook/play -> role
    DependsOnRole,      // role -> role (meta deps)
    ExecutesTask,       // play -> task
    NotifiesHandler,    // task -> handler
    UsesVariable,       // task/template -> variable
    IncludesPlaybook,   // playbook -> playbook
    RendersTemplate,    // task -> template file
}
```

## Task 16.2.2: Ansible Graph Construction (Days 7-9)

Add to `src/extraction/ansible.rs`:

```rust
use crate::graph::backend::GraphBackend;

impl AnsibleParser {
    /// Build graph from parsed playbook
    pub fn build_graph(
        &self,
        playbook: &AnsiblePlaybook,
        backend: &mut dyn GraphBackend,
    ) -> Result<Uuid> {
        // Create playbook node
        let playbook_node = Node::new(
            NodeType::AnsiblePlaybook,
            playbook.name.clone(),
        )
        .with_property("path", playbook.path.to_string_lossy().to_string());
        
        let playbook_id = backend.insert_node(playbook_node)?;
        
        // Process each play
        for play in &playbook.plays {
            let play_id = self.build_play_graph(play, backend)?;
            backend.insert_edge(Edge::new(
                playbook_id,
                play_id,
                EdgeType::Contains,
            ))?;
        }
        
        // Link imported playbooks
        for imported in &playbook.imported_playbooks {
            // Create placeholder node for imported playbook
            let imported_node = Node::new(
                NodeType::AnsiblePlaybook,
                imported.clone(),
            );
            let imported_id = backend.insert_node(imported_node)?;
            
            backend.insert_edge(Edge::new(
                playbook_id,
                imported_id,
                EdgeType::IncludesPlaybook,
            ))?;
        }
        
        Ok(playbook_id)
    }
    
    fn build_play_graph(
        &self,
        play: &Play,
        backend: &mut dyn GraphBackend,
    ) -> Result<Uuid> {
        let play_node = Node::new(NodeType::AnsiblePlay, play.name.clone())
            .with_property("hosts", play.hosts.clone())
            .with_property("become", play.become.to_string());
        
        let play_id = backend.insert_node(play_node)?;
        
        // Create role nodes and edges
        for role_ref in &play.roles {
            let role_node = Node::new(NodeType::AnsibleRole, role_ref.name.clone());
            let role_id = backend.insert_node(role_node)?;
            
            backend.insert_edge(Edge::new(
                play_id,
                role_id,
                EdgeType::IncludesRole,
            ))?;
        }
        
        // Create task nodes
        for task in &play.tasks {
            let task_id = self.build_task_graph(task, backend)?;
            backend.insert_edge(Edge::new(
                play_id,
                task_id,
                EdgeType::ExecutesTask,
            ))?;
        }
        
        // Create handler nodes
        let handler_ids = self.build_handlers_graph(&play.handlers, backend)?;
        
        // Link tasks to handlers via notify
        for task in &play.tasks {
            for handler_name in &task.notify {
                if let Some(&handler_id) = handler_ids.get(handler_name) {
                    let task_node_id = self.find_task_by_name(backend, &task.name)?;
                    backend.insert_edge(Edge::new(
                        task_node_id,
                        handler_id,
                        EdgeType::NotifiesHandler,
                    ))?;
                }
            }
        }
        
        Ok(play_id)
    }
    
    fn build_task_graph(
        &self,
        task: &Task,
        backend: &mut dyn GraphBackend,
    ) -> Result<Uuid> {
        let mut task_node = Node::new(NodeType::AnsibleTask, task.name.clone())
            .with_property("module", task.module.clone());
        
        if let Some(when) = &task.when {
            task_node = task_node.with_property("when", when.clone());
        }
        
        if !task.tags.is_empty() {
            task_node = task_node.with_property("tags", task.tags.join(","));
        }
        
        let task_id = backend.insert_node(task_node)?;
        
        // Extract and link variables used in task
        let all_text = format!("{:?}", task.args); // Simple approach
        let vars = self.extract_jinja_vars(&all_text);
        
        for var_name in vars {
            let var_node = Node::new(NodeType::AnsibleVariable, var_name.clone());
            let var_id = backend.insert_node(var_node)?;
            
            backend.insert_edge(Edge::new(
                task_id,
                var_id,
                EdgeType::UsesVariable,
            ))?;
        }
        
        Ok(task_id)
    }
    
    fn build_handlers_graph(
        &self,
        handlers: &[Handler],
        backend: &mut dyn GraphBackend,
    ) -> Result<HashMap<String, Uuid>> {
        let mut handler_map = HashMap::new();
        
        for handler in handlers {
            let handler_node = Node::new(NodeType::AnsibleHandler, handler.name.clone())
                .with_property("module", handler.module.clone());
            
            let handler_id = backend.insert_node(handler_node)?;
            handler_map.insert(handler.name.clone(), handler_id);
        }
        
        Ok(handler_map)
    }
    
    fn find_task_by_name(&self, backend: &dyn GraphBackend, name: &str) -> Result<Uuid> {
        // Query backend for task with given name
        let nodes = backend.all_nodes()?;
        nodes
            .iter()
            .find(|n| n.node_type == NodeType::AnsibleTask && n.name == name)
            .map(|n| n.id)
            .ok_or_else(|| Error::NotFound(format!("Task not found: {}", name)))
    }
    
    /// Build graph from role
    pub fn build_role_graph(
        &self,
        role: &AnsibleRole,
        backend: &mut dyn GraphBackend,
    ) -> Result<Uuid> {
        let role_node = Node::new(NodeType::AnsibleRole, role.name.clone())
            .with_property("path", role.path.to_string_lossy().to_string());
        
        let role_id = backend.insert_node(role_node)?;
        
        // Add role dependencies
        for dep in &role.meta.dependencies {
            let dep_node = Node::new(NodeType::AnsibleRole, dep.name.clone());
            let dep_id = backend.insert_node(dep_node)?;
            
            backend.insert_edge(Edge::new(
                role_id,
                dep_id,
                EdgeType::DependsOnRole,
            ))?;
        }
        
        // Add tasks
        for task in &role.tasks {
            let task_id = self.build_task_graph(task, backend)?;
            backend.insert_edge(Edge::new(
                role_id,
                task_id,
                EdgeType::Contains,
            ))?;
        }
        
        Ok(role_id)
    }
}
```

Add integration tests:

```rust
#[test]
fn test_ansible_graph_construction() {
    use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
    
    let mut backend = MemoryBackend::new();
    let parser = AnsibleParser::new();
    
    let playbook = create_test_playbook(); // Helper function
    parser.build_graph(&playbook, &mut backend).unwrap();
    
    let nodes = backend.all_nodes().unwrap();
    assert!(nodes.iter().any(|n| n.node_type == NodeType::AnsiblePlaybook));
    assert!(nodes.iter().any(|n| n.node_type == NodeType::AnsibleTask));
    assert!(nodes.iter().any(|n| n.node_type == NodeType::AnsibleRole));
    
    let edges = backend.all_edges().unwrap();
    assert!(edges.iter().any(|e| e.edge_type == EdgeType::IncludesRole));
    assert!(edges.iter().any(|e| e.edge_type == EdgeType::ExecutesTask));
}
```

## Task 16.3.2: Ansible Security Analysis (Days 10-12)

**File**: `src/security/ansible.rs`

```rust
//! Ansible security scanning for common vulnerabilities.

use crate::extraction::ansible::{AnsiblePlaybook, Task};
use crate::security::{SecurityFinding, Severity};
use serde_yaml::Value;
use std::collections::HashSet;

pub struct AnsibleSecurityScanner {
    sensitive_modules: HashSet<String>,
    dangerous_modules: HashSet<String>,
}

impl Default for AnsibleSecurityScanner {
    fn default() -> Self {
        Self::new()
    }
}

impl AnsibleSecurityScanner {
    pub fn new() -> Self {
        let sensitive_modules: HashSet<String> = [
            "shell", "command", "raw", "script",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        
        let dangerous_modules: HashSet<String> = [
            "user", "authorized_key", "mysql_user", "postgresql_user",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        
        Self {
            sensitive_modules,
            dangerous_modules,
        }
    }
    
    pub fn scan_playbook(&self, playbook: &AnsiblePlaybook) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        
        for play in &playbook.plays {
            for task in &play.tasks {
                findings.extend(self.scan_task(task));
            }
            for task in &play.pre_tasks {
                findings.extend(self.scan_task(task));
            }
            for task in &play.post_tasks {
                findings.extend(self.scan_task(task));
            }
        }
        
        findings
    }
    
    fn scan_task(&self, task: &Task) -> Vec<SecurityFinding> {
        let mut findings = Vec::new();
        
        // Check for hardcoded secrets
        if let Some(finding) = self.check_hardcoded_secrets(task) {
            findings.push(finding);
        }
        
        // Check for command injection risks
        if self.sensitive_modules.contains(&task.module) {
            if let Some(finding) = self.check_command_injection(task) {
                findings.push(finding);
            }
        }
        
        // Check for become escalation
        if let Some(finding) = self.check_become_usage(task) {
            findings.push(finding);
        }
        
        // Check for no_log missing on sensitive data
        if let Some(finding) = self.check_no_log_missing(task) {
            findings.push(finding);
        }
        
        findings
    }
    
    fn check_hardcoded_secrets(&self, task: &Task) -> Option<SecurityFinding> {
        let task_str = format!("{:?}", task.args);
        
        let secret_patterns = [
            "password", "passwd", "pwd", "secret", "token", "api_key",
            "private_key", "credentials", "auth",
        ];
        
        for pattern in &secret_patterns {
            if task_str.to_lowercase().contains(pattern) {
                // Check if it's a hardcoded value (not a variable reference)
                if !task_str.contains("{{") {
                    return Some(SecurityFinding {
                        severity: Severity::High,
                        message: format!("Potential hardcoded secret in task '{}'", task.name),
                        location: task.name.clone(),
                        cwe: Some("CWE-798".to_string()),
                        remediation: Some("Use Ansible Vault or variables instead of hardcoded secrets".to_string()),
                    });
                }
            }
        }
        
        None
    }
    
    fn check_command_injection(&self, task: &Task) -> Option<SecurityFinding> {
        // Check if shell/command uses variables unsafely
        if let Some(cmd) = task.args.get("_raw").or_else(|| task.args.get("cmd")) {
            let cmd_str = format!("{:?}", cmd);
            
            if cmd_str.contains("{{") && !cmd_str.contains("| quote") {
                return Some(SecurityFinding {
                    severity: Severity::Critical,
                    message: format!(
                        "Potential command injection in task '{}' - variable not properly quoted",
                        task.name
                    ),
                    location: task.name.clone(),
                    cwe: Some("CWE-78".to_string()),
                    remediation: Some("Use the '| quote' filter on variables in shell commands".to_string()),
                });
            }
        }
        
        None
    }
    
    fn check_become_usage(&self, task: &Task) -> Option<SecurityFinding> {
        if task.become == Some(true) && !self.is_become_necessary(&task.module) {
            return Some(SecurityFinding {
                severity: Severity::Medium,
                message: format!(
                    "Task '{}' uses 'become: yes' which may not be necessary",
                    task.name
                ),
                location: task.name.clone(),
                cwe: Some("CWE-250".to_string()),
                remediation: Some("Only use privilege escalation when required".to_string()),
            });
        }
        
        None
    }
    
    fn check_no_log_missing(&self, task: &Task) -> Option<SecurityFinding> {
        // Check if task deals with sensitive data but doesn't have no_log
        let has_no_log = task.args.get("no_log")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        
        if !has_no_log && self.dangerous_modules.contains(&task.module) {
            let task_str = format!("{:?}", task.args).to_lowercase();
            if task_str.contains("password") || task_str.contains("secret") {
                return Some(SecurityFinding {
                    severity: Severity::Medium,
                    message: format!(
                        "Task '{}' handles sensitive data but 'no_log: true' is not set",
                        task.name
                    ),
                    location: task.name.clone(),
                    cwe: Some("CWE-532".to_string()),
                    remediation: Some("Add 'no_log: true' to prevent logging sensitive data".to_string()),
                });
            }
        }
        
        None
    }
    
    fn is_become_necessary(&self, module: &str) -> bool {
        matches!(
            module,
            "apt" | "yum" | "dnf" | "service" | "systemd" | "user" | "group"
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_hardcoded_secret() {
        let scanner = AnsibleSecurityScanner::new();
        // Create task with hardcoded password
        let findings = scanner.scan_task(&test_task);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::High);
    }
    
    #[test]
    fn test_command_injection_detection() {
        // Test shell/command with unquoted variables
    }
}
```

**Update**: `src/security/mod.rs`

```rust
pub mod ansible;
```

---

# Week 3: CLI, MCP, Testing & Documentation

## Task 16.4.1: CLI Commands (Days 13-14)

**File**: `src/cli/ansible.rs`

```rust
//! Ansible-specific CLI commands.

use crate::extraction::ansible::AnsibleParser;
use crate::analysis::ansible_roles::RoleDependencyAnalyzer;
use crate::security::ansible::AnsibleSecurityScanner;
use crate::error::Result;
use clap::{Args, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Args)]
pub struct AnsibleArgs {
    #[command(subcommand)]
    pub command: AnsibleCommand,
}

#[derive(Debug, Subcommand)]
pub enum AnsibleCommand {
    /// Analyze Ansible roles and show dependencies
    Roles {
        /// Path to roles directory
        #[arg(default_value = "./roles")]
        path: PathBuf,
        
        /// Show dependency graph
        #[arg(long)]
        show_deps: bool,
        
        /// Output format (text, json, mermaid)
        #[arg(long, default_value = "text")]
        format: String,
    },
    
    /// Validate Ansible playbooks
    Validate {
        /// Path to playbook or directory
        path: PathBuf,
    },
    
    /// Run security scan on playbooks
    SecurityScan {
        /// Path to playbook or directory
        path: PathBuf,
        
        /// Minimum severity to report (low, medium, high, critical)
        #[arg(long, default_value = "medium")]
        min_severity: String,
        
        /// Output format (text, json, sarif)
        #[arg(long, default_value = "text")]
        format: String,
    },
}

pub fn run_ansible_command(args: AnsibleArgs) -> Result<()> {
    match args.command {
        AnsibleCommand::Roles { path, show_deps, format } => {
            let analyzer = RoleDependencyAnalyzer::new();
            let graph = analyzer.analyze_roles_dir(&path)?;
            
            match format.as_str() {
                "json" => {
                    println!("{}", serde_json::to_string_pretty(&graph.roles)?);
                }
                "mermaid" => {
                    print_roles_mermaid(&graph);
                }
                _ => {
                    print_roles_text(&graph, show_deps);
                }
            }
            
            Ok(())
        }
        
        AnsibleCommand::Validate { path } => {
            validate_ansible_path(&path)
        }
        
        AnsibleCommand::SecurityScan { path, min_severity, format } => {
            run_security_scan(&path, &min_severity, &format)
        }
    }
}

fn print_roles_text(graph: &crate::analysis::ansible_roles::RoleDependencyGraph, show_deps: bool) {
    println!("Ansible Roles: {}", graph.roles.len());
    println!();
    
    for (name, node) in &graph.roles {
        println!("Role: {}", name);
        println!("  Path: {}", node.path.display());
        
        if show_deps {
            if !node.dependencies.is_empty() {
                println!("  Dependencies:");
                for dep in &node.dependencies {
                    println!("    - {}", dep);
                }
            }
            if !node.dependents.is_empty() {
                println!("  Dependents:");
                for dep in &node.dependents {
                    println!("    - {}", dep);
                }
            }
        }
        
        println!();
    }
    
    // Show topological order
    if let Ok(sorted) = graph.topological_sort() {
        println!("Dependency Order (topological sort):");
        for (i, role) in sorted.iter().enumerate() {
            println!("  {}. {}", i + 1, role);
        }
    }
}

fn print_roles_mermaid(graph: &crate::analysis::ansible_roles::RoleDependencyGraph) {
    println!("graph TD");
    
    for (name, node) in &graph.roles {
        for dep in &node.dependencies {
            println!("    {}[{}] --> {}[{}]", name, name, dep, dep);
        }
    }
}

fn validate_ansible_path(path: &PathBuf) -> Result<()> {
    let parser = AnsibleParser::new();
    
    if path.is_file() {
        match parser.parse_playbook(path) {
            Ok(playbook) => {
                println!("✓ Valid playbook: {}", playbook.name);
                println!("  Plays: {}", playbook.plays.len());
                for play in &playbook.plays {
                    println!("    - {} (hosts: {})", play.name, play.hosts);
                }
            }
            Err(e) => {
                eprintln!("✗ Invalid playbook: {}", e);
                return Err(e);
            }
        }
    } else if path.is_dir() {
        // Validate all .yml files
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let file_path = entry.path();
                if let Some(ext) = file_path.extension() {
                    if ext == "yml" || ext == "yaml" {
                        match parser.parse_playbook(file_path) {
                            Ok(playbook) => {
                                println!("✓ {}: {} plays", file_path.display(), playbook.plays.len());
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
    let parser = AnsibleParser::new();
    let scanner = AnsibleSecurityScanner::new();
    let mut all_findings = Vec::new();
    
    let playbooks = if path.is_file() {
        vec![parser.parse_playbook(path)?]
    } else {
        // Scan all playbooks in directory
        let mut playbooks = Vec::new();
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry?;
            if entry.file_type().is_file() {
                let file_path = entry.path();
                if let Some(ext) = file_path.extension() {
                    if ext == "yml" || ext == "yaml" {
                        if let Ok(playbook) = parser.parse_playbook(file_path) {
                            playbooks.push(playbook);
                        }
                    }
                }
            }
        }
        playbooks
    };
    
    for playbook in &playbooks {
        let findings = scanner.scan_playbook(playbook);
        all_findings.extend(findings);
    }
    
    // Filter by severity
    let min_sev = parse_severity(min_severity);
    all_findings.retain(|f| f.severity >= min_sev);
    
    // Output
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&all_findings)?);
        }
        "sarif" => {
            // TODO: Implement SARIF format
            eprintln!("SARIF format not yet implemented");
        }
        _ => {
            if all_findings.is_empty() {
                println!("✓ No security issues found");
            } else {
                println!("Security Findings: {}", all_findings.len());
                println!();
                for finding in &all_findings {
                    println!("[{:?}] {}", finding.severity, finding.message);
                    println!("  Location: {}", finding.location);
                    if let Some(cwe) = &finding.cwe {
                        println!("  CWE: {}", cwe);
                    }
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

**Update**: `src/cli/mod.rs`

```rust
pub mod ansible;

// In main CLI enum:
#[derive(Debug, Subcommand)]
pub enum Commands {
    // ... existing commands ...
    
    /// Ansible-specific commands
    #[command(subcommand)]
    Ansible(ansible::AnsibleArgs),
}
```

## Task 16.4.2: MCP Tools (Day 15)

**Update**: `src/mcp/tools.rs`

Add new tools to the list:

```rust
// Add to create_tools() function:

tools.push(Tool {
    name: "analyze_ansible_playbook".to_string(),
    description: "Analyze Ansible playbook structure, tasks, roles, and dependencies".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "playbook_path": {
                "type": "string",
                "description": "Path to the Ansible playbook file"
            }
        },
        "required": ["playbook_path"]
    }),
});

tools.push(Tool {
    name: "find_ansible_roles".to_string(),
    description: "Find and analyze Ansible roles with dependency graph".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "roles_path": {
                "type": "string",
                "description": "Path to roles directory",
                "default": "./roles"
            },
            "show_dependencies": {
                "type": "boolean",
                "description": "Include dependency graph",
                "default": true
            }
        }
    }),
});

tools.push(Tool {
    name: "ansible_security_scan".to_string(),
    description: "Scan Ansible playbooks for security vulnerabilities (hardcoded secrets, command injection, etc.)".to_string(),
    input_schema: json!({
        "type": "object",
        "properties": {
            "playbook_path": {
                "type": "string",
                "description": "Path to playbook file or directory"
            },
            "min_severity": {
                "type": "string",
                "description": "Minimum severity level (low, medium, high, critical)",
                "default": "medium"
            }
        },
        "required": ["playbook_path"]
    }),
});
```

Add handler functions:

```rust
async fn handle_tool_call(name: &str, args: serde_json::Value) -> Result<serde_json::Value> {
    match name {
        // ... existing handlers ...
        
        "analyze_ansible_playbook" => {
            let playbook_path: String = serde_json::from_value(args["playbook_path"].clone())?;
            let parser = crate::extraction::ansible::AnsibleParser::new();
            let playbook = parser.parse_playbook(&PathBuf::from(playbook_path))?;
            
            let result = json!({
                "name": playbook.name,
                "plays": playbook.plays.len(),
                "play_details": playbook.plays.iter().map(|p| json!({
                    "name": p.name,
                    "hosts": p.hosts,
                    "tasks": p.tasks.len(),
                    "roles": p.roles.iter().map(|r| &r.name).collect::<Vec<_>>(),
                })).collect::<Vec<_>>(),
            });
            
            Ok(result)
        }
        
        "find_ansible_roles" => {
            let roles_path: String = args.get("roles_path")
                .and_then(|v| v.as_str())
                .unwrap_or("./roles")
                .to_string();
            
            let analyzer = crate::analysis::ansible_roles::RoleDependencyAnalyzer::new();
            let graph = analyzer.analyze_roles_dir(&PathBuf::from(roles_path))?;
            
            Ok(serde_json::to_value(&graph.roles)?)
        }
        
        "ansible_security_scan" => {
            let playbook_path: String = serde_json::from_value(args["playbook_path"].clone())?;
            let min_severity = args.get("min_severity")
                .and_then(|v| v.as_str())
                .unwrap_or("medium");
            
            let parser = crate::extraction::ansible::AnsibleParser::new();
            let scanner = crate::security::ansible::AnsibleSecurityScanner::new();
            
            let playbook = parser.parse_playbook(&PathBuf::from(playbook_path))?;
            let findings = scanner.scan_playbook(&playbook);
            
            Ok(serde_json::to_value(&findings)?)
        }
        
        _ => Err(Error::NotFound(format!("Unknown tool: {}", name)))
    }
}
```

## Task 16.5.1: Comprehensive Testing (Days 16-18)

Create test fixtures in `tests/fixtures/ansible/`:

```yaml
# tests/fixtures/ansible/playbooks/webserver.yml
---
- name: Setup web servers
  hosts: webservers
  become: yes
  
  roles:
    - common
    - nginx
    - certbot
  
  tasks:
    - name: Install nginx
      apt:
        name: nginx
        state: present
      notify: restart nginx
    
    - name: Deploy config
      template:
        src: nginx.conf.j2
        dest: /etc/nginx/nginx.conf
        owner: root
        mode: '0644'
      notify: restart nginx
  
  handlers:
    - name: restart nginx
      service:
        name: nginx
        state: restarted
```

```yaml
# tests/fixtures/ansible/roles/nginx/meta/main.yml
---
dependencies:
  - role: common
  - role: certbot
```

Add comprehensive test suite in `tests/phase16_ansible.rs`:

```rust
//! Phase 16: Ansible Support - Integration Tests

use rbuilder::extraction::ansible::*;
use rbuilder::analysis::ansible_roles::*;
use rbuilder::security::ansible::*;
use rbuilder::graph::backend::{GraphBackend, MemoryBackend};
use std::path::PathBuf;

// ... (include all unit tests from earlier)

#[test]
fn test_full_playbook_to_graph_pipeline() {
    let playbook_path = PathBuf::from("tests/fixtures/ansible/playbooks/webserver.yml");
    let parser = AnsibleParser::new();
    let playbook = parser.parse_playbook(&playbook_path).unwrap();
    
    let mut backend = MemoryBackend::new();
    parser.build_graph(&playbook, &mut backend).unwrap();
    
    let nodes = backend.all_nodes().unwrap();
    let edges = backend.all_edges().unwrap();
    
    assert!(nodes.len() >= 5); // playbook, play, tasks, roles, handlers
    assert!(edges.len() >= 3); // various relationships
}

#[test]
fn test_role_dependency_analysis_complete() {
    let roles_path = PathBuf::from("tests/fixtures/ansible/roles");
    let analyzer = RoleDependencyAnalyzer::new();
    let graph = analyzer.analyze_roles_dir(&roles_path).unwrap();
    
    assert!(graph.roles.len() >= 2);
    
    // Test topological sort
    let sorted = graph.topological_sort().unwrap();
    assert_eq!(sorted.len(), graph.roles.len());
    
    // Verify dependencies appear before dependents
    let nginx_pos = sorted.iter().position(|r| r == "nginx").unwrap();
    let common_pos = sorted.iter().position(|r| r == "common").unwrap();
    assert!(common_pos < nginx_pos); // common must come before nginx
}

#[test]
fn test_security_scan_detects_all_issues() {
    // Test hardcoded secrets
    // Test command injection
    // Test become misuse
    // Test no_log missing
}

// Target: 35+ tests total across all test functions
```

## Task 16.5.2: Documentation (Days 19-21)

**File**: `docs/ansible_support.md`

```markdown
# Ansible Support

rBuilder provides comprehensive support for analyzing Ansible playbooks, roles, and infrastructure-as-code configurations.

## Features

- **Playbook Parsing**: Extract plays, tasks, handlers, variables from Ansible playbooks
- **Role Analysis**: Analyze role dependencies and detect circular dependencies
- **Variable Tracking**: Track Jinja2 variable usage across playbooks and roles
- **Security Scanning**: Detect hardcoded secrets, command injection risks, and privilege escalation issues
- **Graph Integration**: Build dependency graphs of your Ansible infrastructure
- **Query Support**: Query Ansible structures using rBuilder's DSL

## Supported Ansible Versions

- Ansible 2.9+
- Ansible Core 2.11+

## CLI Usage

### Analyze Roles

```bash
# Show all roles
rbuilder ansible roles

# Show role dependencies
rbuilder ansible roles --show-deps

# Output as Mermaid diagram
rbuilder ansible roles --format mermaid > roles.mmd
```

### Validate Playbooks

```bash
# Validate a single playbook
rbuilder ansible validate playbook.yml

# Validate all playbooks in a directory
rbuilder ansible validate ./playbooks/
```

### Security Scan

```bash
# Scan for security issues
rbuilder ansible security-scan playbook.yml

# Show only high/critical issues
rbuilder ansible security-scan playbook.yml --min-severity high

# Output as JSON
rbuilder ansible security-scan . --format json > findings.json
```

## Query Examples

```bash
# Find all Ansible playbooks
rbuilder query "type:AnsiblePlaybook"

# Find tasks using apt module
rbuilder query "type:AnsibleTask module:apt"

# Find roles with dependencies
rbuilder query "type:AnsibleRole" --with-edges DependsOnRole

# Blast radius: what's affected if nginx role changes?
rbuilder analyze blast-radius "roles/nginx"
```

## MCP Integration

AI agents can use these tools:

### `analyze_ansible_playbook`

```json
{
  "tool": "analyze_ansible_playbook",
  "arguments": {
    "playbook_path": "./site.yml"
  }
}
```

### `find_ansible_roles`

```json
{
  "tool": "find_ansible_roles",
  "arguments": {
    "roles_path": "./roles",
    "show_dependencies": true
  }
}
```

### `ansible_security_scan`

```json
{
  "tool": "ansible_security_scan",
  "arguments": {
    "playbook_path": "./playbooks",
    "min_severity": "medium"
  }
}
```

## Security Checks

rBuilder detects the following security issues:

1. **CWE-798**: Hardcoded Secrets
   - Detection: Looks for password/secret keywords without variable references
   - Remediation: Use Ansible Vault or variables

2. **CWE-78**: Command Injection
   - Detection: shell/command tasks with unquoted variables
   - Remediation: Use `| quote` filter

3. **CWE-250**: Unnecessary Privilege Escalation
   - Detection: `become: yes` on tasks that don't need it
   - Remediation: Remove unnecessary become

4. **CWE-532**: Sensitive Data in Logs
   - Detection: Sensitive modules without `no_log: true`
   - Remediation: Add `no_log: true`

## Graph Schema

### Node Types

- `AnsiblePlaybook`: Top-level playbook
- `AnsiblePlay`: A single play within a playbook
- `AnsibleTask`: Task execution
- `AnsibleRole`: Reusable role
- `AnsibleHandler`: Event-triggered task
- `AnsibleVariable`: Variable definition/usage
- `AnsibleTemplate`: Jinja2 template file

### Edge Types

- `IncludesRole`: Playbook/play includes a role
- `DependsOnRole`: Role depends on another role
- `ExecutesTask`: Play executes a task
- `NotifiesHandler`: Task notifies a handler
- `UsesVariable`: Task/template uses a variable
- `IncludesPlaybook`: Playbook imports another playbook
- `RendersTemplate`: Task renders a template

## Limitations

- **Jinja2 Evaluation**: Variables are tracked but not evaluated (static analysis only)
- **Dynamic Includes**: `include_tasks` with variables may not be fully resolved
- **Ansible Plugins**: Custom modules/plugins are not analyzed
- **Inventory**: Basic inventory parsing only, no dynamic inventory support

## Future Enhancements

- Dynamic inventory analysis
- Ansible Galaxy integration
- Molecule test detection
- Performance profiling for playbook execution
```

**Update**: `README.md`

Add to language support section:

```markdown
### Infrastructure as Code

- **Ansible** (Phase 16) ✅
  - Playbook and role parsing
  - Role dependency analysis
  - Security scanning (hardcoded secrets, command injection)
  - Variable tracking (Jinja2)
```

---

# Final Checklist

## Week 1 Deliverables
- [ ] `src/extraction/ansible.rs` implemented (500+ lines)
- [ ] `src/analysis/ansible_roles.rs` implemented (300+ lines)
- [ ] 15+ unit tests for parser and role analyzer
- [ ] Test fixtures created

## Week 2 Deliverables
- [ ] NodeType/EdgeType enums updated in `src/graph/schema.rs`
- [ ] Graph construction methods implemented
- [ ] `src/security/ansible.rs` implemented (250+ lines)
- [ ] 10+ integration tests for graph construction
- [ ] 8+ security scanner tests

## Week 3 Deliverables
- [ ] `src/cli/ansible.rs` implemented (150+ lines)
- [ ] MCP tools added to `src/mcp/tools.rs`
- [ ] 3+ CLI tests
- [ ] 3+ MCP tests
- [ ] `docs/ansible_support.md` completed
- [ ] README.md updated

## Overall Acceptance Criteria
- [ ] **Total tests**: 35+ (unit + integration + security)
- [ ] **No architecture changes**: All code integrates with existing patterns
- [ ] **Grade target**: A (90%+)
- [ ] **All features working**:
  - [ ] Parse playbooks and roles
  - [ ] Build dependency graph
  - [ ] Detect security issues
  - [ ] CLI commands functional
  - [ ] MCP tools working
  - [ ] Query support via existing DSL
- [ ] **Documentation complete**
- [ ] **All tests passing**

## Commit Message

```
Implement Phase 16: Ansible Support (Grade: A)

Add comprehensive Ansible playbook and role analysis following Tier 1 standards:

Parser & Extraction:
- Parse Ansible playbooks (YAML + Jinja2)
- Extract plays, tasks, handlers, roles, variables
- Role dependency analysis with circular detection
- 500+ lines in src/extraction/ansible.rs

Graph Integration:
- Added 7 Ansible-specific node types
- Added 7 Ansible-specific edge types
- Full graph construction from playbooks/roles
- No architecture changes

Security Scanning:
- Detect hardcoded secrets (CWE-798)
- Command injection risks (CWE-78)
- Unnecessary privilege escalation (CWE-250)
- Sensitive data logging (CWE-532)

CLI & MCP:
- ansible subcommand (roles, validate, security-scan)
- 3 new MCP tools for AI agents
- JSON/Mermaid output formats

Testing & Documentation:
- 35+ tests (parser, graph, security, CLI, MCP)
- docs/ansible_support.md user guide
- Updated README

Files: 6 new, 4 modified
Tests: 35+ (100% target)
Grade: A (90%+)
```

---

**Questions for Cursor**: If you encounter any issues or need clarification, check:
1. Existing YAML parsers: `src/extraction/github_actions.rs`, `src/extraction/gitlab_ci.rs`
2. Graph schema patterns: `src/graph/schema.rs`
3. Security scanner template: `src/security/` (if exists, or create based on patterns)
4. MCP tool examples: `src/mcp/tools.rs`

**Remember**: No architecture changes! Integrate with existing `GraphBackend`, `NodeType`, `EdgeType`, query system, and MCP server.
