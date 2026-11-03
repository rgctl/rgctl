//! Structural YAML coverage and extraction fingerprint.
use rgctl_plugin_api::{AstHandlerTable, HandlerMask, handler_table_from_kinds};
use std::sync::OnceLock;
pub(crate) const MANIFEST: &str = include_str!("../ansible-ast-coverage.json");
const HANDLED: &[(&str, HandlerMask)] = &[
    ("stream", HandlerMask::SYMBOL),
    ("document", HandlerMask::SYMBOL),
    ("block_sequence", HandlerMask::SYMBOL),
    ("flow_sequence", HandlerMask::SYMBOL),
    ("block_mapping", HandlerMask::RELATION),
    ("flow_mapping", HandlerMask::RELATION),
    ("block_mapping_pair", HandlerMask::RELATION),
    ("flow_pair", HandlerMask::RELATION),
    ("plain_scalar", HandlerMask::RELATION),
    ("single_quote_scalar", HandlerMask::RELATION),
    ("double_quote_scalar", HandlerMask::RELATION),
    ("block_scalar", HandlerMask::RELATION),
    ("tag", HandlerMask::RELATION),
    ("alias", HandlerMask::RELATION),
];
pub(crate) fn handlers() -> &'static AstHandlerTable {
    static TABLE: OnceLock<AstHandlerTable> = OnceLock::new();
    TABLE.get_or_init(|| handler_table_from_kinds(&crate::language(), HANDLED))
}
pub(crate) fn fingerprint() -> &'static [u8] {
    static BYTES: OnceLock<Box<[u8]>> = OnceLock::new();
    BYTES.get_or_init(|| {
        [
            MANIFEST.as_bytes(),
            include_bytes!("yaml.rs"),
            include_bytes!("extract.rs"),
            rgctl_lang_jinja::JinjaPlugin::default()
                .cache_fingerprint()
                .unwrap_or_default(),
        ]
        .concat()
        .into_boxed_slice()
    })
}
use rgctl_plugin_api::LanguagePlugin;
