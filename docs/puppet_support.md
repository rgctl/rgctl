# Puppet Support (Phase 18)

Puppet manifest and module analysis integrates through the **LanguagePlugin** pipeline (same architecture as Ansible and Chef). Puppet `.pp` manifests and `metadata.json` files are parsed with a regex-based DSL parser and indexed into the knowledge graph.

## Supported artifacts

| File pattern | Node types | Edge types |
|--------------|------------|------------|
| `modules/*/metadata.json` | `PuppetModule` | `DependsOnModule` |
| `modules/*/manifests/*.pp` | `PuppetClass`, `PuppetDefinedType`, `PuppetResource`, `PuppetVariable`, `PuppetFact` | `Defines`, `IncludesClass`, `InheritsClass`, `DeclaresResource`, `NotifiesResource`, `RequiresResource`, `UsesFact` |
| `site.pp`, environment manifests | Same as above | Same as above |

## Build

```bash
cargo build --features lang-puppet
# or with the extended bundle (default)
cargo build --features bundle-extended
```

## CLI

```bash
# Module dependency graph
rbuilder puppet modules ./modules --show-deps

# Validate manifests
rbuilder puppet validate modules/nginx/manifests/init.pp

# Security scan (hardcoded secrets, exec injection, file modes)
rbuilder puppet security-scan ./modules --min-severity medium
```

Use `--from-graph` to read from an indexed `.rbuilder` cache instead of re-parsing files.

## Graph queries

```bash
rbuilder query "type:PuppetModule"
rbuilder query "type:PuppetClass"
rbuilder query "type:PuppetResource"
rbuilder query "puppetmodules"
rbuilder query "puppetclasses"
```

## MCP tools

When running with `mcp-server`:

- `analyze_puppet_module` — summarize modules, classes, and resources
- `find_puppet_classes` — list classes with module dependency order
- `puppet_security_scan` — scan indexed Puppet resources for CWE patterns

## Security checks

The Puppet security scanner inspects `PuppetResource` nodes for:

- **CWE-798** — hardcoded secrets in resource attributes
- **CWE-78** — command injection in `exec` resources with unquoted variables
- **CWE-732** — world-writable file modes (`0666`, `0777`)

## Architecture

```
.pp / metadata.json
       ↓
PuppetPlugin (LanguagePlugin)
       ↓
GraphBuilder → MemoryBackend
       ↓
analysis/puppet_modules.rs | security/puppet.rs | MCP tools
```

Path routing in `LanguageRegistry` sends Puppet paths to the Puppet plugin before generic handlers. Puppet `metadata.json` under `modules/` is excluded from the JSON config plugin.

## Tests

```bash
cargo test --features bundle-extended,mcp-server --test phase18_puppet
```
