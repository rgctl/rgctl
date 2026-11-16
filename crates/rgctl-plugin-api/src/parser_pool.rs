//! Thread-local `tree_sitter::Parser` pool for Rayon extract / CFG workers.
//!
//! Avoids `Parser::new()` + grammar setup per file (and per function in legacy
//! complexity paths). One parser map is kept per OS thread.

use std::cell::RefCell;
use std::collections::HashMap;
use tree_sitter::{Language, Parser, Tree};

struct PooledParser {
    parser: Parser,
    /// Grammar currently configured on `parser` (guards against key collisions like `"unknown"`).
    grammar: Option<Language>,
}

thread_local! {
    static PARSERS: RefCell<HashMap<String, PooledParser>> = RefCell::new(HashMap::new());
    static CAPTURE_LAST_TREE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static LAST_PARSED_TREE: RefCell<Option<(usize, usize, Tree)>> = const { RefCell::new(None) };
}

/// Enable or disable capturing the most recent `Tree` parsed on this worker thread
/// so `--lossless` Green-CST construction can reuse the `Tree` produced during symbol extraction
/// without parsing the file a second time.
pub fn set_capture_last_tree(enable: bool) {
    CAPTURE_LAST_TREE.with(|c| c.set(enable));
    if !enable {
        LAST_PARSED_TREE.with(|t| *t.borrow_mut() = None);
    }
}

/// Take the captured `Tree` if it matches the `source` slice pointer and length, and disable capture.
pub fn take_last_parsed_tree(source: &[u8]) -> Option<Tree> {
    CAPTURE_LAST_TREE.with(|c| c.set(false));
    LAST_PARSED_TREE.with(|cell| {
        let taken = cell.borrow_mut().take()?;
        (taken.0 == source.as_ptr() as usize && taken.1 == source.len()).then_some(taken.2)
    })
}

/// Borrow the thread-local parser for `lang_id`, ensuring `grammar` is set once
/// on first use (or when the grammar identity changes), and run `f`.
///
/// Looks up by `&str` before allocating a new `String` key.
pub fn with_thread_local_parser<R>(
    lang_id: &str,
    grammar: Language,
    f: impl FnOnce(&mut Parser) -> R,
) -> R {
    PARSERS.with(|cell| {
        let mut map = cell.borrow_mut();
        if !map.contains_key(lang_id) {
            let mut parser = Parser::new();
            let active = parser.set_language(&grammar).ok().map(|()| grammar.clone());
            map.insert(
                lang_id.to_string(),
                PooledParser {
                    parser,
                    grammar: active,
                },
            );
        }
        let entry = map.get_mut(lang_id).expect("parser entry just ensured");
        if entry.grammar.as_ref() != Some(&grammar) && entry.parser.set_language(&grammar).is_ok() {
            entry.grammar = Some(grammar);
        }
        f(&mut entry.parser)
    })
}

/// Parse `source` using the thread-local pool.
pub fn parse_with_pool(lang_id: &str, grammar: Language, source: &[u8]) -> Result<Tree, String> {
    PARSERS.with(|cell| {
        let mut map = cell.borrow_mut();
        if !map.contains_key(lang_id) {
            let mut parser = Parser::new();
            parser
                .set_language(&grammar)
                .map_err(|e| format!("set_language({lang_id}): {e}"))?;
            map.insert(
                lang_id.to_string(),
                PooledParser {
                    parser,
                    grammar: Some(grammar.clone()),
                },
            );
        }
        let entry = map.get_mut(lang_id).expect("parser entry just ensured");
        if entry.grammar.as_ref() != Some(&grammar) {
            entry
                .parser
                .set_language(&grammar)
                .map_err(|e| format!("set_language({lang_id}): {e}"))?;
            entry.grammar = Some(grammar);
        }
        let tree = entry
            .parser
            .parse(source, None)
            .ok_or_else(|| format!("parse failed for language `{lang_id}`"))?;
        if CAPTURE_LAST_TREE.with(|c| c.get()) {
            LAST_PARSED_TREE.with(|t| {
                *t.borrow_mut() = Some((source.as_ptr() as usize, source.len(), tree.clone()));
            });
        }
        Ok(tree)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_reuses_parser_same_thread() {
        let grammar: Language = tree_sitter_rust::LANGUAGE.into();
        let a = parse_with_pool("rust", grammar.clone(), b"fn main() {}").unwrap();
        let b = parse_with_pool("rust", grammar, b"fn foo() {}").unwrap();
        assert_eq!(a.root_node().kind(), "source_file");
        assert_eq!(b.root_node().kind(), "source_file");
    }
}
