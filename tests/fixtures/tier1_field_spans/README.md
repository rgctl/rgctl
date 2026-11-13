# Tier 1 field-span contracts

The inventory is checked against every registered Tier 1 language. The integration matrix materializes actual fields/properties/columns, verifies name bytes through mmap snapshots, and checks stable IDs, edges, and semantic diffs after blank-line shifts. Bash, HCL, Markdown, and ERB have explicit non-member applicability contracts.

Field-backed plugins carry typed source spans on `Field`; direct symbol producers retain ordinary symbol spans. Lua covers named table keys and Elixir covers declared struct fields. Computed runtime keys and external type inference are outside this source-position contract.
