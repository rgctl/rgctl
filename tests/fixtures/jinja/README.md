# Structural Jinja fixtures

These sources exercise text-format-independent extraction. They are source
fixtures, not runnable deployments, and require no Python, Jinja, or Ansible
installation. Syntax, lexical binding, and deferred dependency assertions live in
the rgctl-lang-jinja integration tests.

The extractor distinguishes context inputs from macro parameters, loop bindings,
with bindings, and assignments. Macros and named blocks are Function graph nodes
with kind metadata; the template Module owns their structural definitions, and
the synthetic render Function owns top-level reads and calls.

Template and macro dependencies are resolved only under known template roots.
Runtime includes/lookups and unsupported syntax retain evidence and diagnostics.
CFG, PDG, taint, rendering, and application-specific loader execution are outside
the structural support contract.

Run the syntax/scope suite with:

    cargo test -p rgctl-lang-jinja --tests
