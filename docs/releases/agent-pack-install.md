# Agent pack install

Multi-tool agent pack: core router skill `rgctl`, on-demand command skills (`rgctl-review`, `rgctl-vuln`, `rgctl-migrate`), optional structural policy, and a registry of ~40 agent adapters.

## Summary

| Area | Change |
|------|--------|
| **Install** | `rgctl install --skill` installs the core `rgctl` skill (plus `references/workflows.md`) and three workflow skills (`rgctl-review`, `rgctl-vuln`, `rgctl-migrate`) along with host-native slash commands (`/rgctl:review`, `/rgctl:vuln`, `/rgctl:migrate` or `rgctl-*.md`). |
| **Targeting** | `--tools cursor,claude` or `--tools all`. **`--host` is deprecated** (warning only; use `--tools`). |
| **Scope** | `-g` / `--global` for user-level agent dirs. |
| **Policy** | `--with-policy` installs Cursor structural rule snippet. |
| **Discovery** | `install --list-agents` prints `agent-pack/agents/registry.toml` entries. |
| **JSON** | `install -f json` uses **`schema_version`: 3** (`agent`, `workflow`, `kind`, `scope`, …). |
| **Workflow docs** | `skills/rgctl/references/workflows.md` is copied into the pack at build. |
| **Embed** | Agent pack generated in `build.rs`, zipped into the binary (replaces single-tree `include_dir` for install). |

## Upgrade

```bash
rgctl install --skill --tools cursor,claude --force
```

## Related

- [Agent pack walkthrough](https://rgctl.io/docs/guides/agent-skill/)
- GitHub [#84](https://github.com/rgctl/rgctl/issues/84) · OpenSpec change `agent-pack-install`.
