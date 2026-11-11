# Program Slicing — Engineering Design

This document specifies the **Intra- and Inter-Procedural Program Slicing subsystem** in `rgctl` (`crates/rgctl-analysis/src/slicing.rs`, `interprocedural_slicing.rs`, and `src/cli/slice.rs`). It covers backward, forward, taint-aware, and may-alias-expanded slicing over the Program Dependence Graph (PDG), automatic enclosing-function resolution from byte spans, and lazy single-function population of `.rgctl/analysis/cfg_pdg.archive.bin` (`RBCP` v3).

![Program Slicing tab — source editor and slice controls (gbuilder)](../images/design/program-slicing/slice-editor.png)

*Figure 1: **Program Slicing** tab — function picker, line/variable/direction inputs, and CodeMirror source view with highlighted slice lines.*

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. CRITERION RESOLUTION (`src/cli/slice.rs`)                                 │
│                                                                              │
│  Input: `(file, --line, --variable, [--function], --direction, [--with-alias])`│
│    ├─► If `--function` omitted: `index_function_locations` + half-open span  │
│    │     `start_byte <= line_offset < end_byte` selects innermost function   │
│    └─► `ensure_cfg_pdg_for_source`:                                          │
│          • Checks `.rgctl/analysis/cfg_pdg.archive.bin` (`RBCP` v3 TOC)      │
│          • If missing/stale: parses `file`, builds CFG + `(name, scope_id)`  │
│            PDG, and atomically appends a new 32B `TocEntry` + `TOC3` trailer │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. PDG SLICE TRAVERSAL (`crates/rgctl-analysis/src/slicing.rs`)              │
│                                                                              │
│  • Locates seed `PdgNodeId` matching `(line, variable)`                      │
│  • Optional `--with-alias`: expands `variable` with `alias::may_alias_names` │
│    (local copy assignments and field-base aliases)                           │
│  • `BackwardSlicer` (`--direction backward`):                                │
│      Reverse BFS across incoming `DataDependency` + `ControlDependency` edges│
│  • `ForwardSlicer` (`--direction forward`):                                  │
│      Forward BFS across outgoing `DataDependency` + `ControlDependency` edges│
│  • Optional `--taint`:                                                       │
│      Delegates to `TaintAnalyzer` to trace source -> sink paths & sanitizers │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. OUTPUT VIEWS & CONSUMERS                                                  │
│                                                                              │
│  • `--view text` (default): ordered source lines + statement snippets        │
│  • `--view cfg` / `--view pdg`: projected CFG/PDG topology for sliced nodes  │
│  • `rgctl blast-radius --with-slices`: cross-function call-site handoff      │
│  • Dashboard `SliceView.tsx`: interactive CodeMirror line highlighting       │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Executable Backward & Forward Program Slicing** | Weiser (1981), *Program Slicing* (*ICSE '81*); Ottenstein & Ottenstein (1984), *The Program Dependence Graph in a Software Development Environment* | [`compute_slice_with_options`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/slicing.rs#L1-L260) reduces Weiser's dataflow equations to graph reachability over `ProgramDependenceGraph` (`data_deps` $\cup$ `control_deps`) starting at criterion $C = (\text{line}, \text{variable})$. |
| **Inter-Procedural System Dependence Graph (SDG) Slicing** | Horwitz, Reps & Binkley (1990), *Interprocedural Slicing Using Dependence Graphs* (*ACM TOPLAS*, 12(1)) | [`InterproceduralCFG`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/interprocedural_slicing.rs#L1-L240) and [`blast_slice_handoff.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/blast_slice_handoff.rs#L1-L150) bind call-site actual arguments to callee formal `Parameter` PDG nodes (`scope_id = 0`) and return statements back to caller call sites. |
| **Demand-Driven Intra-Procedural May-Alias Expansion** | Steensgaard (1996); lightweight copy/reference propagation | When `--with-alias` is set, [`alias::may_alias_names`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/alias.rs#L1-L140) computes the transitive closure of local alias copies (`let b = &a`, `b.field = ...`) within the function and seeds the slice with all aliased variable names. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Core Slicing Types (`crates/rgctl-analysis/src/slicing.rs`)

```rust
pub struct SliceCriterion {
    pub file_path: String,
    pub function_name: String,
    pub line: usize,
    pub variable: String,
    pub direction: SliceDirection, // Backward | Forward
}

pub struct SliceOptions {
    pub include_control: bool,
    pub max_depth: Option<usize>,
    pub alias_names: Vec<String>,
}

pub struct ProgramSlice {
    pub criterion: SliceCriterion,
    pub lines: Vec<usize>,
    pub statements: Vec<SlicedStatement>,
    pub variables_involved: Vec<String>,
    pub confidence: f64,
}
```

### 3.2 Innermost Function Span Resolution & Lazy v3 Archive Append

1. **Half-Open Byte-Span Function Lookup (`function_containing_line`):**
   - When `--function` is omitted, [`src/cli/slice.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/slice.rs#L49-L58) converts `--line` to its 0-based byte offset (`line_start_byte`) and selects the function with the smallest half-open span `start_byte <= offset < end_byte`. This accurately disambiguates closures, nested functions, and adjacent methods.
2. **Lazy Archive Caching (`ensure_cfg_pdg_for_source`):**
   - `rgctl slice` does **not** require a prior `rgctl discover --with-cfg`. If `.rgctl/analysis/cfg_pdg.archive.bin` does not exist or lacks the target function's `Uuid` / `stable_record_uuid`, `ensure_cfg_pdg_for_source` parses only that file, builds the CFG and PDG, and appends the `CfgPdgRecord` + updated 32-byte `TocEntry` footer (`TOC3`).

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl slice --help` and [`src/cli/mod.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/mod.rs#L623-L653):

```bash
# Backward slice on variable `cart` at line 42 (auto-resolves enclosing function; lazily caches CFG/PDG)
rgctl slice src/OrderService.java --line 42 --variable cart -f json

# Explicit enclosing function + forward slice
rgctl slice src/OrderService.java --line 18 --variable order --function createOrder --direction forward -f json

# Expand criterion with intra-procedural may-alias tracking (copies / reference bases)
rgctl slice src/OrderService.java --line 42 --variable cart --with-alias -f json

# Taint-aware slice from criterion
rgctl slice src/OrderService.java --line 20 --variable userInput --taint -f json

# Project slice onto CFG or PDG topology view
rgctl slice src/OrderService.java --line 42 --variable cart --view pdg -f json
```

### 4.2 JSON Schema (`rgctl slice <file> --line <N> --variable <V> -f json`)

Implemented in [`src/cli/slice_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/slice_output.rs):

```json
{
  "schema_version": "1.0",
  "file": "src/OrderService.java",
  "function": "checkout",
  "line": 42,
  "variable": "cart",
  "direction": "backward",
  "view": "text",
  "lines": [14, 19, 27, 42],
  "statements": [
    {
      "line": 14,
      "text": "Cart cart = cartRepository.findById(cartId);",
      "kind": "Assignment",
      "defs": ["cart"],
      "uses": ["cartRepository", "cartId"]
    },
    {
      "line": 42,
      "text": "paymentGateway.charge(cart.getTotal());",
      "kind": "Call",
      "defs": [],
      "uses": ["paymentGateway", "cart"]
    }
  ]
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Intra-Procedural Slicer** | [`crates/rgctl-analysis/src/slicing.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/slicing.rs) | `SliceCriterion`, `SliceDirection`, `SliceOptions`, `ProgramSlice`, `compute_slice_with_options` |
| **Inter-Procedural Slicer & Handoff** | [`crates/rgctl-analysis/src/interprocedural_slicing.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/interprocedural_slicing.rs), [`blast_slice_handoff.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/blast_slice_handoff.rs) | `InterproceduralCFG`, `compute_interprocedural_slice`, `trace_blast_slices` |
| **May-Alias Helper** | [`crates/rgctl-analysis/src/alias.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/alias.rs) | `may_alias_names` |
| **Lazy Archive Builder** | [`crates/rgctl-analysis/src/cfg_pdg_archive.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_pdg_archive.rs) | `ensure_cfg_pdg_for_source`, `EnsureCfgPdgOpts`, `index_function_locations` |
| **CLI Slice Command** | [`src/cli/slice.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/slice.rs), [`src/cli/slice_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/slice_output.rs) | `SliceArgs`, `run`, `function_containing_line`, `text_slice_json`, `taint_slice_json` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Zero Whole-Repo Overhead for Single-Function Slicing:** `rgctl slice` must never trigger a whole-repository CFG/PDG build; it parses only the target file and appends one `CfgPdgRecord` to `.rgctl/analysis/cfg_pdg.archive.bin`.
2. **Actionable Criterion Diagnostics:** When `(line, variable)` matches no PDG node in the resolved function, `name_the_function_searched` reports the exact function searched (and whether it came from `--function` or `file:line` span resolution) plus all candidate functions defined in that file.

### Verification Commands

```bash
# Run unit and integration slicing tests
cargo test -p rgctl-analysis slicing
cargo test --test slicing
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Dynamic Dispatch & Reflection:** Slicing follows static AST def-use and call edges; reflective field writes or dynamic `eval` invocations are not part of the PDG.
2. **Inter-Procedural Handoff Depth:** `rgctl blast-radius <symbol> --with-slices` bounds cross-function PDG slice handoffs to direct caller/callee argument bindings to prevent exponential context explosion on deep call graphs.
