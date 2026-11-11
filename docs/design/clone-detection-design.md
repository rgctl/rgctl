# Clone Detection (Type 1–4) — Engineering Design

This document specifies the **Multi-Tier Code Clone Detection subsystem** in `rgctl` (`crates/rgctl-analysis/src/clones.rs`, `fragment_clones.rs`, and `src/cli/clones.rs`). It covers Type-1 exact body-hash grouping (`--mode exact`), Type-2 256-bit token-bloom LSH candidate grouping (`--mode bloom`), and Type-3/4 sub-function Single-Entry Single-Exit (SESE) hammock extraction with 2-iteration Weisfeiler–Lehman (`1-WL`) canonical subgraph hashing (`--mode fragment`).

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. DISCOVER-TIMEworker PREP (`SymbolPass1Prep` → `NodeExtension`)           │
│                                                                              │
│  Every Function / Method node carries:                                       │
│    • `code_hash: Option<String>` (BLAKE3 digest of function source bytes)    │
│    • `token_bloom: Option<[u64; 4]>` (256-bit structural token Bloom filter) │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. THREE QUERY-TIME CLONE MODES (`rgctl clones --mode <MODE>`)               │
│                                                                              │
│  MODE 1: `exact` (Type-1 Identical Bodies)                                   │
│    └─► Group functions with `LOC >= min_loc` by identical `code_hash`        │
│                                                                              │
│  MODE 2: `bloom` (Type-2 Renamed / Near-Miss Candidates)                     │
│    ├─► 4-band LSH bucketing over the four `u64` words of `token_bloom`       │
│    ├─► Pairwise bitwise popcount Jaccard `|A & B| / |A | B| >= threshold`    │
│    └─► Connected components → candidate clone groups (`candidates: true`)    │
│                                                                              │
│  MODE 3: `fragment` (Type-3/4 Sub-Function SESE Hammock + 1-WL Isomorphism)  │
│    ├─► Stage 1 (Bitwise Bloom Filter, ~30ms):                                │
│    │     `(func.bloom & seed_bloom) == seed_bloom` prunes 99%+ of functions  │
│    └─► Stage 2 (SESE Hammock + 2-Iter 1-WL Hash, ~150ms):                    │
│          • Load CFG from `.rgctl/analysis/cfg_pdg.archive.bin` (or parse)    │
│          • Extract SESE regions (`idom` + `ipdom`, `3 <= |S| <= 15` stmts)   │
│          • Run 2 iterations of 1-WL color refinement over statement & edge   │
│            kinds → group by canonical `structural_hash`                      │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. DIGEST-KEYED SIDECAR CACHES (`.rgctl/`)                                   │
│                                                                              │
│  • `.rgctl/clones.json`          (`schema_version = 1`, `mode = "exact"`)    │
│  • `.rgctl/clones.bloom.json`    (`schema_version = 1`, `mode = "bloom"`)    │
│  • `.rgctl/clones.fragment.json` (`schema_version = 2`, `mode = "fragment"`) │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **Cryptographic Exact Body Deduplication (Type-1)** | Roy, Cordy & Koschke (2009), *Comparison and Evaluation of Code Clone Detection Techniques and Tools* | [`clones.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/clones.rs#L1-L240) groups functions sharing identical worker-computed BLAKE3 `code_hash` digests in $O(N)$ time without reading source files. |
| **Bitwise Banded Locality-Sensitive Hashing (Type-2)** | Broder (1997); Bloom Filter Jaccard Estimation | In `--mode bloom`, each of the four 64-bit words of `token_bloom: [u64; 4]` serves as an LSH band key (capping bucket size at `512` to skip ubiquitous boilerplate). Candidate pairs are scored via hardware `u64::count_ones` Jaccard similarity $\frac{\sum \text{popcnt}(A_i \land B_i)}{\sum \text{popcnt}(A_i \lor B_i)} \ge \tau$ (default $\tau = 0.85$). |
| **Single-Entry Single-Exit (SESE) Hammock Decomposition** | Johnson, Pearson & Pingali (1994), *The Program Structure Tree: Computing Control Regions in Linear Time* | [`fragment_clones.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/fragment_clones.rs#L1-L280) pairs dominator tree (`p` dominates `q`) and post-dominator tree (`q` post-dominates `p`) equivalences to isolate self-contained sub-function control regions with $3 \le |S| \le 15$ statements. |
| **1-Dimensional Weisfeiler–Lehman (1-WL) Graph Isomorphism** | Weisfeiler & Lehman (1968); Shervashidze et al. (2011), *Weisfeiler-Lehman Graph Kernels* | Each SESE subgraph initializes node colors from normalized statement kinds (`If`, `Loop`, `Call`, `Assign`, `Decl`, `Return`) and refines colors over **2 iterations** of sorted neighbor + edge-kind (`Next`, `IfTrue`, `IfFalse`, def-use) multiset hashing into a canonical `structural_hash`. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 Core Clone Report Structures (`crates/rgctl-analysis/src/clones.rs` & `fragment_clones.rs`)

```rust
pub struct CloneReport {
    pub schema_version: u32, // 1 for exact/bloom
    pub mode: String,        // "exact" | "bloom"
    pub graph_digest: String,
    pub group_count: usize,
    pub groups: Vec<CloneGroup>,
}

pub struct FragmentCloneReport {
    pub schema_version: u32, // 2 for fragment
    pub mode: String,        // "fragment"
    pub graph_digest: String,
    pub seed: Option<FragmentSeedInfo>,
    pub group_count: usize,
    pub groups: Vec<FragmentCloneGroup>,
}
```

### 3.2 Zero Cold-Discover Impact

- Clone detection is strictly **query-time** (`rgctl clones`). It never adds `CLONE_OF` edges to `.rgctl/graph.snapshot.bin` and adds zero milliseconds to `rgctl discover .` (Gate A).

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl clones --help` and [`src/cli/mod.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/mod.rs#L765-L835):

```bash
# 1. Type-1 exact clones (identical code_hash, min 5 LOC, excluding test paths)
rgctl clones --mode exact --min-loc 5 --exclude test -f json

# 2. Type-2 near-miss token-bloom Jaccard candidates (default threshold = 0.85)
rgctl clones --mode bloom --threshold 0.85 --min-loc 5 --exclude test -f json

# 3. Type-3/4 sub-function SESE + 1-WL structural fragment clones seeded by symbol or file:lines
rgctl clones --mode fragment --seed PaymentService::processRefund --lines 45-58 -f json
rgctl clones --mode fragment --seed src/payment.rs:45-58 --min-statements 3 --max-statements 15 -f json

# 4. Query clone group containing a specific symbol
rgctl clones processRefund --file src/PaymentService.java -f json
```

### 4.2 JSON Schema (`rgctl clones --mode fragment --seed <SEED> -f json`, `schema_version = 2`)

```json
{
  "schema_version": 2,
  "mode": "fragment",
  "graph_digest": "8f0e12a4...",
  "filters": {
    "min_statements": 3,
    "max_statements": 15,
    "threshold": 1.0,
    "exclude": []
  },
  "seed": {
    "file": "src/PaymentService.java",
    "start_line": 45,
    "end_line": 58,
    "enclosing_function": "processRefund",
    "structural_hash": "wl2:9c4a11e08b72f103"
  },
  "group_count": 1,
  "groups": [
    {
      "structural_hash": "wl2:9c4a11e08b72f103",
      "size": 2,
      "score": 1.0,
      "members": [
        {
          "id": "11223344-5566-7788-99aa-bbccddee0001",
          "name": "processRefund",
          "file": "src/PaymentService.java",
          "start_line": 45,
          "end_line": 58,
          "enclosing_function": "processRefund",
          "statement_count": 6
        },
        {
          "id": "11223344-5566-7788-99aa-bbccddee0002",
          "name": "cancelOrder",
          "file": "src/OrderService.java",
          "start_line": 82,
          "end_line": 95,
          "enclosing_function": "cancelOrder",
          "statement_count": 6
        }
      ]
    }
  ]
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Type-1 Exact & Type-2 Bloom Engine** | [`crates/rgctl-analysis/src/clones.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/clones.rs) | `CloneDetector`, `CloneReport`, `CloneGroup`, `detect_exact_clones`, `detect_bloom_clones` |
| **Type-3/4 SESE + 1-WL Fragment Engine** | [`crates/rgctl-analysis/src/fragment_clones.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/fragment_clones.rs) | `FragmentCloneDetector`, `FragmentCloneReport`, `extract_sese_regions`, `wl_canonical_hash` |
| **CLI Clones Command** | [`src/cli/clones.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/clones.rs) | `ClonesArgs`, `run` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Stage-1 Bitwise Bloom Pruning for Fragment Clones:** Never run SESE hammock decomposition across all functions in a large repository unpruned; when `--seed` is supplied, Stage-1 bitwise containment `(func.bloom & seed_bloom) == seed_bloom` filters candidates in $\sim 30\text{ ms}$ before Stage-2 CFG/1-WL runs.
2. **LSH Bucket Cap (`512`):** In `--mode bloom`, LSH band buckets exceeding `512` members are skipped to prevent quadratic all-pairs comparisons on trivial getter/setter functions.

### Verification Commands

```bash
# Run exact, bloom, and SESE/1-WL fragment clone unit & integration tests
cargo test -p rgctl-analysis clones
cargo test -p rgctl-analysis fragment_clones
cargo test --test subprocess_golden_path
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Type-1 `exact` Uses Raw Source Byte Hash:** `code_hash` hashes the exact UTF-8 byte slice of the function declaration/body; functions differing only in comments or whitespace have distinct `code_hash` values but are caught by `--mode bloom` ($\tau = 0.85$) or `--mode fragment`.
2. **SESE Statement Window (`3..=15`):** Fragment clone detection defaults to SESE regions between `3` and `15` statements (`--min-statements` / `--max-statements`) so trivial 1-line statements and whole-function bodies are not redundantly reported.
