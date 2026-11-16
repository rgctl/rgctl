# `rgctl-core`

Foundational types (`NodeId`, `NodeType`, `EdgeType`, `Node`, `Edge`, `Parameter`, `Visibility`) and top-level workspace facade re-exporting `rgctl`'s storage, extraction, analysis, pipeline, incremental, and export crates.

---

## Core Responsibilities

1. **Canonical Graph Schema (`rgctl-graph` / [`src/lib.rs`](src/lib.rs)):**
   - [`NodeId`](src/lib.rs): 128-bit UUIDv5 deterministic symbol identifier + dense `u32` CSR index conversions.
   - [`NodeType`](src/lib.rs): Compact `u8`-backed symbol classification (`Function`, `Method`, `Class`, `Interface`, `Struct`, `Enum`, `Trait`, `Module`, `File`, `Field`, `Variable`, `Constant`, `Import`, `Annotation`, `Macro`, `TypeAlias`, `Property`, `Constructor`, `Namespace`, `Component`, `Endpoint`, `DatabaseTable`, `ConfigKey`, etc.).
   - [`EdgeType`](src/lib.rs): Compact `u32`-backed typed relation enum (`Calls`, `Imports`, `Inherits`, `Implements`, `Overrides`, `Contains`, `References`, `Uses`, `Returns`, `Accepts`, `Throws`, `AnnotatedWith`, `Instantiates`, `Exports`, `Defines`, `Mutates`, `Reads`, `DependsOn`, `Tests`, `CoChangesWith`, `DataFlow`, `ControlFlow`, `TaintFlow`, etc.).
2. **Facade Re-Exports (`src/lib.rs`):**
   - Re-exports the public APIs of `rgctl-graph`, `rgctl-analysis`, `rgctl-extraction`, `rgctl-pipeline`, `rgctl-incremental`, `rgctl-registry`, `rgctl-export`, `rgctl-rules`, `rgctl-security`, `rgctl-semantic`, and `rgctl-project-config` for ergonomic library consumption.

---

## Architecture References

- [**Graph Storage & Extraction Architecture**](../../docs/graph-storage-architecture.md)
- [**Graph Analysis, CPG & Reachability Architecture**](../../docs/analysis-architecture.md)
- [**Workspace Code Structure**](../../docs/Code_structure.md)

```bash
# Strict clippy & unit test gate
cargo clippy -p rgctl-core -- -D warnings
cargo test -p rgctl-core
```
