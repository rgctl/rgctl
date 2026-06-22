#!/usr/bin/env python3
"""Scaffold rbuilder-lang-* and rbuilder-bundle-* crates (Phase 21.3–21.4)."""

from __future__ import annotations

import re
import shutil
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC_LANG = ROOT / "src" / "languages"
CRATES = ROOT / "crates"

CUSTOM_BUILTIN = {"rust", "python", "typescript", "javascript", "go", "java"}
CUSTOM_MULTIMODAL_SINGLE = {"sql", "dockerfile", "github_actions", "gitlab_ci", "bash"}
CUSTOM_MULTIMODAL_DIR = {"ansible", "chef", "puppet"}
IAC_LANGS = CUSTOM_MULTIMODAL_DIR

TREE_SITTER_VERSIONS: dict[str, str] = {
    "tree-sitter-rust": "0.24",
    "tree-sitter-python": "0.25",
    "tree-sitter-typescript": "0.23",
    "tree-sitter-javascript": "0.25",
    "tree-sitter-go": "0.25",
    "tree-sitter-java": "0.23",
    "tree-sitter-c": "0.24",
    "tree-sitter-cpp": "0.23",
    "tree-sitter-ruby": "0.23",
    "tree-sitter-php": "0.24",
    "tree-sitter-swift": "0.7",
    "tree-sitter-scala": "0.26",
    "tree-sitter-lua": "0.5",
    "tree-sitter-elixir": "0.3",
    "tree-sitter-erlang": "0.19",
    "tree-sitter-haskell": "0.23",
    "tree-sitter-dart": "0.2",
    "tree-sitter-r": "1.2",
    "tree-sitter-julia": "0.23",
    "tree-sitter-ocaml": "0.25",
    "ts-parser-perl": "1.1",
    "tree-sitter-fortran": "0.6",
    "tree-sitter-verilog": "1.0",
    "tree-sitter-vhdl": "1.4",
    "tree-sitter-pascal": "0.10",
    "tree-sitter-zig": "1.1",
    "tree-sitter-fsharp": "0.3",
    "tree-sitter-bash": "0.25",
}

TREE_SITTER_GIT: dict[str, str] = {
    "tree-sitter-nim": "https://github.com/alaviss/tree-sitter-nim",
    "tree-sitter-crystal": "https://github.com/crystal-lang-tools/tree-sitter-crystal",
}


def lang_crate_id(lang_id: str) -> str:
    return lang_id.replace("_", "-")


def lang_crate_name(lang_id: str) -> str:
    return f"rbuilder-lang-{lang_crate_id(lang_id)}"


def rust_crate_name(lang_id: str) -> str:
    return lang_crate_name(lang_id).replace("-", "_")


def grammar_crate_ident(crate_name: str) -> str:
    return crate_name.replace("-", "_")


def fix_imports(text: str, *, iac: bool = False) -> str:
    replacements = [
        (r"use crate::graph::", "use rbuilder_graph::"),
        (r"crate::graph::", "rbuilder_graph::"),
        (r"crate::languages::plugin_trait::", "rbuilder_plugin_api::"),
        (r"use crate::semantic::type_inference::TypeInferencer", "use rbuilder_semantic::type_inference::TypeInferencer"),
        (r"#\[cfg\(feature = \"lang-bash\"\)\]", ""),
        (r"#\[cfg\(not\(feature = \"lang-bash\"\)\)\][^\n]*\n(?:[^\n]*\n)*?    \}\n", ""),
    ]
    if iac:
        replacements.extend(
            [
                (r"use crate::analysis::ansible_roles::", "use crate::analysis::"),
                (r"use crate::analysis::chef_cookbooks::", "use crate::analysis::"),
                (r"use crate::analysis::puppet_modules::", "use crate::analysis::"),
                (r"use crate::security::ansible::", "use crate::security::"),
                (r"use crate::security::chef::", "use crate::security::"),
                (r"use crate::security::puppet::", "use crate::security::"),
                (r"use crate::languages::multimodal::ansible", "use crate"),
                (r"use crate::languages::multimodal::chef", "use crate"),
                (r"use crate::languages::multimodal::puppet", "use crate"),
                (
                    r"crate::languages::multimodal::ansible::role_dependencies_from_meta",
                    "crate::role_dependencies_from_meta",
                ),
                (
                    r"crate::languages::multimodal::chef::cookbook_dependencies_from_metadata",
                    "crate::cookbook_dependencies_from_metadata",
                ),
                (
                    r"crate::languages::multimodal::chef::parse_content",
                    "crate::parse_content",
                ),
                (
                    r"crate::languages::multimodal::puppet::parse_content",
                    "crate::parse_content",
                ),
                (
                    r"crate::languages::multimodal::puppet::module_dependencies_from_metadata",
                    "crate::module_dependencies_from_metadata",
                ),
            ]
        )
    updated = text
    for pattern, repl in replacements:
        updated = re.sub(pattern, repl, updated)
    return updated


def tree_sitter_dep_line(crate_name: str) -> str:
    if crate_name in TREE_SITTER_GIT:
        return f'{crate_name} = {{ git = "{TREE_SITTER_GIT[crate_name]}" }}'
    version = TREE_SITTER_VERSIONS.get(crate_name, "0.25")
    return f'{crate_name} = "{version}"'


def grammar_loader(lang_id: str, crate_name: str, grammar_export: str) -> tuple[str, str]:
    ident = grammar_crate_ident(crate_name)
    fn_name = f"load_{lang_id.replace('-', '_')}_grammar"
    if grammar_export == "language":
        body = f"{ident}::language()"
    elif grammar_export == "LANGUAGE":
        body = f"{ident}::LANGUAGE.into()"
    else:
        body = f"{ident}::{grammar_export}.into()"
    fn = f"fn {fn_name}() -> tree_sitter::Language {{\n    {body}\n}}\n"
    return fn_name, fn


def write_lang_cargo(lang_id: str, entry: dict) -> str:
    handler = entry["handler"]
    crate_name = lang_crate_name(lang_id)
    lines = [
        "[package]",
        f'name = "{crate_name}"',
        'version = "0.1.0"',
        'edition = "2021"',
        f'description = "rBuilder language plugin: {lang_id}"',
        'license = "MIT OR Apache-2.0"',
        "",
        "[dependencies]",
        "rbuilder-plugin-api = { workspace = true }",
        "rbuilder-registry = { workspace = true }",
    ]
    extra: list[str] = []

    if handler == "custom":
        extra.append("rbuilder-plugin-helpers = { workspace = true }")
        if lang_id in CUSTOM_BUILTIN:
            ts_crate = entry.get("crate") or f"tree-sitter-{lang_id}"
            extra.append("tree-sitter = { workspace = true }")
            extra.append(tree_sitter_dep_line(ts_crate))
            if lang_id in {"python", "javascript"}:
                extra.append("rbuilder-semantic = { workspace = true }")
        if lang_id == "bash":
            extra.append("tree-sitter = { workspace = true }")
            extra.append(tree_sitter_dep_line("tree-sitter-bash"))
        if lang_id in CUSTOM_MULTIMODAL_DIR | CUSTOM_MULTIMODAL_SINGLE:
            extra.append('serde_yaml = "0.9"')
            extra.append('regex = "1"')
            if lang_id in {"dockerfile", "github_actions", "gitlab_ci"}:
                extra.append('serde = { version = "1", features = ["derive"] }')
                extra.append('serde_json = "1"')
        if lang_id in CUSTOM_MULTIMODAL_DIR:
            extra.extend(
                [
                    "rbuilder-error = { workspace = true }",
                    "rbuilder-graph = { workspace = true }",
                    "clap = { version = \"4\", features = [\"derive\"] }",
                    "comfy-table = \"7\"",
                ]
            )
    elif handler == "tree-sitter":
        extra.append("rbuilder-lang-runtime = { workspace = true }")
        extra.append("tree-sitter = { workspace = true }")
        ts_crate = entry.get("crate") or f"tree-sitter-{lang_id}"
        extra.append(tree_sitter_dep_line(ts_crate))
    elif handler == "regex":
        extra.append("rbuilder-lang-runtime = { workspace = true }")
    elif handler == "markdown":
        extra.append("rbuilder-config-formats = { workspace = true }")

    lines.extend(extra)
    return "\n".join(lines) + "\n"


def copy_custom_sources(lang_id: str, entry: dict, dest: Path) -> str:
    plugin_name = entry["plugin"]
    if lang_id in CUSTOM_BUILTIN:
        src = SRC_LANG / "builtin" / f"{lang_id}.rs"
        dest_file = dest / "plugin.rs"
        dest_file.write_text(fix_imports(src.read_text()))
        return plugin_name
    if lang_id in CUSTOM_MULTIMODAL_SINGLE:
        src = SRC_LANG / "multimodal" / f"{lang_id}.rs"
        dest_file = dest / "plugin.rs"
        text = fix_imports(src.read_text())
        text = re.sub(r"#\[cfg\(feature = \"lang-bash\"\)\]\s*", "", text)
        text = re.sub(
            r"#\[cfg\(not\(feature = \"lang-bash\"\)\)\][\s\S]*?fn extract_symbols_inner\([^)]*\)[^{]*\{[^}]*\}\s*",
            "",
            text,
        )
        dest_file.write_text(text)
        if lang_id == "dockerfile":
            patch_dockerfile_plugin(dest)
        return plugin_name
    if lang_id in CUSTOM_MULTIMODAL_DIR:
        mod_src = SRC_LANG / "multimodal" / lang_id / "mod.rs"
        parser_src = SRC_LANG / "multimodal" / lang_id / "parser.rs"
        (dest / "plugin.rs").write_text(fix_imports(mod_src.read_text()))
        (dest / "parser.rs").write_text(fix_imports(parser_src.read_text()))
        analysis_map = {
            "ansible": "ansible_roles.rs",
            "chef": "chef_cookbooks.rs",
            "puppet": "puppet_modules.rs",
        }
        (dest / "analysis.rs").write_text(
            fix_imports((ROOT / "src" / "analysis" / analysis_map[lang_id]).read_text(), iac=True)
        )
        (dest / "cli.rs").write_text(
            fix_imports((ROOT / "src" / "cli" / f"{lang_id}.rs").read_text(), iac=True)
        )
        (dest / "security.rs").write_text(
            fix_imports((ROOT / "src" / "security" / f"{lang_id}.rs").read_text(), iac=True)
        )
        return plugin_name
    raise ValueError(f"unknown custom language {lang_id}")


def write_lang_lib(lang_id: str, entry: dict, dest: Path) -> None:
    handler = entry["handler"]
    plugin_name = entry.get("plugin", "")
    lines = [
        "//! Language plugin crate for rBuilder",
        "",
        "use rbuilder_registry::LanguageRegistry;",
        "use std::sync::Arc;",
        "",
    ]

    if handler == "custom":
        if lang_id in CUSTOM_MULTIMODAL_DIR:
            lines.extend(
                [
                    "pub mod analysis;",
                    "pub mod cli;",
                    "pub mod parser;",
                    "pub mod plugin;",
                    "pub mod security;",
                    "",
                    f"pub use plugin::{plugin_name};",
                    "pub use analysis::*;",
                    "pub use security::*;",
                    "",
                ]
            )
            lines = [l for l in lines if l != "" or lines.index(l) == 0 or True]
            # rebuild without empty conditional lines
            lines = [l for l in [
                "//! Language plugin crate for rBuilder",
                "",
                "pub mod analysis;",
                "pub mod cli;",
                "pub mod parser;",
                "pub mod plugin;",
                "pub mod security;",
                "",
                f"pub use plugin::{plugin_name};",
                "pub use analysis::*;",
                "pub use security::*;",
            ]
              + (["pub use plugin::role_dependencies_from_meta;"] if lang_id == "ansible" else [])
              + (
                  [
                      "pub use plugin::cookbook_dependencies_from_metadata;",
                      "pub use plugin::parse_content;",
                  ]
                  if lang_id == "chef"
                  else []
              )
              + (
                  [
                      "pub use plugin::module_dependencies_from_metadata;",
                      "pub use plugin::parse_content;",
                  ]
                  if lang_id == "puppet"
                  else []
              )
              + [
                "",
                "/// Register this language plugin.",
                "pub fn register(registry: &mut LanguageRegistry) {",
                f"    registry.register_language_plugin(Arc::new({plugin_name}::new().expect(\"init {plugin_name}\")));",
                "}",
            ]]
        else:
            lines.extend(
                [
                    "mod plugin;",
                    f"pub use plugin::{plugin_name};",
                    "",
                    "/// Register this language plugin.",
                    "pub fn register(registry: &mut LanguageRegistry) {",
                    f"    registry.register_language_plugin(Arc::new({plugin_name}::new().expect(\"init {plugin_name}\")));",
                    "}",
                ]
            )
    elif handler == "tree-sitter":
        crate_field = entry.get("crate") or f"tree-sitter-{lang_id}"
        grammar_export = entry.get("grammar_export", "language")
        fn_name, fn_body = grammar_loader(lang_id, crate_field, grammar_export)
        lines.extend(
            [
                "mod config;",
                "",
                "use rbuilder_lang_runtime::TreeSitterLanguagePlugin;",
                "",
                fn_body,
                "",
                "/// Register this language plugin.",
                "pub fn register(registry: &mut LanguageRegistry) {",
                "    registry.register_language_plugin(Arc::new(",
                f"        TreeSitterLanguagePlugin::from_config(&config::CONFIG, {fn_name}),",
                "    ));",
                "}",
            ]
        )
    elif handler == "regex":
        lines.extend(
            [
                "mod config;",
                "",
                "use rbuilder_lang_runtime::RegexLanguagePlugin;",
                "",
                "/// Register this language plugin.",
                "pub fn register(registry: &mut LanguageRegistry) {",
                "    registry.register_language_plugin(Arc::new(",
                "        RegexLanguagePlugin::from_config(&config::CONFIG),",
                "    ));",
                "}",
            ]
        )
    elif handler == "markdown":
        lines.extend(
            [
                "use rbuilder_config_formats::MarkdownPlugin;",
                "",
                "/// Register this language plugin.",
                "pub fn register(registry: &mut LanguageRegistry) {",
                '    registry.register_language_plugin(Arc::new(MarkdownPlugin::new().expect("init markdown")));',
                "}",
            ]
        )
    else:
        raise ValueError(f"unknown handler {handler}")

    (dest / "lib.rs").write_text("\n".join(lines) + "\n")


def create_lang_crate(lang_id: str, entry: dict) -> None:
    crate_dir = CRATES / lang_crate_name(lang_id)
    src = crate_dir / "src"
    if crate_dir.exists():
        shutil.rmtree(crate_dir)
    src.mkdir(parents=True)
    (crate_dir / "Cargo.toml").write_text(write_lang_cargo(lang_id, entry))
    if entry["handler"] == "custom":
        copy_custom_sources(lang_id, entry, src)
    write_lang_lib(lang_id, entry, src)


def create_bundle_crate(bundle_id: str, languages: list[str]) -> None:
    crate_dir = CRATES / f"rbuilder-bundle-{bundle_id}"
    src = crate_dir / "src"
    if crate_dir.exists():
        shutil.rmtree(crate_dir)
    src.mkdir(parents=True)

    dep_lines = [
        "[package]",
        f'name = "rbuilder-bundle-{bundle_id}"',
        'version = "0.1.0"',
        'edition = "2021"',
        f'description = "rBuilder language bundle: {bundle_id}"',
        'license = "MIT OR Apache-2.0"',
        "",
        "[dependencies]",
        "rbuilder-registry = { workspace = true }",
        "rbuilder-config-formats = { workspace = true }",
    ]
    for lang_id in languages:
        dep_lines.append(f"{lang_crate_name(lang_id)} = {{ workspace = true }}")
    (crate_dir / "Cargo.toml").write_text("\n".join(dep_lines) + "\n")

    register_calls = []
    for lang_id in languages:
        rs = rust_crate_name(lang_id)
        register_calls.append(f"    {rs}::register(registry);")

    lib = f"""//! Language bundle: {bundle_id}

use rbuilder_registry::LanguageRegistry;

/// Register config format plugins (yaml, json, toml, properties).
pub fn register_config_formats(registry: &mut LanguageRegistry) {{
    rbuilder_config_formats::register_all(registry);
}}

/// Register all language plugins in this bundle.
pub fn register_languages(registry: &mut LanguageRegistry) {{
{chr(10).join(register_calls)}
}}

/// Default registry with config formats and bundle languages.
pub fn default_registry() -> LanguageRegistry {{
    let mut registry = LanguageRegistry::with_config_formats();
    register_languages(&mut registry);
    registry
}}
"""
    (src / "lib.rs").write_text(lib)


def update_workspace_cargo(lang_ids: list[str], bundle_ids: list[str]) -> None:
    cargo_path = ROOT / "Cargo.toml"
    text = cargo_path.read_text()

    # workspace members
    member_lines = [
        '    ".",',
        '    "rbuilder-macros",',
    ]
    for sub in sorted((CRATES).iterdir()):
        if sub.is_dir() and (sub.name.startswith("rbuilder-") and sub.name not in {
            "rbuilder-plugin-api", "rbuilder-plugin-helpers", "rbuilder-config-formats",
            "rbuilder-lang-runtime", "rbuilder-registry", "rbuilder-error", "rbuilder-graph",
            "rbuilder-extraction", "rbuilder-pipeline", "rbuilder-analysis", "rbuilder-gql",
            "rbuilder-export", "rbuilder-nlp", "rbuilder-security", "rbuilder-project-config",
            "rbuilder-incremental", "rbuilder-semantic", "rbuilder-rules", "rbuilder-mcp",
            "rbuilder-cli", "rbuilder-core",
        } or sub.name.startswith("rbuilder-lang-") or sub.name.startswith("rbuilder-bundle-")):
            pass
    # Build full member list explicitly
    core_members = [
        "crates/rbuilder-plugin-api",
        "crates/rbuilder-plugin-helpers",
        "crates/rbuilder-config-formats",
        "crates/rbuilder-lang-runtime",
        "crates/rbuilder-registry",
        "crates/rbuilder-error",
        "crates/rbuilder-graph",
        "crates/rbuilder-extraction",
        "crates/rbuilder-pipeline",
        "crates/rbuilder-analysis",
        "crates/rbuilder-gql",
        "crates/rbuilder-export",
        "crates/rbuilder-nlp",
        "crates/rbuilder-security",
        "crates/rbuilder-project-config",
        "crates/rbuilder-incremental",
        "crates/rbuilder-semantic",
        "crates/rbuilder-rules",
        "crates/rbuilder-mcp",
        "crates/rbuilder-cli",
        "crates/rbuilder-core",
    ]
    lang_members = [f"crates/{lang_crate_name(l)}" for l in sorted(lang_ids)]
    bundle_members = [f"crates/rbuilder-bundle-{b}" for b in sorted(bundle_ids)]
    all_members = ['    ".",', '    "rbuilder-macros",'] + [f'    "{m}",' for m in core_members + lang_members + bundle_members]

    members_block = "[workspace]\nmembers = [\n" + "\n".join(all_members) + "\n]\n"

    # workspace deps for lang + bundle crates
    ws_deps = []
    for lang_id in sorted(lang_ids):
        name = lang_crate_name(lang_id)
        ws_deps.append(f'{name} = {{ path = "crates/{name}", version = "0.1.0" }}')
    for bundle_id in sorted(bundle_ids):
        name = f"rbuilder-bundle-{bundle_id}"
        ws_deps.append(f'{name} = {{ path = "crates/{name}", version = "0.1.0" }}')
    ws_deps.append('tree-sitter = "0.25"')

    ws_block = "[workspace.dependencies]\n" + "\n".join(
        [
            'rbuilder-plugin-api = { path = "crates/rbuilder-plugin-api", version = "0.1.0" }',
            'rbuilder-plugin-helpers = { path = "crates/rbuilder-plugin-helpers", version = "0.1.0" }',
            'rbuilder-config-formats = { path = "crates/rbuilder-config-formats", version = "0.1.0" }',
            'rbuilder-lang-runtime = { path = "crates/rbuilder-lang-runtime", version = "0.1.0" }',
            'rbuilder-registry = { path = "crates/rbuilder-registry", version = "0.1.0" }',
            'rbuilder-error = { path = "crates/rbuilder-error", version = "0.1.0" }',
            'rbuilder-graph = { path = "crates/rbuilder-graph", version = "0.1.0" }',
            'rbuilder-extraction = { path = "crates/rbuilder-extraction", version = "0.1.0" }',
            'rbuilder-pipeline = { path = "crates/rbuilder-pipeline", version = "0.1.0" }',
            'rbuilder-analysis = { path = "crates/rbuilder-analysis", version = "0.1.0" }',
            'rbuilder-gql = { path = "crates/rbuilder-gql", version = "0.1.0" }',
            'rbuilder-export = { path = "crates/rbuilder-export", version = "0.1.0" }',
            'rbuilder-nlp = { path = "crates/rbuilder-nlp", version = "0.1.0" }',
            'rbuilder-security = { path = "crates/rbuilder-security", version = "0.1.0" }',
            'rbuilder-project-config = { path = "crates/rbuilder-project-config", version = "0.1.0" }',
            'rbuilder-incremental = { path = "crates/rbuilder-incremental", version = "0.1.0" }',
            'rbuilder-semantic = { path = "crates/rbuilder-semantic", version = "0.1.0" }',
            'rbuilder-rules = { path = "crates/rbuilder-rules", version = "0.1.0" }',
            'rbuilder-mcp = { path = "crates/rbuilder-mcp", version = "0.1.0" }',
            'rbuilder-cli = { path = "crates/rbuilder-cli", version = "0.1.0" }',
            'rbuilder-core = { path = "crates/rbuilder-core", version = "0.1.0" }',
            'tempfile = "3"',
        ]
        + ws_deps
    ) + "\n"

    # Replace [workspace] section through [package]
    text = re.sub(r"\[workspace\][\s\S]*?(?=\[package\])", members_block + ws_block + "\n", text, count=1)

    # Remove build.rs reference
    text = re.sub(r'^build = "build\.rs"\n', "", text, flags=re.MULTILINE)

    # Remove build-dependencies section
    text = re.sub(r"\n\[build-dependencies\][\s\S]*?(?=\n\[features\])", "\n", text)

    # Remove tree-sitter optional deps block
    text = re.sub(
        r"\n# Parsing & AST - Programming Languages.*?\n(?:tree-sitter[^\n]*\n)+",
        "\n",
        text,
    )
    text = re.sub(r"^tree-sitter = \"0\.25\"\n", "", text, flags=re.MULTILINE)

    # Add bundle optional deps after rbuilder-cli dep
    bundle_deps = """
rbuilder-bundle-minimal = { workspace = true, optional = true }
rbuilder-bundle-extended = { workspace = true, optional = true }
rbuilder-bundle-full = { workspace = true, optional = true }
rbuilder-bundle-extra = { workspace = true, optional = true }
"""
    if "rbuilder-bundle-full" not in text:
        text = text.replace(
            "rbuilder-cli = { workspace = true }\n",
            "rbuilder-cli = { workspace = true }\n" + bundle_deps,
        )

    # IaC re-export deps
    iac_deps = """
rbuilder-lang-ansible = { workspace = true, optional = true }
rbuilder-lang-chef = { workspace = true, optional = true }
rbuilder-lang-puppet = { workspace = true, optional = true }
"""
    if "rbuilder-lang-ansible" not in text:
        text = text.replace(
            "rbuilder-bundle-extra = { workspace = true, optional = true }\n",
            "rbuilder-bundle-extra = { workspace = true, optional = true }\n" + iac_deps,
        )

    # Replace features section
    features = """[features]
default = ["bundle-full", "nlp-patterns", "mcp-server"]

all-languages = ["bundle-extra"]

bundle-minimal = ["dep:rbuilder-bundle-minimal"]
bundle-extended = ["dep:rbuilder-bundle-extended", "iac-langs"]
bundle-full = ["dep:rbuilder-bundle-full", "iac-langs"]
bundle-extra = ["dep:rbuilder-bundle-extra", "iac-langs"]
iac-langs = ["dep:rbuilder-lang-ansible", "dep:rbuilder-lang-chef", "dep:rbuilder-lang-puppet"]

nlp-patterns = ["rbuilder-nlp/nlp-patterns"]
nlp-cache = ["nlp-patterns", "ndarray", "rbuilder-nlp/nlp-cache"]
nlp-llm = ["nlp-cache", "reqwest", "rbuilder-nlp/nlp-llm"]

mcp-server = ["axum", "tower-http", "rbuilder-mcp/mcp-server", "rbuilder-cli/mcp-server", "rbuilder-core/mcp-server", "rbuilder-error/mcp-server"]
proc-macros = ["rbuilder-macros"]
"""
    text = re.sub(r"\[features\][\s\S]*?(?=\[profile\.release\])", features + "\n", text)

    # Fix bench required-features
    text = text.replace(
        'required-features = ["bundle-minimal", "lang-python"]',
        'required-features = ["bundle-minimal"]',
    )

    cargo_path.write_text(text)


def patch_dockerfile_plugin(dest: Path) -> None:
    plugin = dest / "plugin.rs"
    if not plugin.exists():
        return
    text = plugin.read_text()
    if "fn matches_path" in text:
        return
    needle = "    fn calculate_complexity(\n        _symbol: &Symbol,\n        _source: &[u8],\n    ) -> Result<Option<ComplexityMetrics>> {\n        Ok(None)\n    }\n}"
    replacement = needle.replace(
        "\n}",
        """
    fn matches_path(&self, path: &str) -> bool {
        let name = path.rsplit(['/', '\\\\']).next().unwrap_or(path);
        name.eq_ignore_ascii_case("dockerfile") || name.ends_with(".dockerfile")
    }
}""",
    )
    if needle in text:
        plugin.write_text(text.replace(needle, replacement, 1))


def update_registry_rs() -> None:
    path = ROOT / "src" / "languages" / "registry.rs"
    path.write_text(
        """//! Monolith registry wrapper — delegates to bundle crates.

pub use rbuilder_registry::{plugin_abi, plugin_loader, RegistryStats};

use rbuilder_registry::LanguageRegistry as InnerRegistry;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Once};

static INIT: Once = Once::new();

/// Wire the bundle registry builder for `rbuilder_registry::full_registry()`.
pub fn ensure_initialized() {
    INIT.call_once(|| {
        rbuilder_registry::set_full_registry_builder(build_registry_inner);
    });
}

fn build_registry_inner() -> InnerRegistry {
    #[cfg(feature = "bundle-extra")]
    {
        return rbuilder_bundle_extra::default_registry();
    }
    #[cfg(all(feature = "bundle-full", not(feature = "bundle-extra")))]
    {
        return rbuilder_bundle_full::default_registry();
    }
    #[cfg(all(feature = "bundle-extended", not(any(feature = "bundle-full", feature = "bundle-extra"))))]
    {
        return rbuilder_bundle_extended::default_registry();
    }
    #[cfg(all(feature = "bundle-minimal", not(any(feature = "bundle-extended", feature = "bundle-full", feature = "bundle-extra"))))]
    {
        return rbuilder_bundle_minimal::default_registry();
    }
    InnerRegistry::with_config_formats()
}

/// Build a registry using the active bundle feature.
pub fn build_registry() -> LanguageRegistry {
    ensure_initialized();
    LanguageRegistry(build_registry_inner())
}

/// Registry with bundle-selected language plugins.
pub struct LanguageRegistry(InnerRegistry);

impl LanguageRegistry {
    /// Create a registry with config formats and bundle language plugins.
    pub fn new() -> Self {
        build_registry()
    }

    /// Create an empty registry.
    pub fn empty() -> Self {
        Self(InnerRegistry::empty())
    }

    /// Create a registry with config format plugins only.
    pub fn with_config_formats() -> Self {
        Self(InnerRegistry::with_config_formats())
    }

    /// Consume the wrapper and return the inner registry.
    pub fn into_inner(self) -> InnerRegistry {
        self.0
    }
}

impl std::ops::Deref for LanguageRegistry {
    type Target = InnerRegistry;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for LanguageRegistry {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl From<LanguageRegistry> for std::sync::Arc<InnerRegistry> {
    fn from(value: LanguageRegistry) -> Self {
        std::sync::Arc::new(value.0)
    }
}

impl From<LanguageRegistry> for InnerRegistry {
    fn from(value: LanguageRegistry) -> Self {
        value.0
    }
}

/// No-op alias; wiring happens in [`ensure_initialized`].
pub fn ensure_registry_initialized() {
    ensure_initialized();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_registry_creation() {
        let registry = LanguageRegistry::new();
        let stats = registry.stats();
        assert!(stats.language_plugins >= 1);
        assert_eq!(stats.config_plugins, 4);
    }

    #[test]
    fn test_get_rust_plugin() {
        let registry = LanguageRegistry::new();
        let rust_plugin = registry.get_language_plugin("rust");
        assert!(rust_plugin.is_some());
        assert_eq!(rust_plugin.unwrap().language_id(), "rust");
    }

    #[test]
    fn test_config_plugins() {
        let registry = LanguageRegistry::new();
        assert!(registry.get_config_plugin("yaml").is_some());
        assert!(registry.get_config_plugin("json").is_some());
        assert!(registry.get_config_plugin("toml").is_some());
    }

    #[test]
    fn test_can_process_config_files() {
        let registry = LanguageRegistry::new();
        assert!(registry.can_process_file(Path::new("config.yaml")));
        assert!(registry.can_process_file(Path::new("config.yml")));
        assert!(registry.can_process_file(Path::new("config.json")));
        assert!(registry.can_process_file(Path::new("config.toml")));
    }
}
"""
    )


def update_languages_mod() -> None:
    (ROOT / "src" / "languages" / "mod.rs").write_text(
        """//! Language plugin registry wrapper (languages live in `rbuilder-lang-*` crates).

pub use rbuilder_config_formats as config;
pub use rbuilder_lang_runtime as generic;
pub use rbuilder_plugin_api as plugin_trait;
pub use rbuilder_plugin_helpers as extraction;

pub mod registry;

pub use registry::LanguageRegistry;

/// No-op alias; wiring happens in [`registry::ensure_initialized`].
pub fn ensure_registry_initialized() {
    registry::ensure_initialized();
}
"""
    )


def update_monolith_reexports() -> None:
    (ROOT / "src" / "analysis" / "mod.rs").write_text(
        """//! Graph analysis algorithms (monolith re-exports + IaC from lang crates)

pub use rbuilder_analysis::*;

#[cfg(feature = "iac-langs")]
pub mod ansible_roles {
    pub use rbuilder_lang_ansible::analysis::*;
}

#[cfg(feature = "iac-langs")]
pub mod chef_cookbooks {
    pub use rbuilder_lang_chef::analysis::*;
}

#[cfg(feature = "iac-langs")]
pub mod puppet_modules {
    pub use rbuilder_lang_puppet::analysis::*;
}

#[cfg(feature = "iac-langs")]
pub use ansible_roles::{RoleDependencyAnalyzer, RoleDependencyGraph, RoleNode};
#[cfg(feature = "iac-langs")]
pub use chef_cookbooks::{CookbookDependencyAnalyzer, CookbookDependencyGraph, CookbookNode};
#[cfg(feature = "iac-langs")]
pub use puppet_modules::{ModuleDependencyAnalyzer, ModuleDependencyGraph, ModuleNode};
"""
    )

    (ROOT / "src" / "security" / "mod.rs").write_text(
        """//! Security analysis (monolith re-exports + IaC from lang crates)

pub use rbuilder_security::*;

#[cfg(feature = "iac-langs")]
pub mod ansible {
    pub use rbuilder_lang_ansible::security::*;
}

#[cfg(feature = "iac-langs")]
pub mod chef {
    pub use rbuilder_lang_chef::security::*;
}

#[cfg(feature = "iac-langs")]
pub mod puppet {
    pub use rbuilder_lang_puppet::security::*;
}

#[cfg(feature = "iac-langs")]
pub use ansible::{AnsibleSecurityFinding, AnsibleSecurityScanner, AnsibleSeverity};
#[cfg(feature = "iac-langs")]
pub use chef::{ChefSecurityFinding, ChefSecurityScanner, ChefSeverity};
#[cfg(feature = "iac-langs")]
pub use puppet::{PuppetSecurityFinding, PuppetSecurityScanner, PuppetSeverity};
"""
    )

    (ROOT / "src" / "cli" / "mod.rs").write_text(
        """//! CLI command implementations

pub use rbuilder_cli::cli::*;
pub use rbuilder_cli::{git_util, hooks, multi_repo, output};

#[cfg(feature = "iac-langs")]
pub use rbuilder_lang_ansible::cli as ansible;
#[cfg(feature = "iac-langs")]
pub use rbuilder_lang_chef::cli as chef;
#[cfg(feature = "iac-langs")]
pub use rbuilder_lang_puppet::cli as puppet;
"""
    )

    (ROOT / "src" / "lib.rs").write_text(
        """//! rBuilder - AI-Powered Code Knowledge Graph

#![warn(missing_docs)]
#![warn(clippy::all)]

pub use rbuilder_core::*;

pub mod analysis;
pub mod cli;
pub mod graph;
pub mod languages;
pub mod security;

pub use rbuilder_error::{Error, Result};
pub use rbuilder_graph::CodeGraph;

/// Build information
pub const BUILD_INFO: &str = concat!(
    "rBuilder v",
    env!("CARGO_PKG_VERSION"),
    " (",
    env!("CARGO_PKG_REPOSITORY"),
    ")"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_build_info() {
        assert!(BUILD_INFO.contains("rBuilder"));
    }
}
"""
    )


def remove_old_language_dirs() -> None:
    for rel in ["src/languages/builtin", "src/languages/multimodal"]:
        path = ROOT / rel
        if path.exists():
            shutil.rmtree(path)
    for f in [
        "src/analysis/ansible_roles.rs",
        "src/analysis/chef_cookbooks.rs",
        "src/analysis/puppet_modules.rs",
        "src/cli/ansible.rs",
        "src/cli/chef.rs",
        "src/cli/puppet.rs",
        "src/security/ansible.rs",
        "src/security/chef.rs",
        "src/security/puppet.rs",
        "build.rs",
    ]:
        p = ROOT / f
        if p.exists():
            p.unlink()


def fix_extraction_tests() -> None:
    cargo = ROOT / "crates" / "rbuilder-extraction" / "Cargo.toml"
    text = cargo.read_text()
    if "rbuilder-bundle-full" not in text:
        text = text.replace(
            "[dev-dependencies]\ntempfile = { workspace = true }\n",
            "[dev-dependencies]\ntempfile = { workspace = true }\nrbuilder-bundle-full = { workspace = true }\n",
        )
        cargo.write_text(text)

    for rel in [
        "crates/rbuilder-extraction/src/extractor.rs",
        "crates/rbuilder-extraction/src/discovery/mod.rs",
        "crates/rbuilder-extraction/src/graph_builder.rs",
    ]:
        path = ROOT / rel
        text = path.read_text()
        text = text.replace(
            "rbuilder_registry::full_registry()",
            "rbuilder_bundle_full::default_registry()",
        )
        path.write_text(text)


def update_ci_workflows() -> None:
    bundles_yml = ROOT / ".github" / "workflows" / "language-bundles.yml"
    bundles_yml.write_text(
        """name: Bundle Testing

on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]

env:
  CARGO_TERM_COLOR: always

jobs:
  test-bundles:
    name: ${{ matrix.bundle }}
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        include:
          - bundle: minimal
            test: phase11_bundles
          - bundle: extended
            test: phase11_multimodal
          - bundle: full
            test: phase11_tier2_languages
          - bundle: extra
            test: phase11_tier2_niche_languages
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-bundle-${{ matrix.bundle }}-${{ hashFiles('**/Cargo.lock') }}
      - name: Test bundle crate
        run: cargo test -p rbuilder-bundle-${{ matrix.bundle }}
      - name: Integration test
        run: |
          cargo test -p rbuilder --no-default-features \\
            --features bundle-${{ matrix.bundle }},nlp-patterns,mcp-server \\
            --test ${{ matrix.test }}

  feature-matrix:
    name: ${{ matrix.name }}
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        include:
          - name: bundle-minimal
            features: bundle-minimal,nlp-patterns
            test_args: --test phase11_bundles --lib
          - name: bundle-extended
            features: bundle-extended,nlp-patterns,mcp-server
            test_args: --test phase11_bundles --test phase11_multimodal --test phase11_polyglot_bench
          - name: bundle-full
            features: bundle-full,nlp-patterns,mcp-server
            test_args: --test phase11_bundles --test phase11_tier2_languages --test phase11_multimodal --test phase11_multilang --test phase11_polyglot_bench
          - name: bundle-extra
            features: bundle-extra,nlp-patterns,mcp-server
            test_args: --test phase11_bundles --test phase11_tier2_languages --test phase11_tier2_niche_languages --test phase11_multimodal --test phase11_multilang --test phase11_polyglot_bench
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy
      - uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-bundles-${{ matrix.name }}-${{ hashFiles('**/Cargo.lock') }}
      - name: Build
        run: cargo build --no-default-features --features "${{ matrix.features }}"
      - name: Test
        run: cargo test --no-default-features --features "${{ matrix.features }}" ${{ matrix.test_args }}
      - name: Clippy
        run: cargo clippy --no-default-features --features "${{ matrix.features }}" -- -D warnings
"""
    )

    lang_crates_yml = ROOT / ".github" / "workflows" / "language-crates.yml"
    lang_crates_yml.write_text(
        """name: Language Crates

on:
  push:
    branches: [main, master]
  pull_request:
    branches: [main, master]

env:
  CARGO_TERM_COLOR: always

jobs:
  lang-crates:
    name: ${{ matrix.crate }}
    runs-on: ubuntu-latest
    strategy:
      fail-fast: false
      matrix:
        crate:
          - rbuilder-lang-rust
          - rbuilder-lang-python
          - rbuilder-lang-ansible
          - rbuilder-lang-c
          - rbuilder-lang-kotlin
          - rbuilder-lang-markdown
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test -p ${{ matrix.crate }}
      - run: cargo clippy -p ${{ matrix.crate }} -- -D warnings
"""
    )


def main() -> None:
    with (ROOT / "languages.toml").open("rb") as f:
        config = tomllib.load(f)

    languages: dict = config["languages"]
    bundles: dict = config["bundles"]

    print(f"Creating {len(languages)} language crates...")
    for lang_id, entry in languages.items():
        create_lang_crate(lang_id, entry)
        print(f"  {lang_crate_name(lang_id)}")

    print("Creating bundle crates...")
    for bundle_id, bundle in bundles.items():
        create_bundle_crate(bundle_id, bundle["languages"])
        print(f"  rbuilder-bundle-{bundle_id}")

    update_workspace_cargo(list(languages.keys()), list(bundles.keys()))
    update_registry_rs()
    update_languages_mod()
    update_monolith_reexports()
    remove_old_language_dirs()
    fix_extraction_tests()
    update_ci_workflows()
    print("Done.")


if __name__ == "__main__":
    main()
