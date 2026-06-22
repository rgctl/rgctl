# Action Plan for Cursor: rBuilder Code Quality Fixes

**Created**: June 18, 2026  
**Based on**: CODE_REVIEW_REPORT.md  
**Target**: Phase 19 Code Quality Improvements  
**Estimated Total Effort**: 8-10 hours

---

## Quick Start for Cursor

Execute these tasks in order. Each task is independent and can be completed in a single session.

---

## Task 1: Fix Build Warnings (Priority: P1)

**Effort**: 30 minutes  
**Files**: Various  
**Command First**: `cargo fix --lib -p rbuilder`

### Step 1.1: Run Automated Fix
```bash
cargo fix --lib -p rbuilder --allow-dirty
```

### Step 1.2: Manual Cleanup

**Remove unused imports** (if cargo fix doesn't catch them):
```bash
# Find files with unused GraphBackend import
grep -r "use crate::graph::backend::GraphBackend" src/ --include="*.rs"
```

Expected locations (check both):
- File containing unused `GraphBackend` import
- Action: Remove the import line or add `#[allow(unused_imports)]` if needed for trait

**Fix unused variables**:
```rust
// Find variables named 'question' or 'backend'
// Change:
let question = ...;
let backend = ...;

// To:
let _question = ...;  // Prefix with _ to indicate intentionally unused
let _backend = ...;
```

### Step 1.3: Verify
```bash
cargo build --all-features 2>&1 | grep "warning:"
# Should output nothing or minimal warnings
```

**Deliverables**:
- [ ] `cargo fix` run successfully
- [ ] No unused import warnings
- [ ] No unused variable warnings
- [ ] Build completes with 0 warnings

---

## Task 2: Add Struct Field Documentation (Priority: P1)

**Effort**: 1 hour  
**Impact**: Fixes 16 documentation warnings

### Step 2.1: Identify Missing Docs
```bash
cargo doc --no-deps --all-features 2>&1 | grep "missing documentation for a struct field" -A 3
```

### Step 2.2: Add Documentation

For each struct field missing docs, add a /// comment:

**Pattern to Follow**:
```rust
// ❌ Before
pub struct RoleNode {
    pub name: String,
    pub path: String,
    pub dependencies: Vec<String>,
    pub dependents: Vec<String>,
}

// ✅ After
pub struct RoleNode {
    /// Role name (e.g., "nginx", "postgres", "common")
    pub name: String,
    /// Filesystem path to the role directory
    pub path: String,
    /// List of role names this role depends on (from meta/main.yml)
    pub dependencies: Vec<String>,
    /// List of role names that depend on this role (reverse dependencies)
    pub dependents: Vec<String>,
}
```

### Step 2.3: Common Structs to Document

**File**: `src/analysis/ansible_roles.rs`
```rust
pub struct RoleNode {
    /// Role name identifier
    pub name: String,
    /// Filesystem path to role directory  
    pub path: String,
    /// Roles this depends on
    pub dependencies: Vec<String>,
    /// Roles depending on this
    pub dependents: Vec<String>,
}
```

**File**: `src/analysis/chef_cookbooks.rs`
```rust
pub struct CookbookNode {
    /// Cookbook name
    pub name: String,
    /// Cookbook version
    pub version: String,
    /// Filesystem path
    pub path: String,
    /// Cookbook dependencies
    pub dependencies: Vec<String>,
}
```

**File**: `src/analysis/puppet_modules.rs`
```rust
pub struct ModuleNode {
    /// Module name
    pub name: String,
    /// Module version
    pub version: String,
    /// Filesystem path
    pub path: String,
    /// Module dependencies
    pub dependencies: Vec<String>,
}
```

### Step 2.4: Security Finding Structs

**Files**: `src/security/ansible.rs`, `src/security/chef.rs`, `src/security/puppet.rs`

```rust
pub struct AnsibleSecurityFinding {
    /// Severity level of the finding
    pub severity: AnsibleSeverity,
    /// Human-readable description of the issue
    pub message: String,
    /// Task or playbook name where issue was found
    pub location: String,
    /// CWE identifier (e.g., "CWE-78" for command injection)
    pub cwe: Option<String>,
    /// Recommended fix for the issue
    pub remediation: Option<String>,
    /// Ansible module involved (e.g., "shell", "command")
    pub module: Option<String>,
}
```

Apply same pattern to `ChefSecurityFinding` and `PuppetSecurityFinding`.

### Step 2.5: Verify
```bash
cargo doc --no-deps --all-features 2>&1 | grep "missing documentation"
# Should output nothing
```

**Deliverables**:
- [ ] All public struct fields documented
- [ ] Documentation builds with 0 warnings
- [ ] Consistent doc style across all structs

---

## Task 3: Fix RwLock unwrap() in Memory Backend (Priority: P0)

**Effort**: 2-3 hours  
**File**: `src/graph/backend/memory.rs`  
**Count**: 98 instances  
**Risk**: Critical - lock poisoning vulnerability

### Step 3.1: Understand the Pattern

Current code panics if lock is poisoned:
```rust
// ❌ Current: Panic on poison
let nodes = self.nodes.read().unwrap();
let mut store = self.nodes.write().unwrap();
```

### Step 3.2: Choose Strategy

**Option A: Error Propagation** (Recommended)
```rust
// ✅ Propagate poison as error
let nodes = self.nodes.read()
    .map_err(|e| Error::GraphError(format!("Node lock poisoned: {}", e)))?;
```

**Option B: Expect with Context** (Simpler but still panics)
```rust
// ✅ Panic with context
let nodes = self.nodes.read()
    .expect("Node lock poisoned - graph backend in inconsistent state");
```

**Recommendation**: Use Option A for public methods, Option B for internal helpers.

### Step 3.3: Implementation Plan

**Public Methods** (use Option A):
- `all_nodes()` → return `Result<Vec<Node>, Error>`
- `all_edges()` → return `Result<Vec<Edge>, Error>`
- `find_nodes_by_type()` → already returns Result, good
- `find_nodes_by_name()` → return `Result<Vec<Node>, Error>`
- `add_node()` → return `Result<(), Error>`
- `add_edge()` → return `Result<(), Error>`

**Internal Methods** (use Option B):
- Helper methods called only from other methods
- Can use `.expect()` with clear message

### Step 3.4: Example Refactoring

**Before** (`src/graph/backend/memory.rs`):
```rust
pub fn all_nodes(&self) -> Vec<Node> {
    let nodes = self.nodes.read().unwrap();  // ❌ Line 48
    nodes.values().cloned().collect()
}

pub fn all_edges(&self) -> Vec<Edge> {
    let edges = self.edges.read().unwrap();  // ❌ Line 54
    edges.values().cloned().collect()
}
```

**After**:
```rust
pub fn all_nodes(&self) -> Result<Vec<Node>> {
    let nodes = self.nodes.read()
        .map_err(|e| Error::GraphError(format!("Node lock poisoned: {}", e)))?;
    Ok(nodes.values().cloned().collect())
}

pub fn all_edges(&self) -> Result<Vec<Edge>> {
    let edges = self.edges.read()
        .map_err(|e| Error::GraphError(format!("Edge lock poisoned: {}", e)))?;
    Ok(edges.values().cloned().collect())
}
```

### Step 3.5: Update Trait Definition

**File**: `src/graph/backend.rs` (or wherever `GraphBackend` trait is defined)

```rust
pub trait GraphBackend {
    fn all_nodes(&self) -> Result<Vec<Node>>;  // Changed from Vec<Node>
    fn all_edges(&self) -> Result<Vec<Edge>>;  // Changed from Vec<Edge>
    // ... other methods
}
```

### Step 3.6: Update All Callers

After changing return types, fix all call sites:

```bash
# Find all callers
grep -r "\.all_nodes()" src/ --include="*.rs"
grep -r "\.all_edges()" src/ --include="*.rs"
```

**Before**:
```rust
let nodes = backend.all_nodes();
```

**After**:
```rust
let nodes = backend.all_nodes()?;
// or
let nodes = backend.all_nodes().unwrap_or_default();
```

### Step 3.7: Systematic Replacement

**Read locks** (lines 48, 54, 60, 62, 71, 73, 82, 84, 93, 96, 105, 106, etc.):
```rust
// Pattern to find
.read().unwrap()

// Replace with
.read().map_err(|e| Error::GraphError(format!("Lock poisoned: {}", e)))?
```

**Write locks** (lines 134, 149, 150, etc.):
```rust
// Pattern to find
.write().unwrap()

// Replace with
.write().map_err(|e| Error::GraphError(format!("Lock poisoned: {}", e)))?
```

### Step 3.8: Test After Changes
```bash
cargo test --lib
cargo test --test phase16_ansible
cargo test --test phase17_chef
cargo test --test phase18_puppet
```

**Deliverables**:
- [ ] All RwLock unwrap() replaced with proper error handling
- [ ] GraphBackend trait updated
- [ ] All callers updated
- [ ] All tests passing
- [ ] No unwrap() on locks in `src/graph/backend/memory.rs`

---

## Task 4: Document Safe unwrap() in Parsers (Priority: P1)

**Effort**: 1 hour  
**Files**: 
- `src/languages/multimodal/chef/parser.rs`
- `src/languages/multimodal/puppet/parser.rs`

### Step 4.1: Pattern to Fix

These unwraps are **safe** because regex patterns guarantee groups exist, but should be documented:

**Before**:
```rust
let dep = cap.get(1).unwrap().as_str().to_string();
let resource_type = cap.get(1).unwrap().as_str();
let resource_name = cap.get(2).unwrap().as_str();
```

**After**:
```rust
// Safe: Regex pattern guarantees group 1 exists
let dep = cap.get(1).expect("regex group 1").as_str().to_string();
let resource_type = cap.get(1).expect("regex group 1").as_str();
let resource_name = cap.get(2).expect("regex group 2").as_str();
```

### Step 4.2: Find All Instances

**Chef Parser** (`src/languages/multimodal/chef/parser.rs`):
```bash
grep -n "cap.get.*unwrap()" src/languages/multimodal/chef/parser.rs
```

Expected around lines: 187, 380, 528, 529, etc.

**Puppet Parser** (`src/languages/multimodal/puppet/parser.rs`):
```bash
grep -n "cap.get.*unwrap()" src/languages/multimodal/puppet/parser.rs
```

Expected around lines: 261, 264, 266, 282, 284, 286, 406, 441, 442, 592, etc.

### Step 4.3: Replacement Pattern

Use find-and-replace in each file:

**Find**:
```regex
cap\.get\((\d+)\)\.unwrap\(\)
```

**Replace**:
```rust
cap.get($1).expect("regex group $1")
```

### Step 4.4: Verify Pattern Safety

For each regex pattern, verify the expect() is justified:

**Example** (`chef/parser.rs` line ~183):
```rust
// Regex: r"depends\s+['\"]([^'\"]+)['\"]"
// Pattern has group (1) for the dependency name
let dep = cap.get(1).expect("regex group 1").as_str().to_string();  // ✅ Safe
```

**Example** (`puppet/parser.rs` line ~441):
```rust
// Regex: r"(?m)^(\w+)\s+{\s*['\"]([^'\"]+)['\"]"
// Pattern has groups (1) for resource type, (2) for title
let resource_type = cap.get(1).expect("regex group 1").as_str();  // ✅ Safe
let title = cap.get(2).expect("regex group 2").as_str();          // ✅ Safe
```

### Step 4.5: Test Edge Cases

Add test for malformed input (should not match, not crash):

```rust
#[test]
fn test_malformed_input_doesnt_panic() {
    let parser = ChefParser::new();
    let malformed = "not valid chef code }{}{";
    let (symbols, _) = parser.parse("test.rb", malformed);
    // Should return empty, not panic
    assert!(symbols.is_empty());
}
```

**Deliverables**:
- [ ] All regex unwrap() replaced with expect()
- [ ] Comments added explaining why safe
- [ ] Tests verify no panics on malformed input
- [ ] Grep shows no remaining `cap.get().unwrap()`

---

## Task 5: Refactor Analysis Module unwrap() (Priority: P2)

**Effort**: 2-3 hours  
**Files**: 
- `src/analysis/ansible_roles.rs`
- `src/analysis/chef_cookbooks.rs`
- `src/analysis/puppet_modules.rs`

### Step 5.1: Pattern to Improve

Current code uses unwrap() after insert (safe but unclear):

**Before** (`ansible_roles.rs` lines ~88-95):
```rust
graph.roles.entry(from.name.clone()).or_insert_with(|| RoleNode { ... });
graph.roles.entry(to.name.clone()).or_insert_with(|| RoleNode { ... });

let from_entry = graph.roles.get_mut(&from.name).unwrap();  // ❌
if !from_entry.dependencies.contains(&to.name) {
    from_entry.dependencies.push(to.name.clone());
}

let to_entry = graph.roles.get_mut(&to.name).unwrap();  // ❌
if !to_entry.dependents.contains(&from.name) {
    to_entry.dependents.push(from.name.clone());
}
```

**After**:
```rust
// Use entry API to avoid second lookup
{
    let from_entry = graph.roles.entry(from.name.clone())
        .or_insert_with(|| RoleNode {
            name: from.name.clone(),
            path: from.file_path.clone().unwrap_or_default(),
            dependencies: vec![],
            dependents: vec![],
        });
    
    if !from_entry.dependencies.contains(&to.name) {
        from_entry.dependencies.push(to.name.clone());
    }
}

{
    let to_entry = graph.roles.entry(to.name.clone())
        .or_insert_with(|| RoleNode {
            name: to.name.clone(),
            path: to.file_path.clone().unwrap_or_default(),
            dependencies: vec![],
            dependents: vec![],
        });
    
    if !to_entry.dependents.contains(&from.name) {
        to_entry.dependents.push(from.name.clone());
    }
}
```

### Step 5.2: Apply to All Three Analysis Modules

Same pattern appears in:
- `src/analysis/ansible_roles.rs` (~line 88, 92)
- `src/analysis/chef_cookbooks.rs` (similar locations)
- `src/analysis/puppet_modules.rs` (similar locations)

### Step 5.3: Alternative: Use expect() with Context

If entry API becomes too verbose:

```rust
let from_entry = graph.roles.get_mut(&from.name)
    .expect("role was just inserted");
```

**Deliverables**:
- [ ] All analysis modules refactored
- [ ] No unwrap() after HashMap operations
- [ ] Code is clearer about intent
- [ ] Tests still passing

---

## Task 6: Investigate Test Failure (Priority: P0)

**Effort**: 1 hour  
**Test**: `phase3_integration::test_plugin_registry_install`  
**Error**: `dlopen` failure on macOS

### Step 6.1: Reproduce the Issue
```bash
cargo test --test phase3_integration test_plugin_registry_install -- --nocapture
```

### Step 6.2: Understand the Error

The error message:
```
PluginError("dlopen(...libcustom.so, 0x0005): ... slice is not valid mach-o file")
```

This suggests:
- Test is trying to load a dynamic library
- Library file is not valid for macOS architecture
- Might be cross-platform test issue

### Step 6.3: Possible Solutions

**Option A: Platform-Specific Test**
```rust
#[test]
#[cfg(target_os = "linux")]  // Only run on Linux
fn test_plugin_registry_install() {
    // ...
}
```

**Option B: Skip if Library Loading Fails**
```rust
#[test]
fn test_plugin_registry_install() {
    let result = load_plugin();
    if result.is_err() {
        // Skip test if we can't load plugins on this platform
        eprintln!("Skipping plugin load test (platform limitation)");
        return;
    }
    // ... rest of test
}
```

**Option C: Fix the Test Fixture**
```rust
// Ensure the test builds a valid macOS dylib, not just .so
// Check test setup in tests/phase3_integration.rs
```

### Step 6.4: Review Test Code

Read `tests/phase3_integration.rs` around the `test_plugin_registry_install` function.

Look for:
- How the plugin library is created
- Whether it's platform-specific
- Whether it's a real test or proof-of-concept

### Step 6.5: Recommended Action

Since this is a Phase 3 test (early in the project) and not IaC-related:

```rust
#[test]
#[ignore]  // Temporarily ignore until proper fix
fn test_plugin_registry_install() {
    // TODO: Fix dlopen issue on macOS (see CURSOR_ACTION_PLAN.md Task 6)
    // ...
}
```

**Deliverables**:
- [ ] Test failure investigated
- [ ] Decision made: fix, skip, or mark as platform-specific
- [ ] All tests pass or clearly marked as ignored
- [ ] Issue documented if not fixable now

---

## Task 7: Add Module-Level Examples (Priority: P2)

**Effort**: 2-3 hours  
**Files**: Security scanners, analysis modules

### Step 7.1: Security Scanner Examples

Add to top of each security scanner module:

**File**: `src/security/ansible.rs`
```rust
//! Ansible security scanning against graph task nodes.
//!
//! This module provides security vulnerability detection for Ansible playbooks
//! and roles indexed in the rBuilder knowledge graph.
//!
//! # Example
//!
//! ```no_run
//! use rbuilder::security::ansible::AnsibleSecurityScanner;
//! use rbuilder::graph::CodeGraph;
//!
//! # fn main() -> rbuilder::error::Result<()> {
//! // Load indexed graph
//! let graph = CodeGraph::load_from_repo(".")?;
//!
//! // Scan for security issues
//! let scanner = AnsibleSecurityScanner::new();
//! let findings = scanner.scan_graph(graph.backend());
//!
//! // Filter by severity
//! let critical = AnsibleSecurityScanner::filter_by_severity(
//!     findings,
//!     rbuilder::security::ansible::AnsibleSeverity::High
//! );
//!
//! for finding in critical {
//!     println!("[{:?}] {}", finding.severity, finding.message);
//!     if let Some(cwe) = finding.cwe {
//!         println!("  CWE: {}", cwe);
//!     }
//!     if let Some(fix) = finding.remediation {
//!         println!("  Fix: {}", fix);
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Security Checks
//!
//! - **CWE-78**: Command injection in shell/command/raw modules
//! - **CWE-798**: Hardcoded secrets in task variables
//! - **CWE-732**: Insecure file permissions
//! - **CWE-250**: Unnecessary privilege escalation
//! - **CWE-532**: Sensitive data logging

use crate::graph::backend::MemoryBackend;
// ... rest of file
```

Apply same pattern to:
- `src/security/chef.rs`
- `src/security/puppet.rs`

### Step 7.2: Analysis Module Examples

**File**: `src/analysis/ansible_roles.rs`
```rust
//! Ansible role dependency analysis from the knowledge graph.
//!
//! # Example
//!
//! ```no_run
//! use rbuilder::analysis::ansible_roles::RoleDependencyGraph;
//! use rbuilder::graph::CodeGraph;
//!
//! # fn main() -> rbuilder::error::Result<()> {
//! let graph = CodeGraph::load_from_repo(".")?;
//! let role_graph = RoleDependencyGraph::from_graph(graph.backend())?;
//!
//! // Get dependency order
//! let sorted = role_graph.topological_sort()?;
//! println!("Role execution order: {:?}", sorted);
//!
//! // Find circular dependencies
//! let cycles = role_graph.detect_cycles();
//! if !cycles.is_empty() {
//!     eprintln!("Warning: Circular dependencies found: {:?}", cycles);
//! }
//! # Ok(())
//! # }
//! ```

use crate::error::{Error, Result};
// ... rest of file
```

Apply to:
- `src/analysis/chef_cookbooks.rs`
- `src/analysis/puppet_modules.rs`

### Step 7.3: Verify Examples Compile
```bash
cargo test --doc
```

This runs all documentation examples as tests.

**Deliverables**:
- [ ] All security scanners have module-level examples
- [ ] All analysis modules have examples
- [ ] Examples compile and pass `cargo test --doc`
- [ ] Examples show realistic usage

---

## Task 8: CI/CD Enhancement (Priority: P3)

**Effort**: 2-3 hours  
**File**: `.github/workflows/ci.yml` (or create new)

### Step 8.1: Add Quality Checks to CI

Create or update `.github/workflows/quality.yml`:

```yaml
name: Code Quality

on:
  push:
    branches: [ main, develop ]
  pull_request:
    branches: [ main ]

jobs:
  quality:
    runs-on: ubuntu-latest
    
    steps:
    - uses: actions/checkout@v3
    
    - name: Setup Rust
      uses: actions-rs/toolchain@v1
      with:
        toolchain: stable
        components: rustfmt, clippy
        override: true
    
    - name: Format Check
      run: cargo fmt -- --check
    
    - name: Clippy Check
      run: cargo clippy --all-targets --all-features -- -D warnings
    
    - name: Documentation Check
      run: cargo doc --no-deps --all-features
    
    - name: Test Suite
      run: cargo test --all-features
    
    - name: Security Audit
      run: |
        cargo install cargo-audit
        cargo audit
    
    - name: Automated Code Review
      run: ./scripts/ai_code_review.sh
```

### Step 8.2: Add Coverage Check (Optional)

```yaml
    - name: Test Coverage
      run: |
        cargo install cargo-tarpaulin
        cargo tarpaulin --out Lcov --all-features
    
    - name: Upload Coverage
      uses: codecov/codecov-action@v3
      with:
        files: ./lcov.info
```

### Step 8.3: Add Badge to README

Update `README.md`:

```markdown
[![CI](https://github.com/user/rbuilder/workflows/CI/badge.svg)](https://github.com/user/rbuilder/actions)
[![Code Quality](https://github.com/user/rbuilder/workflows/Code%20Quality/badge.svg)](https://github.com/user/rbuilder/actions)
```

**Deliverables**:
- [ ] CI workflow file created/updated
- [ ] All checks passing in CI
- [ ] Badges added to README
- [ ] CI runs on every PR

---

## Verification Checklist

After completing all tasks, verify:

```bash
# 1. No build warnings
cargo build --all-features 2>&1 | grep "warning:" | wc -l
# Expected: 0

# 2. No doc warnings
cargo doc --no-deps --all-features 2>&1 | grep "warning:" | wc -l
# Expected: 0

# 3. All tests pass
cargo test --all-features
# Expected: All pass (except 1 ignored if Task 6 → ignore)

# 4. Clippy clean
cargo clippy --all-targets --all-features -- -D warnings
# Expected: No warnings

# 5. Format check
cargo fmt -- --check
# Expected: No output

# 6. Automated review
./scripts/ai_code_review.sh
# Expected: ✅ successes, minimal warnings

# 7. Doc examples compile
cargo test --doc
# Expected: All pass

# 8. Count unwrap in critical paths
grep -r "\.unwrap()" src/graph/backend/memory.rs | grep -v test | wc -l
# Expected: 0
```

---

## Summary of Changes

| Task | Files Changed | Lines Changed | Priority | Status |
|------|---------------|---------------|----------|--------|
| **Task 1** | Various | ~10 | P1 | ⬜ |
| **Task 2** | 6-8 files | ~50 | P1 | ⬜ |
| **Task 3** | 2 files | ~200 | P0 | ⬜ |
| **Task 4** | 2 files | ~30 | P1 | ⬜ |
| **Task 5** | 3 files | ~60 | P2 | ⬜ |
| **Task 6** | 1 file | ~5 | P0 | ⬜ |
| **Task 7** | 6 files | ~150 | P2 | ⬜ |
| **Task 8** | 2 files | ~80 | P3 | ⬜ |

**Total**: ~585 lines changed across ~20 files

---

## Execution Order

**Session 1** (2-3 hours): Critical fixes
1. Task 1: Fix build warnings (30 min)
2. Task 6: Investigate test failure (1 hour)
3. Task 3: Start RwLock refactoring (1 hour of 2-3 hour task)

**Session 2** (2-3 hours): Complete critical + high priority
4. Task 3: Complete RwLock refactoring (remaining time)
5. Task 2: Add struct documentation (1 hour)
6. Task 4: Document safe unwrap() (1 hour)

**Session 3** (2-3 hours): Medium priority
7. Task 5: Refactor analysis unwrap() (2-3 hours)

**Session 4** (2-3 hours): Low priority + polish
8. Task 7: Add module examples (2 hours)
9. Task 8: CI/CD setup (1 hour)
10. Final verification

---

## Success Criteria

When all tasks complete:
- ✅ Zero build warnings
- ✅ Zero doc warnings
- ✅ Zero clippy warnings
- ✅ All tests passing (or 1 properly ignored)
- ✅ No unwrap() on RwLock operations
- ✅ All regex unwrap() documented
- ✅ All public structs documented
- ✅ Module examples present and tested
- ✅ CI/CD enforcing quality gates
- ✅ Code review script shows ✅ successes

---

## Questions or Issues?

If you encounter problems:

1. **Test failures**: Check if related to your changes or pre-existing
2. **Type errors**: May need to update callers after signature changes (Task 3)
3. **Unclear patterns**: Refer to CODE_REVIEW_GUIDE.md for examples
4. **Stuck**: Mark task as WIP and document blocker in commit message

---

**Action Plan Created**: June 18, 2026  
**For**: Cursor AI Code Editor  
**By**: Claude Code  
**Target Completion**: 2-3 days (8-10 hours total)
