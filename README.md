# rgctl - Code knowledge graph for LLM agents.

[![Release](https://img.shields.io/github/v/release/sshaaf/rgctl?style=for-the-badge&logo=github&color=0ea5e9)](https://github.com/sshaaf/rgctl/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/sshaaf/rgctl/total?style=for-the-badge&logo=github&color=22c55e)](https://github.com/sshaaf/rgctl/releases)
[![Stars](https://img.shields.io/github/stars/sshaaf/rgctl?style=for-the-badge&logo=github)](https://github.com/sshaaf/rgctl/stargazers)
[![License: MIT](https://img.shields.io/badge/license-MIT-green?style=for-the-badge)](LICENSE)

[![Docs](https://img.shields.io/badge/docs-shaaf.dev%2Frgctl-2563eb?style=flat-square&logo=readthedocs&logoColor=white)](https://shaaf.dev/rgctl)
[![Website](https://img.shields.io/github/actions/workflow/status/sshaaf/rgctl/website.yml?branch=main&style=flat-square&label=website)](https://shaaf.dev/rgctl)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Platforms](https://img.shields.io/badge/platform-macOS%20%7C%20Linux%20%7C%20Windows-555?style=flat-square)](https://github.com/sshaaf/rgctl/releases/latest)
[![tree-sitter](https://img.shields.io/badge/parser-tree--sitter-brightgreen?style=flat-square)](https://tree-sitter.github.io/tree-sitter/)
[![Agents](https://img.shields.io/badge/agents-Cursor%20%7C%20Claude%20%7C%20Codex-111827?style=flat-square)](https://shaaf.dev/rgctl/docs/guides/agent-skill/)
[![Tier 1](https://img.shields.io/badge/languages-14%20Tier%201-8b5cf6?style=flat-square)](https://shaaf.dev/rgctl/docs/languages/)

[![C](https://img.shields.io/badge/C-A8B9CC?style=flat-square&logo=c&logoColor=black)](docs/languages/README.md)
[![C++](https://img.shields.io/badge/C%2B%2B-00599C?style=flat-square&logo=cplusplus&logoColor=white)](docs/languages/README.md)
[![C#](https://img.shields.io/badge/C%23-512BD4?style=flat-square&logo=csharp&logoColor=white)](docs/languages/README.md)
[![Go](https://img.shields.io/badge/Go-00ADD8?style=flat-square&logo=go&logoColor=white)](docs/languages/README.md)
[![Groovy](https://img.shields.io/badge/Groovy-4298B8?style=flat-square&logo=apachegroovy&logoColor=white)](docs/languages/README.md)
[![Java](https://img.shields.io/badge/Java-ED8B00?style=flat-square&logo=openjdk&logoColor=white)](docs/languages/README.md)
[![JavaScript](https://img.shields.io/badge/JavaScript-F7DF1E?style=flat-square&logo=javascript&logoColor=black)](docs/languages/README.md)
[![Kotlin](https://img.shields.io/badge/Kotlin-7F52FF?style=flat-square&logo=kotlin&logoColor=white)](docs/languages/README.md)
[![PHP](https://img.shields.io/badge/PHP-777BB4?style=flat-square&logo=php&logoColor=white)](docs/languages/README.md)
[![Python](https://img.shields.io/badge/Python-3776AB?style=flat-square&logo=python&logoColor=white)](docs/languages/README.md)
[![Ruby](https://img.shields.io/badge/Ruby-CC342D?style=flat-square&logo=ruby&logoColor=white)](docs/languages/README.md)
[![Rust](https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust&logoColor=white)](docs/languages/README.md)
[![TypeScript](https://img.shields.io/badge/TypeScript-3178C6?style=flat-square&logo=typescript&logoColor=white)](docs/languages/README.md)
[![Puppet](https://img.shields.io/badge/Puppet-FFAE1A?style=flat-square&logo=puppet&logoColor=black)](docs/languages/README.md)
[![Markdown](https://img.shields.io/badge/Markdown-000000?style=flat-square&logo=markdown&logoColor=white)](docs/guides/markdown-context-graph.md)

> Index once (`discover`), then ask callers, impact, communities, and slices — compact deterministic JSON for agents, not grepping the tree.

**What the R stands for:** **R**ust · **R**eachability · **R**ich graph (30+ typed relations).

### Without `rgctl` vs with `rgctl`

| | Without (grep / dump files) | With `rgctl` |
|--|----------------------------|--------------|
| Tool calls for “who calls X transitively?” | Many greps + ad-hoc reads | 1–2 (`discover` once, then `callers` / `blast-radius`) |
| Context tokens | Whole files / search dumps into the prompt | Compact `-f json` rows (`schema_version`) |
| Transitive reachability | Easy to miss / invent | Deterministic typed graph + blast-radius |

### Battle-tested at scale

Cold discover and mmap queries on large corpora (see [AGENTS.md](AGENTS.md) Gate A/B): **Linux kernel**, **Kubernetes**, **LLVM**, **Roslyn**, **rustc**, and more — **14 Tier-1 languages**, zero-hydration snapshot reads for structured queries.

```bash
rgctl discover .
rgctl -f json find --type function --limit 20
rgctl -f json callers MyService --depth 1
rgctl -f json blast-radius MyService

# Use with your favorite LLM agent
rgctl install --skill --tools cursor,claude,codex,agents
```

https://github.com/user-attachments/assets/15ec6d91-f716-4cbd-a873-e982ba3c6dca

---

## Try it (5 minutes)

### 1. Install

**One-line release binary** (macOS / Linux):

```bash
curl -fsSL https://shaaf.dev/rgctl/install.sh | sh
rgctl --version
```

Installs to `${RGCTL_INSTALL_DIR:-$HOME/.local/bin}`. Or download a tarball from  
[GitHub Releases](https://github.com/sshaaf/rgctl/releases/latest) and put `rgctl` on your `PATH`.

**Or build from source** (Rust **1.85+**):

```bash
git clone https://github.com/sshaaf/rgctl.git
cd rgctl
cargo build --release --bin rgctl
# If ort/ONNX link fails: add --no-default-features
export PATH="$PWD/target/release:$PATH"
```

Details, PATH, and troubleshooting: **[Installation](docs/installation.md)**.

### 2. Index the in-tree demo

```bash
cd rgctl-tests/ecommerce-java   # from this repo, or any project you care about
rgctl discover . --with-cfg
```

Artifacts land in `{repo}/.rgctl/`. Re-run `discover` after large code changes.

### 3. Ask the graph

```bash
# Inventory
rgctl -f json inventory --by type
rgctl -f json find --type function --limit 10

# Callers / callees
rgctl -f json callers ProductService --depth 1
rgctl -f json relations --edge calls --limit 20

# Impact before you edit a symbol
rgctl -f json blast-radius ProductService
```

Always prefer **`-f json`** for agents and scripts ([JSON API](docs/json-api.md)). Do not scrape stderr.

---

## Use with coding agents

Install the bundled pack (skills) into your IDE tooling:

```bash
rgctl install --skill --tools cursor,claude,codex,agents
```

Then: **discover once → query with `-f json`**. See [Agent pack](docs/guides/agent-skill.md).  
For *your* application repo, optionally paste [USER_AGENTS_TEMPLATE.md](docs/agents/USER_AGENTS_TEMPLATE.md) as `AGENTS.md`.

---

## What it does

| You need… | Command |
|-----------|---------|
| Build the graph | `discover` |
| Find symbols / inventory | `find`, `inventory`, `status` |
| Callers / edges | `callers`, `callees`, `relations` |
| “What breaks if I change X?” | `blast-radius` |
| CFG / data-flow / taint | `slice`, `inspect`, `cpg`, `taint` (need `discover --with-cfg`) |
| OSV / deps / OpenVEX | `vuln triage`, `deps check`, `vuln analyze` |
| Hotspots / clusters | `metrics`, `communities` |
| NL search over functions | `semantic` (opt-in index) |
| CI gates | `check`, `pr-check` |
| Snapshot compare | `diff` |
| Browser UI + HTTP API | `discover --with-dashboard` then `serve` |

Step-by-step feature guides (CoolStore): **[docs/guides](docs/guides/README.md)**.  
Concepts: **[Introduction](docs/Introduction.md)**. Full CLI walkthrough: **[User Guide](docs/user-guide.md)**.

---

## Languages

Tier 1 plugins: **C, C++, C#, Go, Groovy, Java, JavaScript, Kotlin, PHP, Puppet, Python, Ruby, Rust, TypeScript**, plus **markdown**.

Support matrix is generated from `*-ast-coverage.json` — see [Languages](docs/languages/README.md).

---

## Docs

| Doc | For |
|-----|-----|
| [Installation](docs/installation.md) | Install, verify, PATH |
| [Introduction](docs/Introduction.md) | What / why / capability map |
| [Guides](docs/guides/README.md) | Feature how-tos |
| [User Guide](docs/user-guide.md) | ecommerce-java + every command |
| [JSON API](docs/json-api.md) | `-f json` shapes |
| [Docs index](docs/README.md) | Full map |
| [AGENTS.md](AGENTS.md) | Contributing to *this* repo |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Dev setup / PRs |
| [Latest release](docs/releases/v0.4.19.md) | Changelog |

---

## License

MIT — see [LICENSE](LICENSE).
