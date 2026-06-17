# Multi-Modal Analysis

rBuilder analyzes more than source code. Phase 11 adds **multi-modal** plugins that extract
infrastructure and schema artifacts into the same knowledge graph as functions and classes.

## Supported Inputs

| Plugin | File patterns | Graph nodes | Graph edges |
|--------|---------------|-------------|-------------|
| SQL | `*.sql` | `Table` (tables & views) | `References` (foreign keys) |
| Dockerfile | `Dockerfile` (no extension) | `Dependency`, `BuildStep`, `Import` | `Uses` |
| GitHub Actions | `.github/workflows/*.{yml,yaml}` | `Job`, `BuildStep` | `DependsOn` |
| GitLab CI | `.gitlab-ci.yml` | `Job`, `BuildStep` | `DependsOn` |
| Bash | `*.sh`, `*.bash` | `Function` | `Uses` (`source` directives) |

Enable via the `bundle-extended` feature (or individual `lang-sql`, `lang-dockerfile`, etc.).

## SQL DDL (regex-based)

The SQL plugin intentionally uses **line-oriented regex** rather than `tree-sitter-sql`:

- **Zero extra grammar dependency** — keeps `bundle-extended` lean
- **Predictable DDL coverage** — optimized for schema migration files
- **Fast** — no parse tree allocation for large migration histories

### Extracted constructs

- `CREATE TABLE` → `SymbolType::Table` with column fields
- `CREATE VIEW` / `CREATE OR REPLACE VIEW` → `Table` node (`metadata.kind = "view"`)
- `CREATE INDEX` → attached to parent table fields (`field_type = "INDEX"`)
- `REFERENCES` foreign keys → `RelationType::References` edges

### Example

```sql
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    email VARCHAR(255) NOT NULL
);

CREATE TABLE posts (
    id SERIAL PRIMARY KEY,
    user_id INTEGER REFERENCES users(id)
);

CREATE VIEW active_users AS SELECT id FROM users WHERE active;
CREATE UNIQUE INDEX users_email_idx ON users (email);
```

**Graph output:**

- Nodes: `users`, `posts`, `active_users` (`NodeType::Table`)
- Fields on `users`: `id`, `email`, `users_email_idx`
- Edge: `posts` → `users` (`References`)

### Future: tree-sitter-sql

`tree-sitter-sql` may be added later for advanced DDL (triggers, procedures, dialect-specific
syntax). The current regex plugin remains the default for common migration files.

## Dockerfile

Path-based routing matches files named `Dockerfile` (case-insensitive).

```dockerfile
FROM rust:1.75 AS builder
COPY Cargo.toml .
RUN cargo build --release
```

**Extracted:**

- `rust:1.75` → `Dependency`
- `Cargo.toml` → `Import` (COPY source)
- `run_2` → `BuildStep` (RUN command)

Relations link the Dockerfile file node to each dependency via `Uses`.

## CI/CD Pipelines

### GitHub Actions

Only files under `.github/workflows/` are routed to the GitHub Actions plugin (plain `.yml`
config files continue to use the YAML config handler).

```yaml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - run: cargo test
  build:
    needs: test
    runs-on: ubuntu-latest
```

**Extracted:** `test` and `build` jobs; `build` → `test` via `DependsOn`.

### GitLab CI

Matched by filename `.gitlab-ci.yml` at any directory level.

## Bash Scripts

Uses `tree-sitter-bash` for `function_definition` nodes and regex for `source` imports.

```bash
deploy() {
  echo "Deploying..."
}
source ./lib/common.sh
```

## Tier 2 Hybrid Extraction

Some tree-sitter languages use **supplemental regex patterns** in `languages.toml` when the
grammar does not expose top-level definitions (e.g. Elixir `def`/`defmodule`). The generic
tree-sitter plugin merges both sources without duplicates.

To extend a language:

```toml
[languages.elixir]
handler = "tree-sitter"
function_kinds = ["anonymous_function", "stab_clause"]

[[languages.elixir.regex_patterns]]
pattern = '(?m)^\s*defmodule\s+([A-Za-z_.][\w.]*)'
symbol_type = "class"
```

## Performance Benchmarks

Phase 11 includes automated polyglot benchmark tracking:

```bash
# CI threshold test (100 files, <120s)
cargo test --features bundle-extended --test phase11_polyglot_bench

# Local criterion benchmark
cargo bench --features bundle-extended --bench phase11_polyglot
```

The CI workflow (`language-bundles.yml`, `bundle-extra` matrix) runs
`phase11_polyglot_bench` on every push.

## Analyzing a Repository

```bash
cargo run --features bundle-extended -- analyze /path/to/repo
```

Multi-modal files are discovered automatically (including `.github/workflows/`).
