# Chef Support (Phase 17)

rBuilder analyzes Chef cookbooks through the **LanguagePlugin → pipeline → graph** path. Chef `.rb` files under cookbook directories are routed to the Chef plugin before the generic Ruby handler.

## What gets indexed

| Artifact | Node types | Edge types |
|----------|------------|------------|
| `metadata.rb` | `ChefCookbook` | `DependsOnCookbook` |
| `recipes/*.rb` | `ChefRecipe`, `ChefResource` | `DeclaresResource`, `IncludesRecipe`, `UsesTemplate`, `NotifiesResource` |
| `attributes/*.rb` | `ChefAttribute` | `DefinesAttribute` |
| `templates/*.erb` | `ChefTemplate` | `References` (attribute usage) |
| `resources/*.rb` | `ChefCustomResource` | — |

## Enable

```bash
cargo build --features bundle-extended
# or
cargo build --features lang-chef
```

## Index

```bash
rbuilder init ./my-chef-repo
```

## Query examples

```bash
rbuilder gql 'cookbooks'
rbuilder gql 'type:chefrecipe'
rbuilder gql 'resource:execute'
rbuilder gql 'chefrecipes'
```

## CLI

```bash
rbuilder chef cookbooks --path ./cookbooks --show-deps
rbuilder chef cookbooks --from-graph
rbuilder chef validate cookbooks/nginx/recipes/default.rb
rbuilder chef security-scan . --min-severity medium
```

## MCP tools

| Tool | Purpose |
|------|---------|
| `analyze_chef_cookbook` | Summarize cookbooks, recipes, resources in the graph |
| `find_chef_recipes` | List recipes and cookbook dependency order |
| `chef_security_scan` | Flag command injection, hardcoded secrets, insecure modes |

## Security checks

- **CWE-78** — unescaped `#{...}` in `execute` / `bash` / `script`
- **CWE-798** — hardcoded secrets in resource properties
- **CWE-732** — world-writable file modes (`0666`, `0777`)

## Architecture

Implementation lives in `src/languages/multimodal/chef/` as a `LanguagePlugin`. Path-based routing in `LanguageRegistry` sends cookbook Ruby/ERB to Chef before the Ruby plugin.
