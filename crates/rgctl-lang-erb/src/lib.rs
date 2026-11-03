//! ERB (Embedded Ruby) template language plugin for rgctl.
//!
//! Extracts variable references, fact lookups, scope accesses, and control flow
//! from `.erb` templates. Designed primarily for Puppet ERB templates but works
//! with any ERB file.
//!
//! Each ERB block (`<%= %>`, `<% %>`, `<%# %>`) is analyzed to extract:
//! - `@variable` references → `PuppetVariable` symbols
//! - `@facts[...]` references → `PuppetFact` symbols with `UsesFact` edges
//! - `scope['class::param']` → `References` edges to Puppet classes
//! - Control flow constructs (`if`, `each`, `unless`) for complexity metrics
//!
//! Block metadata includes a `translation_tier` (1–4) and, for tiers 1–3,
//! a `jinja2_pattern` hint for downstream ERB→Jinja2 translation.
//!
//! ## Honesty limits (Layer A extraction)
//!
//! - Ruby inside `<% %>` blocks is parsed with regex, not `tree-sitter-ruby`.
//!   Method calls like `@facts.dig(...)` / `@facts.get('os')` are not captured.
//!   Non-`@` local variables and `require` statements produce no graph edges.
//! - `<%graphql %>` directives emit a Property-tagged symbol with GraphQL body
//!   metadata (no Ruby variable / fact / scope edges).
//! - `.epp` (Puppet EPP) templates are out of scope; a future `rgctl-lang-epp`
//!   would cover those.
//! - `content` / `template` are SkipWrapper containers; no layout structure.
//! - No CFG/taint/Layer F analysis — `enable_complexity = false` in `languages.toml`.

use rgctl_registry::LanguageRegistry;
use std::sync::Arc;

#[cfg(test)]
mod ast_coverage;
mod plugin;
mod single_pass;
pub use plugin::ErbPlugin;

/// Register the ERB language plugin.
#[allow(clippy::expect_used, clippy::missing_panics_doc)]
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(ErbPlugin::new().expect("init ErbPlugin")));
}
