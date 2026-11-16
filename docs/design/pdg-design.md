# Program Dependence Graph (PDG) — Engineering Design

This document specifies the **Program Dependence Graph (PDG) and Reaching-Definitions Dataflow subsystem** in `rgctl` (`crates/rgctl-analysis/src/pdg.rs`, `dataflow.rs`, and `cfg_pdg_archive.rs`). It details how control dependencies (from post-dominance) and lexical `(name, scope_id)` data dependencies are combined into statement-level PDGs stored in `.rgctl/analysis/cfg_pdg.archive.bin` (`RBCP` v3).

![Dataflow tab — PDG graph (gbuilder)](../images/design/pdg/pdg-dataflow.png)

*Figure 1: **Dataflow** tab in PDG mode — variable selector, control/data edge toggles, and Sigma.js PDG layout.*

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. INPUT CFG & LEXICAL STATEMENT IR (`ControlFlowGraph`)                     │
│                                                                              │
│  • `BasicBlock` statements with `(var_name, scope_id)` `defs` and `uses`     │
│  • Formal parameters seeded in `Entry` block at `scope_id = 0`               │
└───────────────────┬──────────────────────────────────────┬───────────────────┘
                    │                                      │
                    ▼                                      ▼
┌───────────────────────────────────────┐  ┌───────────────────────────────────┐
│ 2A. POST-DOMINANCE CONTROL EDGES      │  │ 2B. REACHING DEFINITIONS DATAFLOW │
│ (`dominance.rs` + `pdg.rs`)           │  │ (`dataflow.rs`)                   │
│                                       │  │                                   │
│ • Reverse-CFG `PostDominatorTree`     │  │ • Iterative chaotic fixed-point:  │
│ • Branch edge (A -> B) where B does   │  │   `IN[b] = U_{p in pred(b)} OUT[p]`│
│   not post-dominate A emits           │  │   `OUT[b] = GEN[b] U (IN[b]-KILL)`│
│   `ControlDependency { from, to,      │  │ • Keys definitions by lexical     │
│   branch_kind }`                      │  │   `(name, scope_id)`              │
└───────────────────┬───────────────────┘  └───────────────┬───────────────────┘
                    │                                      │
                    └──────────────────┬───────────────────┘
                                       ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. PROGRAM DEPENDENCE GRAPH (`ProgramDependenceGraph`, `pdg.rs`)             │
│                                                                              │
│  • `nodes: Vec<PdgNode>` (statement line, column, text, defs, uses, kind)    │
│  • `data_deps: Vec<DataDependency>` (`from`, `to`, `variable`, `loop_carried`)│
│  • `control_deps: Vec<ControlDependency>` (`from`, `to`, `branch`)           │
│  • Persisted in `.rgctl/analysis/cfg_pdg.archive.bin` (`RBCP` v3)            │
└──────────────────────────────────────┬───────────────────────────────────────┘
                                       │
                                       ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 4. CONSUMERS: SLICING, CPG FLOWS, TAINT & CLI INSPECT                        │
│                                                                              │
│  • `rgctl inspect <symbol> pdg`  • `rgctl slice`  • `rgctl cpg flows`        │
│  • `rgctl taint`                 • `rgctl clones --mode fragment`            │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Program Dependence Graph (PDG)** | Ferrante, Ottenstein & Warren (1987), *The Program Dependence Graph and Its Use in Optimization* (*ACM TOPLAS*, 9(3)) | [`ProgramDependenceGraph::build`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/pdg.rs#L1-L240) unifies statement-level control dependence edges and def-use data dependence edges into a single directed multigraph over `PdgNode` vertices. |
| **Reaching Definitions Dataflow Equations** | Kildall (1973), *A Unified Approach to Global Program Optimization*; Dragon Book §9.2 | [`DataflowAnalyzer`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/dataflow.rs#L1-L210) solves the forward monotone dataflow equations $\text{IN}[b] = \bigcup_{p \in \text{pred}(b)} \text{OUT}[p]$ and $\text{OUT}[b] = \text{GEN}[b] \cup (\text{IN}[b] \setminus \text{KILL}[b])$ over basic blocks, evaluating intra-block statements in execution order (RHS uses resolved against incoming reaching definitions before LHS kill/gen updates). |
| **Lexical Scope Disambiguation (`(name, scope_id)`)** | Static Single Assignment (SSA) / Alpha-renaming (Cytron et al., 1991) | Each variable definition is keyed by `(String, u32)` (`(variable_name, lexical_scope_id)`). A shadowed declaration `let x = ...` in an inner block (`scope_id = 2`) does not kill or alias the outer `x` (`scope_id = 0`), while mutations `x = x + 1` resolve to the innermost active `scope_id` for `x`. |
| **Loop-Carried Data Dependence Classification** | Allen & Kennedy (2001), *Optimizing Compilers for Modern Architectures* | When `PdgBuildOptions::classify_loop_carried` (`rgctl discover --with-dfg-loops`) is enabled, `PdgBuilder` marks `DataDependency.loop_carried = true` whenever the use block can reach the definition block along a CFG cycle containing a `LoopBack` edge. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Core PDG Types (`crates/rgctl-analysis/src/pdg.rs`)

```rust
pub struct ProgramDependenceGraph {
    pub function_name: String,
    pub nodes: Vec<PdgNode>,
    pub data_deps: Vec<DataDependency>,
    pub control_deps: Vec<ControlDependency>,
}

pub struct DataDependency {
    pub from: PdgNodeId,
    pub to: PdgNodeId,
    pub variable: String,
    pub kind: DataDepKind, // Flow (def -> use), Anti, Output
    #[serde(default)]
    pub loop_carried: bool,
}

pub struct ControlDependency {
    pub from: PdgNodeId,
    pub to: PdgNodeId,
    pub branch: bool, // true = TrueBranch, false = FalseBranch
}
```

### 3.2 Storage in `.rgctl/analysis/cfg_pdg.archive.bin` (`RBCP` v3)

- Every [`CfgPdgRecord`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_pdg_archive.rs#L52-L75) in `cfg_pdg.archive.bin` bundles `cfg: Arc<ControlFlowGraph>` and `pdg: Arc<ProgramDependenceGraph>` in a single `bincode` payload indexed by the 32-byte `TocEntry` footer (`TOC_ENTRY_SIZE = 32`, `TOC_TRAILER_SIZE = 16`).
- **On-Demand Single-Function Build (`ensure_cfg_pdg_for_source`):** Running `rgctl slice` or `rgctl cpg flows` on a repository discovered without `--with-cfg` builds only the target function's CFG + PDG and appends it to `cfg_pdg.archive.bin`.

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl inspect --help` and [`src/cli/inspect.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect.rs#L71-L94):

```bash
# Inspect full PDG (both data and control dependencies)
rgctl inspect <symbol> pdg -f json

# Filter to data-only or control-only dependence edges
rgctl inspect <symbol> pdg --edge-layer data -f json
rgctl inspect <symbol> pdg --edge-layer control -f json

# Include detailed per-variable def-use chains
rgctl inspect <symbol> pdg --def-use -f json

# Discover with loop-carried data dependence classification enabled
rgctl discover . --with-cfg --with-dfg-loops
```

### 4.2 JSON Schema (`rgctl inspect <symbol> pdg --def-use -f json`)

Implemented in [`src/cli/inspect_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect_output.rs):

```json
{
  "schema_version": "1.0",
  "symbol": "calculateTotal",
  "layer": "pdg",
  "start_byte": 210,
  "end_byte": 640,
  "name_byte_span": [219, 233],
  "node_count": 8,
  "data_dep_count": 6,
  "control_dep_count": 4,
  "nodes": [
    {
      "id": 0,
      "line": 12,
      "kind": "Parameter",
      "text": "items",
      "defs": ["items"],
      "uses": []
    }
  ],
  "data_deps": [
    {
      "from": 0,
      "to": 3,
      "variable": "items",
      "loop_carried": false
    }
  ],
  "control_deps": [
    {
      "from": 2,
      "to": 4,
      "branch": true
    }
  ]
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **PDG Builder & Graph** | [`crates/rgctl-analysis/src/pdg.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/pdg.rs) | `ProgramDependenceGraph`, `PdgNode`, `DataDependency`, `ControlDependency`, `PdgBuildOptions` |
| **Reaching Definitions Dataflow** | [`crates/rgctl-analysis/src/dataflow.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/dataflow.rs), [`def_use.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/def_use.rs) | `DataflowAnalyzer`, `ReachingDefinitions`, `DefUseChain` |
| **Local May-Alias Expansion** | [`crates/rgctl-analysis/src/alias.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/alias.rs) | `may_alias_names` (invoked when `--with-alias` is passed to `slice` or `cpg flows`) |
| **v3 Archive Persistence** | [`crates/rgctl-analysis/src/cfg_pdg_archive.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_pdg_archive.rs) | `CfgPdgRecord`, `CfgPdgArchiveMmap`, `ensure_cfg_pdg_for_source` |
| **CLI Inspect Command** | [`src/cli/inspect.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect.rs), [`src/cli/inspect_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect_output.rs) | `InspectLayer::Pdg`, `PdgEdgeLayer`, `inspect_pdg_json` |

---

## 5. Performance Guardrails & Verification Checklist

1. **BitSet / Compact Set Dataflow:** Reaching definitions within a function operate over compact definition indices (`0..D`) rather than cloning `HashMap<String, HashSet<Statement>>` at every basic-block transfer step.
2. **Opt-In Loop-Carried Reachability (`--with-dfg-loops`):** Computing CFG back-edge reachability for every def-use pair is gated behind `PdgBuildOptions::classify_loop_carried` so default `--with-cfg` discovery avoids extra $O(B^2)$ reachability checks.

### Verification Commands

```bash
# Run PDG and reaching-definition dataflow unit tests
cargo test -p rgctl-analysis pdg
cargo test -p rgctl-analysis dataflow

# Run integration slicing tests that exercise PDG data + control edges
cargo test --test slicing
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Lexical Scope Dataflow vs. Heap Pointer Aliasing:** Default PDG construction keys definitions by lexical `(name, scope_id)`. Intra-procedural copy/reference aliasing (`let alias = &mut x`) is expanded on demand when `--with-alias` is passed to `rgctl slice` or `rgctl cpg flows` (`alias::may_alias_names`), rather than running whole-program points-to analysis during discover.
2. **Cross-Function Field Flows Use `FieldWriteIndex`:** Mutations to struct/class fields across methods (`this.balance += x` read in `getBalance()`) are tracked via the Hybrid CPG `FieldWriteIndex` (`rgctl cpg mutations`) rather than intra-procedural PDG edges.
