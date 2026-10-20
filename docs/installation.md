# Installation

Everything you need to install rgctl (`rgctl`), choose the right operating mode, and verify the setup works.

**Already installed?** Jump to [Choose your operating mode](#choose-your-operating-mode) or the [User Guide](user-guide.md).

---

## Table of contents

1. [Prerequisites](#prerequisites)
2. [Install rgctl](#install-rgctl)
3. [Add to PATH](#add-to-path)
4. [Verify the installation](#verify-the-installation)
5. [Choose your operating mode](#choose-your-operating-mode)
6. [Install the agent pack](#install-the-agent-pack)
7. [Optional: semantic search setup](#optional-semantic-search-setup)
8. [Upgrading](#upgrading)
9. [Uninstalling](#uninstalling)
10. [Troubleshooting](#troubleshooting)
11. [Next steps](#next-steps)

---

## Prerequisites

| Requirement | Notes |
|-------------|-------|
| **OS** | macOS (Apple Silicon or Intel), Linux (x86_64), Windows (x86_64) |
| **Rust 1.99+** | Only for building from source ([rustup.rs](https://rustup.rs/)). Pre-built binaries need no Rust toolchain. |
| **Git** | For cloning the repository (source builds) |
| **Git LFS** | Optional. Only required if you use `semantic index --embedder code-daemon` (~206 MB ONNX weights). The default `vocab` embedder needs no LFS. |

Disk space: the `.rgctl/` artifacts directory typically uses 50-500 MB depending on repository size and enabled features.

---

## Install rgctl

### Option A -- GitHub release (recommended)

Pre-built binaries are published on the project **Releases** page:

**https://github.com/sshaaf/rgctl/releases**

1. Open the latest release.
2. Download the archive for your platform:

   | Platform | Asset name |
   |----------|------------|
   | macOS (Apple Silicon) | `rgctl-*-aarch64-apple-darwin.tar.gz` |
   | macOS (Intel) | `rgctl-*-x86_64-apple-darwin.tar.gz` |
   | Linux (x86_64) | `rgctl-*-x86_64-unknown-linux-gnu.tar.gz` |
   | Windows | `rgctl-*-x86_64-pc-windows-msvc.zip` |

   **Linux glibc note:** the `x86_64-unknown-linux-gnu` release is linked against a relatively new glibc (CI runner). It may **not** run on older LTS hosts such as **Ubuntu 22.04**. If `./rgctl` fails with `GLIBC_… not found`, build from source on that host (see Option B), or track a future musl/manylinux asset.

3. Extract the archive:

```bash
# macOS / Linux
tar -xzf rgctl-*-aarch64-apple-darwin.tar.gz
./rgctl --version
```

```powershell
# Windows (PowerShell)
Expand-Archive rgctl-*-x86_64-pc-windows-msvc.zip -DestinationPath .
.\rgctl.exe --version
```

### Option B -- Build from source

Requires **Rust 1.99+** (workspace `rust-version`; edition 2024). Check with `rustc --version`.

```bash
git clone https://github.com/sshaaf/rgctl.git
cd rgctl
cargo build --release --bin rgctl
./target/release/rgctl --version
```

All **Tier 1** languages registered in [`languages.toml`](languages.toml) (Rust, Python, Ruby, PHP, JavaScript, TypeScript, Go, Java, C#, C, C++) plus markdown are always included in the binary — no per-language feature flags.

The repository tracks **`Cargo.lock`** (binary workspace). Prefer `cargo build --locked` / `cargo check --locked` so dependency versions match CI. `ort` is pinned exactly while it remains on a `2.0.0-rc.*` train.

**Default features** include `semantic-onnx` (links `ort` / ONNX Runtime for the optional `code-daemon` embedder). If the build fails linking `ort-sys` or you do not need ONNX, build without default features (vocab embedder still works):

```bash
cargo build --release --bin rgctl --no-default-features
```

**Optional ONNX weights** (only for `--embedder code-daemon`):

```bash
git lfs pull   # ~206 MB; skip if using the default vocab embedder
```

**Optional Konveyor rulesets submodule** (only for `--with-kantra` embedded catalog from source; release binaries already include the compiled catalog):

```bash
git submodule update --init crates/rgctl-kantra/assets/rulesets
# or: ./scripts/init-kantra-rulesets.sh
```

Without the submodule, `cargo build` still succeeds using the in-repo fixture ruleset. See [`crates/rgctl-kantra/README.md`](../crates/rgctl-kantra/README.md).

---

## Add to PATH

### macOS / Linux -- user-local

```bash
mkdir -p ~/.local/bin
cp /path/to/rgctl ~/.local/bin/
chmod +x ~/.local/bin/rgctl
```

Add to `~/.zshrc` or `~/.bashrc`:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

Reload:

```bash
source ~/.zshrc   # or ~/.bashrc
```

### macOS / Linux -- system-wide

```bash
sudo cp /path/to/rgctl /usr/local/bin/
```

### Windows

1. Copy `rgctl.exe` to a folder such as `C:\Tools\rgctl\`.
2. Open **Settings > System > About > Advanced system settings > Environment Variables**.
3. Under **User variables**, edit `Path` and add `C:\Tools\rgctl`.
4. Open a new terminal.

### Per-project (no PATH change)

```bash
alias rgctl='/path/to/rgctl'
```

---

## Verify the installation

```bash
rgctl --version
```

Run a quick smoke test on any repository:

```bash
cd /path/to/any/repo
rgctl discover .
rgctl find --type function --limit 5
```

If both commands produce output without errors, the installation is working.

---

## Choose your operating mode

rgctl supports two operating modes:

### CLI (one-shot commands)

The default. Run `discover` once (writes `{repo}/.rgctl/`), then issue queries as separate processes.

```bash
rgctl discover .
rgctl -f json find --type function --limit 10 | jq '.returned'
rgctl -f json blast-radius MyFunction
```

**Best for:** CI/CD pipelines, shell scripts, IDE agents (spawn `rgctl -f json`), automation.

### HTTP server

A foreground HTTP server with an optional browser dashboard. Keeps the graph in memory for fast repeated queries.

```bash
rgctl serve --open              # starts on http://127.0.0.1:8080, opens browser
rgctl serve --port 3000         # custom port
rgctl serve --host 0.0.0.0     # bind all interfaces (team sharing)
rgctl serve --query-only        # API only, no dashboard
rgctl serve --no-pipeline       # serve existing artifacts, skip auto-pipeline
```

Query the API:

```bash
curl -s http://127.0.0.1:8080/api/status | jq .
curl -s -X POST http://127.0.0.1:8080/api/semantic/query \
  -H "Content-Type: application/json" \
  -d '{"query":"main entry","limit":5}'
```

**Best for:** repeated queries in one session, team exploration, agent integration over HTTP, visual dashboard browsing.

See the [HTTP Server and Dashboard guide](guides/http-server-and-dashboard.md) and [HTTP Server and Dashboard guide](guides/http-server-and-dashboard.md).

### Mode comparison

| | CLI | HTTP server |
|---|---|---|
| **Transport** | Process per command | HTTP `127.0.0.1:8080` |
| **Artifact location** | `{repo}/.rgctl/` | Same (reads in-repo artifacts) |
| **Dashboard** | No | Yes (optional) |
| **Auto pipeline** | No (manual `discover`) | Yes (unless `--no-pipeline`) |
| **Use case** | Agents, CI, scripts | Team, repeated queries, visual |
| **Output** | stdout (text or `-f json`) | HTTP JSON |

### Migrating from daemon cache

Background daemon mode is retired. Artifacts live only under `{repo}/.rgctl/`. If you still have files under `~/.rgctl/cache/{reponame}/.rgctl/`, copy that directory into the repo as `.rgctl/` manually (or re-run `rgctl discover .`).

---

## Install the agent pack

After `rgctl` is on your PATH, install the **embedded agent pack** into the **target repository** (same root you use for `discover`):

```bash
rgctl install --skill --tools cursor,claude,codex,antigravity,agents
rgctl -r /path/to/repo install --skill --tools cursor
rgctl install --list-agents
```

This copies from the binary (no network):

- **Skill `rgctl`** — `SKILL.md`, `references/` (including `workflows.md`)
- **Optional policy** — `--with-policy` (Cursor structural rule snippet)

Install no longer writes separate `rgctl-discover` / `rgctl-impact` / … skill directories.

Default **`--tools`** (omit flag) is **`cursor`, `claude`, `codex`, `agents`, `antigravity`**. Use **`--tools all`** for the full registry (~40 paths). Unknown tool ids warn on stderr; **`--global`** requires `supports_global: true` per agent. **`--host`** is deprecated. Use **`-g`** for a global install under your home directory. If a managed file differs from the bundle, the command exits **1** unless you pass **`--force`**.

Install does **not** run `discover` — index the repo separately (`rgctl discover .`).

**Full reference:** [Agent pack walkthrough](guides/agent-skill.md) · [USER_AGENTS_TEMPLATE](agents/USER_AGENTS_TEMPLATE.md) · [AGENTS.md](../AGENTS.md) (contribute to rgctl)

---

## Optional: semantic search setup

Semantic search is **not** part of `discover` -- it requires a separate indexing step:

```bash
rgctl semantic index                          # default vocab embedder (no LFS needed)
rgctl -f json semantic query "checkout flow"  # search
```

**Embedder options:**

| Embedder | Command | Requirements | Quality |
|----------|---------|-------------|---------|
| `vocab` (default) | `semantic index` | None | Good (compiled token table) |
| `hash` | `semantic index --embedder hash` | None | Fast, lower quality (CI/testing) |
| `code-daemon` | `semantic index --embedder code-daemon` | Git LFS (~206 MB ONNX) | Best |

For document section search: `semantic index --scope docs --embedder hash`.

See the [Semantic Search guide](guides/semantic-search.md).

---

## Upgrading

### From a release binary

Download the new release from [GitHub Releases](https://github.com/sshaaf/rgctl/releases) and replace the old binary:

```bash
cp /path/to/new/rgctl ~/.local/bin/rgctl
chmod +x ~/.local/bin/rgctl
rgctl --version
```

After upgrading, refresh the agent pack in each repository:

```bash
rgctl install --skill --tools cursor,claude --force
```

### From source

```bash
git pull
cargo build --release --bin rgctl
```

### Re-index after upgrading

The `.rgctl/` snapshot format may change between versions. Re-run `discover` after upgrading:

```bash
rgctl discover .
```

---

## Uninstalling

1. Remove the binary:

```bash
rm ~/.local/bin/rgctl          # or wherever you placed it
# Windows: delete rgctl.exe from C:\Tools\rgctl\
```

2. Remove cached artifacts (optional):

```bash
rm -rf /path/to/repo/.rgctl
# Legacy daemon cache (if present):
rm -rf ~/.rgctl/cache
```

3. Remove agent skill files (optional):

```bash
rm -rf /path/to/repo/.claude/skills/rgctl
rm -rf /path/to/repo/.agents/skills/rgctl
rm -rf /path/to/repo/.cursor/skills/rgctl
```

4. Remove the PATH entry from your shell profile if you added one.

---

## Troubleshooting

### `rgctl: command not found`

The binary is not on your PATH. Verify:

```bash
which rgctl          # macOS / Linux
where.exe rgctl      # Windows
```

If empty, revisit [Add to PATH](#add-to-path). For GUI apps (Cursor, VS Code), note that they may not inherit your shell PATH — use absolute paths in agent configs.

### `discover` fails or produces no output

- Ensure you are in a directory with source files in a [supported language](languages/README.md), or pass an explicit path: `rgctl discover /path/to/repo` or `cd repo && rgctl discover .`.
- **Do not** use `rgctl -r PATH discover .` from another cwd — the `.` ignores `-r` and indexes your shell directory instead.
- Check `rgctl --version` works first.
- Try verbose mode: `rgctl discover . -v`
- For detailed timing: `RUST_LOG=info rgctl discover . -v`

### Queries fail with "no graph found"

Run `discover` first on the repo you mean to query. Artifacts should appear at `{repo}/.rgctl/`. Old daemon caches under `~/.rgctl/cache/` can be copied into the repo by hand, or just rediscover.

### Slow `discover` on large repositories

Start with the default mode (no extra flags). Add `--with-cfg`, `--with-taint`, `--with-dashboard`, `--with-harmonic` only when you need those features. See [User Guide -- Troubleshooting](user-guide.md#18-troubleshooting) for tuning large repos.

### Build from source fails

- Confirm **Rust 1.99+** (workspace `rust-version`): `rustc --version`
- Update Rust: `rustup update`
- Clean build: `cargo clean && cargo build --release --bin rgctl`
- ONNX / `ort-sys` link errors: `cargo build --release --bin rgctl --no-default-features` (disables default `semantic-onnx`; default `vocab` semantic search still works)
- Linux binary from GitHub Releases fails with missing `GLIBC_*`: the gnu release needs a newer glibc than Ubuntu 22.04; build from source on the target host (musl/manylinux assets are a tracked follow-up)

---

## Next steps

| Goal | Where to go |
|------|-------------|
| Full CLI walkthrough | [User Guide](user-guide.md) |
| Concepts and architecture | [Introduction](Introduction.md) |
| Use rgctl with agents | [Agent pack](guides/agent-skill.md) · [USER_AGENTS_TEMPLATE](agents/USER_AGENTS_TEMPLATE.md) |
| Contribute to rgctl (agent README) | [AGENTS.md](../AGENTS.md) |
| Step-by-step feature guides | [Guides](guides/README.md) |
| JSON output reference | [JSON API](json-api.md) |
| HTTP API details | [HTTP Server and Dashboard](guides/http-server-and-dashboard.md) |
| Supported languages | [Languages](languages/README.md) |
