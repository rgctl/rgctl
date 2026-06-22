//! Generic extraction helpers for language plugins

pub mod complexity;
pub mod tree_sitter;

pub use complexity::ComplexityCalculator;
pub use tree_sitter::{
    extract_name_from_node, extract_parameters_generic, extract_symbols_by_kinds, node_to_location,
    parse_source, symbol_type_for_kind,
};
