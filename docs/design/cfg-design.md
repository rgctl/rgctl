# Control-Flow Graph (CFG) — Engineering Design

This document specifies the **per-function Control-Flow Graph (CFG) subsystem** in `rgctl` (`crates/rgctl-analysis/src/cfg.rs`, `cfg_builder.rs`, and `cfg_pdg_archive.rs`). It covers single-parse AST lowering into basic blocks and typed branch edges, parameter and signature initialization, the v3 indexed `.rgctl/analysis/cfg_pdg.archive.bin` binary layout, CLI inspection contracts, and language-specific lowering rules.

![CFG tab — blocks and Sigma graph (gbuilder)](../images/design/cfg/cfg-control-flow.png)

*Figure 1: **CFG / PDG Analysis** tab — function list, basic-block table, and interactive Sigma.js CFG layout.*

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. SINGLE-PARSE OR ON-DEMAND AST LOWERING (`cfg_builder.rs`)                 │
│                                                                              │
│  A. Eager (`rgctl discover . --with-cfg`):                                   │
│     • `extract_with_cfg_pdg` reuses the worker's in-memory `tree_sitter::Tree`│
│     • Lowers every `Function` / `Method` / `Constructor` in a single parse   │
│                                                                              │
│  B. Lazy On-Demand (`rgctl slice` / `rgctl cpg flows` / `rgctl inspect`):    │
│     • `ensure_cfg_pdg_for_source` parses only the target file and appends    │
│       the function's `CfgPdgRecord` to `.rgctl/analysis/cfg_pdg.archive.bin` │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. CFG IR CONSTRUCTION (`crates/rgctl-analysis/src/cfg.rs`)                  │
│                                                                              │
│  `ControlFlowGraph`:                                                         │
│    ├─► `Entry` block (`BlockId(0)`): seeds formal parameters (`Parameter`    │
│    │     defs at `(param_name, scope_id = 0)`)                               │
│    ├─► `BasicBlock` table: `statements: Vec<Statement>` with lexical         │
│    │     `ScopeStack` (`scope_id`) and def/use variable sets                 │
│    ├─► `CfgEdge` list: `Unconditional`, `TrueBranch`, `FalseBranch`,         │
│    │     `LoopBack`, `SwitchCase(String)`, `Exception`, `Finally`            │
│    └─► `Exit` block: unified post-dominator sink for `return` / `throw`      │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. V3 MMAP ARCHIVE & DOWNSTREAM CONSUMERS (`cfg_pdg_archive.rs`)             │
│                                                                              │
│  `.rgctl/analysis/cfg_pdg.archive.bin` (`RBCP` v3 + `TOC3` footer):          │
│    ├─► `DominatorTree::build(&cfg)`   → Immediate dominators & frontiers     │
│    ├─► `ProgramDependenceGraph::build`→ Control + `(name, scope_id)` data    │
│    ├─► `rgctl inspect <symbol> cfg`   → JSON / Mermaid / DOT / text dump     │
│    └─► `rgctl-dashboard` (`cfg_index.json` + lazy `archive_only` detail)     │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Maximal Basic Blocks & Leader Partitioning** | Aho, Lam, Sethi & Ullman (2006), *Compilers: Principles, Techniques, and Tools* (Dragon Book, §8.4) | [`CfgBuilder`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_builder.rs#L1-L260) splits linear statement sequences at branch conditions, merge targets, loop headers, and exception handlers into `BasicBlock` records with explicit `BlockKind` (`Entry`, `Normal`, `Branch`, `LoopHeader`, `LoopBody`, `TryBlock`, `CatchBlock`, `FinallyBlock`, `Exit`). |
| **Short-Circuit Boolean Evaluation Lowering** | Clarke & Emerson (1981); standard compiler IR lowering for `&&` / `\|\|` | `wire_condition` decomposes compound `&&` and `\|\|` expressions (and Rust `let_condition` chains) into intermediate conditional basic blocks with `TrueBranch` / `FalseBranch` edges so short-circuit control dependencies are preserved in the PDG. |
| **Lexical Scope-Indexed Statement IR** | Cytron et al. (1991), *Efficiently Computing Static Single Assignment Form and the Control Dependence Graph* | As `CfgBuilder` walks nested AST blocks, it maintains a monotonic `scope_id` stack (`ScopeStack`). Every variable definition (`defs`) and reference (`uses`) on a `Statement` is tagged with `(name, scope_id)` so inner variable shadowing never aliases outer declarations while RHS-before-LHS (`let x = x + 1`) resolves accurately. |
| **Structured Exception & Resource Unwind** | Gosling et al., *Java Language Specification* (§14.20); Rust `?` (`Try`) operator semantics | `try`/`catch`/`finally`, Java `try-with-resources` (synthetic reverse-order `.close()` calls), C# `using`/`lock`, and Rust `?` (`try_expression`) emit explicit `Exception` and `Finally` edges to handler blocks or the terminal `Exit` sink. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 In-Memory CFG Structures (`crates/rgctl-analysis/src/cfg.rs`)

```rust
pub struct ControlFlowGraph {
    pub function_name: String,
    pub file_path: Option<String>,
    pub start_line: usize,
    pub end_line: usize,
    pub entry: BlockId,
    pub exit: BlockId,
    pub blocks: Vec<BasicBlock>,
    pub edges: Vec<CfgEdge>,
    pub parameters: Vec<String>,
}
```

- **Parameter & Signature Binding:**
  - In the main graph (`graph.snapshot.bin`), a callable's full parameter list (`Vec<GraphParameter>` with `name`, `param_type`, `default_value`) lives in `NodeExtension.parameters` and its signature string in `NodeRow.signature_off`.
  - In `ControlFlowGraph`, formal parameter names are extracted from the function AST node's `parameters` child and recorded both in `ControlFlowGraph.parameters` and as initial `StatementKind::Parameter` definitions in the `Entry` block (`scope_id = 0`) so downstream reaching-definition analysis (`dataflow.rs`) and interprocedural slicing (`interprocedural_slicing.rs`) map caller arguments to callee parameters by position and name.

### 3.2 `RBCP` v3 Indexed Archive Layout (`.rgctl/analysis/cfg_pdg.archive.bin`)

Defined in [`crates/rgctl-analysis/src/cfg_pdg_archive.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_pdg_archive.rs#L28-L185):

```text
+-----------------------------------------------------------------------+
| Header: magic = b"RBCP" (4B) | version = 3 (u32 LE)                   |
|         has_digest (u8) | [digest_len (u32 LE) | digest_bytes]        |
+-----------------------------------------------------------------------+
| Contiguous bincode CfgPdgRecord Payloads                              |
|   Record 0: { function_id: Uuid, code_hash, function_name,            |
|               file_path, cfg: Arc<ControlFlowGraph>,                  |
|               pdg: Arc<ProgramDependenceGraph> }                      |
|   Record 1: ...                                                       |
+-----------------------------------------------------------------------+
| 8-Byte Alignment Padding                                              |
+-----------------------------------------------------------------------+
| Sorted 32-Byte TocEntry Array (toc_count * 32B, sorted by uuid)       |
|   [0..16]  uuid: [u8; 16]                                             |
|   [16..24] offset: u64 (LE)                                           |
|   [24..28] len: u32 (LE)                                              |
|   [28..32] flags: u32 (LE) (0 = PRIMARY, 1 = STABLE_ALIAS)            |
+-----------------------------------------------------------------------+
| 16-Byte EOF Trailer (TOC_TRAILER_SIZE = 16)                           |
|   [file_len - 16 .. file_len - 8] toc_offset: u64 (LE)                |
|   [file_len - 8  .. file_len - 4] toc_count: u32 (LE)                 |
|   [file_len - 4  .. file_len]     footer_magic: u32 (0x3354_4F43)     |
+-----------------------------------------------------------------------+
```

- **Dual UUID Addressing (`TOC_FLAG_PRIMARY` + `TOC_FLAG_STABLE_ALIAS`):** Every function record writes two 32-byte `TocEntry` rows pointing to the same `(offset, len)` payload: one keyed by the graph node's `Uuid` (`TOC_FLAG_PRIMARY = 0`) and one keyed by `stable_record_uuid(file_path, function_name)` (`TOC_FLAG_STABLE_ALIAS = 1`).
- **Lazy Atomic Append (`ensure_cfg_pdg_for_source`):** When `rgctl slice` or `rgctl cpg flows` targets a function not yet in `cfg_pdg.archive.bin`, `ensure_cfg_pdg_for_source` parses only that file, serializes the new `CfgPdgRecord` over the old TOC footer, and writes a newly sorted TOC + 16-byte `TOC3` trailer.

### 3.3 Multi-Language CFG Lowering Matrix

| Language | Lowered Control Constructs (`cfg_builder.rs`) |
| :--- | :--- |
| **Rust** | `if` / `if let` / `let_condition` chains, `&&`/`\|\|` short-circuit, `match` arms + guard conditions, `?` (`try_expression` → `TrueBranch` continue / `FalseBranch` early return), `for`/`while`/`while let`/`loop`, `'label:` break/continue (`breakable_stack`), `panic!`/`todo!`/`unimplemented!`/`unreachable!` → `Exception` exit, `.await` suspend split. |
| **Java** | `if`, `&&`/`\|\|` short-circuit, classic `for` (`continue` → update block) & `enhanced_for_statement`, `switch_statement` (implicit fallthrough) & `switch_expression` (`switch_rule` arrow arms), labeled `break`/`continue`, `try`/`catch`/`finally`, `try-with-resources` (reverse-order synthetic `.close()` before `finally`), `throw_statement`. |
| **Go** | `if` (with init statement), `for` (3-clause, condition-only, infinite, `range`), `switch` / `type_switch` (no implicit fallthrough unless `fallthrough` statement), `select` communication cases, labeled `break`/`continue`/`goto`, `defer` unwind before return, `panic` → `Exception`. |
| **C / C++** | `if` (including C++17 `condition_clause` initializer), `&&`/`\|\|`, `for` / C++ `for_range_loop`, `while` / `do-while`, `switch` / `case_statement` (implicit fallthrough), ternary `?:`, `goto` / forward labels, `abort`/`exit`/`_Exit`, C `setjmp`/`longjmp` (intra-procedural jump back), C++ `try`/`catch`/`throw`, `co_await`/`co_yield`/`co_return`. |
| **C#** | `if`, `&&`/`\|\|`, `for`/`foreach`, `switch_statement` (`when` guards, `goto case`/`goto default`) & `switch_expression`, `??` / `?.` null-conditional splits, `try`/`catch`/`when`/`finally`, `using` statement & `using var` declaration (`Dispose()`), `lock` (`Monitor.Exit`), `await`, `yield return`/`yield break`, lambdas/local functions. |
| **Python / TS / JS / Ruby / PHP / Kotlin** | `if`/`elif`/`else`, ternary, `for`/`while`/`foreach`, `try`/`except`/`finally` (`rescue`/`ensure`), `match`/`switch`, `with` context managers, `raise`/`throw`, `yield`/`await`. |

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl inspect --help` and [`src/cli/inspect.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect.rs):

```bash
# Optional whole-repo CFG/PDG precomputation during discover
rgctl discover . --with-cfg

# Inspect a function's CFG (text summary, JSON, Mermaid, or Graphviz DOT)
rgctl inspect <symbol> cfg
rgctl inspect <symbol> cfg --prune -f json
rgctl inspect <symbol> cfg --prune -f mermaid
rgctl inspect <symbol> cfg -f dot

# Disambiguate overloaded or same-named symbols by file, class, or line
rgctl inspect <symbol> --file src/Service.java --class Service --line 42 cfg -f json
```

### 4.2 JSON Schema (`rgctl inspect <symbol> cfg -f json`)

Implemented in [`src/cli/inspect_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect_output.rs):

```json
{
  "schema_version": "1.0",
  "symbol": "processOrder",
  "layer": "cfg",
  "pruned": true,
  "start_byte": 412,
  "end_byte": 1180,
  "name_byte_span": [424, 436],
  "entry_block": 0,
  "exit_block": 1,
  "block_count": 6,
  "edge_count": 7,
  "blocks": [
    {
      "id": 0,
      "kind": "Entry",
      "label": "entry",
      "start_line": 18,
      "end_line": 18,
      "statement_count": 2
    }
  ],
  "edges": [
    {
      "from": 0,
      "to": 2,
      "kind": "Unconditional"
    }
  ]
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **CFG Data Structures** | [`crates/rgctl-analysis/src/cfg.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg.rs) | `ControlFlowGraph`, `BasicBlock`, `BlockId`, `BlockKind`, `CfgEdge`, `CfgEdgeKind`, `Statement` |
| **Multi-Language AST Lowering** | [`crates/rgctl-analysis/src/cfg_builder.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_builder.rs) | `CfgBuilder`, `build_cfg_for_function`, `wire_condition`, `ScopeStack` |
| **v3 Mmap Archive & Lazy Append** | [`crates/rgctl-analysis/src/cfg_pdg_archive.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/cfg_pdg_archive.rs) | `CfgPdgArchive`, `CfgPdgArchiveMmap`, `CfgPdgRecord`, `TocEntry`, `ensure_cfg_pdg_for_source` |
| **CLI Inspect Command** | [`src/cli/inspect.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect.rs), [`src/cli/inspect_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/inspect_output.rs) | `run`, `InspectArgs`, `inspect_cfg_json`, `cfg_to_mermaid`, `cfg_to_dot` |
| **Dashboard Export & UI** | [`crates/rgctl-dashboard/src/cfg_export.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-dashboard/src/cfg_export.rs), [`dashboard/src/CfgView.tsx`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/dashboard/src/CfgView.tsx) | `export_cfg_index`, `CfgView` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Single-Parse AST Reuse:** During `rgctl discover --with-cfg`, `extract_with_cfg_pdg` lowers the CFG from the existing `tree_sitter::Tree` on the Rayon worker thread; never re-parse the source file a second time.
2. **$O(1)$ Archive Open & $O(\log F)$ Lookup:** `CfgPdgArchiveMmap::open` reads only the 16-byte `TOC3` EOF trailer (`file_len - 16`) and binary-searches the 32-byte `TocEntry` slice without deserializing untouched function records.
3. **Large-Repo Dashboard Gate:** When exporting the static dashboard bundle (`--with-universe`), `cfg_index.json` switches to `detail_mode: "archive_only"` on large repositories so per-function JSON files are not eagerly written to disk.

### Verification Commands

```bash
# Run multi-language CFG lowering & v3 archive unit tests
cargo test -p rgctl-analysis cfg

# Verify CLI inspect cfg output formats (json, mermaid, dot)
cargo test --test subprocess_golden_path
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Implicit RAII / `Drop` Destructors Not Expanded:** Rust `Drop::drop` and C++ RAII stack-unwinding destructors are not synthesized as explicit basic-block nodes at every scope exit (unlike Java `try-with-resources` and C# `using`, which have explicit lexical resource blocks).
2. **Async / Coroutine Lowering Is Intra-Function:** `.await`, `co_await`, and `yield` split basic blocks with suspend/resume edges inside the enclosing function's CFG rather than synthesizing a separate compiler state-machine struct.
3. **Computed `goto *ptr` & Inter-Procedural `longjmp`:** C/C++ computed indirect jumps (`goto *table[i]`) and cross-function `longjmp` unwinds are not statically wired across functions.
