//! Single-pass AST coverage dispatch for Markdown (`KindDispatchTable` + `is_passive`).
//!
//! Semantic extraction remains in [`crate::extract`]; this module owns the
//! process-wide handler table / jump table and a DFS that gates on
//! [`HandlerMask::is_passive`] so `dispatch_lookup_tracked` records every visit
//! for `AstCoverageTracker::assert_all_exercised`.

use crate::extract::extract;
use crate::parse::parse_markdown;
use rgctl_plugin_api::{
    AstHandlerTable, ExtractAllResult, HandlerMask, KindDispatchTable, Result,
    dispatch_lookup_tracked, dispatch_table_from_manifest, handler_table_from_kinds,
};
use std::path::Path;
use std::sync::OnceLock;
use tree_sitter::Node;

const MANIFEST_JSON: &str = include_str!("../markdown-ast-coverage.json");

/// Plugin-handled AST kinds (source of truth for `declared_ast_handlers`).
/// Keep in sync with non-Literal active handlers in `markdown-ast-coverage.json`.
const HANDLED_KINDS: &[(&str, HandlerMask)] = &[
    ("atx_heading", HandlerMask::SYMBOL),
    ("fenced_code_block", HandlerMask::SYMBOL),
    ("indented_code_block", HandlerMask::SYMBOL),
    ("link_destination", HandlerMask::RELATION),
    ("link_reference_definition", HandlerMask::SYMBOL),
    ("setext_heading", HandlerMask::SYMBOL),
];

static HANDLERS: OnceLock<AstHandlerTable> = OnceLock::new();
static DISPATCH: OnceLock<KindDispatchTable> = OnceLock::new();

fn grammar() -> tree_sitter::Language {
    tree_sitter_md::LANGUAGE.into()
}

/// Process-wide AST handler table (coverage sync + introspection).
pub fn ast_handlers() -> &'static AstHandlerTable {
    HANDLERS.get_or_init(|| handler_table_from_kinds(&grammar(), HANDLED_KINDS))
}

fn dispatch() -> &'static KindDispatchTable {
    DISPATCH.get_or_init(|| dispatch_table_from_manifest(&grammar(), MANIFEST_JSON))
}

/// Walk the block AST with `mask.is_passive()` gating, then run semantic extract.
pub fn extract_all_single_pass(file_path: &Path, source: &[u8]) -> Result<ExtractAllResult> {
    let parsed = parse_markdown(source, file_path)?;
    walk_with_passive_gate(parsed.block.root_node());
    extract(&parsed, file_path, source)
}

fn walk_with_passive_gate(node: Node<'_>) {
    let mask = dispatch_lookup_tracked(dispatch(), node.kind_id());
    if mask.is_passive() {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_with_passive_gate(child);
        }
        return;
    }
    // Active kinds (Symbol / Relation): still recurse so nested actives
    // (`link_destination` under `link_reference_definition`) are visited.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_with_passive_gate(child);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rgctl_plugin_api::LanguagePlugin;
    use std::path::Path;

    #[test]
    fn handled_kinds_cover_active_manifest_entries() {
        let handlers = ast_handlers();
        for &(name, mask) in HANDLED_KINDS {
            let (_id, registered) = handlers
                .get_by_name(name)
                .unwrap_or_else(|| panic!("missing HANDLED_KINDS entry {name}"));
            assert_eq!(registered, mask, "{name}");
        }
    }

    #[test]
    fn single_pass_matches_extract_on_links() {
        let path = Path::new("docs/guide.md");
        let source = b"# Title\n\n[ADR](./adr.md)\n\n[pay]: ./pay.md\n";
        let via_sp = extract_all_single_pass(path, source).expect("single_pass");
        let plugin = crate::MarkdownPlugin::new().expect("plugin");
        let via_plugin = plugin.extract_all(path, source).expect("plugin");
        assert_eq!(via_sp.symbols.len(), via_plugin.symbols.len());
        assert_eq!(via_sp.relations.len(), via_plugin.relations.len());
    }
}
