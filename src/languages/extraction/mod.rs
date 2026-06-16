//! Generic extraction helpers (Phase 7.2)

pub mod complexity;
pub mod tree_sitter;

pub use complexity::ComplexityCalculator;
pub use tree_sitter::{
    extract_name_from_node, extract_parameters_generic, extract_symbols_by_kinds,
    node_to_location, symbol_type_for_kind,
};
