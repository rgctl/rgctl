# Hybrid Code Property Graph (CPG) — Engineering Design

This document specifies the **Two-Resolution Hybrid Code Property Graph (CPG) architecture** in `rgctl` (`crates/rgctl-analysis/src/cpg.rs`, `field_write.rs`, `ast_skeleton.rs`, and `src/cli/cpg.rs`). It explains how `rgctl` provides Joern-grade CPG queries (cross-repo call topology, statement-level data/control flows, typed field-mutation proofs, and coarse AST skeletons) by joining the hot `RGCL` v2 snapshot (`L_repo`) with on-demand or prebuilt procedural sidecars (`L_proc`) through a unified virtual query facade (`L_query`).

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ LAYER 1 (`L_repo`): ALWAYS-ON REPOSITORY TOPOLOGY (`graph.snapshot.bin`)     │
│                                                                              │
│  • 64B `NodeRow` + 40B `EdgeRow` (`RGCL` v2 mmap)                            │
│  • Call graph (`Calls`), type hierarchy (`Inherits`, `Implements`,           │
│    `Overrides`), containment (`Contains`), and cold `NodeExtension`          │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │Joined by Function `Uuid` / `StableKey`
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ LAYER 2 (`L_proc`): PROCEDURAL CPG SIDECARS (`.rgctl/analysis/`)             │
│                                                                              │
│  • `cfg_pdg.archive.bin` (`RBCP` v3 + 32B `TocEntry` + `TOC3` trailer):      │
│      - Built eagerly via `discover --with-cfg` OR lazily per function via    │
│        `ensure_cfg_pdg_for_source` (`cpg flows` / `cpg slice`)               │
│  • `field_write.index.bin` (built during `discover --with-cfg`):             │
│      - Inverted index of typed field assignments (`obj.field = ...`,         │
│        `this.field = ...`) keyed by receiver `TypeName` and `member`         │
│  • `ast_skeleton.archive.bin` (opt-in via `discover --with-ast-skeleton`):   │
│      - Coarse per-function AST control/call/assign hierarchy                 │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ LAYER 3 (`L_query`): VIRTUAL CPG FACADE (`rgctl cpg` / `POST /api/cpg`)      │
│                                                                              │
│  • `cpg status`    → Reports `L_repo` + `L_proc` archive readiness & counts  │
│  • `cpg function`  → Resolves `L_repo` symbol metadata + `L_proc` presence   │
│  • `cpg calls`     → Bridges `L_repo` incoming/outgoing call neighborhood    │
│  • `cpg mutations` → Queries `field_write.index.bin` (`--exclude-ctors`)     │
│  • `cpg flows`     → Forward/backward PDG reachability (`--with-alias`)      │
│  • `cpg pdg` / `cpg slice` / `cpg ast` / `cpg export`                        │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Code Property Graph (CPG)** | Yamaguchi, Golde, Arp & Rieck (2014), *Modeling and Discovering Vulnerabilities with Code Property Graphs* (*IEEE S&P*) | Rather than fusing every AST token, CFG edge, and DFG edge of a 70,000-file repository into a single monolithic graph (which causes multi-GB RSS blowups), `rgctl` factors the CPG into **`L_repo`** (`graph.snapshot.bin`) and **`L_proc`** (`cfg_pdg.archive.bin` + `field_write.index.bin`) joined lazily in **`L_query`**. |
| **LLM-Guided CPG Slicing & Mutation Proofs** | *Codebadger: Bridging Code Property Graphs and Language Models* (ICSE 2026); *CodexGraph* (NAACL 2025) | Exposes high-signal JSON verbs (`rgctl cpg mutations`, `rgctl cpg flows`, `rgctl cpg calls`) so LLM agents prove immutability (e.g., converting a mutable DTO class to an immutable `record`) or trace data flows in 2–3 CLI turns without loading full source files into the context window. |
| **Soundness-Bounded Local Type Recovery for Field Writes** | Rapid Type Analysis (Bacon & Sweeney, 1996) + local symbol table resolution | During `--with-cfg` extraction, `field_write.rs` resolves the receiver `obj` in `obj.field = rhs` against enclosing class fields (`this.field`), formal `Parameter` type annotations, and local variable declarations (`let order: OrderDTO = ...`), classifying unresolved dynamic receivers into an explicit `Unresolved` bucket (`--include-unresolved`). |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 `.rgctl/analysis/` Hybrid CPG Artifact Suite

| Artifact Path | Magic / Version | When Written | Contents & Lookup Complexity |
| :--- | :--- | :--- | :--- |
| `.rgctl/graph.snapshot.bin` | `RGCL` v2 | Always (`discover`) | `L_repo` macro topology (`NodeRow`, `EdgeRow`, CSR). Unchanged by CPG flags. |
| `.rgctl/analysis/cfg_pdg.archive.bin` | `RBCP` v3 (`TOC3`) | Eager (`--with-cfg`) or Lazy (`cpg flows` / `cpg slice`) | Contiguous `bincode` `CfgPdgRecord` payloads + 8-byte aligned sorted 32-byte `TocEntry` footer (`O(1)` mmap open, `O(log F)` binary search). |
| `.rgctl/analysis/field_write.index.bin` | `RBFW` v1 | `discover --with-cfg` | Inverted index mapping `receiver_type -> Vec<FieldWriteFact>` and `function_id -> Vec<FieldWriteFact>` for $O(1)$ type mutation queries. |
| `.rgctl/analysis/ast_skeleton.archive.bin` | `RBAS` v1 | `discover --with-ast-skeleton` | Coarse per-function syntax skeleton (declaration, block, branch, loop, call, assignment nodes) without leaf punctuation tokens. |

### 3.2 Typed Field-Write Fact (`FieldWriteFact`)

```rust
pub struct FieldWriteFact {
    pub function_id: Uuid,
    pub function_name: String,
    pub is_constructor: bool,
    pub receiver_local: Option<String>,
    pub receiver_type: Option<String>,
    pub member: String,
    pub file_path: String,
    pub line: usize,
    pub code_snippet: String,
    pub kind: FieldWriteKind, // DirectField | ThisField | Unresolved
}
```

- **Tier-1 Language Parity (Layer F):** Field-write extraction and local receiver type recovery are implemented and tested across all Tier-1 languages (**Java, C#, Go, C, C++, TypeScript, JavaScript, Python, Rust, PHP, Ruby**).

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Subcommands (`rgctl cpg`)

Verified against `rgctl cpg --help` and [`src/cli/cpg.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/cpg.rs#L19-L80):

```bash
# 1. Check L_repo and L_proc archive status
rgctl cpg status -f json

# 2. Inspect L_repo function node + L_proc availability
rgctl cpg function OrderProcessor::process -f json

# 3. Query incoming/outgoing CALLS neighborhood for a function
rgctl cpg calls OrderProcessor::process -f json

# 4. Prove whether any non-constructor method mutates OrderDTO fields (requires `discover --with-cfg`)
rgctl discover . --with-cfg
rgctl cpg mutations OrderDTO --exclude-ctors -f json
rgctl cpg mutations OrderDTO --exclude-ctors --member status --include-unresolved -f json

# 5. Trace forward/backward data + control flows (lazily populates cfg_pdg.archive.bin if missing!)
rgctl cpg flows --file src/OrderProcessor.java --line 114 --variable order --direction forward -f json
rgctl cpg flows --file src/OrderProcessor.java --line 114 --variable order --with-alias -f json

# 6. Inspect PDG, slice, or coarse AST skeleton through the CPG facade
rgctl cpg pdg OrderProcessor::process --edge-layer data --def-use -f json
rgctl cpg slice src/OrderProcessor.java --line 114 --variable order -f json
rgctl cpg ast OrderProcessor::process -f json

# 7. Export scoped CPG (L_repo + optional L_proc / field writes) to GraphML or GraphSON
rgctl cpg export --format graphml --output /tmp/order_cpg.graphml --path-contains order --include-l-proc --include-field-writes
```

### 4.2 JSON Schema (`rgctl cpg mutations <TYPE> --exclude-ctors -f json`)

```json
{
  "schema_version": "1.0",
  "type_name": "OrderDTO",
  "exclude_ctors": true,
  "member_filter": null,
  "mutation_count": 1,
  "unresolved_count": 0,
  "mutations": [
    {
      "function_id": "9f8c2b10-4a1e-5d32-8c11-0a2b3c4d5e6f",
      "function_name": "OrderProcessor::process",
      "is_constructor": false,
      "receiver_local": "order",
      "receiver_type": "OrderDTO",
      "member": "status",
      "file": "src/OrderProcessor.java",
      "line": 114,
      "code_snippet": "order.status = \"PROCESSED\";",
      "kind": "DirectField"
    }
  ]
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Virtual CPG Query Engine** | [`crates/rgctl-analysis/src/cpg.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cpg.rs) | `cpg_status`, `cpg_function`, `cpg_calls`, `cpg_mutations`, `cpg_flows`, `export_cpg` |
| **Field-Write Index & Type Recovery** | [`crates/rgctl-analysis/src/field_write.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/field_write.rs) | `FieldWriteFact`, `FieldWriteIndex`, `MutationQuery` |
| **Coarse AST Skeleton Archive** | [`crates/rgctl-analysis/src/ast_skeleton.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/ast_skeleton.rs) | `AstSkeletonArchive`, `AstSkeletonNode` |
| **v3 CFG/PDG Archive** | [`crates/rgctl-analysis/src/cfg_pdg_archive.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_pdg_archive.rs) | `CfgPdgArchiveMmap`, `ensure_cfg_pdg_for_source` |
| **CLI & HTTP Service Dispatch** | [`src/cli/cpg.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/cpg.rs) | `CpgAction`, `run`, `cpg_action_to_service` (`POST /api/cpg`) |

---

## 5. Performance Guardrails & Verification Checklist

1. **Zero Default-Discover Impact:** Default `rgctl discover .` never builds `cfg_pdg.archive.bin`, `field_write.index.bin`, or `ast_skeleton.archive.bin`, preserving Linux-kernel cold discover wall time and `graph.snapshot.bin` digest stability.
2. **Lazy Single-Function Flows vs. Whole-Repo Mutations:** `rgctl cpg flows` and `rgctl cpg slice` work immediately after a default `rgctl discover .` by lazily appending single functions to `cfg_pdg.archive.bin`. Only repo-wide `rgctl cpg mutations` requires `rgctl discover --with-cfg`.

### Verification Commands

```bash
# Run Hybrid CPG and field-write mutation unit & integration tests
cargo test -p rgctl-analysis cpg
cargo test -p rgctl-analysis field_write
cargo test --test subprocess_golden_path
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Soundness-Bounded Local Receiver Type Resolution:** Receiver types for `obj.field = ...` are recovered from local variable declarations, formal parameters, enclosing class fields, and imports without running a full compiler type-checker. Dynamic or unannotated receivers are placed in the `Unresolved` bucket (queryable via `--include-unresolved`).
2. **Reflection, Bytecode & Macro Codegen:** Reflective field mutations (`Field.setAccessible(true)`), JNI native writes, or Lombok bytecode-synthesized setters not present in source files are outside static tree-sitter CPG visibility.
