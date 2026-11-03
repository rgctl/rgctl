# Static Ansible support

The plugin exposes playbooks, plays, role identities, tasks, handlers and source
variable declarations using existing `Ansible*` node types. Task/block/rescue/
always sections retain their nesting and semantic positions without depending
on line numbers or claiming execution order. One pooled YAML parse feeds borrowed
mapping/sequence views and decoded scalar maps into the shared Jinja engine.

Builtin short and fully qualified action names, including literal action mappings,
preserve the original spelling. Unknown collection actions remain source facts.
`ExecutesTask`, `IncludesRole`, `DependsOnRole`, `IncludesPlaybook` and `DependsOn`
represent static execution/declaration/reuse dependencies. A builtin template task
uses `RendersTemplate` to the canonical local **File**; the remote `dest` remains
metadata. Imported Jinja macros use callable `Calls` edges. Dependency edges are
not included in call-only blast radius.

Resolution uses exact source/role/loader identities. A basename in another role
never substitutes for a missing local template. Unavailable caller-role stacks,
ambiguous reused-task contexts, dynamic paths, filtered targets and missing
handlers remain unresolved. Literal listen topics fan out in the known role/play
scope; duplicate shadowing names are ambiguous. Literal template-module syntax
settings are retained, with `external_profile_unresolved` when they differ from
the target's independently known profile. Per-invocation environment reparsing
is not claimed. Free-form module argument strings preserve action text; use
literal YAML/action mappings for statically resolved template parameters.

`vars`, `register`, `set_fact` and loop declarations are source candidates, not
winning inventory values. Bare/list `when`, `failed_when`, `changed_when`, and
`until` conditions share Jinja scope/expression handling with templated scalars.
Ansible magic/fact inputs retain their source attribution and original YAML
locations. `!unsafe` values stay literal; vault/unsupported tags and malformed
recognized sources retain diagnostics while independent files continue.

As with Puppet inventory support, inventory YAML, `group_vars`, `host_vars`, and
role defaults/vars stay on generic config extraction. No dynamic inventory,
lookup execution, collection loading, vault decryption, effective precedence,
CFG/PDG or taint execution is performed.

```toml
[languages.ansible]
include = ["automation/tasks/*.yml", "automation/extensionless"]
[languages.jinja]
include = ["web/templates/*.html"]
template_roots = ["templates"]
```

Associations are repository-relative globs. Protected manifests/workflows reject
association overrides. Conflicting Ansible/Jinja patterns and invalid/escaping
roots are errors. Selection is final on workers: `-l ansible` includes embedded
expressions, `-l jinja` standalone templates, and `-l ansible,jinja` both. `j2` and
`jinja2` alias Jinja; generic YAML is not dual-registered as a language plugin.

Compact scalar JSON-string properties preserve action/settings/reference and
diagnostic evidence after mmap reload. References report resolution, candidate
paths, resolved IDs and reasons (including unknown search context, dynamic,
missing/filtered and ambiguous targets). Node/edge discriminants and existing
JSON envelope versions remain unchanged. Path/settings-aware facts and the
atomic digest-bound `.rgctl/template-dependencies.bin` sidecar replay affected
referrers when targets or context change. Routing/syntax/language-selection
changes explicitly rebuild the graph.
