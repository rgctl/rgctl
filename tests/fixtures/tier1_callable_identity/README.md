# Tier 1 callable regressions

`inventory.json` must match `rgctl_ast_coverage::bundled_specs()` exactly. Each
entry points to real source for that language. The integration test in
`tests/tier1_callable_identity.rs` reads these files directly.

For callable languages, `11` and `21` are distinct body markers. Editing one
marker must change exactly one code hash without changing node IDs. Parameter
renames and line shifts must preserve IDs. The test also writes and reopens a
columnar snapshot for every callable fixture.

Class-based examples exercise inherited methods. Rust exercises trait ownership;
Elixir exercises clauses and guards; dynamic languages exercise runtime binding;
C includes a prototype and its implementation. Markdown, HCL, and ERB deliberately
have no Function nodes and document why overload tests do not apply.

Additional inline examples cover overload calls, generic arity, C++ function
pointer declarators and const receivers, Swift argument labels, C# ref parameters,
Kotlin extension receivers, SQL schema ownership, and repeated definitions.

Run from the repository root:

```sh
cargo +1.99.0 test --test tier1_callable_identity
```
