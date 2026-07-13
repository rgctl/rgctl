# rbuilder-core

Public library facade for rBuilder — one dependency for graph, analysis, pipeline, dashboard helpers, and related crates.

## Unique code

Only `memory::MemoryMonitor` (RSS tracking via sysinfo) lives in this crate. All graph and analysis logic is in workspace crates re-exported from `lib.rs`.

## Re-export policy

Add a crate to `rbuilder-core` when:

1. Library consumers commonly need it alongside graph/analysis APIs, and
2. The crate has a stable public surface suitable for re-export.

Prefer direct dependencies on domain crates (`rbuilder-graph`, `rbuilder-analysis`) for slim binaries or plugins that need only one layer.

## Key modules

| Re-export | Crate |
|-----------|-------|
| `graph` | `rbuilder-graph` |
| `analysis` | `rbuilder-analysis` |
| `pipeline` | `rbuilder-pipeline` |
| `gql` | `rbuilder-gql` |
| `incremental` | `rbuilder-incremental` |

## Version

`rbuilder_core::VERSION` matches this crate's package version.
