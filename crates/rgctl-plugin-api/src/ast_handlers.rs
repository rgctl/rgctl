//! AST handler introspection for bi-directional `*-ast-coverage.json` sync.
//!
//! Plugins MAY expose a [`AstHandlerTable`] via
//! [`LanguagePlugin::declared_ast_handlers`](crate::LanguagePlugin::declared_ast_handlers)
//! so `rgctl-ast-coverage` can verify that every non-`Skip` / non-`Literal`
//! manifest kind is registered in the plugin dispatch table (and vice versa).

use std::collections::HashMap;

/// Bitmask of how a grammar node kind is handled during extraction / CFG / skeleton.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct HandlerMask(pub u8);

impl HandlerMask {
    /// Kind produces a symbol node.
    pub const SYMBOL: Self = Self(1 << 0);
    /// Kind produces a relation / edge.
    pub const RELATION: Self = Self(1 << 1);
    /// Kind participates in CFG statement construction.
    pub const CFG_STMT: Self = Self(1 << 2);
    /// Kind contributes to AST skeleton / outline.
    pub const AST_SKELETON: Self = Self(1 << 3);
    /// Kind is a literal / token kept for completeness only.
    pub const LITERAL: Self = Self(1 << 4);
    /// Kind is intentionally ignored.
    pub const SKIP: Self = Self(1 << 5);

    /// Empty mask.
    pub const fn empty() -> Self {
        Self(0)
    }

    /// Bitwise OR of two masks.
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// True if any bit of `other` is set.
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// True if this mask is only `SKIP` and/or `LITERAL` (no semantic handlers).
    pub const fn is_passive(self) -> bool {
        !self.intersects(Self(
            Self::SYMBOL.0 | Self::RELATION.0 | Self::CFG_STMT.0 | Self::AST_SKELETON.0,
        ))
    }

    /// Map a `*-ast-coverage.json` handler label to a mask.
    pub fn from_coverage_label(label: &str) -> Option<Self> {
        Some(match label {
            "Symbol" => Self::SYMBOL,
            "Relation" => Self::RELATION,
            "CfgStatement" => Self::CFG_STMT,
            "AstSkeleton" => Self::AST_SKELETON,
            "Literal" => Self::LITERAL,
            "Skip" | "SkipWrapper" | "SkipTypeToken" | "Unimplemented" => Self::SKIP,
            _ => return None,
        })
    }

    /// Primary coverage label for this mask (prefer semantic over Skip/Literal).
    pub fn to_coverage_label(self) -> &'static str {
        if self.intersects(Self::SYMBOL) {
            "Symbol"
        } else if self.intersects(Self::RELATION) {
            "Relation"
        } else if self.intersects(Self::CFG_STMT) {
            "CfgStatement"
        } else if self.intersects(Self::AST_SKELETON) {
            "AstSkeleton"
        } else if self.intersects(Self::LITERAL) {
            "Literal"
        } else {
            "Skip"
        }
    }
}

impl std::ops::BitOr for HandlerMask {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl std::ops::BitOrAssign for HandlerMask {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

/// Mapping from tree-sitter grammar `kind_id` (`u16`) to [`HandlerMask`].
///
/// Built once per language (typically from `{lang}-ast-coverage.json` via
/// `lang.id_for_node_kind`) and reused for O(1) dispatch / coverage sync.
#[derive(Debug, Clone, Default)]
pub struct AstHandlerTable {
    by_kind_id: HashMap<u16, HandlerMask>,
    /// Kind name → kind_id for reverse lookup / drift reports.
    by_kind_name: HashMap<String, u16>,
}

impl AstHandlerTable {
    /// Empty table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert or union a handler for a grammar kind.
    pub fn insert(&mut self, kind_id: u16, kind_name: impl Into<String>, mask: HandlerMask) {
        let name = kind_name.into();
        self.by_kind_name.insert(name, kind_id);
        self.by_kind_id
            .entry(kind_id)
            .and_modify(|m| *m |= mask)
            .or_insert(mask);
    }

    /// Lookup by grammar kind id.
    pub fn get(&self, kind_id: u16) -> Option<HandlerMask> {
        self.by_kind_id.get(&kind_id).copied()
    }

    /// Lookup by grammar kind name.
    pub fn get_by_name(&self, kind_name: &str) -> Option<(u16, HandlerMask)> {
        let id = *self.by_kind_name.get(kind_name)?;
        let mask = self.get(id)?;
        Some((id, mask))
    }

    /// Iterate `(kind_id, kind_name, mask)` entries.
    pub fn iter(&self) -> impl Iterator<Item = (u16, &str, HandlerMask)> + '_ {
        self.by_kind_name.iter().filter_map(|(name, &id)| {
            self.by_kind_id
                .get(&id)
                .map(|&mask| (id, name.as_str(), mask))
        })
    }

    /// Number of registered kinds.
    pub fn len(&self) -> usize {
        self.by_kind_id.len()
    }

    /// True if empty.
    pub fn is_empty(&self) -> bool {
        self.by_kind_id.is_empty()
    }

    /// Build from coverage-manifest `(kind_name, handler_label)` pairs using a live grammar.
    pub fn from_coverage_entries(
        language: &tree_sitter::Language,
        entries: impl IntoIterator<Item = (impl AsRef<str>, impl AsRef<str>)>,
    ) -> Self {
        let mut table = Self::new();
        for (name, label) in entries {
            let name = name.as_ref();
            let Some(mask) = HandlerMask::from_coverage_label(label.as_ref()) else {
                continue;
            };
            let kind_id = language.id_for_node_kind(name, true);
            if kind_id == 0 {
                // Anonymous / unknown — still record by name with id 0 skipped for dispatch.
                continue;
            }
            table.insert(kind_id, name.to_string(), mask);
        }
        table
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_labels_round_trip() {
        for label in [
            "Symbol",
            "Relation",
            "CfgStatement",
            "AstSkeleton",
            "Literal",
            "Skip",
        ] {
            let m = HandlerMask::from_coverage_label(label).unwrap();
            assert_eq!(m.to_coverage_label(), label);
        }
    }

    #[test]
    fn passive_masks() {
        assert!(HandlerMask::SKIP.is_passive());
        assert!(HandlerMask::LITERAL.is_passive());
        assert!(HandlerMask::SKIP.union(HandlerMask::LITERAL).is_passive());
        assert!(!HandlerMask::SYMBOL.is_passive());
        assert!(!HandlerMask::RELATION.union(HandlerMask::SKIP).is_passive());
    }
}
