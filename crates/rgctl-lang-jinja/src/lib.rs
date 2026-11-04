//! Structural Jinja template and expression extraction.
//!
//! Named macros and blocks are callable Function symbols with distinguishing
//! kind metadata; template roots are Modules and render functions are synthetic.
//! Loop, with, macro-parameter, import, and assignment scopes are explicit.
//! Project template dependencies and macro invocations are typed deferred source
//! references for exact repository resolution, never fuzzy Python call guesses.
//!
//! Parser profiles honor literal file headers and explicit caller defaults.
//! Runtime loaders/plugins, rendering, inventory precedence, CFG/PDG, and taint
//! execution are unsupported. Unsupported syntax is reported as partial output,
//! and error regions do not produce confident source dependencies.

use rgctl_plugin_api::{
    AstHandlerTable, ComplexityMetrics, ExtractAllResult, LanguageCapabilities, LanguagePlugin,
    Relation, Result, Symbol,
};
use std::path::Path;

mod syntax;
pub use syntax::{ParsedTemplate, SyntaxProfile, parse_expression, parse_fragment, parse_template};
mod expression;
pub use expression::{
    ExpressionFacts, Invocation, InvocationKind, VariableAccess, expression_facts,
};
mod coverage;
mod extract;
pub use extract::extract_parsed_template;

/// The pinned grammar used by this extractor.
pub fn language() -> tree_sitter::Language {
    tree_sitter_jinja::language()
}

/// Structural language plugin.
#[derive(Debug, Default)]
pub struct JinjaPlugin {
    syntax: SyntaxProfile,
}

impl JinjaPlugin {
    /// Construct a structural extractor with explicit project syntax defaults.
    pub fn with_syntax(syntax: SyntaxProfile) -> Self {
        Self { syntax }
    }
}

impl LanguagePlugin for JinjaPlugin {
    fn extract_with_syntax(
        &self,
        file_path: &Path,
        source: &[u8],
        syntax: &rgctl_plugin_api::JinjaSyntax,
    ) -> Result<ExtractAllResult> {
        extract::extract(file_path, source, syntax)
    }
    fn language_id(&self) -> &str {
        "jinja"
    }
    fn file_extensions(&self) -> Vec<&str> {
        vec!["j2", "jinja", "jinja2"]
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
        extract::extract(file_path, source, &self.syntax)
    }
    fn declared_ast_handlers(&self) -> Option<&AstHandlerTable> {
        Some(coverage::handlers())
    }
    fn cache_fingerprint(&self) -> Option<&[u8]> {
        Some(coverage::fingerprint())
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

/// Register structural jinja extraction.
pub fn register(registry: &mut rgctl_registry::LanguageRegistry) {
    registry.register_language_plugin(std::sync::Arc::new(JinjaPlugin::default()));
}
