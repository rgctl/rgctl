#!/usr/bin/env python3
"""
Audit and enforce rgctl Tier-1 vs Tier-2 language coverage across all 5 pillars:
  Pillar 1: Structured passive classification (SkipWrapper, SkipTypeToken, Unimplemented)
            & semantic node-kind denylist audit
  Pillar 2: Quantitative semantic AST coverage ratio, grade (A+, A, B, C), and ratchet floor
  Pillar 3: Dispatch table gating (mask.is_passive()) & 3-way manifest sync
  Pillar 4: Cross-layer behavioral parity (Layer A plugin, Layer B CFG/PDG, Layer C taint, Layer F field_write_locals)
  Pillar 5: Test exercise enforcement (hard assert_all_exercised vs soft fallback vs missing)

Usage:
  python3 scripts/check-tier1-coverage.py
  python3 scripts/check-tier1-coverage.py --json
  python3 scripts/check-tier1-coverage.py --language java
  python3 scripts/check-tier1-coverage.py --run-tests
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

try:
    import tomllib  # Python 3.11+
except ImportError:
    import tomli as tomllib  # type: ignore


REPO_ROOT = Path(__file__).resolve().parent.parent

ACTIVE_LABELS = {
    "Symbol",
    "Relation",
    "Property",
    "CfgStatement",
    "DefUse",
    "AstSkeleton",
    "Literal",
}
PASSIVE_LABELS = {"Skip", "SkipWrapper", "SkipTypeToken", "Unimplemented"}


def is_semantic_node_kind(kind: str) -> bool:
    """Mirrors crates/rgctl-ast-coverage/src/lib.rs::is_semantic_node_kind."""
    suffixes = (
        "_declaration",
        "_definition",
        "_statement",
        "_expression",
        "_invocation",
        "_clause",
        "_pattern",
        "_directive",
    )
    if any(kind.endswith(s) for s in suffixes):
        return True
    if "_call" in kind or "_creation" in kind:
        return True
    if kind.startswith("lambda") or kind.startswith("closure"):
        return True
    return False


def grade_for_ratio(ratio: float) -> str:
    """Mirrors crates/rgctl-ast-coverage/src/lib.rs::CoverageGrade."""
    pct = ratio * 100.0
    if pct >= 98.0:
        return "A+"
    if pct >= 75.0:
        return "A"
    if pct >= 50.0:
        return "B"
    return "C"


GRADE_RANK = {"C": 0, "B": 1, "A": 2, "A+": 3}


@dataclass
class AstCoverageAudit:
    id: str
    manifest_path: str
    total_grammar_kinds: int
    handled_kinds: int
    skip_kinds: int
    skip_wrapper_kinds: int
    skip_type_token_kinds: int
    unimplemented_kinds: int
    semantic_denominator: int
    coverage_ratio: float
    current_grade: str
    min_handled_kinds: int
    target_grade: str
    ratchet_ok: bool
    skip_justifications_count: int
    legacy_semantic_skips: int
    legacy_semantic_skip_examples: List[str]
    kinds_needed_for_a: int
    kinds_needed_for_a_plus: int
    has_passive_dispatch_gate: bool
    has_single_pass: bool
    layer_a_plugin: bool
    layer_b_cfg: bool
    layer_c_taint: bool
    layer_f_field_write: bool
    depth_test_mode: str  # "hard_assert", "soft_fallback", "none"
    effective_tier: str   # "Tier-1 Gold", "Tier-1 Standard", "Tier-1 Partial", "Tier-1 DSL"
    blockers_to_gold: List[str] = field(default_factory=list)


def parse_bundled_specs_from_rust() -> Dict[str, Tuple[str, int, str]]:
    """Parse bundled_specs() in crates/rgctl-ast-coverage/src/lib.rs."""
    lib_rs = (REPO_ROOT / "crates/rgctl-ast-coverage/src/lib.rs").read_text(encoding="utf-8")
    specs: Dict[str, Tuple[str, int, str]] = {}
    pattern = re.compile(
        r'CoverageSpec\s*\{\s*id:\s*"([^"]+)",\s*crate_dir:\s*"([^"]+)",\s*'
        r'manifest_file:\s*"([^"]+)",.*?'
        r"min_handled_kinds:\s*(\d+),\s*target_grade:\s*CoverageGrade::(\w+)",
        re.DOTALL,
    )
    grade_map = {"APlus": "A+", "A": "A", "B": "B", "C": "C"}
    for match in pattern.finditer(lib_rs):
        lang_id, crate_dir, manifest_file, min_handled, target_grade = match.groups()
        rel_path = f"crates/{crate_dir}/{manifest_file}"
        specs[lang_id] = (rel_path, int(min_handled), grade_map.get(target_grade, target_grade))
    return specs


def parse_language_profiles() -> Tuple[Set[str], Set[str]]:
    """Return (cfg_enabled_ids, taint_enabled_ids) from crates/rgctl-analysis/src/language_profile.rs."""
    src = (REPO_ROOT / "crates/rgctl-analysis/src/language_profile.rs").read_text(encoding="utf-8")
    cfg_ids: Set[str] = set()
    taint_ids: Set[str] = set()
    pattern = re.compile(
        r'LanguageAnalysisProfile\s*\{\s*id:\s*"([^"]+)",.*?'
        r"cfg_enabled:\s*(true|false),\s*taint_enabled:\s*(true|false)",
        re.DOTALL,
    )
    for match in pattern.finditer(src):
        lang_id, cfg_en, taint_en = match.groups()
        if cfg_en == "true":
            cfg_ids.add(lang_id)
        if taint_en == "true":
            taint_ids.add(lang_id)
    return cfg_ids, taint_ids


def parse_field_write_languages() -> Set[str]:
    """Return language ids handled in crates/rgctl-analysis/src/field_write_locals.rs."""
    src = (REPO_ROOT / "crates/rgctl-analysis/src/field_write_locals.rs").read_text(encoding="utf-8")
    fn_block = re.search(
        r"pub fn language_visit_fn\(.*?\n\}",
        src,
        re.DOTALL,
    )
    if not fn_block:
        return set()
    text = fn_block.group(0)
    return set(re.findall(r'"([a-z0-9_]+)"', text))


def inspect_depth_test_mode(lang_id: str) -> str:
    """Check whether crates/rgctl-lang-{id}/tests/*_depth_extraction.rs uses hard or soft assert."""
    test_dir = REPO_ROOT / f"crates/rgctl-lang-{lang_id}/tests"
    if not test_dir.is_dir():
        return "none"
    for path in sorted(test_dir.glob("*.rs")):
        text = path.read_text(encoding="utf-8")
        if "assert_all_exercised" in text:
            if "match tracker.assert_all_exercised()" in text or "assert_present_kinds_exercised" in text:
                return "soft_fallback"
            return "hard_assert"
    return "none"


def audit_tier1_languages() -> List[AstCoverageAudit]:
    specs = parse_bundled_specs_from_rust()
    cfg_ids, taint_ids = parse_language_profiles()
    fw_ids = parse_field_write_languages()

    results: List[AstCoverageAudit] = []
    for lang_id, (rel_path, min_handled, target_grade) in specs.items():
        manifest_file = REPO_ROOT / rel_path
        manifest = json.loads(manifest_file.read_text(encoding="utf-8"))
        named_kinds: Dict[str, str] = manifest.get("handlers", {})
        justifications: Dict[str, str] = manifest.get("skip_justifications", {})

        total = len(named_kinds)
        handled = 0
        skip = 0
        skip_wrapper = 0
        skip_type_token = 0
        unimplemented = 0
        legacy_semantic_skips: List[str] = []

        for kind, label in sorted(named_kinds.items()):
            if label in ACTIVE_LABELS:
                handled += 1
            elif label == "Skip":
                skip += 1
                if is_semantic_node_kind(kind):
                    legacy_semantic_skips.append(kind)
            elif label == "SkipWrapper":
                skip_wrapper += 1
            elif label == "SkipTypeToken":
                skip_type_token += 1
            elif label == "Unimplemented":
                unimplemented += 1

        denom = handled + skip + unimplemented
        ratio = handled / denom if denom > 0 else 1.0
        current_grade = grade_for_ratio(ratio)
        ratchet_ok = (handled >= min_handled) and (
            GRADE_RANK[current_grade] >= GRADE_RANK[target_grade]
        )

        # How many additional handled kinds (converting from Unimplemented) to reach A (75%) / A+ (98%)
        import math

        needed_a = max(0, math.ceil(0.75 * denom) - handled)
        needed_a_plus = max(0, math.ceil(0.98 * denom) - handled)

        single_pass_path = REPO_ROOT / f"crates/rgctl-lang-{lang_id}/src/single_pass.rs"
        has_single_pass = single_pass_path.is_file()
        has_passive_gate = False
        if has_single_pass:
            sp_text = single_pass_path.read_text(encoding="utf-8")
            has_passive_gate = "is_passive()" in sp_text

        layer_b = lang_id in cfg_ids
        layer_c = lang_id in taint_ids
        layer_f = lang_id in fw_ids
        depth_mode = inspect_depth_test_mode(lang_id)

        blockers: List[str] = []
        is_dsl = lang_id in {"erb", "sql", "hcl", "markdown"}
        if GRADE_RANK[current_grade] < GRADE_RANK["A+"]:
            blockers.append(
                f"Coverage grade is {current_grade} ({ratio*100:.1f}%, need +{needed_a_plus} active/wrapped kinds for A+)"
            )
        if legacy_semantic_skips:
            blockers.append(
                f"{len(legacy_semantic_skips)} semantic AST kinds still use raw 'Skip' (need active handler or SkipWrapper+justification)"
            )
        if not has_passive_gate:
            blockers.append("single_pass.rs not gated on mask.is_passive()")
        if not is_dsl:
            if not layer_b:
                blockers.append("Missing Layer B (CFG/PDG)")
            if not layer_c:
                blockers.append("Missing Layer C (Taint rules)")
            if not layer_f:
                blockers.append("Missing Layer F (field_write_locals)")
        if depth_mode != "hard_assert":
            blockers.append(f"Test exercise gate is '{depth_mode}' (need unconditional assert_all_exercised)")

        if not blockers and not is_dsl:
            effective_tier = "Tier-1 Gold"
        elif is_dsl:
            effective_tier = "Tier-1 DSL"
        elif layer_b and layer_c and layer_f:
            effective_tier = "Tier-1 Deep"
        else:
            effective_tier = "Tier-1 Shallow"

        results.append(
            AstCoverageAudit(
                id=lang_id,
                manifest_path=rel_path,
                total_grammar_kinds=total,
                handled_kinds=handled,
                skip_kinds=skip,
                skip_wrapper_kinds=skip_wrapper,
                skip_type_token_kinds=skip_type_token,
                unimplemented_kinds=unimplemented,
                semantic_denominator=denom,
                coverage_ratio=round(ratio, 4),
                current_grade=current_grade,
                min_handled_kinds=min_handled,
                target_grade=target_grade,
                ratchet_ok=ratchet_ok,
                skip_justifications_count=len(justifications),
                legacy_semantic_skips=len(legacy_semantic_skips),
                legacy_semantic_skip_examples=legacy_semantic_skips[:8],
                kinds_needed_for_a=needed_a,
                kinds_needed_for_a_plus=needed_a_plus,
                has_passive_dispatch_gate=has_passive_gate,
                has_single_pass=has_single_pass,
                layer_a_plugin=True,
                layer_b_cfg=layer_b,
                layer_c_taint=layer_c,
                layer_f_field_write=layer_f,
                depth_test_mode=depth_mode,
                effective_tier=effective_tier,
                blockers_to_gold=blockers,
            )
        )
    return results


def load_all_languages_toml() -> Tuple[List[str], List[str], List[str]]:
    """Return (custom_tier1, mainframe_custom, tier2_queries) from languages.toml."""
    data = tomllib.loads((REPO_ROOT / "languages.toml").read_text(encoding="utf-8"))
    langs = data.get("languages", {})
    specs = parse_bundled_specs_from_rust()
    custom_with_manifest: List[str] = []
    mainframe_custom: List[str] = []
    tier2_queries: List[str] = []
    for lang_id, cfg in langs.items():
        handler = cfg.get("handler", "")
        if handler == "custom":
            if lang_id in specs:
                custom_with_manifest.append(lang_id)
            else:
                mainframe_custom.append(lang_id)
        elif handler == "queries":
            tier2_queries.append(lang_id)
    for extra in specs:
        if extra not in custom_with_manifest:
            custom_with_manifest.append(extra)
    return custom_with_manifest, mainframe_custom, sorted(tier2_queries)


def print_report(
    audits: List[AstCoverageAudit],
    mainframe_custom: List[str],
    tier2_queries: List[str],
    single_lang: Optional[str] = None,
) -> None:
    if single_lang:
        matched = [a for a in audits if a.id == single_lang]
        if not matched:
            print(f"Language '{single_lang}' not found in Tier-1 AST coverage manifests.", file=sys.stderr)
            sys.exit(1)
        a = matched[0]
        print(f"=== Tier-1 Enforcement Audit: {a.id} ({a.effective_tier}) ===")
        print(f"  Manifest:              {a.manifest_path}")
        print(f"  Total Grammar Kinds:   {a.total_grammar_kinds}")
        print(f"  Active Handled Kinds:  {a.handled_kinds} (ratchet min: {a.min_handled_kinds})")
        print(
            f"  Passive Breakdown:     Skip={a.skip_kinds}, SkipWrapper={a.skip_wrapper_kinds}, "
            f"SkipTypeToken={a.skip_type_token_kinds}, Unimplemented={a.unimplemented_kinds}"
        )
        print(f"  Skip Justifications:   {a.skip_justifications_count}")
        print(
            f"  Semantic Coverage:     {a.handled_kinds}/{a.semantic_denominator} "
            f"({a.coverage_ratio*100:.1f}%) -> Grade {a.current_grade} (target: {a.target_grade})"
        )
        print(f"  Legacy Semantic Skips: {a.legacy_semantic_skips}")
        if a.legacy_semantic_skip_examples:
            print(f"    Examples:            {', '.join(a.legacy_semantic_skip_examples)}")
        print(f"  Passive Dispatch Gate: {'YES' if a.has_passive_dispatch_gate else 'NO'}")
        print(
            f"  Analysis Layers:       A(Plugin)={'Y' if a.layer_a_plugin else 'N'} "
            f"B(CFG/PDG)={'Y' if a.layer_b_cfg else 'N'} "
            f"C(Taint)={'Y' if a.layer_c_taint else 'N'} "
            f"F(FieldWrite)={'Y' if a.layer_f_field_write else 'N'}"
        )
        print(f"  Test Exercise Gate:    {a.depth_test_mode}")
        if a.blockers_to_gold:
            print("  Blockers to Tier-1 Gold:")
            for b in a.blockers_to_gold:
                print(f"    - {b}")
        else:
            print("  Status:                PASSES ALL 5 PILLARS (Tier-1 Gold Reference)")
        return

    gold = [a for a in audits if a.effective_tier == "Tier-1 Gold"]
    standard = [a for a in audits if a.effective_tier == "Tier-1 Deep"]
    partial = [a for a in audits if a.effective_tier == "Tier-1 Shallow"]
    dsl = [a for a in audits if a.effective_tier == "Tier-1 DSL"]

    print("=" * 118)
    print("  rgctl LANGUAGE TIER & 5-PILLAR ENFORCEMENT REPORT")
    print("=" * 118)
    print(
        f"  Total Languages: {len(audits) + len(mainframe_custom) + len(tier2_queries)}  |  "
        f"Tier-1 Gold: {len(gold)}  |  Tier-1 Deep: {len(standard)}  |  "
        f"Tier-1 Shallow: {len(partial)}  |  Tier-1 DSL: {len(dsl)}  |  "
        f"Mainframe Custom: {len(mainframe_custom)}  |  Tier-2 (tags.scm): {len(tier2_queries)}"
    )
    print("-" * 118)
    header = (
        f"{'Lang':<11} {'Effective Tier':<16} {'Handled/Sem':<12} {'Cov %':>6} "
        f"{'Grd':>3} {'Min':>4} {'Wrap/Tok/Unimp':<14} {'SemSkip':>7} "
        f"{'Gate':>4} {'Layers':<8} {'TestGate':<13}"
    )
    print(header)
    print("-" * 118)

    for a in sorted(audits, key=lambda x: (-GRADE_RANK[x.current_grade], -x.coverage_ratio, x.id)):
        hs = f"{a.handled_kinds}/{a.semantic_denominator}"
        wtu = f"{a.skip_wrapper_kinds}/{a.skip_type_token_kinds}/{a.unimplemented_kinds}"
        layers = (
            ("A" if a.layer_a_plugin else "-")
            + ("B" if a.layer_b_cfg else "-")
            + ("C" if a.layer_c_taint else "-")
            + ("F" if a.layer_f_field_write else "-")
        )
        gate_str = "YES" if a.has_passive_dispatch_gate else "NO"
        print(
            f"{a.id:<11} {a.effective_tier:<16} {hs:<12} {a.coverage_ratio*100:>5.1f}% "
            f"{a.current_grade:>3} {a.min_handled_kinds:>4} {wtu:<14} {a.legacy_semantic_skips:>7} "
            f"{gate_str:>4} {layers:<8} {a.depth_test_mode:<13}"
        )

    print("-" * 118)
    print(f"Mainframe Custom Plugins ({len(mainframe_custom)}, line-oriented, no tree-sitter AST manifest):")
    print("  " + ", ".join(mainframe_custom))
    print(f"\nTier-2 Declarative tags.scm Plugins ({len(tier2_queries)}, symbol/reference extraction only):")
    for i in range(0, len(tier2_queries), 12):
        print("  " + ", ".join(tier2_queries[i : i + 12]))
    print("=" * 118)


def main() -> int:
    parser = argparse.ArgumentParser(description="Audit rgctl Tier-1 vs Tier-2 language enforcement.")
    parser.add_argument("--json", action="store_true", help="Emit machine-readable JSON report")
    parser.add_argument("--language", "-l", type=str, help="Inspect a single Tier-1 language in detail")
    parser.add_argument(
        "--run-tests",
        action="store_true",
        help="Also run cargo test -p rgctl-ast-coverage and tier1_parity_gate",
    )
    args = parser.parse_args()

    audits = audit_tier1_languages()
    _, mainframe_custom, tier2_queries = load_all_languages_toml()

    ratchet_failures = [a for a in audits if not a.ratchet_ok]

    if args.json:
        payload = {
            "summary": {
                "total_languages": len(audits) + len(mainframe_custom) + len(tier2_queries),
                "tier1_gold": [a.id for a in audits if a.effective_tier == "Tier-1 Gold"],
                "tier1_deep": [a.id for a in audits if a.effective_tier == "Tier-1 Deep"],
                "tier1_shallow": [a.id for a in audits if a.effective_tier == "Tier-1 Shallow"],
                "tier1_dsl": [a.id for a in audits if a.effective_tier == "Tier-1 DSL"],
                "mainframe_custom": mainframe_custom,
                "tier2_queries": tier2_queries,
                "all_ratchets_passed": len(ratchet_failures) == 0,
            },
            "tier1_audits": [asdict(a) for a in audits],
        }
        print(json.dumps(payload, indent=2))
    else:
        print_report(audits, mainframe_custom, tier2_queries, single_lang=args.language)

    if ratchet_failures:
        for f in ratchet_failures:
            print(
                f"FAIL: {f.id} ratchet failed: handled={f.handled_kinds} (min={f.min_handled_kinds}), "
                f"grade={f.current_grade} (target={f.target_grade})",
                file=sys.stderr,
            )
        return 1

    if args.run_tests:
        cmds = [
            ["cargo", "test", "-p", "rgctl-ast-coverage"],
            ["cargo", "test", "-p", "rgctl-analysis", "--test", "tier1_parity_gate"],
        ]
        for cmd in cmds:
            print(f"\n==> Running: {' '.join(cmd)}")
            res = subprocess.run(cmd, cwd=REPO_ROOT)
            if res.returncode != 0:
                return res.returncode

    return 0


if __name__ == "__main__":
    sys.exit(main())
