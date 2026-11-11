# Semantic Search & Late Fusion — Engineering Design

This document specifies the **Sign-Quantized Semantic Search, Structural Token-Bloom Sketching, Call-Graph Jacobi Diffusion, and Multi-Signal Late Fusion subsystem** in `rgctl` (`crates/rgctl-analysis/src/semantic_*.rs`, `crates/rgctl-graph/src/structural_sketch.rs`, and `src/cli/semantic.rs`). It details how `rgctl` indexes function symbols into compact binary Hamming rows (`.rgctl/semantic_index.bin`) without impacting default cold `discover`, and blends semantic similarity with graph PageRank, Blast Radius, and 256-bit token-bloom sketches at query time.

![Search tab — query form and fusion-ranked results (gbuilder)](../images/design/semantic-search/semantic-search-results.png)

*Figure 1: Dashboard **Search** tab — Ready badge, query controls (late fusion, keyword AND), and ranked hit table.*

---

## 1. System Overview & End-to-End Data Flow

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ 1. DISCOVER-TIME PREPARATION (`crates/rgctl-extraction` + `rgctl-graph`)     │
│                                                                              │
│  Rayon Extraction Worker (`SymbolPass1Prep`):                                │
│    ├─► Computes BLAKE3 `code_hash` of function source body                   │
│    └─► Computes 256-bit `token_bloom: [u64; 4]` (`structural_sketch.rs`)     │
│        over split camelCase / snake_case declaration + body identifier tokens│
│        (Persisted in cold `NodeExtension`; zero extra pass!)                 │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 2. OPT-IN SEMANTIC INDEXING (`rgctl semantic index`)                         │
│                                                                              │
│  For each Function/Method node (reusing unchanged `code_hash` rows):         │
│    ├─► Embedder Backend (`--embedder vocab | hash | code-daemon | onnx`):    │
│    │     • Default `vocab`: compiled 256-d token projection table (offline)  │
│    ├─► Optional Call-Graph Jacobi Diffusion (`--diffuse`):                   │
│    │     • Blends dense `f32` vectors with callee/caller neighbors (`α=0.25`)│
│    └─► 1-Bit Sign Quantization (`x >= 0.0 -> 1`, `32 bytes` per 256-d vector)│
│        → Writes `.rgctl/semantic_index.bin`                                  │
└───────────────────────────────────────┬──────────────────────────────────────┘
                                        │
                                        ▼
┌──────────────────────────────────────────────────────────────────────────────┐
│ 3. TWO-STAGE QUERY & LATE FUSION (`rgctl semantic query` / `/api/semantic`)  │
│                                                                              │
│  Stage 1: Hardware `POPCNT` Hamming Candidate Pool (`default pool = 256`)    │
│  Stage 2: Multi-Signal Late Fusion Re-Ranking (`semantic_fusion.rs`):        │
│    `Score = w_sem·Sem + w_tok·TokenOverlap + w_bloom·BloomJaccard`           │
│           `+ w_pr·norm(PageRank) + w_blast·norm(BlastRadius)`                │
│  Optional Stage 3: Call-Graph Neighborhood Expansion (`--expand neighbors`)  │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 2. Theoretical Foundations Mapped to `rgctl`

| Concept | Theoretical Origin | How `rgctl` Applies It in Rust |
| :--- | :--- | :--- |
| **1-Bit Sign Random Projections (SimHash / Charikar LSH)** | Charikar (2002), *Similarity Estimation Techniques from Rounding Algorithms* (*STOC*) | [`semantic_search.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/semantic_search.rs#L1-L260) quantizes 256-dimensional `f32` embeddings (`1,024 bytes`) into 256-bit packed `[u64; 4]` signatures (`32 bytes` — a $32\times$ memory reduction), approximating cosine similarity via hardware `u64::count_ones(a ^ b)` (`POPCNT`). |
| **Knowledge Distillation into Static Token Projections** | Hinton, Vinyals & Dean (2015); Static word/subtoken projection distillation | Default `--embedder vocab` ([`semantic_vocab.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/semantic_vocab.rs#L1-L180)) uses a compiled 256-d token projection table distilled from the neural `code-daemon` teacher (`rgctl semantic distill`), running at $>100,000$ functions/sec on CPU with zero external model files or ONNX runtime dependencies. |
| **Graph Signal Diffusion (Jacobi Graph Convolution)** | Kipf & Welling (2017); Personalized PageRank feature propagation | When `--diffuse` is passed, [`semantic_diffuse.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/semantic_diffuse.rs#L1-L140) performs $T = 2$ iterations of Jacobi smoothing $\mathbf{h}_u^{(t+1)} = (1 - \alpha)\mathbf{h}_u^{(t)} + \alpha \frac{1}{|N(u)|}\sum_{v \in N(u)} \mathbf{h}_v^{(t)}$ ($\alpha = 0.25$) over the call graph prior to 1-bit sign quantization. |
| **Reciprocal / Linear Multi-Signal Late Fusion** | Cormack, Clarke & Buettcher (2009); Hybrid Lexical-Dense Retrieval | [`semantic_fusion.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/semantic_fusion.rs#L1-L210) re-ranks the top-256 Hamming candidates by combining semantic Hamming similarity, exact subtoken name overlap, 256-bit `token_bloom` sketch Jaccard similarity, PageRank, and Blast Radius. |

---

## 3. Binary Layout, Data Structures & Memory Discipline

### 3.1 256-Bit Structural Token-Bloom Sketch (`[u64; 4]`)

Implemented in [`crates/rgctl-graph/src/structural_sketch.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/structural_sketch.rs):
- Computed on Rayon extraction workers during `SymbolPass1Prep` from split identifier subtokens (`processOrderItem` → `process`, `order`, `item`).
- Stored in `NodeExtension.token_bloom: Option<[u64; 4]>` (`32 bytes`).
- Enables `--keyword-and` filtering and sub-linear clone pre-filtering without re-reading source files.

### 3.2 `.rgctl/semantic_index.bin` Binary Layout

| Field | Type | Description |
| :--- | :--- | :--- |
| `magic` / `version` | `[u8; 4]` / `u32` | Binary format version header |
| `model_id` | `String` | Embedder & option fingerprint (e.g. `vocab-accumulate-v2:256:decl` or `code-daemon:256:bodies:diffuse`) |
| `dimensions` | `u32` | Bit width of quantized signatures (default `256` = `32 bytes` per row) |
| `rows` | `Vec<SemanticRow>` | `node_id: Uuid`, `code_hash: [u8; 32]`, `bits: Vec<u8>` (`32 bytes`), plus cached `token_bloom: [u64; 4]` |

---

## 4. CLI Contract, JSON Schema & Implementation Map

### 4.1 CLI Commands

Verified against `rgctl semantic --help` and [`src/cli/mod.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/mod.rs#L1404-L1520):

```bash
# 1. Build .rgctl/semantic_index.bin (default: compiled vocab embedder, 256-d, incremental)
rgctl semantic index -f json

# 2. Build with body tokens and call-graph Jacobi diffusion
rgctl semantic index --embed-bodies --diffuse --diffuse-alpha 0.25 --diffuse-iters 2 -f json

# 3. Build with neural ONNX / code-daemon or deterministic FNV sign-hash
rgctl semantic index --embedder code-daemon
rgctl semantic index --embedder hash

# 4. Query functions with multi-signal late fusion (default) and keyword-AND filter
rgctl semantic query "shopping cart checkout" --limit 10 -f json
rgctl semantic query "order validation" --keyword-and --expand neighbors --expand-depth 2 -f json

# 5. Query pooled Fast-Leiden community embeddings
rgctl semantic query "payment gateway" --scope community -f json
```

### 4.2 JSON Schema (`rgctl semantic query "<QUERY>" -f json`)

Implemented in [`src/cli/semantic_output.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/semantic_output.rs):

```json
{
  "schema_version": "1.0",
  "query": "shopping cart checkout",
  "embedder": "vocab",
  "fusion_enabled": true,
  "results": [
    {
      "id": "1a2b3c4d-5e6f-7a8b-9c0d-112233445566",
      "name": "checkoutCart",
      "qualified_name": "com.example.CartService::checkoutCart",
      "file_path": "src/CartService.java",
      "start_line": 48,
      "score": 0.891,
      "semantic_score": 0.843,
      "sketch_overlap": 0.92,
      "pagerank": 0.0084
    }
  ]
}
```

### 4.3 Implementation Map

| Component | Source File | Key Types & Functions |
| :--- | :--- | :--- |
| **Hamming Index & Search** | [`crates/rgctl-analysis/src/semantic_search.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/semantic_search.rs) | `SemanticIndex`, `SemanticRow`, `DEFAULT_EMBEDDING_DIMENSIONS` (`256`), `DEFAULT_CANDIDATE_POOL` (`256`) |
| **Vocab Projection Embedder** | [`crates/rgctl-analysis/src/semantic_vocab.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/semantic_vocab.rs) | `VocabEmbedder`, `distill_vocab_matrix` |
| **Call-Graph Jacobi Diffusion** | [`crates/rgctl-analysis/src/semantic_diffuse.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/semantic_diffuse.rs) | `diffuse_embeddings` |
| **Late Fusion & Hybrid Expansion** | [`crates/rgctl-analysis/src/semantic_fusion.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/semantic_fusion.rs), [`semantic_hybrid.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-analysis/src/semantic_hybrid.rs) | `fuse_candidates`, `expand_call_neighborhood` |
| **256-Bit Token Bloom Sketch** | [`crates/rgctl-graph/src/structural_sketch.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/crates/rgctl-graph/src/structural_sketch.rs) | `compute_token_bloom`, `bloom_jaccard` |
| **CLI & HTTP Endpoints** | [`src/cli/semantic.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/semantic.rs), [`src/cli/semantic_api.rs`](file:///Users/sshaaf/git/rust/rgctl/rbuilder/src/cli/semantic_api.rs) | `SemanticCommands`, `GET /api/semantic/status`, `POST /api/semantic/query` |

---

## 5. Performance Guardrails & Verification Checklist

1. **Compiled `vocab` Default (No Mandatory ONNX / Python):** Default `rgctl semantic index` uses the compiled `vocab` embedder (`AGENTS.md`), requiring zero external model weights or GPU/ONNX runtimes unless `--embedder onnx` or `--embedder code-daemon` (`semantic-onnx` feature) is explicitly selected.
2. **Incremental `code_hash` Reuse:** When `--incremental` (default `true`) is active and `model_id` matches, unchanged functions reuse their 32-byte quantized bit row from `.rgctl/semantic_index.bin`.

### Verification Commands

```bash
# Run semantic search, vocab embedder, diffusion, and fusion unit tests
cargo test -p rgctl-analysis semantic

# Run CLI semantic index & query sanity tests
cargo test --test subprocess_golden_path
```

---

## 6. Known Limitations & By-Design Boundaries

1. **Index-Time Diffusion Memory Budget (`--diffuse`):** `--diffuse` holds two dense `f32` buffers of size $N_{\text{functions}} \times D \times 4$ bytes ($\approx 3.8\text{ GiB}$ at $1.86\text{M}$ functions $\times 256$ dims) and forces a full re-embedding pass; keep `--diffuse` off on kernel-scale repositories unless sufficient RAM is available.
2. **Declaration vs. Body Token Indexing:** By default, `rgctl semantic index` embeds symbol names, qualified names, parameter names/types, and docstrings from graph metadata without re-reading source files (`--embed-bodies` opts into re-reading function bodies). Existing 256-bit `token_bloom` sketches already capture body tokens from initial discovery.
