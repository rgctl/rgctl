# rBuilder as an AI Coding Agent Assistant

## Vision

**rBuilder arms AI coding agents (like Claude Code) with deep, queryable knowledge of codebases, enabling more accurate and context-aware code assistance.**

---

## Primary Use Case: AI Agent Integration

### The Problem
AI coding agents today have limited codebase understanding:
- Must read files sequentially to understand architecture
- Can't quickly answer "what breaks if I change X?"
- No structural understanding of communities/modules
- Limited config-to-code relationship knowledge
- Struggle with impact analysis across large codebases

### The Solution
rBuilder provides a **queryable knowledge graph** that agents can interrogate:
- Instant architectural understanding (communities, modules)
- Impact analysis in milliseconds (not minutes of file reading)
- Configuration relationships pre-computed
- Complexity and quality metrics built-in
- Natural language interface for easy agent integration

---

## Integration Patterns

### 1. MCP (Model Context Protocol) Server

**What is MCP?**
MCP is a protocol for connecting AI assistants to external data sources and tools. Claude Code, Cursor, and other agents support MCP.

**rBuilder MCP Server Implementation:**

```rust
// src/mcp/server.rs
use mcp_sdk::{Server, Tool, Resource};

pub struct RBuilderMCPServer {
    graph: Arc<GraphBackend>,
    nlp_engine: NLPQueryEngine,
}

impl Server for RBuilderMCPServer {
    fn name(&self) -> &str { "rbuilder" }
    
    fn tools(&self) -> Vec<Tool> {
        vec![
            Tool {
                name: "query_codebase",
                description: "Query the codebase knowledge graph using natural language",
                parameters: json!({
                    "question": "string - Natural language question about the codebase"
                }),
            },
            Tool {
                name: "impact_analysis",
                description: "Analyze what would break if a symbol is changed or deleted",
                parameters: json!({
                    "symbol": "string - Function, class, or module name",
                    "depth": "number - How many hops to traverse (default: 3)"
                }),
            },
            Tool {
                name: "find_by_complexity",
                description: "Find functions/classes by complexity threshold",
                parameters: json!({
                    "min_complexity": "number",
                    "labels": "array - Optional label filters"
                }),
            },
            Tool {
                name: "get_community_info",
                description: "Get information about architectural communities/modules",
                parameters: json!({
                    "community_name": "string - Optional, returns all if omitted"
                }),
            },
            Tool {
                name: "config_analysis",
                description: "Analyze configuration usage, find unused keys, missing env vars",
                parameters: json!({
                    "analysis_type": "enum - unused_keys | missing_env | secrets | drift"
                }),
            },
            Tool {
                name: "symbol_info",
                description: "Get detailed information about a function, class, or module",
                parameters: json!({
                    "symbol_name": "string",
                    "include_callers": "boolean",
                    "include_dependencies": "boolean"
                }),
            },
        ]
    }
    
    fn resources(&self) -> Vec<Resource> {
        vec![
            Resource {
                uri: "rbuilder://graph/schema",
                name: "Graph Schema",
                description: "Overview of graph structure, node types, edge types",
            },
            Resource {
                uri: "rbuilder://graph/stats",
                name: "Graph Statistics",
                description: "Overall codebase statistics and metrics",
            },
            Resource {
                uri: "rbuilder://communities",
                name: "Architectural Communities",
                description: "Detected architectural modules and their boundaries",
            },
        ]
    }
    
    async fn execute_tool(&self, name: &str, params: Value) -> Result<Value> {
        match name {
            "query_codebase" => {
                let question = params["question"].as_str().unwrap();
                let result = self.nlp_engine.query(question).await?;
                Ok(json!({
                    "answer": result.natural_answer,
                    "query": result.query,
                    "confidence": result.confidence,
                }))
            }
            "impact_analysis" => {
                let symbol = params["symbol"].as_str().unwrap();
                let depth = params["depth"].as_u64().unwrap_or(3);
                let impact = self.analyze_impact(symbol, depth)?;
                Ok(json!(impact))
            }
            // ... other tools
            _ => Err(Error::UnknownTool)
        }
    }
}
```

**MCP Server Deployment:**

```bash
# Start as stdio server (for Claude Code integration)
rbuilder mcp serve --transport stdio

# Start as HTTP server (for team-wide access)
rbuilder mcp serve --transport http --port 3000

# Configure in Claude Code's MCP settings
cat >> ~/.claude/mcp_servers.json <<EOF
{
  "rbuilder": {
    "command": "rbuilder",
    "args": ["mcp", "serve", "--transport", "stdio"],
    "cwd": "/path/to/your/project"
  }
}
EOF
```

### 2. Direct API Integration

**REST API for AI Agents:**

```rust
// src/api/server.rs
#[derive(OpenApi)]
struct ApiDoc;

#[utoipa::path(
    post,
    path = "/api/query",
    request_body = QueryRequest,
    responses(
        (status = 200, description = "Query result", body = QueryResponse)
    )
)]
async fn query_endpoint(
    State(state): State<AppState>,
    Json(req): Json<QueryRequest>,
) -> Result<Json<QueryResponse>> {
    let result = state.nlp_engine.query(&req.question).await?;
    Ok(Json(QueryResponse {
        answer: result.natural_answer,
        cypher_query: result.query,
        results: result.raw_results,
        confidence: result.confidence,
    }))
}

#[utoipa::path(
    get,
    path = "/api/impact/{symbol}",
    params(
        ("symbol" = String, Path, description = "Symbol to analyze"),
        ("depth" = Option<u32>, Query, description = "Traversal depth")
    ),
    responses(
        (status = 200, description = "Impact analysis", body = ImpactAnalysis)
    )
)]
async fn impact_endpoint(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
    Query(params): Query<ImpactParams>,
) -> Result<Json<ImpactAnalysis>> {
    let depth = params.depth.unwrap_or(3);
    let impact = state.graph.analyze_impact(&symbol, depth)?;
    Ok(Json(impact))
}
```

**API Endpoints:**
- `POST /api/query` - Natural language query
- `GET /api/impact/:symbol` - Impact analysis
- `GET /api/complexity` - Complexity metrics
- `GET /api/communities` - Architectural communities
- `GET /api/config/analysis` - Config analysis
- `GET /api/symbol/:name` - Symbol details
- `POST /api/cypher` - Direct Cypher query

### 3. CLI for Agent Scripts

**Agent-friendly CLI output:**

```bash
# JSON output for parsing
rbuilder ask "How many React components?" --format json

# Structured data for scripts
rbuilder impact verify_token --format json --depth 3

# Machine-readable reports
rbuilder stats --format json
```

---

## Agent Workflow Examples

### Example 1: AI Agent Helps User Refactor

**User**: "Help me refactor the authentication system"

**Claude Code** (using rBuilder MCP):
1. Query: "What functions are in the auth community?"
2. Query: "What's the complexity of each auth function?"
3. Query: "What external modules depend on auth?"
4. Analysis: Identifies high-complexity functions, suggests refactoring
5. Query: "What breaks if I change authenticate_with_mfa()?"
6. Shows impact analysis, helps user plan migration

**Without rBuilder**: Claude would need to read dozens of files, manually trace dependencies, miss cross-file relationships.

**With rBuilder**: Instant architectural understanding, precise impact analysis, confident refactoring suggestions.

### Example 2: Bug Fix Assistance

**User**: "There's a bug in the payment processing, can you help?"

**Claude Code** (using rBuilder):
1. Query: "Show me all payment-related functions"
2. Query: "Which payment functions have high complexity and no tests?"
3. Identifies `process_payment_with_retry()` (complexity: 45, no tests)
4. Query: "What functions call process_payment_with_retry()?"
5. Understands blast radius before suggesting fix

### Example 3: Configuration Audit

**User**: "Our configs are a mess, can you clean them up?"

**Claude Code** (using rBuilder):
1. Query: "Which config keys are unused?"
2. Query: "Find missing environment variables"
3. Query: "Find hardcoded secrets in config files"
4. Generates cleanup plan with impact assessment
5. Shows which code would break if each key is removed

---

## Agent-Specific Features

### 1. Context Compression

**Problem**: AI agents have limited context windows. Reading entire files wastes tokens.

**Solution**: rBuilder provides **compressed, relevant context**:

```rust
pub struct SymbolContext {
    pub name: String,
    pub location: FileLocation,
    pub signature: String,
    pub complexity: ComplexityMetrics,
    pub labels: Vec<String>,
    pub summary: String,  // One-sentence description
    pub callers: Vec<String>,  // Just names, not full code
    pub dependencies: Vec<String>,
    pub community: String,
}

// Agent asks: "Tell me about verify_token()"
// Instead of reading entire file, get compressed context:
{
  "name": "verify_token",
  "location": "src/auth/jwt.rs:89",
  "signature": "fn verify_token(token: &str) -> Result<Claims>",
  "complexity": {"cyclomatic": 12, "cognitive": 15},
  "labels": ["security:critical", "auth:core"],
  "summary": "Validates JWT signature and extracts claims",
  "callers": ["authenticate_user", "refresh_session", "admin_impersonate"],
  "dependencies": ["jwt::decode", "get_public_key", "validate_claims"],
  "community": "auth"
}
```

**Token savings**: Instead of 2000+ tokens for full file, ~200 tokens for context.

### 2. Incremental Context Building

**Pattern**: Agent asks follow-up questions to build context gradually.

```
Agent: "What's in the auth module?"
rBuilder: [List of 67 functions with summaries]

Agent: "Tell me more about authenticate_with_mfa"
rBuilder: [Detailed context for that function]

Agent: "What calls it?"
rBuilder: [List of 5 callers with their contexts]

Agent: "Show me the most complex caller"
rBuilder: [Full context of authenticate_user()]
```

Each query returns just what's needed, not everything.

### 3. Pre-computed Insights

**Instead of asking the agent to analyze**, rBuilder provides ready-to-use insights:

```json
{
  "function": "process_payment",
  "insights": {
    "risk_level": "high",
    "reasons": [
      "High complexity (cyclomatic: 28)",
      "Security-critical (handles payment data)",
      "Called by 8 different endpoints",
      "No integration tests",
      "Uses deprecated payment gateway API"
    ],
    "recommendations": [
      "Add comprehensive integration tests",
      "Migrate to new payment gateway API",
      "Consider splitting into smaller functions",
      "Add monitoring for payment failures"
    ],
    "migration_path": {
      "estimated_effort": "2-3 days",
      "blockers": ["Need new API credentials", "Requires QA approval"],
      "test_strategy": "Add tests for each payment method first"
    }
  }
}
```

### 4. Diff-Aware Queries

**When user is working on a branch**, rBuilder knows what changed:

```bash
# Agent asks: "What did I change?"
rbuilder diff --since main --format json

# Agent asks: "What tests should I run?"
rbuilder impact --changed-files --test-suggestions

# Agent asks: "What's the risk of this PR?"
rbuilder analyze-pr --risk-assessment
```

---

## MCP Tool Usage Examples

### Claude Code Using rBuilder MCP Tools

**Scenario 1: Understanding Architecture**

```
User: "Can you explain the architecture of this codebase?"

Claude Code:
- Calls: query_codebase("What are the architectural communities?")
- Gets: 8 communities with purposes and boundaries
- Calls: get_community_info() for each community
- Synthesizes: "This is a microservices architecture with 8 main modules..."
```

**Scenario 2: Safe Refactoring**

```
User: "Rename verify_token to validate_token"

Claude Code:
- Calls: impact_analysis("verify_token", depth=3)
- Gets: 23 affected functions, risk assessment
- Calls: symbol_info("verify_token", include_callers=true)
- Plans refactoring: "This will affect 23 functions. I'll update them in this order..."
- Shows preview: "Here's what will change..."
- User approves
- Claude makes changes safely
```

**Scenario 3: Code Review**

```
User: "Review this PR for issues"

Claude Code:
- Calls: query_codebase("What changed in this PR?")
- For each changed function:
  - Calls: find_by_complexity(min_complexity=20)
  - Calls: config_analysis("secrets")
  - Calls: impact_analysis(function_name)
- Generates review: "Found 3 issues: 1. New function has high complexity..."
```

---

## Performance Considerations for Agents

### 1. Query Response Time

**Target**: < 100ms for 99% of queries

**Why**: Agents make multiple queries per interaction. Slow queries = slow agents.

**Optimizations**:
- Pre-computed graph metrics (communities, centrality)
- Indexed queries (by symbol name, file path, labels)
- Cached NLP patterns (no LLM call for common questions)
- Incremental graph updates (< 5s for changed files)

### 2. Context Window Efficiency

**Compressed Responses**: Return minimal, structured data

```rust
// BAD: Returns full source code (thousands of tokens)
{
  "function": "verify_token",
  "source": "/* 200 lines of code */"
}

// GOOD: Returns structured metadata (hundreds of tokens)
{
  "function": "verify_token",
  "signature": "fn verify_token(token: &str) -> Result<Claims>",
  "complexity": 12,
  "callers": ["authenticate_user", "refresh_session"],
  "labels": ["security:critical"]
}
```

### 3. Parallel Queries

**MCP supports batching**. Agent can ask multiple questions at once:

```rust
// Agent batches 3 queries:
[
  query_codebase("What's in the auth module?"),
  find_by_complexity(min_complexity=20),
  config_analysis("unused_keys")
]

// rBuilder processes in parallel, returns all results
```

---

## Agent Capabilities Enabled by rBuilder

| Capability | Without rBuilder | With rBuilder |
|------------|------------------|---------------|
| **Understand architecture** | Read 50+ files sequentially | Query communities (< 100ms) |
| **Impact analysis** | Trace dependencies manually, error-prone | Precise graph traversal, 100% accurate |
| **Find complex code** | Scan all files, calculate metrics | Pre-computed, instant results |
| **Config audit** | Grep for usage, manual verification | Code-to-config graph, automated analysis |
| **Suggest refactoring** | Limited context, conservative | Full knowledge graph, confident suggestions |
| **Security review** | Keyword search, manual inspection | Labeled security nodes, flow analysis |
| **Test suggestions** | Guess from file names | Precise impact analysis, coverage data |
| **Cross-language understanding** | Separate analysis per language | Unified graph, cross-lang relationships |

---

## Deployment Scenarios

### 1. Personal Developer Setup

```bash
# One-time setup per project
cd ~/my-project
rbuilder init .

# Start MCP server for Claude Code
rbuilder mcp serve --transport stdio &

# Now Claude Code can query the graph
```

### 2. Team-Wide Deployment

```bash
# Shared server for all team members
rbuilder mcp serve --transport http --port 3000 --host 0.0.0.0

# Team members configure Claude Code to use shared server
# Benefits: One graph for whole team, always up-to-date
```

### 3. CI/CD Integration

```yaml
# .github/workflows/update-graph.yml
name: Update Knowledge Graph

on:
  push:
    branches: [main]

jobs:
  update-graph:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - name: Update rBuilder graph
        run: |
          rbuilder update --since ${{ github.event.before }}
          rbuilder export --format json --output graph.json
      - name: Upload artifact
        uses: actions/upload-artifact@v3
        with:
          name: knowledge-graph
          path: graph.json
```

---

## Future: Agent-to-Agent Communication

**Vision**: Multiple AI agents collaborating via rBuilder

```
Code Review Agent:
  - Queries rBuilder for complexity metrics
  - Queries for test coverage
  - Identifies high-risk changes

Architecture Agent:
  - Queries for community structure
  - Identifies architectural violations
  - Suggests improvements

Security Agent:
  - Queries for security-labeled functions
  - Analyzes data flows
  - Identifies vulnerabilities

All agents share the same knowledge graph, no duplicate analysis.
```

---

## Why This Matters

**Traditional AI Coding Assistants**:
- Reactive: Answer questions by reading files
- Limited: Can only see what's in immediate context
- Slow: Must re-analyze on every interaction
- Error-prone: Miss cross-file relationships

**AI Coding Assistants + rBuilder**:
- Proactive: Understand architecture upfront
- Comprehensive: Full codebase knowledge graph
- Fast: Pre-computed insights, instant queries
- Accurate: Graph-based relationship tracking

**Result**: AI agents become **10x more effective** at complex tasks like refactoring, architecture review, and impact analysis.
