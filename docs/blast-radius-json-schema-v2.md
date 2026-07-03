# Blast-radius JSON output — schema v2 (target metadata)

**Effective:** P2.2 — `schema_version: 2` on `rbuilder -f json blast-radius …`.

Schema v2 extends the v1 nested payload (`target`, `metrics`, `topology`, `gatekeeping`) with **polyglot target routing** fields. Sections other than `target` are unchanged from v1.

Canonical catalog: [cli-output-schemas.md](cli-output-schemas.md) §1.

---

## What changed (v1 → v2)

| Field | v1 | v2 |
|-------|----|----|
| `schema_version` | `1` | `2` |
| `target.language` | absent | `"java"`, `"rust"`, `"python"`, or `"unknown"` |
| `target.signature` | absent | method signature string when known; **key omitted** when unknown |
| `target.canonical_fqn` | absent | uniform `Class::method` (e.g. `OrderService::process`) |
| `metrics.caller_depth_limit` | absent | integer when `--depth N` caps `topology.impact_zone`; **omitted** for full closure |

**Unchanged:** other `metrics` / `topology` / `gatekeeping` shapes, text output, exit codes, fast-path behavior (depth filter applies on cache hits too).

---

## `target` block (v2)

```json
{
  "id": "424d403b-1b2c-4a3d-8e9f-0c1b2a3f4e5d",
  "symbol": "process",
  "class_context": "OrderService",
  "file_path": "java/com/example/OrderService.java",
  "language": "java",
  "signature": "public void process(String orderId) {",
  "canonical_fqn": "OrderService::process"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `language` | string | Lowercase id from graph node `properties.language` or file extension fallback |
| `signature` | string \| omitted | Type-erased signature from graph `Node.signature` (tree-sitter at discover time) |
| `canonical_fqn` | string | Language-agnostic routing id; Java dot FQNs normalized to `Class::method` |

### FQN policy

| Location | Notation | Use |
|----------|----------|-----|
| `target.canonical_fqn` | `Class::method` | **Routing** — prefer for agents and overload disambiguation |
| `topology.*.fqn` | language-native (e.g. `com.example.OrderService.process`) | **Display** — opaque text; chain on `id` UUID |

Do not parse `topology.fqn` with language-specific regex; use `target.canonical_fqn` + `target.id`.

---

## `--depth` and `metrics.caller_depth_limit`

```bash
rbuilder -f json blast-radius OrderService::process --depth 5
```

| Hop | Meaning |
|-----|---------|
| 1 | Direct callers only (in `impact_zone`; `direct_callers` unchanged) |
| 2 | Callers of direct callers, etc. |
| omitted | Full transitive upstream closure (no `caller_depth_limit` key) |

When capped, `metrics.impact_zone_size` and `metrics.score` reflect the filtered zone. Policy checks (`--policy-file`) use the same filtered set.

---

## Cache / discover requirements

Target metadata is populated at **`discover`** into:

- `macro_call_index.bin` → `symbol_context` (language, signature, canonical_fqn)
- `macro_call_index.db` → `macro_call_candidates` columns

**Re-run `discover`** after upgrading to refresh SQLite columns and v2 fields on cache hits.

---

## Migration (jq)

```bash
# Prefer canonical routing id (new)
jq '.target.canonical_fqn'

# Language filter
jq '.target.language'

# Overload signature
jq '.target.signature // empty'

# Still valid: topology UUID chaining
jq '[.topology.direct_callers[].id]'
```

---

## Verification

```bash
cargo test --test cli_output test_blast_radius_target_v2_metadata
cargo test --test subprocess_golden_path blast_radius_json_exit_zero_after_discover
```

---

## Related

- [blast-radius-json-schema-v1.md](blast-radius-json-schema-v1.md) — v1 break from flat JSON
- [cli-output-schemas.md](cli-output-schemas.md) — full CLI schema catalog
- `src/cli/blast_radius_output.rs` — `BLAST_RADIUS_SCHEMA_VERSION`
- `crates/rbuilder-analysis/src/macro_call_lookup.rs` — `canonical_fqn_from_node`, cache columns
