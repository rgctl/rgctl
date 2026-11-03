//! Static Ansible source structure and shared Jinja template analysis.
//!
//! Plays, tasks, handlers, inline variables, role dependencies, notifications,
//! and template sources are extracted without executing automation. Embedded
//! scalars reuse the Jinja scope engine and map locations back to original YAML.
//! Imports/includes and syntax overrides retain typed deferred references.
//!
//! As in Puppet extraction, inventory resolution and effective runtime values
//! are outside this plugin. Inventory/group_vars/host_vars/role defaults remain
//! generic configuration data. Vault and unsupported tags/regions are diagnosed;
//! unsafe-tagged strings remain literal. No Python/Ansible runtime, CFG, PDG,
//! taint, collection downloads, or plugin execution is required.

use rgctl_plugin_api::{
    ComplexityMetrics, Error, ExtractAllResult, LanguageCapabilities, LanguagePlugin, Relation,
    Result, Symbol,
};
use std::path::Path;
mod coverage;
mod extract;
mod yaml;

/// The pinned grammar used by this extractor.
pub fn language() -> tree_sitter::Language {
    tree_sitter_yaml::LANGUAGE.into()
}

/// Structural language plugin.
#[derive(Debug, Default)]
pub struct AnsiblePlugin;

impl AnsiblePlugin {
    /// Parse a possible Ansible source, declining ordinary configuration YAML.
    ///
    /// # Errors
    /// Returns errors for invalid source encoding or parser failure.
    pub fn extract_candidate(
        &self,
        path: &Path,
        source: &[u8],
    ) -> Result<Option<ExtractAllResult>> {
        extract::extract(path, source, false)
    }
}

impl LanguagePlugin for AnsiblePlugin {
    fn declared_ast_handlers(&self) -> Option<&rgctl_plugin_api::AstHandlerTable> {
        Some(coverage::handlers())
    }
    fn cache_fingerprint(&self) -> Option<&[u8]> {
        Some(coverage::fingerprint())
    }
    fn extract_candidate(
        &self,
        file_path: &Path,
        source: &[u8],
    ) -> Result<Option<ExtractAllResult>> {
        extract::extract(file_path, source, false)
    }
    fn language_id(&self) -> &str {
        "ansible"
    }
    fn file_extensions(&self) -> Vec<&str> {
        Vec::new()
    }
    fn grammar(&self) -> Option<tree_sitter::Language> {
        Some(language())
    }
    fn capabilities(&self) -> LanguageCapabilities {
        LanguageCapabilities {
            extracts_functions: true,
            extracts_types: false,
            extracts_modules: true,
            extracts_relations: true,
            calculates_complexity: false,
            extracts_documentation: false,
            supports_incremental: false,
        }
    }
    fn extract_all(&self, file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
        extract::extract(file_path, source, true)?
            .ok_or_else(|| Error::UnsupportedLanguage("ansible".into()))
    }
    fn matches_path(&self, path: &str) -> bool {
        extract::matches_path(Path::new(path))
    }
    fn extract_symbols(&self, file_path: &Path, source: &[u8]) -> Result<Vec<Symbol>> {
        Ok(self.extract_all(file_path, source)?.symbols)
    }
    fn extract_relations(
        &self,
        file_path: &Path,
        source: &[u8],
        _symbols: &[Symbol],
    ) -> Result<Vec<Relation>> {
        Ok(self.extract_all(file_path, source)?.relations)
    }
    fn calculate_complexity(
        &self,
        _symbol: &Symbol,
        _source: &[u8],
    ) -> Result<Option<ComplexityMetrics>> {
        Ok(None)
    }
}

/// Register structural ansible extraction.
pub fn register(registry: &mut rgctl_registry::LanguageRegistry) {
    registry.register_language_plugin(std::sync::Arc::new(AnsiblePlugin));
}
