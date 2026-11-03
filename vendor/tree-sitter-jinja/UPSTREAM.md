# Vendored Jinja grammar

Source: https://github.com/cathaysia/tree-sitter-jinja
Tag: v0.13.0
Commit: c213d3745ccdcaaa858869181c7b1bf9557a025f
License: Apache-2.0 (see LICENSE).
Local grammar revision: see RGCTL_REVISION.

Only the Jinja grammar, generated C parser/scanner/headers, Rust binding,
shared JavaScript grammar inputs, and upstream corpus are included.
`upstream/` preserves the shared generator inputs from the repository root.
The Jinja grammar's require paths are relocated to these local copies.
The Rust dependency is constrained to rgctl's tree-sitter 0.25 series.
Local changes: optional scoped blocks and named endblock, multiple call-block
parameters, recursive binary operands, fielded postfix access/calls, qualified
filters/tests, comma-separated from-import bindings, Jinja boolean/null aliases,
signed unary expressions, hash lines treated as literal text by
default, and scanner state-length validation. Explicit line syntax is normalized
by rgctl's syntax adapter before grammar parsing.
Package metadata declares the included Rust binding and Jinja suffixes; editor
queries and other language bindings are omitted from this narrow vendor copy.
This third-party package is excluded from rgctl's lockstep workspace versions.

## Reproduce generated sources

Install tree-sitter-cli 0.25.10 as a maintainer tool, then run from this directory:

```sh
tree-sitter generate --abi 14
tree-sitter test
```

Cargo consumers build the checked-in parser.c/scanner.c with cc. They do not
need Node, tree-sitter-cli, Python, Jinja, or Ansible installed.
When changing a grammar input, regenerate, increment RGCTL_REVISION, and update
rgctl-lang-jinja/jinja-ast-coverage.json and its extraction fingerprint together.
