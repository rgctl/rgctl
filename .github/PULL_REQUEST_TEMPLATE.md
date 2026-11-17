## Description

Brief description of what this PR does.

Fixes #(issue number)

## Type of Change

- [ ] Bug fix (non-breaking change which fixes an issue)
- [ ] New feature (non-breaking change which adds functionality)
- [ ] Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] Performance improvement
- [ ] Documentation update
- [ ] Language support (new language plugin)
- [ ] Refactoring (no functional changes)

## Changes Made

List the main changes:

- Change 1
- Change 2
- Change 3

## Testing

How has this been tested?

- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Tested with `cargo test`
- [ ] Manual testing performed

**Test environment:**
- OS: (e.g., macOS, Linux, Windows)
- Rust version: (output of `rustc --version`)

## Checklist

- [ ] `./scripts/preflight-pr.sh` passes (formatting, `RGCTL_AST_COVERAGE_STRICT=1`, clippy `--all-targets -- -D warnings`, unit & core integration tests)
- [ ] Reviewed diff against the **[PR Review & Coding Practices Checklist](../docs/internal/pr-review-checklist.md)** (no `serde_json::Map` on hot `Symbol`/`Relation` paths, no `O(calls × symbols)` scans, streaming `--with-cfg`, bounded AST/CFG recursion, no silent `limit: Some(N)` correctness caps)
- [ ] I have added tests that prove my fix/feature works (using canonical `-f json` verbs / Rust snapshot APIs — never `rgctl gql`)
- [ ] Documentation updated and all CLI flags/examples verified (if applicable)
- [ ] CONTRIBUTING.md guidelines followed (no AI commit trailers)
- [ ] All commits are **signed** and include **DCO sign-off** (`Signed-off-by:`) — see [Developer Certificate of Origin](https://developercertificate.org/) and [contributor checklist §6](../docs/contributor-checklist.md#6-documentation--pr)

## Performance Impact

If this PR touches extraction, `GraphBuilder`, `discover`, `--with-cfg`, or query engines:

- [ ] Cold `[profile]` run (`cargo build --release --bin rgctl` + fresh `rm -rf .rgctl/`) verified against [docs/internal/profile.md](../docs/internal/profile.md) / `tests/cold_profile_gates.rs`
- [ ] `[profile]` stage accounting holds (`|wall_secs - accounted_secs| < 0.5 s`, `cargo test --test profile_stage_accounting`)
- [ ] No measurable wall-time or peak-RSS regression (attach before/after `[profile] discover summary` if significant)

## Breaking Changes

If this is a breaking change:

**What breaks:**
- Describe what existing functionality is affected

**Migration path:**
- How should users update their code/configuration?

## Screenshots (if applicable)

Add screenshots for UI changes or visual improvements.

## Additional Notes

Add any additional context about the PR here.
