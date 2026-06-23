# Phase 21: Workspace Architecture Migration (Revised)

> **Execution Instructions for Cursor Agent**  
> **Status**: Ready for implementation  
> **Estimated Time**: 9 weeks (incremental with checkpoints)  
> **Risk Level**: Medium (mitigated by Phase 21.0 foundation work)

## North Star

Transform rBuilder from a single-crate monolith with Cargo feature flags into a **layered Cargo workspace** where:

1. **No language code lives in core** — all 44 languages are external crates.
2. **Dependency direction is enforced** — language crates depend only on `rbuilder-plugin-api` / helpers; core never depends on `rbuilder-lang-*`.
3. **Registration happens at the bundle/binary layer** — not inside graph, extraction, or analysis.
4. **CI unused-import failures are eliminated** — each crate compiles only what it owns.
5. **Third-party language plugins are possible** — stable `rbuilder-plugin-api` on crates.io.

**Non-negotiable rule**: If a module imports `tree-sitter-*`, implements `LanguagePlugin`, or contains language-specific path heuristics, it belongs in a language crate — not in core.

---

## Target Dependency Graph

```mermaid
flowchart BT
  api[rbuilder-plugin-api]
  helpers[rbuilder-plugin-helpers]
  macros[rbuilder-macros]
  formats[rbuilder-config-formats]
  runtime[rbuilder-lang-runtime]
  registry[rbuilder-registry]
  graph[rbuilder-graph]
  extract[rbuilder-extraction]
  pipeline[rbuilder-pipeline]
  analysis[rbuilder-analysis]
  gql[rbuilder-gql]
  export[rbuilder-export]
  nlp[rbuilder-nlp]
  security[rbuilder-security]
  langs[rbuilder-lang-*]
  bundles[rbuilder-bundle-*]
  cli[rbuilder-cli]
  bin[rbuilder binary]

  helpers --> api
  macros --> api
  formats --> api
  runtime --> helpers
  runtime --> api
  registry --> api
  registry --> formats
  graph --> api
  extract --> api
  extract --> graph
  extract --> registry
  pipeline --> extract
  pipeline --> graph
  analysis --> graph
  gql --> graph
  export --> graph
  nlp --> graph
  security --> graph
  langs --> runtime
  langs --> helpers
  bundles --> registry
  bundles --> langs
  bundles --> runtime
  cli --> pipeline
  cli --> analysis
  cli --> bundles
  bin --> cli
```

### Forbidden edges (enforced in CI via `cargo-deny`)

| From | Must NOT depend on |
|------|-------------------|
| `rbuilder-plugin-api` | anything in workspace |
| `rbuilder-lang-*` | `rbuilder-graph`, `rbuilder-analysis`, `rbuilder-cli`, `rbuilder-extraction` |
| `rbuilder-graph` | `rbuilder-lang-*`, `rbuilder-bundle-*` |
| `rbuilder-analysis` | `rbuilder-lang-*` |
| `rbuilder-extraction` | `rbuilder-lang-*` |
| `rbuilder-registry` | `rbuilder-lang-*` (struct + routing only) |
| Any core crate | `rbuilder-bundle-*` |

---

## Workspace Layout (Final)

```
rBuilder/
├── Cargo.toml                    # [workspace] virtual manifest — no [package]
├── crates/
│   ├── rbuilder-plugin-api/      # Stable plugin contract (extracted verbatim from plugin_trait.rs)
│   ├── rbuilder-plugin-helpers/  # tree-sitter helpers, complexity
│   ├── rbuilder-config-formats/  # yaml, json, toml, properties, markdown plugins
│   ├── rbuilder-lang-runtime/    # TreeSitterLanguagePlugin, RegexLanguagePlugin, LanguageConfig
│   ├── rbuilder-registry/        # LanguageRegistry (empty by default)
│   ├── rbuilder-graph/           # schema, backends, CodeGraph
│   ├── rbuilder-extraction/      # discovery, extractor, graph_builder
│   ├── rbuilder-pipeline/        # ProcessingPipeline
│   ├── rbuilder-analysis/        # graph analysis (NO IaC-specific modules)
│   ├── rbuilder-gql/
│   ├── rbuilder-export/
│   ├── rbuilder-nlp/
│   ├── rbuilder-security/        # generic security (NO IaC-specific modules)
│   ├── rbuilder-project-config/  # rbuilder.yaml, secrets, drift, usage detector
│   ├── rbuilder-incremental/
│   ├── rbuilder-semantic/
│   ├── rbuilder-rules/
│   ├── rbuilder-mcp/             # api + mcp + watch (feature: mcp-server)
│   ├── rbuilder-cli/             # CLI command wiring
│   ├── rbuilder/                 # Binary crate (thin: main + bundle selection)
│   ├── rbuilder-core/            # Facade: pub use for library consumers
│   ├── rbuilder-lang-<name>/     # 44 language crates
│   ├── rbuilder-bundle-minimal/
│   ├── rbuilder-bundle-extended/
│   ├── rbuilder-bundle-full/
│   ├── rbuilder-bundle-extra/
│   └── rbuilder-integration-tests/
├── rbuilder-macros/              # Updated to use rbuilder-plugin-api::Error
├── benches/                      # Or moved into owning crates
└── tests/fixtures/               # Shared fixtures (path deps from integration-tests)
```

**Root package**: Virtual workspace only. The published `rbuilder` library crate lives at `crates/rbuilder-core` (facade) and the CLI at `crates/rbuilder`.

---

## Module → Crate Assignment

### `rbuilder-plugin-api`

| Source | Notes |
|--------|-------|
| `src/languages/plugin_trait.rs` | **Copy verbatim** — do not simplify |
| `src/error.rs` (plugin-related variants only) | `PluginError`, `UnsupportedLanguage`, `ParseError` |

Exports: `LanguagePlugin`, `ConfigFormatPlugin`, `Symbol`, `Relation`, `SymbolType`, `RelationType`, `ConfigKey`, `ComplexityMetrics`, `SourceLocation`, `Parameter`, `Field`, `Error`, `Result`.

### `rbuilder-plugin-helpers`

| Source | Notes |
|--------|-------|
| `src/languages/extraction/tree_sitter.rs` | Update imports to `rbuilder_plugin_api` |
| `src/languages/extraction/complexity.rs` | |
| `src/languages/generic/regex_extract.rs` | |

### `rbuilder-config-formats`

| Source | Notes |
|--------|-------|
| `src/languages/config/yaml.rs` | |
| `src/languages/config/json.rs` | |
| `src/languages/config/toml_plugin.rs` | |
| `src/languages/config/properties.rs` | |
| `src/languages/config/markdown.rs` | |

Each format plugin exports `register(registry: &mut LanguageRegistry)`.

### `rbuilder-lang-runtime`

| Source | Notes |
|--------|-------|
| `src/languages/generic/tree_sitter_plugin.rs` | |
| `src/languages/generic/regex_plugin.rs` | |
| `src/languages/generic/config.rs` | Replace `include!(OUT_DIR/...)` with per-crate `LanguageConfig` |
| `languages.toml` | **Retained temporarily** as manifest input for codegen in this crate only |

Provides factory functions:

```rust
pub fn tree_sitter_plugin(config: &'static LanguageConfig, grammar: GrammarFn) -> TreeSitterLanguagePlugin;
pub fn regex_plugin(config: &'static LanguageConfig) -> RegexLanguagePlugin;
```

### `rbuilder-registry`

| Source | Notes |
|--------|-------|
| `src/languages/registry.rs` | **Strip** `register_all_language_plugins`, `LanguageRegistry::new()` auto-registration, and all `#[cfg(feature = "lang-*")]` |
| `src/languages/plugin_loader.rs` | |
| `src/languages/plugin_abi.rs` | |

`LanguageRegistry` API:

```rust
impl LanguageRegistry {
    pub fn empty() -> Self;
    pub fn with_config_formats() -> Self;  // registers yaml/json/toml/properties only
    pub fn register_language_plugin(&mut self, plugin: Arc<dyn LanguagePlugin>);
    pub fn register_config_plugin(&mut self, plugin: Arc<dyn ConfigFormatPlugin>);
    // get_plugin_for_file uses extension map + plugin.matches_path() — NO concrete parser imports
}
```

**Add to `LanguagePlugin` trait** (in plugin-api, before extraction):

```rust
/// Path-based routing for languages that share extensions (e.g. .yml for ansible vs config).
fn matches_path(&self, path: &str) -> bool {
    path.rsplit('.').next()
        .map(|ext| self.file_extensions().iter().any(|e| e == ext))
        .unwrap_or(false)
}
```

IaC plugins override this with `is_ansible_path` / `is_chef_path` / `is_puppet_path` logic moved into their crates.

### `rbuilder-graph`

| Source |
|--------|
| `src/graph/` (all) |

Depends on `rbuilder-plugin-api` for `SourceLocation`, `Parameter` only.

### `rbuilder-extraction`

| Source |
|--------|
| `src/discovery/` |
| `src/extraction/` |

### `rbuilder-pipeline`

| Source |
|--------|
| `src/pipeline/` |
| `src/parallel.rs` |

### `rbuilder-analysis`

| Source | Notes |
|--------|-------|
| `src/analysis/blast_radius.rs` | |
| `src/analysis/callgraph.rs` | |
| `src/analysis/centrality.rs` | |
| `src/analysis/cfg*.rs` | |
| `src/analysis/community.rs` | |
| `src/analysis/complexity.rs` | |
| `src/analysis/dataflow.rs` | |
| `src/analysis/def_use.rs` | |
| `src/analysis/dependency.rs` | |
| `src/analysis/dominance.rs` | |
| `src/analysis/flow_cache.rs` | |
| `src/analysis/graph_utils.rs` | |
| `src/analysis/interprocedural_*.rs` | |
| `src/analysis/pdg.rs` | |
| `src/analysis/slicing.rs` | |
| `src/analysis/taint.rs` | |
| `src/analysis/type_inference.rs` | |

**Moved OUT of core** (into language crates — see IaC section below):

| Source | Destination |
|--------|-------------|
| `src/analysis/ansible_roles.rs` | `rbuilder-lang-ansible` |
| `src/analysis/chef_cookbooks.rs` | `rbuilder-lang-chef` |
| `src/analysis/puppet_modules.rs` | `rbuilder-lang-puppet` |

### `rbuilder-gql`

| Source |
|--------|
| `src/gql/` |

### `rbuilder-export`

| Source |
|--------|
| `src/export/` |

### `rbuilder-nlp`

| Source |
|--------|
| `src/nlp/` |

### `rbuilder-security`

| Source | Notes |
|--------|-------|
| `src/security/analyzer.rs` | |
| `src/security/cve_patterns.rs` | |
| `src/security/mod.rs` | |

**Moved OUT of core**:

| Source | Destination |
|--------|-------------|
| `src/security/ansible.rs` | `rbuilder-lang-ansible` |
| `src/security/chef.rs` | `rbuilder-lang-chef` |
| `src/security/puppet.rs` | `rbuilder-lang-puppet` |

### `rbuilder-project-config`

| Source |
|--------|
| `src/config/` |

### `rbuilder-incremental`

| Source |
|--------|
| `src/incremental/` |
| `src/changes/` |

### `rbuilder-semantic`

| Source |
|--------|
| `src/semantic/` |

### `rbuilder-rules`

| Source |
|--------|
| `src/rules/` |

### `rbuilder-mcp` (feature: `mcp-server`)

| Source |
|--------|
| `src/api/` |
| `src/mcp/` |
| `src/watch.rs` |

### `rbuilder-cli`

| Source | Notes |
|--------|-------|
| `src/cli/chat.rs` | |
| `src/cli/diagram.rs` | |
| `src/cli/update.rs` | |
| `src/cli/workspace.rs` | |
| `src/cli/mcp.rs` | feature-gated |
| `src/cli/serve.rs` | feature-gated |
| `src/output/` | |
| `src/git_util.rs` | |
| `src/hooks/` | |
| `src/multi_repo/` | |

**Moved OUT of core** (IaC CLI delegates to lang crates):

| Source | Destination |
|--------|-------------|
| `src/cli/ansible.rs` | `rbuilder-lang-ansible::cli` |
| `src/cli/chef.rs` | `rbuilder-lang-chef::cli` |
| `src/cli/puppet.rs` | `rbuilder-lang-puppet::cli` |

`rbuilder-cli` calls `rbuilder_lang_ansible::cli::run(...)` via bundle feature flags or optional deps — never imports parsers directly.

### `rbuilder` (binary)

| Source |
|--------|
| `src/main.rs` |

Thin wiring: selects bundle, builds registry, runs CLI.

### `rbuilder-core` (facade)

Re-exports public API for library users:

```rust
pub use rbuilder_graph::*;
pub use rbuilder_extraction::*;
pub use rbuilder_analysis::*;
pub use rbuilder_plugin_api as plugin;
pub use rbuilder_registry::LanguageRegistry;
// ... etc
```

### `rbuilder-lang-<name>` (44 crates)

Each language crate owns **all** language-specific code. Structure:

```
crates/rbuilder-lang-rust/
├── Cargo.toml
└── src/
    ├── lib.rs          # pub struct RustPlugin; impl LanguagePlugin; pub fn register()
    ├── plugin.rs       # extraction logic (from builtin/rust.rs)
    └── config.rs       # static LanguageConfig (from languages.toml entry)
```

#### Language inventory (44 total)

| Handler | Count | Languages | Crate strategy |
|---------|-------|-----------|----------------|
| `custom` | 14 | rust, python, typescript, javascript, go, java, sql, dockerfile, github_actions, gitlab_ci, ansible, chef, puppet, bash | Full plugin impl in crate |
| `tree-sitter` | 23 | c, cpp, ruby, php, swift, scala, lua, elixir, erlang, haskell, dart, r, julia, nim, ocaml, perl, fortran, verilog, vhdl, pascal, zig, fsharp, crystal | Thin crate: `LanguageConfig` + `rbuilder-lang-runtime::tree_sitter_plugin` |
| `regex` | 6 | kotlin, csharp, cobol, scheme, clojure, assembly | Thin crate: `LanguageConfig` + `rbuilder-lang-runtime::regex_plugin` |
| `markdown` | 1 | markdown | Uses `rbuilder-config-formats::MarkdownPlugin` or dedicated crate |

#### IaC language crates (extra modules)

`rbuilder-lang-ansible`, `rbuilder-lang-chef`, `rbuilder-lang-puppet` additionally contain:

```
src/
├── lib.rs
├── plugin.rs       # LanguagePlugin (from multimodal/*/mod.rs + parser)
├── parser.rs       # from multimodal/*/parser.rs
├── analysis.rs     # from src/analysis/*_roles.rs etc.
├── security.rs     # from src/security/*.rs
└── cli.rs          # from src/cli/*.rs
```

Each exports:

```rust
pub fn register(registry: &mut LanguageRegistry) { ... }
```

### `rbuilder-bundle-*` (4 crates)

Bundles **compose** languages and **register** them. This replaces `build.rs` + Cargo feature matrix.

```rust
// crates/rbuilder-bundle-full/src/lib.rs
use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

pub fn register_languages(registry: &mut LanguageRegistry) {
    rbuilder_lang_rust::register(registry);
    rbuilder_lang_python::register(registry);
    // ... all languages in this bundle
}

pub fn register_config_formats(registry: &mut LanguageRegistry) {
    rbuilder_config_formats::register_all(registry);
}

pub fn default_registry() -> LanguageRegistry {
    let mut registry = LanguageRegistry::empty();
    register_config_formats(&mut registry);
    register_languages(&mut registry);
    registry
}
```

| Bundle | Languages | Replaces |
|--------|-----------|----------|
| `rbuilder-bundle-minimal` | rust, python, javascript, typescript, go | `bundle-minimal` feature |
| `rbuilder-bundle-extended` | minimal + 16 more | `bundle-extended` feature |
| `rbuilder-bundle-full` | extended + 10 more | `bundle-full` feature (default) |
| `rbuilder-bundle-extra` | full + 13 more | `bundle-extra` feature |

### `rbuilder-integration-tests`

| Source | Notes |
|--------|-------|
| `tests/phase*.rs` | Dev-depend on `rbuilder-bundle-*` |
| `tests/common/` | Shared test helpers |
| `tests/fixtures/` | Stay at repo root, referenced via path |

### `rbuilder-macros`

Update `LanguagePlugin` derive to reference `rbuilder_plugin_api::Error` instead of `crate::error::Error`.

---

## Phase 21.0: Foundation (Week 1) — DO THIS FIRST

**Goal**: Establish contracts and decouple core from language implementations **before** moving any files.

### 21.0.1 Dependency audit

```bash
# Add to repo (dev dependency or CI tool)
cargo install cargo-deny
```

Create `deny.toml` with banned dependency paths (see Forbidden edges table).

### 21.0.2 Add `matches_path` to `LanguagePlugin`

In `src/languages/plugin_trait.rs`, add default `matches_path`. Update Ansible/Chef/Puppet plugins to override with existing `is_*_path` logic.

### 21.0.3 Decouple registry from concrete parsers

In `src/languages/registry.rs`:

- Remove direct imports of `AnsibleParser`, `ChefParser`, `PuppetParser`
- Route path-sensitive files by iterating registered plugins and calling `matches_path()`
- Register IaC plugins **before** generic yaml handlers (order matters)

### 21.0.4 Decouple CLI/analysis/security from parsers

Refactor call sites to use:

- `registry.get_plugin_for_file(path)` for extraction, or
- Lang crate public API: `rbuilder_lang_ansible::analysis::RoleDependencyGraph` (temporary `use` from monolith until crates exist)

**Checkpoint**: `cargo test --workspace` (monolith) passes. `rg 'languages::multimodal|languages::builtin' src/analysis src/cli src/security src/languages/registry.rs` returns zero hits outside `src/languages/`.

### 21.0.5 Document error boundaries

| Crate | Error type |
|-------|-----------|
| `rbuilder-plugin-api` | `rbuilder_plugin_api::Error` |
| `rbuilder-graph` | `rbuilder_graph::Error` or shared `rbuilder-error` |
| `rbuilder-extraction` | wraps graph + plugin errors |
| Binary / CLI | `anyhow::Error` at top level |

Decision: introduce `crates/rbuilder-error` with full `Error` enum (from current `src/error.rs`) if multiple core crates need it. Plugin API keeps its own minimal error for plugin authors.

### 21.0.6 Freeze plugin API snapshot

```bash
# Create reference copy for diff review during extraction
cp src/languages/plugin_trait.rs .github/PLUGIN_API_SNAPSHOT.rs
```

### 21.0.7 Validation

- [ ] All tests pass on monolith
- [ ] Registry routing works for ansible/chef/puppet fixtures
- [ ] `deny.toml` committed (may not enforce workspace edges until crates exist)
- [ ] No concrete parser imports outside `src/languages/`

---

## Phase 21.1: Workspace Scaffold + API Crates (Week 2)

**Goal**: Create workspace structure and extract leaf crates without moving graph/analysis yet.

### Steps

#### 1.1 Convert root to virtual workspace

**File**: `Cargo.toml` (root)

```toml
[workspace]
members = [
    "crates/rbuilder-plugin-api",
    "crates/rbuilder-plugin-helpers",
    "crates/rbuilder-config-formats",
    "crates/rbuilder-lang-runtime",
    "crates/rbuilder-registry",
    "rbuilder-macros",
]
resolver = "2"

[workspace.dependencies]
rbuilder-plugin-api = { path = "crates/rbuilder-plugin-api", version = "0.2" }
rbuilder-plugin-helpers = { path = "crates/rbuilder-plugin-helpers", version = "0.2" }
rbuilder-config-formats = { path = "crates/rbuilder-config-formats", version = "0.2" }
rbuilder-lang-runtime = { path = "crates/rbuilder-lang-runtime", version = "0.2" }
rbuilder-registry = { path = "crates/rbuilder-registry", version = "0.2" }
tree-sitter = "0.25"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
regex = "1"
thiserror = "1"
uuid = { version = "1", features = ["v4", "serde"] }

[profile.release]
lto = true
codegen-units = 1
strip = true
```

Keep monolith temporarily as `crates/rbuilder-legacy` OR keep `src/` at root with a `[package]` until Phase 21.2 — **recommended**: move monolith to `crates/rbuilder-legacy` in one commit to free the root.

#### 1.2 Create `rbuilder-plugin-api`

Extract verbatim from `plugin_trait.rs` + minimal error types. Monolith re-exports:

```rust
pub use rbuilder_plugin_api::*;
```

#### 1.3 Create `rbuilder-plugin-helpers`, `rbuilder-config-formats`, `rbuilder-lang-runtime`, `rbuilder-registry`

Follow module map above. Monolith depends on these via path deps and deletes moved files.

#### 1.4 Update `rbuilder-macros`

Point to `rbuilder_plugin_api::Error`.

#### 1.5 Validation

```bash
cargo build -p rbuilder-plugin-api
cargo build -p rbuilder-registry
cargo test -p rbuilder-config-formats
# Monolith/legacy still builds and passes all tests
cargo test
```

---

## Phase 21.2: Core Engine Crates (Week 3)

**Goal**: Split graph, extraction, pipeline, analysis out of monolith. Still **zero** language crates.

### Create crates

- `rbuilder-graph`
- `rbuilder-extraction`
- `rbuilder-pipeline`
- `rbuilder-analysis` (without IaC modules — already moved to lang dirs in 21.0)
- `rbuilder-gql`
- `rbuilder-export`
- `rbuilder-nlp`
- `rbuilder-security` (without IaC modules)
- `rbuilder-project-config`
- `rbuilder-incremental`
- `rbuilder-semantic`
- `rbuilder-rules`
- `rbuilder-mcp`
- `rbuilder-cli`
- `rbuilder-core` (facade)

### Monolith becomes thin

`crates/rbuilder` binary depends on `rbuilder-core` + **temporary** inline language registration (last monolith vestige) until Phase 21.3.

### Validation

```bash
cargo build --workspace
cargo test -p rbuilder-graph
cargo test -p rbuilder-extraction
cargo test -p rbuilder-analysis
# Verify no lang deps in core
cargo tree -p rbuilder-analysis | grep rbuilder-lang  # must be empty
```

---

## Phase 21.3: Language Crate Extraction (Weeks 4–6)

**Goal**: All 44 languages in `rbuilder-lang-*` crates. Core has **zero** language code.

### Week 4: IaC + multimodal custom languages (highest coupling)

Extract in order:

1. `rbuilder-lang-ansible` (+ analysis, security, cli modules)
2. `rbuilder-lang-chef`
3. `rbuilder-lang-puppet`
4. `rbuilder-lang-bash`
5. `rbuilder-lang-sql`
6. `rbuilder-lang-dockerfile`
7. `rbuilder-lang-github_actions`
8. `rbuilder-lang-gitlab_ci`

Each crate:

```toml
[dependencies]
rbuilder-plugin-api = { workspace = true }
rbuilder-plugin-helpers = { workspace = true }
rbuilder-lang-runtime = { workspace = true }  # if needed
serde = { workspace = true }
regex = { workspace = true }
# tree-sitter-bash etc. as needed
```

```bash
cargo test -p rbuilder-lang-ansible
cargo test -p rbuilder-lang-puppet
```

### Week 5: Builtin custom languages

Extract: `rust`, `python`, `typescript`, `javascript`, `go`, `java`

### Week 6: Tree-sitter thin + regex + markdown

For each tree-sitter language, create thin crate:

```rust
// crates/rbuilder-lang-c/src/lib.rs
use rbuilder_lang_runtime::{tree_sitter_plugin, LanguageConfig};

static CONFIG: LanguageConfig = LanguageConfig {
    id: "c",
    extensions: &["c", "h"],
    function_kinds: &["function_definition"],
    class_kinds: &["struct_specifier"],
    enable_complexity: true,
    enable_type_inference: false,
    regex_patterns: None,
};

pub fn register(registry: &mut LanguageRegistry) {
    let plugin = tree_sitter_plugin(&CONFIG, || tree_sitter_c::language());
    registry.register_language_plugin(Arc::new(plugin));
}
```

Migrate `languages.toml` entries into `static LanguageConfig` per crate. After all 44 exist, **delete** root `languages.toml` and `build.rs`.

### Validation

```bash
# No language code in core
rg 'tree-sitter-' crates/rbuilder-graph crates/rbuilder-extraction crates/rbuilder-analysis
# → zero results

# Each language standalone
for c in crates/rbuilder-lang-*; do cargo test -p $(basename $c); done
```

---

## Phase 21.4: Bundle Crates + Wire Binary (Week 7)

**Goal**: Replace Cargo feature matrix with bundle composition.

### Create bundles

See bundle table above. Each bundle's `Cargo.toml` lists only its language crate deps.

### Update binary

**File**: `crates/rbuilder/Cargo.toml`

```toml
[dependencies]
rbuilder-cli = { path = "../rbuilder-cli" }
rbuilder-bundle-full = { path = "../rbuilder-bundle-full" }  # default

[features]
default = ["bundle-full"]
bundle-minimal = ["rbuilder-bundle-minimal"]
bundle-extended = ["rbuilder-bundle-extended"]
bundle-full = ["rbuilder-bundle-full"]
bundle-extra = ["rbuilder-bundle-extra"]
mcp-server = ["rbuilder-cli/mcp-server", "rbuilder-mcp"]
```

**File**: `crates/rbuilder/src/main.rs`

```rust
let registry = Arc::new(rbuilder_bundle_full::default_registry());
```

### Remove legacy feature flags

Delete from old `Cargo.toml`:

- All `lang-*` features
- All `bundle-*` features (replaced by bundle crates)
- `build.rs`
- `languages.toml`

### Validation

```bash
cargo build --release -p rbuilder
./target/release/rbuilder init tests/fixtures/rust
cargo test -p rbuilder-bundle-full
cargo test -p rbuilder-bundle-minimal
```

---

## Phase 21.5: CI, Tests, Docs (Week 8)

### Integration test crate

**File**: `crates/rbuilder-integration-tests/Cargo.toml`

```toml
[package]
name = "rbuilder-integration-tests"
publish = false

[dev-dependencies]
rbuilder-bundle-full = { path = "../rbuilder-bundle-full" }
rbuilder-bundle-extended = { path = "../rbuilder-bundle-extended" }
# ...

[[test]]
name = "phase11_bundles"
path = "../../tests/phase11_bundles.rs"
```

### CI workflows

**File**: `.github/workflows/language-bundles.yml`

```yaml
name: Bundle Testing
on: [push, pull_request]
jobs:
  test-bundles:
    name: ${{ matrix.bundle }}
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        include:
          - bundle: minimal
            test: phase11_bundles
          - bundle: extended
            test: phase11_multimodal
          - bundle: full
            test: phase11_tier2_languages
          - bundle: extra
            test: phase11_tier2_niche_languages
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test -p rbuilder-bundle-${{ matrix.bundle }}
      - run: cargo test -p rbuilder-integration-tests --test ${{ matrix.test }}
```

**File**: `.github/workflows/language-crates.yml` — matrix over `crates/rbuilder-lang-*` (test + clippy).

**File**: `.github/workflows/deps.yml` (NEW)

```yaml
name: Dependency Policy
on: [push, pull_request]
jobs:
  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: cargo install cargo-deny
      - run: cargo deny check
```

### Documentation

| File | Purpose |
|------|---------|
| `docs/ARCHITECTURE.md` | Crate layering diagram + forbidden edges |
| `docs/PLUGIN_DEVELOPMENT.md` | Third-party plugin guide |
| `docs/MIGRATION_TO_0.2.md` | User/library migration |
| `deny.toml` | Dependency policy |

---

## Phase 21.6: Release (Week 9)

1. `git tag v0.2.0`
2. Update `CHANGELOG.md`
3. Publish to crates.io (order matters):
   - `rbuilder-plugin-api`
   - `rbuilder-plugin-helpers`
   - `rbuilder-config-formats`
   - `rbuilder-lang-runtime`
   - `rbuilder-registry`
   - `rbuilder-lang-*` (can batch)
   - `rbuilder-bundle-*`
   - `rbuilder-core`
   - `rbuilder` (binary)
4. Update `README.md` architecture diagram
5. Close Phase 21 tracking issue

---

## Validation Checklist

### Phase 21.0 (Foundation)
- [ ] `matches_path` on `LanguagePlugin`
- [ ] Registry uses `matches_path`, not concrete parsers
- [ ] No parser imports in `analysis/`, `cli/`, `security/`, `registry.rs`
- [ ] All monolith tests pass

### Phase 21.1 (API crates)
- [ ] `rbuilder-plugin-api` matches `PLUGIN_API_SNAPSHOT.rs`
- [ ] `rbuilder-registry` has no language registrations in `new()`
- [ ] `cargo deny` config committed

### Phase 21.2 (Core engine)
- [ ] `cargo tree -p rbuilder-analysis` has no `rbuilder-lang-*`
- [ ] `cargo tree -p rbuilder-graph` has no `rbuilder-lang-*`
- [ ] All core crate tests pass

### Phase 21.3 (Languages)
- [ ] 44 `rbuilder-lang-*` crates exist
- [ ] `rg 'languages::' crates/rbuilder-graph crates/rbuilder-extraction crates/rbuilder-analysis` → empty
- [ ] `build.rs` and `languages.toml` deleted
- [ ] Each lang crate: `cargo clippy -- -D warnings` clean

### Phase 21.4 (Bundles)
- [ ] `rbuilder-bundle-full::default_registry()` registers all expected plugins
- [ ] Binary works: `rbuilder init tests/fixtures/rust`
- [ ] Output identical to pre-migration baseline (snapshot test)

### Phase 21.5 (CI/Docs)
- [ ] Bundle CI matrix passes
- [ ] Per-language CI matrix passes
- [ ] `cargo deny check` passes
- [ ] Integration tests migrated

### Phase 21.6 (Release)
- [ ] crates.io publishing complete
- [ ] `cargo install rbuilder` works

---

## Rollback Instructions

### After Phase 21.0 only
Revert decoupling commits; no workspace changes to undo.

### After workspace scaffold (21.1+)
```bash
git checkout main -- Cargo.toml src/
git clean -fd crates/
```

### Partial rollback (keep API crates, revert languages)
```bash
# Keep crates/rbuilder-plugin-api, restore monolith registration
git checkout main -- build.rs languages.toml src/languages/
```

### Full revert
```bash
git revert <phase-21-merge-commit-range>
```

---

## Success Metrics

| Metric | Before | Target |
|--------|--------|--------|
| Incremental lang change rebuild | ~3 min (full crate) | ~30s (one lang crate) |
| CI wall time | ~25 min | <8 min |
| `rbuilder-bundle-minimal` dep count | 50+ | <20 |
| Language code in core | 100% | **0%** |
| Circular dependency risk | High (logical) | **Zero** (enforced by deny.toml) |

```bash
# Verify no languages in core
rg -l 'LanguagePlugin|tree-sitter-' crates/rbuilder-graph crates/rbuilder-extraction \
  crates/rbuilder-analysis crates/rbuilder-registry
# → no matches

# Verify registration only in bundles
rg 'register_language_plugin' crates/ --glob '!crates/rbuilder-lang-*' --glob '!crates/rbuilder-bundle-*'
# → only rbuilder-registry (method def) and rbuilder-config-formats
```

---

## Execution Order Summary

```
21.0 Foundation        Decouple registry/CLI/analysis from parsers; add matches_path
21.1 API crates        plugin-api, helpers, config-formats, lang-runtime, registry
21.2 Core engine       graph, extraction, pipeline, analysis, gql, nlp, cli, facade
21.3 Languages (44)    All rbuilder-lang-* — NOTHING language-related left in core
21.4 Bundles             Compose languages; wire binary; delete build.rs
21.5 CI + docs           Integration tests, deny.toml, workflows
21.6 Release             v0.2.0, crates.io
```

**Start with 21.0. Do not create language crates until registry decoupling is complete and core crates compile without any `rbuilder-lang-*` dependency.**
