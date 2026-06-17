# Phase 13: Real-time Updates & Automation

This guide covers **watch mode**, **git hooks**, and **MCP integration** for keeping the rBuilder knowledge graph in sync with your repository.

## Quick start

```bash
# Index the repo once
rbuilder init

# Install git hooks (pre-commit, post-commit, post-checkout)
rbuilder init-hooks

# Watch for file changes and re-index incrementally
rbuilder watch
```

## Watch mode

Watch mode monitors the repository for file changes and runs **incremental graph updates** (only touched files are re-indexed).

### CLI

```bash
# Default: current directory, 500ms debounce
rbuilder watch

# Custom path and debounce window
rbuilder watch /path/to/repo --debounce-ms 1000
```

### Configuration (`rbuilder.toml`)

```toml
[watch]
debounce_ms = 500
```

Rapid saves are batched: the graph updates after no new events for `debounce_ms` milliseconds.

### What is watched

- **CREATE**, **MODIFY**, and **DELETE** events on discovered source files
- Ignores `.git/` and `.rbuilder/` directories
- Uses the same file discovery rules as `rbuilder init`

### MCP watch mode

Run the MCP server with background watch enabled:

```bash
# stdio transport — notifications on stdout
rbuilder mcp serve --watch

# HTTP transport — poll for latest notification
rbuilder mcp serve --transport http --port 3000 --watch
curl http://127.0.0.1:3000/notifications/latest
```

**stdio notification format:**

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/graph_updated",
  "params": {
    "timestamp": 1718659200,
    "files_changed": ["src/auth.rs"],
    "nodes_added": 2,
    "nodes_removed": 1,
    "edges_changed": 3
  }
}
```

Clients can query tools immediately after a notification; the in-memory graph is updated before the notification is sent.

## Git hooks

### Install

```bash
rbuilder init-hooks          # skip if hooks already exist
rbuilder init-hooks --force  # overwrite existing hooks
```

This writes scripts to `.git/hooks/` and creates `rbuilder.toml` if missing.

### Pre-commit

Runs blast-radius analysis on **staged** files:

```bash
rbuilder detect-changes --json <staged-files>
```

| Risk level | Behavior |
|------------|----------|
| CRITICAL | Blocks commit |
| HIGH | Prompts to continue |
| LOW / MEDIUM | Allows commit |

Bypass: `git commit --no-verify`

### Post-commit

After each successful commit, updates the graph for committed files:

```bash
rbuilder update --files <committed-files>
```

### Post-checkout (branch switches)

On branch checkout (`$3 == 1`), diffs old vs new HEAD and updates changed files:

```bash
rbuilder update --files <changed-files>
```

## Configuration reference

```toml
[hooks]
pre_commit = true
post_commit = true
post_checkout = true
block_on_risk = "CRITICAL"   # CRITICAL | HIGH | MEDIUM | LOW
blast_radius_threshold = 50

[watch]
debounce_ms = 500
```

## Manual commands

```bash
# Analyze risk for specific files (or staged files if none given)
rbuilder detect-changes --json src/api.rs

# Update specific files without a full incremental scan
rbuilder update --files src/foo.rs src/bar.rs
```

## Troubleshooting

| Issue | Fix |
|-------|-----|
| Hook skipped | Ensure `rbuilder` is on `PATH` in git hook environment |
| Watch not updating | Run `rbuilder init` first; only discovered files are tracked |
| MCP stale data | Use `rbuilder mcp serve --watch` so graph updates in-process |
| Pre-commit always passes | No graph at repo root — run `rbuilder init` |
| Hook overwrite refused | Use `rbuilder init-hooks --force` |

## Architecture

```
File change (notify)
       │
       ▼
  Debounce (500ms)
       │
       ▼
IncrementalUpdater.update_files()
       │
       ├──► .rbuilder/ graph on disk
       └──► MCP AppState (in-memory, when --watch)
                │
                └──► notifications/graph_updated
```
