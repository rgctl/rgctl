//! Generic extraction helpers for language plugins

pub mod callable_identity;
pub mod complexity;
pub mod ecmascript;
pub mod framework;
pub mod green_cst;
pub mod python;
pub mod spans;
pub mod tree_sitter;

pub use framework::{
    apply_framework_catalog, format_route_target, load_with_repo_overrides, normalize_route_path,
};

pub use complexity::ComplexityCalculator;
pub use ecmascript::{
    bound_function_expression_name, ecmascript_function_symbol_name, extract_cjs_require_symbols,
    extract_class_extends_relations, extract_import_symbols, find_child_kind,
    is_ecmascript_function_node, is_function_expression_kind, simple_type_name,
    type_name_from_node,
};
pub use green_cst::{GreenCstFileBuilder, pack_rel_delta_and_width, unpack_rel_delta_and_width};
pub use python::{
    containing_class_name, decorator_name_and_args, decorators_for_node,
    extract_class_extends_relations as extract_python_class_extends_relations,
    extract_decorator_relations, extract_import_symbols as extract_python_import_symbols,
    python_simple_type_name, source_location as python_source_location,
};
pub use spans::{
    compute_symbol_byte_spans, declaration_header_signature, field_source_span, field_symbol_spans,
    record_call_site_spans, record_symbol_spans, record_symbol_spans_from_source,
};
pub use tree_sitter::{
    extract_name_from_node, extract_parameters_generic, extract_symbols_by_kinds, node_to_location,
    parse_source, symbol_type_for_kind,
};
