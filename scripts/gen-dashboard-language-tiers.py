#!/usr/bin/env python3
"""Regenerate dashboard/src/v2/languageTiers.ts from languages.toml."""

from __future__ import annotations

import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOML = ROOT / "languages.toml"
OUT = ROOT / "dashboard" / "src" / "v2" / "languageTiers.ts"


def main() -> None:
    langs = tomllib.loads(TOML.read_text())["languages"]
    lines = [
        "/**",
        " * Per-language support tiers recorded in `languages.toml` (repo SSOT).",
        " *",
        " * Generated for the Instrument coverage display: only languages with a stated tier",
        " * can appear as covered, and only when the bundle actually contains their files.",
        " * Regenerate: `python3 scripts/gen-dashboard-language-tiers.py`",
        " */",
        "",
        "export type LanguageTier = 1 | 2;",
        "",
        "export const LANGUAGE_TIER: Record<string, LanguageTier> = {",
    ]
    for id_, meta in sorted(langs.items()):
        handler = meta.get("handler")
        tier = 1 if handler == "custom" else 2 if handler == "queries" else None
        if tier is None:
            continue
        lines.append(f"  {id_!r}: {tier},")
    lines.append("};")
    lines.append("")
    lines.append("/** File extension (no dot) → language id. First wins on collisions. */")
    lines.append("export const EXTENSION_TO_LANGUAGE: Record<string, string> = {")
    seen: dict[str, str] = {}
    for id_, meta in langs.items():
        if meta.get("handler") not in ("custom", "queries"):
            continue
        for ext in meta.get("extensions") or []:
            if ext not in seen:
                seen[ext] = id_
                lines.append(f"  {ext!r}: {id_!r},")
    lines.append("};")
    lines.append("")
    OUT.write_text("\n".join(lines) + "\n")
    print(f"wrote {OUT.relative_to(ROOT)} ({len(langs)} langs, {len(seen)} extensions)")


if __name__ == "__main__":
    main()
