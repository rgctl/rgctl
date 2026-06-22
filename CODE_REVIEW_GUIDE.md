# Code Review Guide for rBuilder

This guide defines code review standards for the rBuilder project, combining Rust idioms with project-specific patterns.

## General Rust Idioms

### ✅ DO: Use Idiomatic Patterns

**Error Handling**
```rust
// ✅ Good: Return Result types
pub fn parse_file(path: &Path) -> Result<Vec<Symbol>> {
    let content = std::fs::read_to_string(path)?;
    Ok(parse_content(&content))
}

// ❌ Bad: Unwrap or panic
pub fn parse_file(path: &Path) -> Vec<Symbol> {
    let content = std::fs::read_to_string(path).unwrap();
    parse_content(&content)
}
```

**String Parameters**
```rust
// ✅ Good: Accept &str for flexibility
pub fn analyze_code(source: &str) -> Analysis {
    // ...
}

// ❌ Bad: Require owned String
pub fn analyze_code(source: String) -> Analysis {
    // ...
}
```

**Iterator Chains**
```rust
// ✅ Good: Functional iterator patterns
let symbols: Vec<_> = nodes
    .iter()
    .filter(|n| n.node_type == NodeType::Function)
    .map(|n| Symbol::from_node(n))
    .collect();

// ❌ Bad: Imperative loops
let mut symbols = Vec::new();
for node in &nodes {
    if node.node_type == NodeType::Function {
        symbols.push(Symbol::from_node(node));
    }
}
```

**Pattern Matching**
```rust
// ✅ Good: Exhaustive match expressions
match node.node_type {
    NodeType::Function => handle_function(node),
    NodeType::Class => handle_class(node),
    NodeType::Module => handle_module(node),
    _ => Ok(()),
}

// ❌ Bad: Boolean checks for enums
if node.node_type == NodeType::Function {
    handle_function(node)
} else if node.node_type == NodeType::Class {
    handle_class(node)
}
```

### ❌ AVOID: Anti-Patterns

**Premature Optimization**
```rust
// ❌ Bad: Complex optimization without profiling
pub fn find_nodes(&self) -> Vec<&Node> {
    // Complex caching, unsafe pointers, etc.
}

// ✅ Good: Simple, correct implementation first
pub fn find_nodes(&self) -> Vec<&Node> {
    self.nodes.iter().collect()
}
```

**Overuse of Unsafe**
```rust
// ❌ Bad: Unnecessary unsafe
unsafe {
    std::ptr::read(ptr)
}

// ✅ Good: Safe alternatives when possible
value.clone()
```

**Overly Restrictive Bounds**
```rust
// ❌ Bad: Unnecessarily strict
pub fn process<T: Send + Sync + 'static + Clone>(data: T) {}

// ✅ Good: Minimal necessary bounds
pub fn process<T: Clone>(data: T) {}
```

## Project-Specific Standards

### Plugin Architecture

**LanguagePlugin Implementation**
```rust
// ✅ Good: Complete trait implementation
impl LanguagePlugin for ChefPlugin {
    fn language_id(&self) -> &str {
        "chef"
    }

    fn file_extensions(&self) -> Vec<&str> {
        vec![]  // Path-based routing instead
    }

    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        let text = std::str::from_utf8(source)?;
        Ok(self.parser.parse(file_path, text).0)
    }
}
```

**Path-Based Routing Pattern**
```rust
// ✅ Good: Early return for non-matching paths
pub fn parse(&self, file_path: &Path, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
    if !Self::is_chef_path(&file_path.to_string_lossy()) {
        return (vec![], vec![]);
    }
    // Parse logic here
}

// ❌ Bad: Deep nesting
pub fn parse(&self, file_path: &Path, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
    if Self::is_chef_path(&file_path.to_string_lossy()) {
        // Deep nested logic
    } else {
        (vec![], vec![])
    }
}
```

### Graph Operations

**Node Creation**
```rust
// ✅ Good: Use constructor and builder pattern
let mut node = Node::new(NodeType::ChefResource, name.to_string());
node.properties.insert("resource_type".into(), resource_type.into());
node.signature = Some(props.to_string());

// ❌ Bad: Manual struct construction
let node = Node {
    node_type: NodeType::ChefResource,
    name: name.to_string(),
    properties: HashMap::new(),
    signature: None,
    // Missing fields...
};
```

**Query Pattern**
```rust
// ✅ Good: Use backend methods
let nodes = backend
    .find_nodes_by_type(NodeType::ChefResource)
    .unwrap_or_default();

// ❌ Bad: Direct iteration
let mut nodes = Vec::new();
for node in backend.all_nodes() {
    if node.node_type == NodeType::ChefResource {
        nodes.push(node);
    }
}
```

### Security Scanning

**CWE Pattern Detection**
```rust
// ✅ Good: Clear severity levels and remediation
ChefSecurityFinding {
    severity: ChefSeverity::Critical,
    message: format!("Potential command injection in {resource_type}"),
    location: name.to_string(),
    cwe: Some("CWE-78".into()),
    remediation: Some("Use Shellwords.escape for interpolation".into()),
    resource_type: Some(resource_type.to_string()),
}

// ❌ Bad: Generic warnings without context
ChefSecurityFinding {
    severity: ChefSeverity::Medium,
    message: "Security issue".to_string(),
    location: name.to_string(),
    cwe: None,
    remediation: None,
    resource_type: None,
}
```

### CLI Design

**Subcommand Pattern**
```rust
// ✅ Good: Descriptive args with defaults
#[derive(Debug, Subcommand)]
pub enum ChefCommand {
    SecurityScan {
        path: PathBuf,
        #[arg(long, default_value = "medium")]
        min_severity: String,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        from_graph: bool,
    },
}

// ❌ Bad: No defaults or unclear args
#[derive(Debug, Subcommand)]
pub enum ChefCommand {
    SecurityScan {
        path: PathBuf,
        severity: String,
        fmt: String,
        graph: bool,
    },
}
```

**Output Format Support**
```rust
// ✅ Good: Match with exhaustive patterns
match format.as_str() {
    "json" => println!("{}", serde_json::to_string_pretty(&data)?),
    "mermaid" => print_mermaid_diagram(&data),
    _ => print_text_output(&data),
}

// ❌ Bad: If-else chains
if format == "json" {
    // ...
} else if format == "mermaid" {
    // ...
}
```

## Testing Requirements

### Test Coverage

**Minimum Test Count**: 30+ tests per phase
```rust
// ✅ Good: Comprehensive test suite
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_detection() { /* ... */ }

    #[test]
    fn test_symbol_extraction() { /* ... */ }

    #[test]
    fn test_relation_extraction() { /* ... */ }

    #[test]
    fn test_security_scanning() { /* ... */ }

    #[test]
    fn test_graph_integration() { /* ... */ }

    // ... 25+ more tests
}
```

**Test Structure**
```rust
// ✅ Good: Arrange-Act-Assert pattern
#[test]
fn test_command_injection_detection() {
    // Arrange
    let scanner = ChefSecurityScanner::new();
    let mut node = Node::new(NodeType::ChefResource, "run_cmd".into());
    node.properties.insert("resource_type".into(), "execute".into());
    node.signature = Some("command #{user_input}".into());

    // Act
    let findings = scanner.scan_node(&node);

    // Assert
    assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-78")));
}

// ❌ Bad: Unclear test structure
#[test]
fn test_scanner() {
    let scanner = ChefSecurityScanner::new();
    let node = make_node();
    assert!(scanner.scan_node(&node).len() > 0);
}
```

### Integration Tests

**Fixture Pattern**
```rust
// ✅ Good: Reusable test fixtures
fn create_test_cookbook() -> &'static str {
    r#"
package 'nginx' do
  action :install
end
"#
}

#[test]
fn test_recipe_parsing() {
    let source = create_test_cookbook();
    let parser = ChefParser::new();
    let (symbols, _) = parser.parse("cookbooks/nginx/recipes/default.rb", source);
    assert!(!symbols.is_empty());
}
```

## Documentation Standards

### Public API Documentation

**Required Elements**
```rust
/// Scans Chef resource nodes for security vulnerabilities.
///
/// # Examples
///
/// ```
/// use rbuilder::security::chef::ChefSecurityScanner;
/// use rbuilder::graph::schema::{Node, NodeType};
///
/// let scanner = ChefSecurityScanner::new();
/// let node = Node::new(NodeType::ChefResource, "test".into());
/// let findings = scanner.scan_node(&node);
/// ```
///
/// # Security Checks
///
/// - CWE-78: Command injection in execute/bash/script resources
/// - CWE-798: Hardcoded secrets
/// - CWE-732: Insecure file permissions
pub fn scan_node(&self, node: &Node) -> Vec<ChefSecurityFinding> {
    // Implementation
}
```

**Module Documentation**
```rust
//! Chef security scanning against graph resource nodes.
//!
//! This module provides security scanning capabilities for Chef cookbooks
//! indexed in the rBuilder knowledge graph. It detects common security
//! anti-patterns and maps them to CWE identifiers.
```

### User-Facing Documentation

**CLI Help Text**
```rust
// ✅ Good: Clear descriptions
/// Run security scan on Chef cookbooks
SecurityScan {
    /// Path to cookbooks directory
    path: PathBuf,
    /// Minimum severity level to report (low, medium, high, critical)
    #[arg(long, default_value = "medium")]
    min_severity: String,
}
```

## Error Handling

### Error Types

**Use Custom Error Enum**
```rust
// ✅ Good: Specific error variants
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Parse error in {file}:{line} - {message}")]
    ParseError {
        file: PathBuf,
        line: usize,
        message: String,
    },
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ❌ Bad: Generic error strings
pub type Error = String;
```

**Propagate Errors with ?**
```rust
// ✅ Good: Use ? operator
pub fn analyze_cookbook(path: &Path) -> Result<Analysis> {
    let content = std::fs::read_to_string(path)?;
    let parsed = parse_metadata(&content)?;
    Ok(create_analysis(parsed))
}

// ❌ Bad: Manual error handling
pub fn analyze_cookbook(path: &Path) -> Result<Analysis> {
    match std::fs::read_to_string(path) {
        Ok(content) => match parse_metadata(&content) {
            Ok(parsed) => Ok(create_analysis(parsed)),
            Err(e) => Err(e),
        },
        Err(e) => Err(e.into()),
    }
}
```

## Code Review Checklist

### For Reviewers

- [ ] Code follows Rust idioms (iterators, pattern matching, error handling)
- [ ] No unnecessary `unsafe`, `unwrap()`, or `expect()` calls
- [ ] Public APIs are documented with examples
- [ ] Tests cover happy path and edge cases (30+ tests for new phases)
- [ ] Error messages are actionable and include context
- [ ] Security patterns follow CWE mapping conventions
- [ ] CLI commands have clear help text and sensible defaults
- [ ] No hardcoded paths or magic strings
- [ ] Plugin implementation follows LanguagePlugin trait pattern
- [ ] Graph operations use backend methods correctly
- [ ] No performance bottlenecks (profile before optimizing)
- [ ] Code is readable without comments (self-documenting)

### For Authors

Before requesting review:

- [ ] Run `cargo test` - all tests pass
- [ ] Run `cargo clippy` - no warnings
- [ ] Run `cargo fmt` - code is formatted
- [ ] Run `cargo doc --open` - documentation builds
- [ ] Add/update integration tests in `tests/`
- [ ] Update relevant documentation in `docs/`
- [ ] Verify security scanning patterns are correct
- [ ] Test CLI commands manually
- [ ] Check for TODO/FIXME comments

## Common Issues

### Memory & Performance

**Clone vs Reference**
```rust
// ✅ Good: Borrow when possible
pub fn process_nodes(&self, nodes: &[Node]) -> Vec<Symbol> {
    nodes.iter().map(|n| Symbol::from_node(n)).collect()
}

// ❌ Bad: Unnecessary clones
pub fn process_nodes(&self, nodes: Vec<Node>) -> Vec<Symbol> {
    nodes.iter().map(|n| Symbol::from_node(n)).collect()
}
```

**String Allocations**
```rust
// ✅ Good: Use &str when not storing
fn check_pattern(&self, text: &str) -> bool {
    text.contains("pattern")
}

// ❌ Bad: Force String allocation
fn check_pattern(&self, text: String) -> bool {
    text.contains("pattern")
}
```

### Naming Conventions

**Clear, Descriptive Names**
```rust
// ✅ Good
pub struct CookbookDependencyGraph {
    pub cookbooks: HashMap<String, CookbookNode>,
}

// ❌ Bad
pub struct CDG {
    pub cbs: HashMap<String, CBNode>,
}
```

**Verb Functions, Noun Structs**
```rust
// ✅ Good
fn scan_node(&self, node: &Node) -> Vec<Finding>
struct ChefSecurityScanner

// ❌ Bad
fn node_scanner(&self, node: &Node) -> Vec<Finding>
struct ScanningChef
```

## Resources

- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- [Idiomatic Rust Resources](https://corrode.dev/blog/idiomatic-rust-resources/)
- [Rust Design Patterns](https://rust-unofficial.github.io/patterns/)
- [thiserror crate](https://docs.rs/thiserror/) - Error handling
- [clap crate](https://docs.rs/clap/) - CLI design

## Version

This guide applies to rBuilder phases 16+ (multi-modal IaC support).
Last updated: 2026-06-18
