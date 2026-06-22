#!/usr/bin/env python3
"""Generate per-crate `src/config.rs` from `languages.toml` (Phase 21 follow-up)."""

from __future__ import annotations

import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LANG_TOML = ROOT / "languages.toml"
CRATES = ROOT / "crates"


def lang_crate_dir(lang_id: str) -> Path:
    return CRATES / f"rbuilder-lang-{lang_id.replace('_', '-')}"


def escape_rust_str(s: str) -> str:
    return s.replace("\\", "\\\\").replace('"', '\\"')


def symbol_type(sym: str) -> str:
    mapping = {
        "function": "SymbolType::Function",
        "class": "SymbolType::Class",
        "struct": "SymbolType::Struct",
        "enum": "SymbolType::Enum",
        "interface": "SymbolType::Interface",
        "macro": "SymbolType::Macro",
    }
    if sym not in mapping:
        raise ValueError(f"Unknown symbol_type: {sym}")
    return mapping[sym]


def generate_config_rs(lang_id: str, lang: dict) -> str:
    exts = ", ".join(f'"{e}"' for e in lang.get("extensions", []))
    fn_kinds = ", ".join(f'"{k}"' for k in lang.get("function_kinds", []))
    cls_kinds = ", ".join(f'"{k}"' for k in lang.get("class_kinds", []))
    patterns = lang.get("regex_patterns", [])

    lines = [
        "//! Static language configuration for this plugin crate.",
        "",
        "use rbuilder_lang_runtime::LanguageConfig;",
    ]
    if patterns:
        lines.append("use rbuilder_lang_runtime::RegexPatternConfig;")
        lines.append("use rbuilder_plugin_api::SymbolType;")
    lines.append("")

    if patterns:
        lines.append("static REGEX_PATTERNS: &[RegexPatternConfig] = &[")
        for pat in patterns:
            sym = symbol_type(pat["symbol_type"])
            lines.append(
                f'    RegexPatternConfig {{ pattern: "{escape_rust_str(pat["pattern"])}", '
                f"symbol_type: {sym} }},"
            )
        lines.append("];")
        lines.append("")
        regex_value = "Some(REGEX_PATTERNS)"
    else:
        regex_value = "None"

    lines.extend(
        [
            "/// Language configuration embedded at compile time.",
            "pub static CONFIG: LanguageConfig = LanguageConfig {",
            f'    id: "{lang_id}",',
            f"    extensions: &[{exts}],",
            f"    function_kinds: &[{fn_kinds}],",
            f"    class_kinds: &[{cls_kinds}],",
            f"    enable_complexity: {str(lang.get('enable_complexity', True)).lower()},",
            f"    enable_type_inference: {str(lang.get('enable_type_inference', False)).lower()},",
            f"    regex_patterns: {regex_value},",
            "};",
            "",
        ]
    )
    return "\n".join(lines)


def tree_sitter_lib_rs(lang_id: str, grammar_fn: str, lang: dict) -> str:
    grammar_crate = lang.get("crate", f"tree-sitter-{lang_id.replace('_', '-')}")
    ident = grammar_crate.replace("-", "_")
    export = lang.get("grammar_export", "language")
    if export == "language":
        grammar_body = f"{ident}::language()"
    elif export == "LANGUAGE":
        grammar_body = f"{ident}::LANGUAGE.into()"
    else:
        grammar_body = f"{ident}::{export}.into()"
    return f"""//! Language plugin crate for rBuilder

mod config;

use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

use rbuilder_lang_runtime::TreeSitterLanguagePlugin;

fn {grammar_fn}() -> tree_sitter::Language {{
    {grammar_body}
}}

/// Register this language plugin.
pub fn register(registry: &mut LanguageRegistry) {{
    registry.register_language_plugin(Arc::new(
        TreeSitterLanguagePlugin::from_config(&config::CONFIG, {grammar_fn}),
    ));
}}
"""


def regex_lib_rs(lang_id: str) -> str:
    return f"""//! Language plugin crate for rBuilder

mod config;

use rbuilder_registry::LanguageRegistry;
use std::sync::Arc;

use rbuilder_lang_runtime::RegexLanguagePlugin;

/// Register this language plugin.
pub fn register(registry: &mut LanguageRegistry) {{
    registry.register_language_plugin(Arc::new(
        RegexLanguagePlugin::from_config(&config::CONFIG),
    ));
}}
"""


def main() -> None:
    data = tomllib.loads(LANG_TOML.read_text())
    languages = data["languages"]

    tree_sitter = []
    regex_langs = []

    for lang_id, lang in languages.items():
        handler = lang["handler"]
        if handler == "tree-sitter":
            tree_sitter.append((lang_id, lang))
        elif handler == "regex":
            regex_langs.append((lang_id, lang))

    for lang_id, lang in tree_sitter + regex_langs:
        crate_dir = lang_crate_dir(lang_id)
        if not crate_dir.is_dir():
            print(f"skip {lang_id}: missing {crate_dir}")
            continue

        config_path = crate_dir / "src" / "config.rs"
        config_path.write_text(generate_config_rs(lang_id, lang))
        print(f"wrote {config_path.relative_to(ROOT)}")

        lib_path = crate_dir / "src" / "lib.rs"
        if lang["handler"] == "tree-sitter":
            grammar_fn = f"load_{lang_id}_grammar"
            lib_path.write_text(tree_sitter_lib_rs(lang_id, grammar_fn, lang))
        else:
            lib_path.write_text(regex_lib_rs(lang_id))
        print(f"wrote {lib_path.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
