# CLI Refactoring: Simplified, Intuitive Commands

## Core Philosophy

**`rbuilder .` does everything by default** - like `git .`, `cargo build`, or `npm install`

## New Command Structure

### Primary Command: `rbuilder <path>`

```bash
# Do everything: index, analyze, extract dependencies, run security scans
rbuilder .                    # Analyze current directory (all languages, all analysis)
rbuilder /path/to/repo        # Analyze specific directory
rbuilder . --watch            # Analyze and watch for changes
rbuilder . --exclude "tests/**,*.min.js"
rbuilder . --languages "rust,python,ansible"  # Only specific languages
```

**What it does automatically:**
- ✅ Discover all files (respects .gitignore)
- ✅ Parse all languages (via plugin system)
- ✅ Extract symbols, functions, classes, modules
- ✅ Build dependency graph
- ✅ Run complexity analysis
- ✅ Run security scans (secrets, vulnerabilities, misconfigurations)
- ✅ Detect communities
- ✅ Calculate centrality
- ✅ Save graph to `.rbuilder/`

### Selective Analysis: `rbuilder analyze`

**Use when you want to:**
- Re-run specific analysis without re-indexing
- Skip expensive analyses
- Run analysis on already-indexed graph

```bash
# Re-run only security analysis
rbuilder analyze --security

# Re-run only dependency analysis
rbuilder analyze --dependencies

# Run complexity + centrality (skip security)
rbuilder analyze --complexity --centrality

# Filter by language
rbuilder analyze --security --language ansible

# Run everything (equivalent to full re-index, but faster if graph exists)
rbuilder analyze --all
```

### Update: `rbuilder update`

**Incremental updates** when you already have a graph:

```bash
# Update changed files since last run
rbuilder update

# Force full rebuild
rbuilder update --force

# Update specific files
rbuilder update --files src/main.rs crates/rbuilder-core/src/lib.rs

# Update since git commit
rbuilder update --since abc123
```

### Query Commands (unchanged, but cleaner)

```bash
rbuilder ask "what are the security issues?"
rbuilder gql "MATCH (f:Function)-[:CALLS]->(g) RETURN f, g"
rbuilder stats
rbuilder chat
```

### Visualization Commands

```bash
rbuilder diagram "type:Function" --format mermaid
rbuilder serve          # Web UI
rbuilder export --format json --output graph.json
```

### Specialized Commands

```bash
rbuilder slice --file src/main.rs --line 42 --variable user_id
rbuilder blast-radius auth_verify --depth 10
rbuilder watch          # Alias for: rbuilder . --watch
```

### Configuration & Workspace

```bash
rbuilder config --unused        # Find unused config keys
rbuilder config --secrets       # Find hardcoded secrets
rbuilder config --missing-env   # Find missing env vars

rbuilder workspace add /path/to/repo2 --namespace backend
rbuilder workspace sync
rbuilder workspace list
```

## Removed Commands

### ❌ `init` - Replaced by `rbuilder <path>`
```bash
# OLD
rbuilder init .
rbuilder init --languages rust,python

# NEW
rbuilder .
rbuilder . --languages rust,python
```

### ❌ `ansible`, `chef`, `puppet` - Handled automatically
```bash
# OLD (IaC-specific commands)
rbuilder ansible roles --show-deps
rbuilder ansible security-scan
rbuilder chef cookbooks
rbuilder puppet modules

# NEW (generic, works for all languages)
rbuilder .                                    # Auto-detects and analyzes IaC
rbuilder analyze --dependencies               # All dependencies (IaC + code)
rbuilder analyze --security                   # All security (IaC + code)
rbuilder analyze --dependencies --language ansible
rbuilder gql "MATCH (r:AnsibleRole)-[:DEPENDS_ON]->(dep)"
rbuilder ask "show me ansible role dependencies"
```

### ❌ `detect-changes` - Integrated into update
```bash
# OLD
rbuilder detect-changes src/main.rs

# NEW
rbuilder update --files src/main.rs
rbuilder blast-radius main  # If you want impact analysis
```

### ❌ `init-hooks` - Rename to `hooks install`
```bash
# OLD
rbuilder init-hooks --force

# NEW
rbuilder hooks install --force
rbuilder hooks uninstall
rbuilder hooks list
```

## Full Command Reference (After Refactoring)

```
Usage: rbuilder [OPTIONS] [PATH] [COMMAND]

Arguments:
  [PATH]  Repository path (default: current directory)

Options:
  -v, --verbose              Enable verbose output
  -l, --languages <LANGS>    Include only specific languages (comma-separated)
  -e, --exclude <PATTERNS>   Exclude patterns (comma-separated)
  -w, --watch                Watch for file changes and auto-update
  -h, --help                 Print help
  -V, --version              Print version

Commands:
  # Core Operations
  update          Update graph incrementally
  analyze         Run specific analyses on existing graph
  
  # Querying
  ask             Query using natural language
  gql             Execute graph query language
  chat            Interactive conversational mode
  stats           Show statistics
  
  # Advanced Analysis
  slice           Backward program slice
  blast-radius    Impact analysis for a symbol
  
  # Visualization & Export
  diagram         Generate diagrams
  serve           Start web server
  export          Export graph
  
  # Configuration
  config          Configuration analysis (unused keys, secrets, env vars)
  hooks           Git hooks management (install, uninstall, list)
  
  # Multi-repo
  workspace       Workspace management (add, remove, sync, list)
  
  # MCP
  mcp             MCP server for AI agents
  
  help            Print this message or help for subcommand
```

## Examples

### Getting Started
```bash
# Analyze current directory (does everything)
rbuilder .

# Analyze specific directory
rbuilder ~/projects/my-app

# Analyze with exclusions
rbuilder . --exclude "node_modules/**,dist/**"

# Analyze only Rust and Python
rbuilder . --languages rust,python

# Analyze and watch for changes
rbuilder . --watch
```

### Working with Existing Graph
```bash
# Update after making changes
rbuilder update

# Re-run security analysis only
rbuilder analyze --security

# Query the graph
rbuilder ask "what functions call authenticate?"
rbuilder gql "MATCH (f:Function {name: 'main'}) RETURN f"

# Get stats
rbuilder stats
rbuilder stats --hotspots
```

### IaC Analysis (No Special Commands!)
```bash
# Analyze everything (auto-detects Ansible/Chef/Puppet)
rbuilder .

# View Ansible role dependencies
rbuilder ask "show ansible role dependencies"
rbuilder gql "MATCH (r:AnsibleRole)-[:DEPENDS_ON]->(dep) RETURN r.name, dep.name"

# View security issues
rbuilder analyze --security
rbuilder config --secrets

# Filter by language in queries
rbuilder ask "what security issues are in ansible playbooks?"
```

### Advanced Use Cases
```bash
# Impact analysis
rbuilder blast-radius authenticate --depth 5

# Program slicing
rbuilder slice --file src/auth.rs --line 42 --variable user_token

# Generate diagrams
rbuilder diagram "type:Function" --format mermaid --output arch.md
rbuilder diagram "MATCH (f:Function)-[:CALLS]->(g)" --format png

# Export for external tools
rbuilder export --format json --output graph.json
rbuilder export --format cypher --output import.cypher

# Configuration analysis
rbuilder config --unused           # Find unused config keys
rbuilder config --secrets          # Find hardcoded secrets
rbuilder config --missing-env      # Find missing env vars
rbuilder config --drift config/*.yaml  # Compare configs
```

### Multi-Repository Workspaces
```bash
# Add repos to workspace
rbuilder workspace add ../backend --namespace backend
rbuilder workspace add ../frontend --namespace frontend

# Sync and link across repos
rbuilder workspace sync

# Analyze workspace
rbuilder .

# Query across repos
rbuilder ask "which frontend components call backend APIs?"
```

## Implementation Changes

### File: `src/main.rs`

```rust
#[derive(Parser)]
#[command(name = "rbuilder")]
#[command(about = "AI-powered code knowledge graph", long_about = None)]
struct Cli {
    /// Repository path (default: current directory)
    #[arg(default_value = ".")]
    path: Option<String>,

    #[command(subcommand)]
    command: Option<Commands>,

    /// Enable verbose output
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Include only specific languages (comma-separated)
    #[arg(short, long, global = true)]
    languages: Option<String>,

    /// Exclude patterns (comma-separated)
    #[arg(short, long, global = true)]
    exclude: Option<String>,

    /// Watch for file changes and auto-update
    #[arg(short, long)]
    watch: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Update graph incrementally
    Update {
        #[arg(long)]
        since: Option<String>,
        #[arg(long)]
        force: bool,
        #[arg(long)]
        files: Vec<String>,
    },

    /// Run specific analyses (use when you want to skip some analyses)
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
        
        /// Analyze dependencies (works for all languages)
        #[arg(long)]
        dependencies: bool,
        
        /// Run security analysis (secrets, vulnerabilities, misconfigurations)
        #[arg(long)]
        security: bool,
        
        /// Filter by language (ansible, chef, puppet, python, rust, etc.)
        #[arg(long)]
        language: Option<String>,
        
        /// Output format
        #[arg(long, default_value = "text")]
        format: String,
        
        /// Run all analyses
        #[arg(long)]
        all: bool,
    },

    // ... rest of commands (ask, gql, chat, stats, etc.)
    
    /// Git hooks management
    Hooks {
        #[command(subcommand)]
        command: HooksCommands,
    },
}

#[derive(Subcommand)]
enum HooksCommands {
    /// Install git hooks
    Install {
        #[arg(long)]
        force: bool,
    },
    /// Uninstall git hooks
    Uninstall,
    /// List installed hooks
    List,
}

fn main() {
    let cli = Cli::parse();
    
    // If no subcommand provided, do full analysis
    if cli.command.is_none() {
        // rbuilder . or rbuilder /path/to/repo
        run_full_analysis(&cli.path.unwrap_or(".".to_string()), &cli);
        return;
    }
    
    // Handle subcommands
    match cli.command.unwrap() {
        Commands::Update { .. } => { /* ... */ }
        Commands::Analyze { .. } => { /* ... */ }
        // ...
    }
}

fn run_full_analysis(path: &str, cli: &Cli) {
    // 1. Discover files
    // 2. Index all languages
    // 3. Run ALL analyses:
    //    - Extract symbols, dependencies
    //    - Calculate complexity
    //    - Run security scans
    //    - Detect communities
    //    - Calculate centrality
    // 4. Save to .rbuilder/
    // 5. If --watch, enter watch mode
}
```

## Migration Guide

### For Users

**Before:**
```bash
# Multiple steps
rbuilder init .
rbuilder ansible security-scan
rbuilder analyze --community
```

**After:**
```bash
# One command does everything
rbuilder .

# Then query as needed
rbuilder ask "show security issues"
```

### For CI/CD

**Before:**
```bash
rbuilder init .
rbuilder ansible security-scan --min-severity high --format json > findings.json
rbuilder export --format json --output graph.json
```

**After:**
```bash
rbuilder .  # Does everything
rbuilder analyze --security --format json > findings.json
rbuilder export --format json --output graph.json
```

## Benefits

1. **Simplicity**: One command to do everything (`rbuilder .`)
2. **Consistency**: All languages/tools treated equally
3. **Discoverability**: No need to know IaC-specific commands
4. **Intuitive**: Similar to other CLI tools (`git .`, `cargo build`)
5. **Performance**: `analyze` allows selective re-analysis without re-indexing
6. **Fewer commands**: ~15 commands → ~10 commands

## Backward Compatibility

Add deprecation warnings for removed commands:
```bash
$ rbuilder init .
Warning: 'rbuilder init' is deprecated. Use 'rbuilder .' instead.
[Continues with init...]

$ rbuilder ansible roles
Error: 'rbuilder ansible' has been removed. Use 'rbuilder .' to analyze, then:
  - View dependencies: rbuilder ask "show ansible role dependencies"
  - Security scan: rbuilder analyze --security --language ansible
```
