# Structural Jinja support

`JinjaPlugin` parses full templates and bare expressions through a pooled parser
and a source-mapped syntax adapter. It supports standard suffixes (`j2`, `jinja`,
`jinja2`), recognized Ansible role template directories, and explicit associations.
Ordinary HTML/SQL and alternate dialects such as ERB retain their existing routes.

A template root is a `Module`. Each template adds a synthetic `<render>`
`Function`; macros and named blocks are also `Function` nodes, distinguished by
`kind=jinja_render`, `jinja_macro`, and `jinja_block`. Function inventory counts
therefore include render scopes and blocks. Macro parameters, loop/with/caller
bindings, assignments and import aliases have lexical scope. Context inputs are
`Variable` nodes; `ansible_facts` uses `UsesFact` rather than Puppet node kinds.

Exact macro invocations produce `Calls` only to callable definitions in their
lexical template or a specifically imported template. A macro declaration is not
a call. Filters/tests/lookups retain qualified names and argument evidence; no
Python functions or external plugins are fabricated. Underscore-prefixed/nested
macros are not exported across templates. `caller`, `super` and environment
builtins do not become guessed project calls.

Loader dependencies use `Extends` between template Modules, `DependsOn` for
include, and `Uses` for import. Ordered include lists choose the first eligible
candidate under the known loader roots. Configured roots and recognized role
roots supply context; a generic suffix alone does not imply a relative loader.
Dependency edges do not expand call-only blast radius.

The `rgctl.toml` settings are optional:

```toml
[languages.jinja]
include = ["web/templates/*.html", "templates/extensionless"]
template_roots = ["templates"]
[languages.jinja.syntax]
variable_start_string = "[["
variable_end_string = "]]"
# line_statement_prefix = "#"  # disabled unless explicitly configured
```

Valid literal `#jinja2:` headers override project defaults. Quoted/raw/comment
regions and unequal-length delimiters retain original UTF-8/CRLF locations.
Invalid profiles produce diagnostic roots. Malformed/unsupported regions are
partial; independently valid declarations survive. `source_references_json`,
`source_diagnostics_json`, `expression_facts_json` and `syntax_profile_json` are
scalar JSON-string node properties, so they survive columnar mmap reload and
remain available to existing graph-property queries without changing envelopes.

References preserve raw targets, modifiers, candidates, resolved IDs and explicit
reasons such as `dynamic_target`, `unknown_loader_root`,
`missing_or_filtered_target`, `missing_or_inaccessible_macro` and
`ambiguous_macro`. No runtime rendering, custom loaders/extensions, CFG/PDG,
taint or Python execution is claimed. Grammar provenance and regeneration are in
[`vendor/tree-sitter-jinja/UPSTREAM.md`](../../vendor/tree-sitter-jinja/UPSTREAM.md).
