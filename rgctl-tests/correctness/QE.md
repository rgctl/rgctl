# QE sanity gates policy

rgctl QE for map collisions, graph correctness, and semantic search follows **option B**:

- Required checks **fail the suite** (non-zero exit / CI red) until the underlying bug is fixed.
- Do **not** downgrade a failing required check to `best_effort` / `#[ignore]` solely to green CI.
- **Do not test via `rgctl gql` (`gql` is deprecated):** All correctness and QE harnesses must assert via canonical structured JSON verbs (`rgctl -f json find`, `callers`, `callees`, `relations`, `inventory`, `blast-radius`, `inspect`, `slice`, `cpg`, `security`, `clones`, `semantic`, `review`) or typed Rust snapshot APIs — never Cypher-like `rgctl gql`.
- Product/engine fixes are **separate** changes when working in a pure QE lane, or fixed directly in engine crates when delivering end-to-end correctness fixes; never weaken assertions to mask product bugs.

## When a required check fails

1. Open a GitHub issue (do not “fix” the assertion away).
2. Leave the test required so maintainer-ordered CI stays red until the fix lands.
3. Link the issue from the test comment or fixture `note` field when helpful.

### Issue template

```markdown
Title: [QE] <short failure summary>

## Repro
```bash
cargo test --test <target> <filter>
# or
rgctl -r <repo> -f json <command>
```

## Expected
<what the oracle asserts>

## Actual
<output / panic / mismatch>

## Test pointer
- File: `<path>`
- Check / oracle id: `<id>`

## Notes
Pure QE find — fix in a separate PR.
```

## Suites

| Suite | Command |
|-------|---------|
| Map collisions | `cargo test --test map_collision_qe` |
| GraphBuilder collision unit | `cargo test -p rgctl-extraction qe_` — ambiguous FQN/suffix resolve fixed in [#27](https://github.com/rgctl/rgctl/issues/27) |
| Graph correctness | `cargo test --test graph_correctness` |
| Semantic search | `cargo test --test semantic_search_qe` |
| Cross-feature consistency | `cargo test --test cross_feature_qe` (CALLS ↔ blast ↔ CFG; analysis_results/macro blast caches may be empty on flat graphs — [#28](https://github.com/rgctl/rgctl/issues/28) won't-fix) |

## CI

These run as **named steps** in [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) when a maintainer adds the `ci` label or uses `workflow_dispatch`. There is no assumption of branch-protection required checks.

See also: [SCHEMA.md](./SCHEMA.md), OpenSpec change `qe-sanity-gates`.
