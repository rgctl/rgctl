//! Coverage and parser/adaptor fingerprint.
use rgctl_plugin_api::{
    AstHandlerTable, HandlerMask, KindDispatchTable, dispatch_table_from_manifest,
    handler_table_from_kinds,
};
use std::sync::OnceLock;

pub(crate) const MANIFEST: &str = include_str!("../jinja-ast-coverage.json");
const HANDLED: &[(&str, HandlerMask)] = &[
    ("source", HandlerMask::SYMBOL),
    ("macro_block", HandlerMask::SYMBOL),
    ("block_block", HandlerMask::SYMBOL),
    ("for_block", HandlerMask::SYMBOL),
    ("with_block", HandlerMask::SYMBOL),
    ("set_block", HandlerMask::SYMBOL),
    ("set_statement", HandlerMask::SYMBOL),
    ("call_block", HandlerMask::SYMBOL),
    ("render_expression", HandlerMask::RELATION),
    ("include_statement", HandlerMask::RELATION),
    ("import_statement", HandlerMask::RELATION),
    ("extends_statement", HandlerMask::RELATION),
    ("if_statement", HandlerMask::RELATION),
    ("elif_statement", HandlerMask::RELATION),
    ("do_statement", HandlerMask::RELATION),
    ("filter_statement", HandlerMask::RELATION),
    ("autoescape_statement", HandlerMask::RELATION),
    ("function_call", HandlerMask::RELATION),
    ("identifier", HandlerMask::RELATION),
    ("binary_expression", HandlerMask::RELATION),
    ("expression", HandlerMask::RELATION),
    ("builtin_test", HandlerMask::RELATION),
];
pub(crate) fn handlers() -> &'static AstHandlerTable {
    static TABLE: OnceLock<AstHandlerTable> = OnceLock::new();
    TABLE.get_or_init(|| handler_table_from_kinds(&crate::language(), HANDLED))
}
pub(crate) fn dispatch() -> &'static KindDispatchTable {
    static TABLE: OnceLock<KindDispatchTable> = OnceLock::new();
    TABLE.get_or_init(|| dispatch_table_from_manifest(&crate::language(), MANIFEST))
}
pub(crate) fn fingerprint() -> &'static [u8] {
    static BYTES: OnceLock<Box<[u8]>> = OnceLock::new();
    BYTES.get_or_init(|| {
        [
            MANIFEST.as_bytes(),
            include_bytes!("../../rgctl-plugin-api/src/jinja_syntax.rs"),
            include_str!("../../../vendor/tree-sitter-jinja/RGCTL_REVISION").as_bytes(),
            include_str!("syntax.rs").as_bytes(),
            include_str!("expression.rs").as_bytes(),
            include_str!("extract.rs").as_bytes(),
        ]
        .concat()
        .into_boxed_slice()
    })
}
