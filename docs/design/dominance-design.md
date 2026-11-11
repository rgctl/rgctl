# Dominance & Post-Dominance — Engineering Design

This document specifies the **Dominator Tree, Post-Dominator Tree, and Dominance Frontier subsystem** in `rgctl` (`crates/rgctl-analysis/src/dominance.rs`). It details how immediate dominators (`idom`), reverse-CFG post-dominators (`pdom`), and dominance frontiers (`DF`) are computed over each function's `ControlFlowGraph` to drive Program Dependence Graph (PDG) control dependencies, SESE hammock clone detection, and `rgctl inspect <symbol> dom`.

![Dominator tree view (gbuilder)](../images/design/dominance/dominance-tree.png)

*Figure 1: **Dataflow** tab with view mode **Dominator Tree** — immediate dominator hierarchy and statement frontier sizes.*

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. INPUT CONTROL-FLOW GRAPH (`ControlFlowGraph`)                             │
│                                                                              │
│  • Forward CFG (`entry` → `blocks` → `edges` → `exit`)                       │
│  • Reverse CFG (transpose edges rooted at `exit`, with virtual edges from    │
│    infinite loops / non-returning blocks to preserve reachability)           │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. DOMINANCE & POST-DOMINANCE ENGINE (`crates/rgctl-analysis/src/dominance.rs`)│
│                                                                              │
│  A. Reverse Post-Order (RPO) Numbering from `entry`                          │
│  B. Cooper–Harvey–Kennedy Iterative Fixed-Point (`DominatorTree::build`):    │
│     • Computes `idom: HashMap<BlockId, BlockId>`                             │
│     • Computes `dom_tree_children: HashMap<BlockId, Vec<BlockId>>`           │
│     • Computes `frontiers: HashMap<BlockId, HashSet<BlockId>>`               │
│  C. Reverse-CFG Post-Dominance (`PostDominatorTree::build`):                 │
│     • Computes `ipdom: HashMap<BlockId, BlockId>` on reverse CFG             │
│     • Derives control-dependence edges (`u --CD--> v`) for `PdgBuilder`      │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. DOWNSTREAM CONSUMERS                                                      │
│                                                                              │
│  • `PdgBuilder` (`pdg.rs`)           → Control dependence edges (`Control`)  │
│  • `fragment_clones.rs`              → SESE hammock region detection         │
│  • `rgctl inspect <symbol> dom`      → JSON / Mermaid / text dominator tree  │
│  • Dashboard `DataflowView.tsx`      → Interactive Dominator Tree mode       │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Simple, Fast Iterative Dominance (CHK)** | Cooper, Harvey & Kennedy (2001), *A Simple, Fast Dominance Algorithm* (*Software: Practice and Experience*, 31(4)) | [`DominatorTree::build`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/dominance.rs#L45-L185) numbers reachable CFG blocks in Reverse Post-Order (RPO) and iteratively intersects immediate dominators along RPO fingers (`intersect(b1, b2)`) until fixed-point convergence. |
| **Dominance Frontiers ($DF$)** | Cytron, Ferrante, Rosen, Wegman & Zadeck (1991), *Efficiently Computing Static Single Assignment Form and the Control Dependence Graph* | For every join block $b$ with $|\text{preds}(b)| \ge 2$, a runner walks up the dominator tree from each predecessor $p \in \text{preds}(b)$ until reaching $\text{idom}(b)$, inserting $b$ into $DF(runner)$. |
| **Post-Dominance & Control Dependence** | Ferrante, Ottenstein & Warren (1987), *The Program Dependence Graph and Its Use in Optimization* | [`PostDominatorTree::build`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/dominance.rs#L190-L310) runs CHK dominance on the reverse CFG rooted at `cfg.exit`. For every branch edge $(A \to B)$ where $B$ does not post-dominate $A$, all nodes on the post-dominator tree path from $B$ up to (excluding) $\text{ipdom}(A)$ are control-dependent on $A$. |
| **Single-Entry Single-Exit (SESE) Hammocks** | Johnson, Pearson & Pingali (1994), *The Program Structure Tree: Computing Control Regions in Linear Time* | [`fragment_clones.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/fragment_clones.rs#L1-L140) pairs block $p$ dominates $q$ with $q$ post-dominates $p$ to extract self-contained SESE hammock subgraphs for 1-WL structural clone detection (`rgctl clones --mode fragment`). |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Core Rust Types (`crates/rgctl-analysis/src/dominance.rs`)

```rust
pub struct DominatorTree {
    pub root: BlockId,
    /// Immediate dominator map: block -> idom(block)
    pub idom: HashMap<BlockId, BlockId>,
    /// Dominator tree children: parent -> Vec<child>
    pub children: HashMap<BlockId, Vec<BlockId>>,
    /// Dominance frontier for each block (join points where dominance stops)
    pub frontiers: HashMap<BlockId, HashSet<BlockId>>,
}

pub struct PostDominatorTree {
    pub root: BlockId,
    /// Immediate post-dominator map: block -> ipdom(block)
    pub ipdom: HashMap<BlockId, BlockId>,
    /// Post-dominator tree children
    pub children: HashMap<BlockId, Vec<BlockId>>,
}
```

### 3.2 Complexity & Residency Invariants

- **Time Complexity:** $O(B + E)$ in practice on structured and irreducible intra-procedural CFGs (where $B$ is basic-block count, typically $5 \le B \le 250$ per function).
- **Zero Persistent Overhead:** `DominatorTree` is computed in microseconds from `ControlFlowGraph` and its resulting control-dependence edges are persisted directly inside `ProgramDependenceGraph` in `.rgctl/analysis/cfg_pdg.archive.bin` (`RBCP` v3), avoiding redundant on-disk dominator tables.

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl inspect --help` and [`src/cli/inspect.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect.rs#L95-L120):

```bash
# Inspect immediate dominator tree for a function
rgctl inspect <symbol> dom

# Include dominance frontier sets for each basic block
rgctl inspect <symbol> dom --frontiers -f json

# Export dominator tree as a Mermaid diagram
rgctl inspect <symbol> dom --frontiers -f mermaid
```

### 4.2 JSON Schema (`rgctl inspect <symbol> dom --frontiers -f json`)

Implemented in [`src/cli/inspect_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect_output.rs):

```json
{
  "schema_version": "1.0",
  "symbol": "validateRequest",
  "layer": "dom",
  "start_byte": 120,
  "end_byte": 890,
  "name_byte_span": [127, 142],
  "root_block": 0,
  "nodes": [
    {
      "block_id": 0,
      "kind": "Entry",
      "idom": null,
      "children": [2],
      "frontier": []
    },
    {
      "block_id": 2,
      "kind": "Branch",
      "idom": 0,
      "children": [3, 4, 5],
      "frontier": [5]
    }
  ]
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Dominator & Post-Dominator Engine** | [`crates/rgctl-analysis/src/dominance.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/dominance.rs) | `DominatorTree`, `PostDominatorTree`, `intersect`, `compute_frontiers` |
| **PDG Control Dependence Lowering** | [`crates/rgctl-analysis/src/pdg.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/pdg.rs) | `ProgramDependenceGraph::build`, `compute_control_dependencies` |
| **SESE Hammock Region Extraction** | [`crates/rgctl-analysis/src/fragment_clones.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/fragment_clones.rs) | `extract_sese_regions` |
| **CLI Inspect Command** | [`src/cli/inspect.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect.rs), [`src/cli/inspect_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect_output.rs) | `InspectLayer::Dom`, `inspect_dom_json`, `dom_to_mermaid` |
| **Dashboard Dominator View** | [`dashboard/src/DataflowView.tsx`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/dashboard/src/DataflowView.tsx), [`dashboard/src/dataflowEngine.ts`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/dashboard/src/dataflowEngine.ts) | `computeDominatorGraph` |

---

## 5. Performance Guardrails & Verification Checklist

1. **RPO Finger Intersection:** Never materialize bitset dominator matrices ($O(B^2)$ memory); always use the RPO array + two-finger `intersect(b1, b2)` loop from Cooper–Harvey–Kennedy.
2. **Unreachable Block Handling:** Unreachable basic blocks (e.g., dead code after `return`) are skipped during RPO numbering so `DominatorTree::build` never enters an infinite loop on disconnected CFG components.

### Verification Commands

```bash
# Run dominance, post-dominance, and frontier unit tests
cargo test -p rgctl-analysis dominance

# Verify CLI inspect dom JSON and Mermaid output
cargo test --test subprocess_golden_path
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Non-Terminating Loop Virtual Exit Edges:** When a function contains an infinite loop (`loop {}` or `while(true)` without `break`/`return`) that cannot reach `cfg.exit` in the forward CFG, reverse-CFG post-dominance attaches a virtual edge from the loop sink to `exit` so blocks inside the loop still receive well-defined post-dominators.
2. **Intra-Procedural Scope:** Dominance and post-dominance operate strictly over a single function's `ControlFlowGraph`; cross-function call dependencies are modeled via `InterproceduralCFG` and `BlastRadiusEngine`.
