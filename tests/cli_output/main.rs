//! CLI JSON output schema sanity tests — Layer 1 (unit, no subprocess).
//!
//! Each module tests the typed serializer in `src/cli/<command>_output.rs` via
//! fixture builders (`fixture_*`, `build_*_response`). Fast serde-shape checks
//! that run without spawning the `rbuilder` binary.
//!
//! For subprocess coverage see:
//! - `subprocess_golden_path.rs` — discover + blast-radius golden paths
//! - `all_commands_sanity.rs` — all JSON commands in one audit loop
//!
//! Documentation: `docs/cli-io-sanity-audit.md`

mod blast_radius;
mod check;
mod discover;
mod gql;
mod inspect;
mod metrics;
mod slice;
mod uuid_resolution;
