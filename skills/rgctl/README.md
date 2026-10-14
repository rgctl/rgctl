# rgctl Skill

A skill for answering structural questions about codebases using the rgctl CLI graph.

## Quick Stats

- **Meta skill:** `rgctl` (router + structured verb tables + references)
- **Companion skill + command:** `rgctl-review` / `/rgctl:review` (PR/branch review → `REVIEW_REPORT.md`)
- **Reference files:** command encyclopedia, workflows, communities & policy
- **Workflow families (docs only):** discover, impact, flow, search, migrate, gate, vuln, review
- **Agent query path:** `find` / `callers` / `callees` / `relations` / `inventory` / `status` (no Cypher)

## Structure

```
skills/rgctl/                 # Meta skill (this tree)
skills/rgctl-review/          # Review protocol skill
agent-pack/commands/          # Thin slash-command stubs (codegen → per adapter)
```

`rgctl install --skill` writes the meta skill, companion skills, and slash commands (OpenSpec dual). It does **not** create retired `rgctl-discover` / `rgctl-impact` / … skill directories.

## What's Covered

### Main SKILL.md (Always Loaded)

- When to use rgctl
- **CLI subprocess workflow** — spawn `rgctl -f json` for agents
- **Workflow families:**
  1. Discovery & Indexing
  1b. Migration roadmap (`--export-migration-hints`)
  2. Query & Search (structured verbs + communities)
  3. Impact & Safety (includes policy checks)
  4. Metrics & Analysis
  5. Code Analysis (CFG/PDG/slicing)
  6. Export & Visualization
- **NL routing table** (user utterances → commands)
- Failure playbook

### References (Loaded On-Demand)

#### command-encyclopedia.md
- Structured query verbs and domain commands
- JSON sample responses
- Prerequisites and pitfalls
- "What to report" guidelines

#### workflows.md
- Worked NL scenarios (discover, impact, flow, search, migrate, gate, vuln)
- Edit this file directly; `rgctl install --skill` copies it as-is

#### communities-and-policy.md
- **Community Detection:** list, semantic scope, ownership workflows
- **CI Policy Checks:** schema, CI integration, calibration

## Design Principles

✅ **Progressive disclosure** - Main skill lean, details in references
✅ **Workflow-centric** - Organized by user intent, not commands
✅ **CLI-first** - Agents use `rgctl -f json` structured verbs
✅ **Clear routing** - Natural language → command mapping
✅ **No Cypher in skill surface** - Agents must not invent MATCH strings

## Installation

From a target repository (not the rgctl source tree unless you are dogfooding):

```bash
rgctl install --skill --tools cursor,claude,codex,antigravity,agents
```

Installs a single skill named `rgctl`. See [Agent pack walkthrough](../../docs/guides/agent-skill.md).

If an older pack left `rgctl-discover` / `rgctl-impact` / … directories behind, delete them manually — install no longer writes those folders.

**Maintainers:** edit `references/workflows.md` in this tree; the agent pack copies it at `rgctl` build time.

## See Also

- [User Guide](../../docs/user-guide.md) - Complete CLI tutorial
- [Agent recipes](../../docs/agent-recipes.md) - Copy-paste CLI workflows
- [HTTP Server and Dashboard](../../docs/guides/http-server-and-dashboard.md) - Optional `rgctl serve` for dashboard
- [JSON API](../../docs/json-api.md) - Schema specifications
- [All Guides](../../docs/guides/README.md) - Feature-specific guides
