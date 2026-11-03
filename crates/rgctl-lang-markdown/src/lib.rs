//! Markdown language support via tree-sitter-md.

#[cfg(test)]
mod ast_coverage;
mod extract;
mod parse;
mod plugin;
mod single_pass;
mod slug;

use rgctl_registry::LanguageRegistry;
use std::sync::Arc;

pub use plugin::MarkdownPlugin;
pub use plugin::MarkdownPlugin as Plugin;

/// Process-wide AST handler table for coverage sync.
pub use single_pass::ast_handlers;

/// Register the markdown language plugin.
#[allow(clippy::expect_used, clippy::missing_panics_doc)]
pub fn register(registry: &mut LanguageRegistry) {
    registry.register_language_plugin(Arc::new(
        MarkdownPlugin::new().expect("init MarkdownPlugin"),
    ));
}
