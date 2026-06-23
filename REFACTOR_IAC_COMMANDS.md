# Refactoring Plan: Remove IaC-Specific CLI Commands

## Problem
Currently, Ansible, Chef, and Puppet have separate top-level CLI commands, inconsistent with how other languages (Python, Rust, TypeScript, Go, etc.) are handled through the plugin system.

## Root Cause
- These were added as "Phase 16/17/18" features after the core architecture was established
- They provide domain-specific analysis (dependencies, security) that should be generalized

## What These Commands Currently Do

### `rbuilder ansible`
- **roles**: Analyze role dependencies → should be `analyze --dependencies`
- **validate**: Validate playbooks → generic validation
- **security-scan**: Security scanning → should be `analyze --security`

### `rbuilder chef`
- **cookbooks**: Analyze cookbook dependencies → should be `analyze --dependencies`
- **security-scan**: Security scanning → should be `analyze --security`

### `rbuilder puppet`
- **modules**: Analyze module dependencies → should be `analyze --dependencies`
- **security-scan**: Security scanning → should be `analyze --security`

## Proposed Solution

### Phase 1: Extend the `analyze` command

```rust
/// Run analysis on the graph
Analyze {
    /// Run community detection
    #[arg(long)]
    community: bool,

    /// Calculate complexity metrics
    #[arg(long)]
    complexity: bool,

    /// Compute centrality scores
    #[arg(long)]
    centrality: bool,

    /// Analyze dependencies (IaC roles/cookbooks/modules, package imports, etc.)
    #[arg(long)]
    dependencies: bool,

    /// Run security analysis (secrets, vulnerabilities, misconfigurations)
    #[arg(long)]
    security: bool,

    /// Filter by language (ansible, chef, puppet, python, rust, etc.)
    #[arg(long)]
    language: Option<String>,

    /// Filter by node type (role, cookbook, module, function, class, etc.)
    #[arg(long)]
    node_type: Option<String>,

    /// Output format (text, json, mermaid)
    #[arg(long, default_value = "text")]
    format: String,

    /// Run all analyses
    #[arg(long)]
    all: bool,
}
```

### Phase 2: Make analysis automatic during indexing

Modify the plugin system to automatically:
1. Extract dependencies during `init`/`update` (already happening)
2. Run security scans during `init`/`update` (new)
3. Store findings in the graph as nodes/edges

### Phase 3: Query findings through existing interfaces

```bash
# View Ansible role dependencies
rbuilder gql "MATCH (r:AnsibleRole)-[:DEPENDS_ON]->(dep) RETURN r, dep"

# View security findings
rbuilder ask "show me all security issues in ansible playbooks"

# View stats
rbuilder stats --security-report

# Interactive exploration
rbuilder chat
> "What security issues are in my Ansible roles?"
```

### Phase 4: Deprecate old commands

Add deprecation warnings:
```bash
$ rbuilder ansible roles
Warning: 'rbuilder ansible' is deprecated. Use 'rbuilder analyze --dependencies --language ansible' instead.
```

Then remove in next major version.

## Benefits

1. **Consistency**: All languages treated equally through plugin system
2. **Discoverability**: Users don't need to know IaC-specific commands
3. **Extensibility**: Adding new analysis types doesn't require new top-level commands
4. **Composability**: Can analyze multiple languages at once
5. **Simplicity**: Fewer commands to learn and maintain

## Migration Path

### Current Usage
```bash
# Old way
rbuilder ansible roles --show-deps --format mermaid
rbuilder ansible security-scan --min-severity high
rbuilder chef cookbooks --from-graph
rbuilder puppet modules --show-deps
```

### After Refactoring
```bash
# New way (consistent with other analyses)
rbuilder analyze --dependencies --language ansible --format mermaid
rbuilder analyze --security --language ansible --min-severity high
rbuilder analyze --dependencies --language chef
rbuilder analyze --dependencies --language puppet
```

### Better: Auto-run during indexing
```bash
# Just index the repo once
rbuilder init

# Then query whenever needed
rbuilder gql "MATCH (r:AnsibleRole)-[:DEPENDS_ON]->(dep) RETURN r.name, dep.name"
rbuilder ask "what are the ansible security issues?"
rbuilder stats --security-report
```

## Implementation Steps

1. [ ] Add `--dependencies` and `--security` flags to `analyze` command
2. [ ] Refactor Ansible/Chef/Puppet CLI logic into generic analyzers
3. [ ] Update plugin API to support auto-running security scans
4. [ ] Add deprecation warnings to `ansible`, `chef`, `puppet` commands
5. [ ] Update documentation and examples
6. [ ] Remove deprecated commands in next major version (v2.0)

## Files to Change

- `src/main.rs` - Remove Ansible/Chef/Puppet commands, extend Analyze
- `crates/rbuilder-lang-ansible/src/cli.rs` - Extract to generic analyzer
- `crates/rbuilder-lang-chef/src/cli.rs` - Extract to generic analyzer  
- `crates/rbuilder-lang-puppet/src/cli.rs` - Extract to generic analyzer
- `crates/rbuilder-analysis/src/lib.rs` - Add generic dependency/security analyzers
- Documentation - Update all examples

## Backward Compatibility

Keep the old commands with deprecation warnings for 1-2 minor versions, then remove.
