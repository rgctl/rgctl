# Structural Ansible fixtures

These fixtures describe automation as source code and are not deployments.
They require no inventory service, Ansible installation, Python environment,
collection installation, or execution of lookup/filter plugins.

The static graph connects playbooks, plays, role/task sources, handlers, template
files, and variable/fact references. Named/unnamed tasks remain distinct across
block, rescue, and always sections. Inline variable definitions, registered
results, and set_fact keys are source declaration candidates, not calculated
runtime values or precedence winners.

Inventory YAML, group_vars, host_vars, and role defaults/vars continue through
generic configuration extraction. Dynamic role/template names, unavailable
runtime context, vault data, aliases/tags that cannot be resolved statically,
and externally supplied incompatible template profiles retain diagnostics or
unresolved evidence. Unsafe-tagged strings are literal.

Scalar mapping tests compare supported YAML decoding with serde_yaml as a test
oracle and verify original physical source positions for quoted, escaped,
folded/literal, CRLF, and UTF-8 inputs.

Run the source and scalar suite with:

    cargo test -p rgctl-lang-ansible --tests
