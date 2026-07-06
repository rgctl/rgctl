---
name: rbuilder
description: Deep codebase understanding and migration planning using rBuilder's knowledge graph analysis. Use this skill when the user asks about code architecture, dependencies, change impact ("what breaks if I change X?", "blast radius"), migration planning, refactoring large codebases, call graphs, program slicing, data flow analysis, or explicitly mentions rbuilder. IMPORTANT - Trigger this skill proactively when the user mentions modernization, application migration, understanding legacy code structure, or planning large-scale code changes, as rBuilder excels at these tasks.
---

# rBuilder Skill

Use rBuilder CLI to analyze codebases through knowledge graph queries, enabling deep architectural understanding, impact analysis, and migration planning.

## What rBuilder Does

rBuilder transforms code repositories into queryable knowledge graphs, providing:
- **Impact Analysis**: "What breaks if I change this function?" with precise blast radius calculation
- **Architecture Understanding**: Call graphs, dependencies, communities, complexity metrics
- **Migration Planning**: Identify high-risk changes, architectural hotspots, circular dependencies
- **Program Analysis**: CFG, PDG, data flow, taint analysis, program slicing
- **Multi-language Support**: 35+ languages including Java, Rust, Python, TypeScript, JavaScript, Go, Kotlin, C#, and more

## When to Use This Skill

Trigger this skill when the user:
- Asks about code architecture ("show me dependencies", "what calls this function?")
- Needs change impact analysis ("what breaks if I change X?", "blast radius of Y")
- Mentions migration, modernization, or refactoring ("migrate this app", "plan refactoring")
- Wants to understand legacy codebases or complex systems
- Asks about call graphs, program slicing, data flow, or taint analysis
- Explicitly mentions "rbuilder" or wants knowledge graph analysis

## Installation Check

Before running any rBuilder commands, check if it's installed:

```bash
rbuilder --version
```

If not found, guide the user to install it:

**Installation from GitHub Releases (recommended):**
1. Visit https://github.com/sshaaf/rBuilder/releases
2. Download the latest release for your platform:
   - macOS (Apple Silicon): `rbuilder-*-aarch64-apple-darwin.tar.gz`
   - macOS (Intel): `rbuilder-*-x86_64-apple-darwin.tar.gz`
   - Linux: `rbuilder-*-x86_64-unknown-linux-gnu.tar.gz`
   - Windows: `rbuilder-*-x86_64-pc-windows-msvc.zip`
3. Extract and add to PATH:
   ```bash
   mkdir -p ~/.local/bin
   cp rbuilder ~/.local/bin/
   export PATH="$HOME/.local/bin:$PATH"
   ```

**Build from source (requires Rust 1.70+):**
```bash
git clone https://github.com/sshaaf/rBuilder.git
cd rBuilder
cargo build --release
./target/release/rbuilder --version
```

## Repository Indexing

Before querying, the repository must be indexed. Check if `.rbuilder/` directory exists in the project root.

If not indexed, ask the user about their preference:

### Indexing Options

Present these options:

1. **Fast Mode (recommended for most cases)**
   - Command: `rbuilder discover .`
   - Includes: Standard graph analysis, complexity, communities, centrality
   - Speed: ~15 seconds for 150 files, ~5 minutes for 8,000 files
   - Good for: Architecture queries, impact analysis, metrics

2. **Deep Mode (for advanced program analysis)**
   - Command: `rbuilder discover . --cfg`
   - Includes: Everything from fast mode + CFG/PDG/taint/dominance analysis
   - Speed: Slower (minutes to hours for large codebases)
   - Required for: Program slicing, taint analysis, detailed control flow
   - When to suggest: User asks about data flow, slicing, or taint tracking

3. **Full Mode (deep + security scanning)**
   - Command: `rbuilder discover . --all`
   - Includes: Deep mode + secret scanning on config files
   - Use when: Security analysis is needed

**Ask the user:** "I need to index this repository first. Would you like:
- **Fast mode** (quick, good for architecture/impact analysis) 
- **Deep mode** (slower, includes program slicing and data flow)

For reference, a 150-file codebase takes ~15 seconds in fast mode, while an 8,000-file codebase takes ~5 minutes."

### Running Discovery

After the user chooses:

```bash
cd /path/to/repository

# Fast mode
rbuilder discover .

# Deep mode
rbuilder discover . --cfg

# Full mode (deep + security)
rbuilder discover . --all

# With language filters
rbuilder discover . -l java,typescript -e target,node_modules

# Verbose output
rbuilder discover . -v
```

## Core Commands

### 1. Impact Analysis (Blast Radius)

The most powerful feature for migration planning - answers "what breaks if I change X?"

```bash
# Basic blast radius
rbuilder blast-radius FunctionName

# With depth limit (faster)
rbuilder blast-radius FunctionName --depth 3

# JSON output for scripting
rbuilder -f json blast-radius FunctionName

# Disambiguate if needed
rbuilder blast-radius methodName --class ClassName
rbuilder blast-radius methodName --file path/to/File.java
```

**When to use:** User asks about change impact, refactoring safety, migration risk assessment.

**Output includes:**
- Impact score (risk level)
- Direct callers
- Transitive impact zone
- Community boundaries crossed
- Recommendations for safe rollout

### 2. Architecture Queries (GQL)

Query the knowledge graph with graph query language:

```bash
# List all functions
rbuilder gql 'MATCH (n:Function) RETURN n'

# Find specific patterns
rbuilder gql "MATCH (n:Function) WHERE n.name LIKE '*Service' RETURN n"

# Call relationships
rbuilder gql 'MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a,b LIMIT 20'

# Multi-hop dependencies
rbuilder gql 'MATCH (a:Function)-[:CALLS*1..3]->(b:Function) RETURN a,b'

# JSON output
rbuilder -f json gql 'MATCH (n:Function) RETURN n' | jq '.rows'
```

**Common node types:** Function, Class, Interface, Module, File, Import, ConfigKey

**Common edge types:** CALLS, IMPORTS, CONTAINS, DEPENDS_ON, IMPLEMENTS

### 3. Program Slicing

Line-level backward/forward slicing - requires deep mode indexing.

```bash
# Backward slice (what influences this variable?)
rbuilder slice path/to/File.java \
  --line 45 \
  --variable varName \
  --function ClassName

# Forward slice (what does this variable affect?)
rbuilder slice path/to/File.java \
  --line 45 \
  --variable varName \
  --function ClassName \
  --direction forward

# Taint analysis
rbuilder slice path/to/File.java \
  --line 45 \
  --variable userInput \
  --function ClassName \
  --taint
```

**When to use:** User needs to understand data flow, trace variable dependencies, or check for security vulnerabilities.

### 4. Control Flow Analysis

Inspect CFG, PDG, dominance - requires deep mode indexing.

```bash
# Control flow graph
rbuilder inspect FunctionName cfg

# As Mermaid diagram
rbuilder -f mermaid inspect FunctionName cfg

# Program dependence graph
rbuilder inspect FunctionName pdg --edge-layer data

# Dominator tree
rbuilder inspect FunctionName dom --frontiers
```

**When to use:** User needs deep program understanding, debugging complex logic, or understanding control dependencies.

### 5. Graph Metrics

Architectural analytics and hotspot identification:

```bash
# All metrics
rbuilder metrics

# PageRank (identify important functions)
rbuilder metrics --pagerank

# Betweenness (find architectural bottlenecks)
rbuilder metrics --betweenness

# Communities (architectural modules)
rbuilder metrics --communities

# JSON output
rbuilder -f json metrics --pagerank | jq .
```

**When to use:** User wants to understand architectural structure, find hotspots, identify refactoring candidates.

### 6. Export Graph

Export for external analysis or visualization:

```bash
# Full graph as JSON
rbuilder export --export-format json --export-output graph.json

# GraphML for graph tools
rbuilder export --export-format graphml --export-output graph.graphml

# Subgraph query
rbuilder export \
  --export-format graphml \
  --export-output subgraph.graphml \
  --query "MATCH (n:Function) WHERE n.name LIKE '*Service' RETURN n"

# Mermaid diagram
rbuilder export --export-format mermaid --export-output graph.mmd --query all
```

## Migration Planning Workflow

When the user mentions migration or modernization, follow this workflow:

### Step 1: Index the Repository
Ask about fast vs deep mode (see Indexing section above).

### Step 2: Understand Architecture
```bash
# Find all entry points (endpoints, main functions)
rbuilder gql "MATCH (n:Function) WHERE n.name LIKE '*Endpoint' OR n.name LIKE '*Controller' OR n.name = 'main' RETURN n"

# Identify communities (architectural modules)
rbuilder metrics --communities

# Find high-complexity functions
rbuilder -f json gql 'MATCH (n:Function) RETURN n' | jq '.rows[] | select(.complexity > 15)'
```

### Step 3: Assess Change Impact
For each component the user wants to change:
```bash
# Calculate blast radius
rbuilder blast-radius ComponentName

# Identify dependencies
rbuilder gql "MATCH (n:Function {name: 'ComponentName'})-[:CALLS|DEPENDS_ON*1..2]->(dep) RETURN dep"
```

### Step 4: Identify Hotspots
```bash
# PageRank to find critical functions
rbuilder metrics --pagerank

# Betweenness to find bottlenecks
rbuilder metrics --betweenness
```

### Step 5: Plan Migration
Based on the analysis:
1. **Low-risk first:** Functions with small blast radius, low centrality
2. **Test critical paths:** High PageRank/betweenness functions need extensive testing
3. **Respect boundaries:** Stay within communities when possible
4. **Incremental approach:** Use depth-limited blast radius to plan phases

## Advanced Features

### Query Daemon (for repeated queries)
When running many queries in a session:

```bash
# Terminal 1: Start daemon
rbuilder serve

# Terminal 2: Queries auto-connect to daemon
rbuilder blast-radius FunctionName
rbuilder gql 'MATCH (n:Function) RETURN n'
```

### CI Policy Checks
For continuous integration:

```bash
# Check changed functions against policy
rbuilder check --policy-file policy.json
```

### Language Filtering
When indexing specific languages:

```bash
rbuilder discover . -l java,typescript -e target,node_modules,dist
```

## Output Formats

All commands support multiple output formats:

```bash
# Human-readable text (default)
rbuilder gql 'query'

# JSON for scripting
rbuilder -f json gql 'query' | jq .

# Mermaid diagram
rbuilder -f mermaid inspect FunctionName cfg

# Graphviz DOT
rbuilder -f graphviz inspect FunctionName cfg

# Write to file
rbuilder -f json -o output.json blast-radius FunctionName
```

## Troubleshooting

### Graph not found
Run `rbuilder discover .` in the repository root first.

### Symbol not found
Search for the exact name:
```bash
rbuilder gql "MATCH (n:Function) WHERE n.name LIKE '*PartialName*' RETURN n"
```

### Slow discovery
- Use fast mode (default) for most use cases
- Add deep mode (`--cfg`) only when program slicing is needed
- Use language filters: `-l java,typescript -e target,node_modules`

### Installation issues
- Verify installation: `rbuilder --version`
- Check PATH: `which rbuilder`
- Refer user to: https://github.com/sshaaf/rBuilder/releases

## Best Practices

1. **Start with fast indexing** unless the user explicitly needs slicing/taint analysis
2. **Use JSON output** for complex queries to parse programmatically
3. **Limit blast radius depth** for large codebases to get faster results
4. **Export graphs** for visualization in tools like Gephi or yEd
5. **Run metrics first** to understand the architecture before planning changes
6. **Use the query daemon** when running multiple queries in the same session

## Example Usage Scenarios

### Scenario 1: Understanding a Legacy Java App
```bash
# Index the codebase
rbuilder discover . -l java -e target

# Find entry points
rbuilder gql "MATCH (n:Function) WHERE n.name LIKE '*Endpoint' RETURN n"

# Understand architecture
rbuilder metrics --communities

# Find hotspots
rbuilder metrics --pagerank
```

### Scenario 2: Planning a Function Refactor
```bash
# Check impact
rbuilder blast-radius MyFunction

# Find all callers
rbuilder gql "MATCH (caller:Function)-[:CALLS]->(n:Function {name: 'MyFunction'}) RETURN caller"

# Analyze complexity
rbuilder -f json gql "MATCH (n:Function {name: 'MyFunction'}) RETURN n" | jq '.rows[0].complexity'
```

### Scenario 3: Migration Risk Assessment
```bash
# Index with deep analysis
rbuilder discover . --all

# Identify high-risk components
rbuilder metrics --betweenness

# For each component, check blast radius
rbuilder blast-radius ComponentName --depth 5

# Export for documentation
rbuilder export --export-format mermaid --export-output architecture.mmd --query all
```

## Remember

- **Always check installation** before running commands
- **Always ask about indexing preference** (fast vs deep) if not already indexed
- **Use fast mode by default** and only suggest deep mode when the user needs slicing/taint
- **Provide context** about why you're running specific commands
- **Parse JSON output** when you need to analyze results programmatically
- **Explain timing expectations** when indexing large codebases
- **Point to GitHub repo** (https://github.com/sshaaf/rBuilder) for detailed docs

The goal is to help users understand their codebase deeply and plan large-scale changes safely.
