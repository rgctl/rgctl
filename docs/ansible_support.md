# Ansible Support (Phase 16)

rBuilder analyzes Ansible playbooks, roles, variables, and templates through the same **LanguagePlugin → pipeline → graph** path as GitHub Actions and other multi-modal plugins. No separate extraction pipeline is required.

## What gets indexed

| Artifact | Node types | Edge types |
|----------|------------|------------|
| Playbooks (`playbooks/`, `site.yml`) | `AnsiblePlaybook`, `AnsiblePlay`, `AnsibleTask`, `AnsibleHandler` | `ExecutesTask`, `IncludesRole`, `NotifiesHandler`, `IncludesPlaybook` |
| Roles (`roles/*/meta/main.yml`) | `AnsibleRole` | `DependsOnRole` |
| Role tasks/handlers | `AnsibleTask`, `AnsibleHandler` | `ExecutesTask`, `RendersTemplate`, `UsesVariable` |
| `group_vars/` / `host_vars/` | `AnsibleVariable` | `Defines` |
| Jinja2 templates (`.j2`) | `AnsibleTemplate`, `AnsibleVariable` | `UsesVariable` |

Ansible YAML under `.github/workflows/` and GitLab CI files are **not** routed to the Ansible plugin.

## Enable the plugin

Ansible is included in `bundle-extended` and above:

```bash
cargo build --features bundle-extended
```

Or enable only Ansible:

```bash
cargo build --features lang-ansible
```

## Index a repository

```bash
rbuilder init ./my-ansible-project
```

## Query examples

```bash
rbuilder gql 'type:ansibletask'
rbuilder gql 'module:shell'
rbuilder gql 'playbooks'
rbuilder gql 'ansibleroles'
```

Compound queries:

```bash
rbuilder gql 'type:ansibletask AND module:shell'
```

## CLI

```bash
# Role dependency report (filesystem or graph)
rbuilder ansible roles --path ./roles --show-deps
rbuilder ansible roles --from-graph

# Validate playbooks
rbuilder ansible validate playbooks/site.yml

# Security scan (graph-backed when indexed)
rbuilder ansible security-scan . --min-severity medium
```

## MCP tools

| Tool | Purpose |
|------|---------|
| `analyze_ansible_playbook` | Summarize playbooks, plays, tasks, roles in the graph |
| `find_ansible_roles` | List roles and topological dependency order |
| `ansible_security_scan` | Flag shell injection, hardcoded secrets, missing `no_log`, etc. |

## Security checks

The Ansible security scanner inspects indexed `AnsibleTask` / `AnsibleHandler` nodes for:

- **CWE-78** — unquoted Jinja2 in `shell` / `command` modules
- **CWE-798** — hardcoded secrets in task arguments
- **CWE-250** — unnecessary `become: true`
- **CWE-532** — sensitive modules without `no_log`

## Architecture note

The implementation lives in `src/languages/multimodal/ansible/` as a `LanguagePlugin`. Path-based routing in `LanguageRegistry` sends Ansible YAML to this plugin before the generic YAML config handler, matching the GitHub Actions pattern.
