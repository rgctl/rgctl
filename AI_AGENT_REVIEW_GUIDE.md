# AI Agent Code Review Guide for rBuilder

This guide is specifically designed for AI coding agents (Claude Code, GitHub Copilot, Cursor, OpenCode, etc.) to perform systematic code reviews on the rBuilder project.

## Quick Start for AI Agents

When reviewing rBuilder code, follow this checklist:

1. **Read**: `CODE_REVIEW_GUIDE.md` (human-readable standards)
2. **Check**: This guide for automated review patterns
3. **Review**: Against phase-specific requirements in `.github/TASK_PLAN.md`
4. **Report**: Findings using the template at the end of this document

---

## Review Scope by File Pattern

### Language Plugins: `src/languages/**/*.rs`

**What to Check**:
```rust
// ✅ Must implement LanguagePlugin trait
impl LanguagePlugin for YourPlugin {
    fn language_id(&self) -> &str;
    fn file_extensions(&self) -> Vec<&str>;
    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>>;
    fn extract_relations(&self, file_path: &Path, source: &[u8], symbols: &[Symbol]) -> Result<Vec<Relation>>;
}

// ✅ Path-based routing should return early
pub fn parse(&self, file_path: &Path, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
    if !Self::is_relevant_path(file_path) {
        return (vec![], vec![]);  // ✅ Early return pattern
    }
    // Parse logic
}

// ❌ Anti-pattern: Deep nesting
pub fn parse(&self, file_path: &Path, source: &str) -> (Vec<Symbol>, Vec<Relation>) {
    if Self::is_relevant_path(file_path) {
        // Many nested levels...
    }
}
```

**Automated Checks**:
```bash
# Check 1: All plugins implement the trait
grep -r "impl LanguagePlugin" src/languages/

# Check 2: No unwrap() in plugin code
grep -r "\.unwrap()" src/languages/ | grep -v "test"

# Check 3: Proper error propagation with ?
grep -r "Result<" src/languages/ | wc -l
```

---

### Multi-Modal Plugins: `src/languages/multimodal/**/*.rs`

**Required Structure**:
```
src/languages/multimodal/<tool>/
  ├── mod.rs           # Plugin implementation
  ├── parser.rs        # DSL/YAML parser
  └── tests/           # Unit tests (optional, usually in tests/)
```

**What to Check**:

1. **Parser Module** (`parser.rs`):
```rust
// ✅ Must have path detection
impl Parser {
    pub fn is_ansible_path(path: &str) -> bool {
        path.contains("/playbooks/") 
            || path.contains("/roles/")
            || path.ends_with("playbook.yml")
    }
}

// ✅ Must return (Vec<Symbol>, Vec<Relation>)
pub fn parse(&self, file_path: &str, content: &str) -> (Vec<Symbol>, Vec<Relation>) {
    // Implementation
}

// ❌ Don't panic on parse errors
// Bad: panic!("Parse failed")
// Good: Log warning and return empty results
```

2. **Analysis Module** (if applicable):
```rust
// ✅ Dependency graph analysis
pub struct DependencyGraph {
    pub items: HashMap<String, ItemNode>,
}

impl DependencyGraph {
    pub fn topological_sort(&self) -> Result<Vec<String>> { }
    pub fn detect_cycles(&self) -> Vec<Vec<String>> { }
}
```

3. **Integration Pattern**:
```rust
// ✅ Plugin must integrate with LanguageRegistry
// Check in src/languages/registry.rs:
registry.register_multimodal(Box::new(AnsiblePlugin::new()?));
```

**Automated Checks**:
```bash
# Check for consistent naming
find src/languages/multimodal -name "*.rs" -exec basename {} \; | sort | uniq -c

# Check for missing security modules
ls -la src/security/{ansible,chef,puppet}.rs

# Check for missing CLI modules
ls -la src/cli/{ansible,chef,puppet}.rs

# Check for missing tests
ls -la tests/phase{16,17,18}_*.rs
```

---

### Security Scanners: `src/security/*.rs`

**Required Pattern**:
```rust
// ✅ Severity enum
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

// ✅ Finding struct with CWE
pub struct SecurityFinding {
    pub severity: Severity,
    pub message: String,
    pub location: String,
    pub cwe: Option<String>,           // ✅ Must map to CWE
    pub remediation: Option<String>,   // ✅ Must provide remediation
    pub resource_type: Option<String>,
}

// ✅ Scanner struct
pub struct SecurityScanner {
    dangerous_patterns: HashSet<String>,
}

impl SecurityScanner {
    pub fn new() -> Self { }
    pub fn scan_graph(&self, backend: &MemoryBackend) -> Vec<SecurityFinding> { }
    pub fn scan_node(&self, node: &Node) -> Vec<SecurityFinding> { }
    pub fn filter_by_severity(findings: Vec<SecurityFinding>, min: Severity) -> Vec<SecurityFinding> { }
}
```

**CWE Mapping Requirements**:
- **CWE-78**: Command injection (exec/bash/script resources with interpolation)
- **CWE-798**: Hardcoded secrets (passwords, tokens, API keys)
- **CWE-732**: Insecure file permissions (0666, 0777)
- **CWE-250**: Unnecessary privilege escalation (become: yes)
- **CWE-532**: Sensitive data logging (debug: var=secret)

**Automated Checks**:
```bash
# Check 1: All scanners have CWE mappings
for scanner in src/security/{ansible,chef,puppet}.rs; do
    echo "Checking $scanner..."
    grep -c "CWE-" "$scanner"
done

# Check 2: All findings include remediation
grep -A 5 "SecurityFinding" src/security/*.rs | grep "remediation"

# Check 3: Severity levels are used correctly
grep "severity: .*Severity::" src/security/*.rs
```

---

### CLI Commands: `src/cli/*.rs`

**Required Structure**:
```rust
// ✅ Command enum with clap derive
#[derive(Debug, Subcommand)]
pub enum ToolCommand {
    /// Analyze items and show dependencies
    Items {
        #[arg(default_value = "./path")]
        path: PathBuf,
        #[arg(long)]
        show_deps: bool,
        #[arg(long, default_value = "text")]
        format: String,
        #[arg(long)]
        from_graph: bool,
    },
    /// Validate files
    Validate { path: PathBuf },
    /// Run security scan
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

// ✅ Run function with proper error handling
pub fn run_command(repo: &Path, args: ToolArgs) -> Result<()> {
    match args.command {
        ToolCommand::Items { path, show_deps, format, from_graph } => {
            // Implementation with proper Result handling
        }
        // Other commands...
    }
}

// ✅ Format output support
match format.as_str() {
    "json" => println!("{}", serde_json::to_string_pretty(&data)?),
    "mermaid" => print_mermaid(&data),
    _ => print_text(&data),
}
```

**Consistency Requirements**:

All IaC CLI commands must support:
- `--show-deps` flag for dependency visualization
- `--format` flag with options: `text`, `json`, `mermaid`
- `--from-graph` flag to use indexed graph vs on-disk parsing
- `--min-severity` flag for security scans

**Automated Checks**:
```bash
# Check CLI consistency
for cli in src/cli/{ansible,chef,puppet}.rs; do
    echo "Checking $cli..."
    grep -q "show_deps" "$cli" && echo "✅ has show_deps"
    grep -q "format" "$cli" && echo "✅ has format"
    grep -q "from_graph" "$cli" && echo "✅ has from_graph"
    grep -q "min_severity" "$cli" && echo "✅ has min_severity"
done
```

---

### Graph Operations: `src/graph/*.rs`

**What to Check**:
```rust
// ✅ Good: Use backend methods
let nodes = backend
    .find_nodes_by_type(NodeType::Function)
    .unwrap_or_default();

// ❌ Bad: Manual iteration
let mut nodes = Vec::new();
for node in backend.all_nodes().unwrap() {
    if node.node_type == NodeType::Function {
        nodes.push(node);
    }
}

// ✅ Good: Proper node construction
let mut node = Node::new(NodeType::ChefResource, name.to_string());
node.properties.insert("key".into(), "value".into());
node.signature = Some(signature.to_string());

// ❌ Bad: Direct struct instantiation
let node = Node {
    node_type: NodeType::ChefResource,
    name: name.to_string(),
    // Missing fields...
};
```

**Schema Validation**:
```rust
// ✅ Ensure NodeType includes all new types
pub enum NodeType {
    // Core types
    File, Module, Function, Class, Method,
    
    // Ansible types
    AnsiblePlaybook, AnsibleRole, AnsibleTask, AnsibleHandler,
    
    // Chef types
    ChefCookbook, ChefRecipe, ChefResource, ChefAttribute, ChefTemplate,
    
    // Puppet types (when implemented)
    PuppetModule, PuppetClass, PuppetResource, PuppetVariable,
}

// ✅ Ensure EdgeType includes all relationships
pub enum EdgeType {
    // Core edges
    Imports, Calls, Inherits,
    
    // Ansible edges
    IncludesRole, HasTask, NotifiesHandler, UsesTemplate,
    
    // Chef edges
    DependsOnCookbook, DeclaresResource, IncludesRecipe,
    
    // Puppet edges (when implemented)
    DependsOnModule, IncludesClass, RequiresResource,
}
```

---

### Test Files: `tests/phase*.rs`

**Minimum Requirements**:
- **30+ tests per phase** (target: 35+)
- Tests must cover:
  - Path detection
  - Symbol extraction
  - Relation extraction
  - Graph integration
  - Query execution
  - Security scanning
  - Dependency analysis

**Test Structure Pattern**:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    // ✅ Good test: Clear AAA pattern
    #[test]
    fn test_path_detection() {
        // Arrange
        let parser = AnsibleParser::new();
        let path = "roles/nginx/tasks/main.yml";
        
        // Act
        let result = AnsibleParser::is_ansible_path(path);
        
        // Assert
        assert!(result);
    }

    // ✅ Good: Test name describes what it tests
    #[test]
    fn test_security_scanner_detects_command_injection() {
        let scanner = SecurityScanner::new();
        let mut node = Node::new(NodeType::Resource, "exec".into());
        node.signature = Some("command #{user_input}".into());
        
        let findings = scanner.scan_node(&node);
        
        assert!(findings.iter().any(|f| f.cwe.as_deref() == Some("CWE-78")));
    }

    // ❌ Bad: Unclear test name and purpose
    #[test]
    fn test_thing() {
        let result = do_stuff();
        assert!(result.is_ok());
    }
}
```

**Automated Checks**:
```bash
# Check 1: Test count per phase
for test_file in tests/phase{16,17,18}_*.rs; do
    count=$(grep -c "^fn test_" "$test_file")
    echo "$test_file: $count tests"
    [ $count -ge 30 ] && echo "✅ Meets minimum" || echo "❌ Below minimum (30+)"
done

# Check 2: All tests pass
cargo test --all-features

# Check 3: No ignored tests
grep "#\[ignore\]" tests/*.rs
```

---

### Documentation: `docs/*.md`

**Required Files for Each IaC Tool**:
- `docs/ansible_support.md` ✅
- `docs/chef_support.md` ✅
- `docs/puppet_support.md` (pending Phase 18)

**Required Sections**:
```markdown
# Tool Support (Phase N)

## What gets indexed
[Table of artifacts, node types, edge types]

## Enable
[Feature flag commands]

## Index
[Index commands]

## Query examples
[CLI query examples with expected output]

## CLI
[All CLI commands with examples]

## MCP tools
[Table of MCP tools and their purposes]

## Security checks
[CWE mappings with descriptions]

## Architecture
[Implementation overview]
```

**Automated Checks**:
```bash
# Check all sections present
for doc in docs/{ansible,chef,puppet}_support.md; do
    [ -f "$doc" ] || continue
    echo "Checking $doc..."
    grep -q "## What gets indexed" "$doc" && echo "✅ Has 'What gets indexed'"
    grep -q "## Enable" "$doc" && echo "✅ Has 'Enable'"
    grep -q "## Query examples" "$doc" && echo "✅ Has 'Query examples'"
    grep -q "## CLI" "$doc" && echo "✅ Has 'CLI'"
    grep -q "## MCP tools" "$doc" && echo "✅ Has 'MCP tools'"
    grep -q "## Security checks" "$doc" && echo "✅ Has 'Security checks'"
done
```

---

## Automated Review Script

Use this script to perform automated checks:

```bash
#!/bin/bash
# scripts/ai_code_review.sh

set -e

echo "🤖 AI Agent Code Review for rBuilder"
echo "===================================="
echo ""

# 1. Format Check
echo "📋 Checking code format..."
cargo fmt -- --check || {
    echo "❌ Format check failed. Run: cargo fmt"
    exit 1
}
echo "✅ Format check passed"
echo ""

# 2. Clippy Check
echo "📋 Running clippy..."
cargo clippy --all-targets --all-features -- -D warnings || {
    echo "❌ Clippy found issues"
    exit 1
}
echo "✅ Clippy passed"
echo ""

# 3. Test Check
echo "📋 Running tests..."
cargo test --all-features || {
    echo "❌ Tests failed"
    exit 1
}
echo "✅ All tests passed"
echo ""

# 4. Test Count Check
echo "📋 Checking test coverage..."
for test_file in tests/phase{16,17,18}_*.rs; do
    [ -f "$test_file" ] || continue
    count=$(grep -c "^    fn test_" "$test_file" || echo "0")
    echo "$test_file: $count tests"
    if [ $count -ge 30 ]; then
        echo "  ✅ Meets minimum (30+)"
    else
        echo "  ⚠️  Below target: $count/30"
    fi
done
echo ""

# 5. Security Pattern Check
echo "📋 Checking security patterns..."
for scanner in src/security/{ansible,chef,puppet}.rs; do
    [ -f "$scanner" ] || continue
    cwe_count=$(grep -c "CWE-" "$scanner")
    echo "$scanner: $cwe_count CWE patterns"
    if [ $cwe_count -ge 3 ]; then
        echo "  ✅ Has multiple CWE patterns"
    else
        echo "  ⚠️  Limited CWE coverage: $cwe_count"
    fi
done
echo ""

# 6. Documentation Check
echo "📋 Checking documentation..."
cargo doc --no-deps --all-features 2>&1 | grep -i "warning" && {
    echo "⚠️  Documentation warnings found"
} || {
    echo "✅ Documentation builds cleanly"
}
echo ""

# 7. Unwrap Detection (dangerous patterns)
echo "📋 Checking for dangerous patterns..."
unwrap_count=$(grep -r "\.unwrap()" src/ --include="*.rs" | grep -v "test" | wc -l)
expect_count=$(grep -r "\.expect(" src/ --include="*.rs" | grep -v "test" | wc -l)
echo "Found $unwrap_count unwrap() calls (excluding tests)"
echo "Found $expect_count expect() calls (excluding tests)"
if [ $((unwrap_count + expect_count)) -gt 50 ]; then
    echo "  ⚠️  High count of unwrap/expect - review error handling"
else
    echo "  ✅ Reasonable unwrap/expect usage"
fi
echo ""

# 8. Plugin Consistency Check
echo "📋 Checking plugin consistency..."
for plugin_dir in src/languages/multimodal/{ansible,chef,puppet}; do
    [ -d "$plugin_dir" ] || continue
    plugin_name=$(basename "$plugin_dir")
    echo "Checking $plugin_name plugin..."
    
    [ -f "$plugin_dir/mod.rs" ] && echo "  ✅ Has mod.rs" || echo "  ❌ Missing mod.rs"
    [ -f "$plugin_dir/parser.rs" ] && echo "  ✅ Has parser.rs" || echo "  ❌ Missing parser.rs"
    [ -f "src/security/$plugin_name.rs" ] && echo "  ✅ Has security scanner" || echo "  ⚠️  Missing security scanner"
    [ -f "src/cli/$plugin_name.rs" ] && echo "  ✅ Has CLI commands" || echo "  ⚠️  Missing CLI commands"
    
    test_file=$(find tests -name "phase*_$plugin_name.rs" | head -1)
    if [ -f "$test_file" ]; then
        echo "  ✅ Has test file: $test_file"
    else
        echo "  ⚠️  Missing test file"
    fi
done
echo ""

echo "🎉 Code review complete!"
```

**Make it executable**:
```bash
chmod +x scripts/ai_code_review.sh
```

**Run it**:
```bash
./scripts/ai_code_review.sh
```

---

## Phase-Specific Review Checklists

### Phase 16: Ansible Support

- [ ] `src/languages/multimodal/ansible/mod.rs` implements `LanguagePlugin`
- [ ] `src/languages/multimodal/ansible/parser.rs` parses YAML playbooks/roles
- [ ] Parser detects Jinja2 variables correctly
- [ ] `src/analysis/ansible_roles.rs` builds dependency graph
- [ ] `src/security/ansible.rs` detects 4+ CWE patterns
- [ ] `src/cli/ansible.rs` has 3 subcommands (roles, validate, security-scan)
- [ ] `tests/phase16_ansible.rs` has 30+ tests ✅ (34 tests)
- [ ] `docs/ansible_support.md` complete

### Phase 17: Chef Support

- [ ] `src/languages/multimodal/chef/mod.rs` implements `LanguagePlugin`
- [ ] `src/languages/multimodal/chef/parser.rs` parses Chef Ruby DSL with regex
- [ ] Parser extracts resources, recipes, metadata, attributes
- [ ] `src/analysis/chef_cookbooks.rs` builds dependency graph
- [ ] `src/security/chef.rs` detects 3+ CWE patterns
- [ ] `src/cli/chef.rs` has 3 subcommands (cookbooks, validate, security-scan)
- [ ] `tests/phase17_chef.rs` has 30+ tests ✅ (33 tests)
- [ ] `docs/chef_support.md` complete

### Phase 18: Puppet Support

- [ ] `src/languages/multimodal/puppet/mod.rs` implements `LanguagePlugin`
- [ ] `src/languages/multimodal/puppet/parser.rs` parses Puppet DSL
- [ ] Parser extracts modules, classes, resources, facts
- [ ] `src/analysis/puppet_modules.rs` builds dependency graph
- [ ] `src/security/puppet.rs` detects 3+ CWE patterns
- [ ] `src/cli/puppet.rs` has 3 subcommands
- [ ] `tests/phase18_puppet.rs` has 30+ tests
- [ ] `docs/puppet_support.md` complete

---

## Review Report Template

Use this template when reporting review findings:

```markdown
# Code Review Report: [Component Name]

**Reviewer**: [AI Agent Name]  
**Date**: [Date]  
**Files Reviewed**: [List of files]  
**Review Type**: [Full | Partial | Security | Performance]

---

## Summary

[Brief overview of review scope and overall findings]

---

## Metrics

- **Lines of Code**: XXX
- **Test Count**: XX tests
- **Test Coverage**: XX%
- **Clippy Warnings**: XX
- **Documentation Coverage**: XX%

---

## Findings

### Critical Issues (Must Fix)

#### Issue 1: [Title]
- **Severity**: Critical
- **File**: `path/to/file.rs:123`
- **Description**: [What's wrong]
- **Impact**: [Why it's critical]
- **Recommendation**: [How to fix]
- **Example**:
```rust
// ❌ Current code
// ...

// ✅ Recommended fix
// ...
```

### High Priority Issues

[Same format as critical]

### Medium Priority Issues

[Same format]

### Low Priority / Suggestions

[Same format]

---

## Positive Findings

### Well-Implemented Patterns

1. **[Pattern Name]**: [Description of what's done well]
2. **[Pattern Name]**: [Description]

---

## Test Coverage Analysis

- **Unit Tests**: XX/YY functions covered
- **Integration Tests**: XX scenarios covered
- **Edge Cases**: [List important edge cases tested]
- **Missing Tests**: [List gaps]

---

## Security Review

- **CWE Patterns Detected**: [List]
- **Remediation Coverage**: XX%
- **False Positives**: [List any found]
- **Missing Patterns**: [List recommended additions]

---

## Performance Notes

- **Bottlenecks Identified**: [List]
- **Memory Usage**: [Observations]
- **Optimization Opportunities**: [List]

---

## Documentation Review

- **API Documentation**: [Complete | Partial | Missing]
- **Examples**: [Working | Needs Update | Missing]
- **User Documentation**: [Complete | Needs Update]

---

## Recommendations

### Immediate Actions (This Sprint)
1. [Action item]
2. [Action item]

### Short-term (Next Sprint)
1. [Action item]
2. [Action item]

### Long-term (Future Consideration)
1. [Action item]
2. [Action item]

---

## Checklist Completion

**Code Quality**:
- [ ] No clippy warnings
- [ ] No unwrap() in production code
- [ ] Proper error handling
- [ ] Consistent naming conventions

**Testing**:
- [ ] 30+ tests minimum
- [ ] All tests pass
- [ ] Edge cases covered
- [ ] Integration tests present

**Documentation**:
- [ ] Public APIs documented
- [ ] Examples provided
- [ ] User documentation complete

**Architecture**:
- [ ] Follows LanguagePlugin pattern
- [ ] Consistent with existing plugins
- [ ] Proper graph integration

**Security**:
- [ ] Security scanner implemented
- [ ] CWE patterns mapped
- [ ] Remediation guidance provided

---

## Overall Assessment

**Grade**: [A+ | A | B | C | D | F]

**Recommendation**: [Approve | Approve with Changes | Needs Rework]

**Rationale**: [Why this grade/recommendation]

---

**Reviewed By**: [Agent Name]  
**Review Duration**: [Time spent]  
**Next Review**: [Date or "After fixes applied"]
```

---

## Integration with Development Workflow

### For AI Agents Performing Reviews

1. **Before Starting Review**:
   ```bash
   git pull origin main
   cargo clean
   cargo build --all-features
   cargo test --all-features
   ```

2. **Run Automated Checks**:
   ```bash
   ./scripts/ai_code_review.sh
   ```

3. **Read Relevant Documentation**:
   - `CODE_REVIEW_GUIDE.md` (standards)
   - `.github/TASK_PLAN.md` (requirements for phase)
   - `docs/<tool>_support.md` (feature documentation)

4. **Perform Manual Review**:
   - Check patterns from this guide
   - Validate against phase requirements
   - Look for phase-specific issues

5. **Generate Report**:
   - Use the template above
   - Include specific file paths and line numbers
   - Provide actionable recommendations

6. **Create Issues** (if needed):
   ```bash
   gh issue create --title "[Review] Component Name - Issue Summary" \
     --body "$(cat review_report.md)" \
     --label "code-review,quality"
   ```

---

## Common Anti-Patterns to Flag

### 1. Error Handling
```rust
// ❌ Panics on error
.unwrap()
.expect("message")
panic!("error")

// ✅ Proper error handling
.map_err(|e| Error::from(e))?
.unwrap_or_default()
return Err(Error::ParseError { ... });
```

### 2. String Allocations
```rust
// ❌ Unnecessary allocation
fn process(s: String) -> String {
    s.to_uppercase()
}

// ✅ Borrow when possible
fn process(s: &str) -> String {
    s.to_uppercase()
}
```

### 3. Cloning in Loops
```rust
// ❌ Clones in hot path
for item in items {
    process(item.clone());
}

// ✅ Borrow
for item in &items {
    process(item);
}
```

### 4. Missing Error Context
```rust
// ❌ Generic error
Err("failed".into())

// ✅ Specific context
Err(Error::ParseError {
    file: path.to_path_buf(),
    line: 42,
    message: format!("Expected '{{', found '{}'", token),
})
```

### 5. Inconsistent Naming
```rust
// ❌ Inconsistent
struct ansibleParser { }  // Should be PascalCase
fn ParseNode() { }        // Should be snake_case

// ✅ Consistent
struct AnsibleParser { }
fn parse_node() { }
```

---

## Resources for AI Agents

- **Rust API Guidelines**: https://rust-lang.github.io/api-guidelines/
- **Clippy Lint List**: https://rust-lang.github.io/rust-clippy/master/
- **Rust Security Advisory DB**: https://rustsec.org/
- **CWE Database**: https://cwe.mitre.org/
- **Project Task Plan**: `.github/TASK_PLAN.md`
- **Code Standards**: `CODE_REVIEW_GUIDE.md`

---

**Version**: 1.0  
**Last Updated**: June 18, 2026  
**Applies to**: rBuilder Phases 16-19+  
**Maintained by**: rBuilder Core Team
