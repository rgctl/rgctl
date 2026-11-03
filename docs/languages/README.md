# Languages

Per-language markdown guides here were removed. **Single source of truth** for what each Tier 1 and Tier 2 plugin handles:

| Artifact | Role |
|----------|------|
| `crates/rgctl-lang-*/{id}-ast-coverage.json` | Tier 1 (`handler = "custom"`): named tree-sitter kinds → handlers (`Symbol`, `Relation`, `CfgStatement`, `AstSkeleton`, `Property`, `Literal`, `SkipWrapper`, `SkipTypeToken`, `Unimplemented`; raw `"Skip"` banned; ≥98% `A+`) + grammar pin |
| `crates/rgctl-lang-*/queries/tags.scm` | Tier 2 (`handler = "queries"`): declarative S-expression queries (`@definition.*`, `@reference.call`, `@reference.class`, `@reference.implementation`, `@import.path`, `@name`, `@doc`) compiled by `QueryLanguagePlugin` |
| [`languages.toml`](../../languages.toml) | Extensions, aliases, `function_kinds`, `class_kinds`, plugin / grammar crate metadata |

The **website** ([rgctl-website](https://github.com/rgctl/rgctl-website)) builds `/docs/languages/` (and `/docs/languages/{id}/`) for both Tier 1 (`*-ast-coverage.json`) and Tier 2 (`queries/tags.scm`) languages at `prebuild` / `predev` via `scripts/copy-lang-coverage.mjs` (checks out this repo as `RGCTL_REPO`). Do not reintroduce hand-written coverage tables here.

## Related

- [Tier 1 language support](../tier-1-language-support.md) — Layers A–F contributor bar (`handler = "custom"`)
- [Tier 2 language support](../tier-2-language-support.md) — Declarative `tags.scm` query plugins (`handler = "queries"`)
- [Markdown context](../markdown-context.md) — doc markup plugin (also has a coverage JSON)
- Honesty notes (where present): `docs/*-extract-honesty.md`
- Langfeature / structured-query smoke harnesses under `rgctl-tests/` (per-language ecommerce fixtures).
