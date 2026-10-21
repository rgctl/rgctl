# Watch mode and incremental update

Keep the structural graph aligned with the working tree **without** re-running full `discover` on every edit.

## Commands

| Command | Role |
|---------|------|
| `rgctl update` | One-shot incremental patch (hash-diff, `--files`, or `--since`) |
| `rgctl serve --watch` | Debounced filesystem watcher → same incremental updater |
| `rgctl status` | Reports `index_current` / dirty file counts (no extract); also `watcher_alive` / queue depth (`-f json` → `schema_version` 3) |
| `rgctl discover` | Cold index + optional analysis (`--with-cfg`, migration, Kantra, …) |

`discover --files path1,path2` remains a compatible alias for `update --files`.

## update vs discover

- **`update`** extracts/compacts **changed paths only** into `graph.snapshot.bin` and refreshes `file_hashes.json`. Analysis sidecars keyed on graph digest (CFG/PDG archive, blast snapshot, semantic index, …) are **invalidated**.
- **`discover`** walks the tree and can rebuild analysis artifacts when you pass the relevant flags. Use it for cold start and deliberate analysis refresh — not as the default agent “refresh” step.

## serve --watch

```bash
rgctl serve --watch --open
# or query-only:
rgctl serve --watch --query-only --no-pipeline
```

Behavior:

1. OS notifications (via `notify`) on the session repo
2. Debounce (`rgctl.toml` → `[watch] debounce_ms`, default **500**)
3. Filter to language extensions; skip `.git`, `target`, `node_modules`, `.rgctl`
4. Poll for CLI update queue + coalesce with FS pending paths
5. `IncrementalUpdater` on a dedicated watch thread (sole compact writer)
6. Existing serve digest poll hot-reloads the mmap graph

Default `serve` (without `--watch`) does **not** watch sources.

### Exclusive writer (one watcher per repo)

`.rgctl/watch.lock` elects a **sole snapshot writer** (the live `serve --watch` process).

- A second `rgctl serve --watch` on the same repo **exits immediately** with a clear error (does not start HTTP).
- While a live watcher holds the lock, `rgctl update` **enqueues** work to `.rgctl/update_queue.jsonl` and waits for `.rgctl/update_results/<request_id>.json` by default (does **not** compact in the CLI process).
- Use `--no-wait` to return after enqueue with `{ queued: true, request_id }`.
- Use `--wait-timeout <secs>` (default **60**, or `RGCTL_UPDATE_WAIT_TIMEOUT_SECS`) to bound the wait.
- `--force` is **rejected** while a live watcher is elected (stop watch or run full `discover`).
- Stopping the first `serve --watch` releases the lock (normal process exit). A hard kill can leave a stale `watch.lock` — the next `update` / `serve --watch` reclaims it when the recorded PID is dead.

You do **not** need to stop `serve --watch` to refresh the graph after bulk edits or missed notify events — run `rgctl update`.

## Agent guidance

1. Prefer `status` → if `index_current == false` → `update` once (works with or without watch)
2. Long edit sessions: run `serve --watch` in the background; keep using `update` when needed
3. Do **not** loop full `discover` for dirty trees
4. Empty `blast-radius` for a file that exists on disk → `update --files <path>`, retry, then source fallback
5. Optional: `status` fields `watcher_alive` / `watcher_pid` / `update_queue_pending` for diagnostics

## Config

```toml
# rgctl.toml
[watch]
debounce_ms = 500
```

## Related

- Issues [#69](https://github.com/sshaaf/rgctl/issues/69) / [#58](https://github.com/sshaaf/rgctl/issues/58) (hooks still follow-on)
- [JSON API](../json-api.md) — `status` / `update` schemas
- [HTTP Server and Dashboard](http-server-and-dashboard.md)
