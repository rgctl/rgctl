# rbuilder-graph

Graph storage and query layer for [rBuilder](https://github.com/sshaaf/rBuilder).

See [docs/graph-storage-architecture.md](../../docs/graph-storage-architecture.md).

## Modules

| Module | Role |
|--------|------|
| `schema` | Node/edge types |
| `backend::MemoryBackend` | In-memory store + indexes |
| `backend::GraphBackend` | Minimal trait (insert/get/query) |
| `query` | Mini query language |
| `snapshot` | Prepared + mmap snapshots (v1) |
| `columnar_snapshot` | Columnar mmap v2 |
| `code_graph` | High-level `CodeGraph` API |

## GraphBackend vs MemoryBackend

`GraphBackend` covers basic CRUD and string queries. Performance APIs (`edge_topology_typed`, batch iterators, typed indexes) require `MemoryBackend` directly — the only production backend today.

## Analysis integration

`rbuilder-analysis` builds `PetGraphView` from `edge_topology_typed()` or mmap snapshots. Do not use `MemoryBackend::calculate_pagerank` for new code — use `rbuilder-analysis::CentralityAnalyzer`.

## Tests

```bash
cargo test -p rbuilder-graph
cargo clippy -p rbuilder-graph -- -D warnings
```

Golden-repo validation: [scripts/validate-golden-repos.sh](../../scripts/validate-golden-repos.sh)
