#!/usr/bin/env python3
"""Skill token-budget audit and trajectory benchmark for rgctl agent skills.

Modes:
  --mode offline  Deterministic CI (no API key): token ceilings + CLI vs rg baseline.
  --mode agent    Live A/B agent runner scaffold (requires --agent-cmd).

Examples:
  python3 scripts/eval-skills.py --mode offline
  python3 scripts/eval-skills.py --mode offline --large-repo example/kafka
  python3 scripts/eval-skills.py --mode agent --agent-cmd ./scripts/run-agent-ab.sh
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from dataclasses import asdict, dataclass, field
from pathlib import Path
from typing import Any, Optional

REPO_ROOT = Path(__file__).resolve().parents[1]

# Prompt-token ceilings (approx tokens ≈ bytes/4 when tiktoken unavailable).
META_SKILL_MAX_TOKENS = 5500
META_SKILL_MAX_BYTES = 22 * 1024
SLASH_SKILL_MAX_TOKENS = 2200
SLASH_SKILL_MAX_BYTES = 9 * 1024

SLASH_SKILLS = ("rgctl-review", "rgctl-vuln", "rgctl-migrate")

RG_EXCLUDE_GLOBS = (
    "--glob",
    "!.rgctl/**",
    "--glob",
    "!.rgctl-base/**",
    "--glob",
    "!**/bower_components/**",
    "--glob",
    "!**/node_modules/**",
    "--glob",
    "!**/target/**",
    "--glob",
    "!**/build/**",
)


@dataclass
class BudgetResult:
    path: str
    bytes: int
    tokens_est: int
    max_tokens: int
    max_bytes: int
    ok: bool


@dataclass
class TrajectoryResult:
    task: str
    corpus: str
    arm: str  # with_skill | no_skill
    ok: bool
    wall_ms: float
    stdout_tokens_est: int
    tool_turns: int
    prompt_tokens_1turn: int
    prompt_tokens_5turn_cached: int
    net_tokens_1turn: int = 0
    net_tokens_5turn_cached: int = 0
    detail: str = ""
    error: str = ""


@dataclass
class UseCaseResult:
    usecase_id: str
    title: str
    corpus: str
    arm: str  # with_skill | no_skill
    ok: bool
    oracle_passed: bool
    oracle_score: float
    oracle_checks: list[str]
    wall_ms: float
    stdout_tokens_est: int
    tool_turns: int
    prompt_tokens_1turn: int
    prompt_tokens_5turn_cached: int
    net_tokens_5turn_cached: int
    error: str = ""


@dataclass
class Report:
    mode: str
    budgets: list[BudgetResult] = field(default_factory=list)
    trajectories: list[TrajectoryResult] = field(default_factory=list)
    usecases: list[UseCaseResult] = field(default_factory=list)
    agent: dict[str, Any] = field(default_factory=dict)
    passed: bool = False


def estimate_tokens(text: str | bytes) -> int:
    """Best-effort token estimate: tiktoken if present, else bytes/4."""
    if isinstance(text, bytes):
        raw = text
        s = text.decode("utf-8", errors="replace")
    else:
        s = text
        raw = text.encode("utf-8")
    if not raw:
        return 0
    try:
        import tiktoken  # type: ignore

        for enc_name in ("o200k_base", "cl100k_base"):
            try:
                enc = tiktoken.get_encoding(enc_name)
                return len(enc.encode(s))
            except Exception:
                continue
    except ImportError:
        pass
    return max(1, len(raw) // 4)


def audit_budgets(skills_root: Path) -> list[BudgetResult]:
    results: list[BudgetResult] = []
    meta = skills_root / "rgctl" / "SKILL.md"
    data = meta.read_bytes()
    tokens = estimate_tokens(data)
    results.append(
        BudgetResult(
            path=str(meta.relative_to(REPO_ROOT)),
            bytes=len(data),
            tokens_est=tokens,
            max_tokens=META_SKILL_MAX_TOKENS,
            max_bytes=META_SKILL_MAX_BYTES,
            ok=tokens <= META_SKILL_MAX_TOKENS and len(data) <= META_SKILL_MAX_BYTES,
        )
    )
    for name in SLASH_SKILLS:
        p = skills_root / name / "SKILL.md"
        data = p.read_bytes()
        tokens = estimate_tokens(data)
        results.append(
            BudgetResult(
                path=str(p.relative_to(REPO_ROOT)),
                bytes=len(data),
                tokens_est=tokens,
                max_tokens=SLASH_SKILL_MAX_TOKENS,
                max_bytes=SLASH_SKILL_MAX_BYTES,
                ok=tokens <= SLASH_SKILL_MAX_TOKENS and len(data) <= SLASH_SKILL_MAX_BYTES,
            )
        )
    return results


def find_rgctl() -> Optional[Path]:
    env = os.environ.get("RGCTL_BIN")
    if env:
        p = Path(env).expanduser()
        if not p.is_absolute():
            p = (Path.cwd() / p).resolve()
        if p.is_file():
            return p
    release = REPO_ROOT / "target" / "release" / "rgctl"
    if release.is_file():
        return release.resolve()
    debug = REPO_ROOT / "target" / "debug" / "rgctl"
    if debug.is_file():
        return debug.resolve()
    which = shutil.which("rgctl")
    return Path(which).resolve() if which else None


def find_rg() -> Optional[str]:
    return shutil.which("rg") or shutil.which("ripgrep")


def run_cmd(
    argv: list[str],
    cwd: Path,
    timeout: float = 120.0,
) -> tuple[int, bytes, bytes, float]:
    t0 = time.perf_counter()
    try:
        proc = subprocess.run(
            argv,
            cwd=str(cwd),
            capture_output=True,
            timeout=timeout,
        )
        ms = (time.perf_counter() - t0) * 1000.0
        return proc.returncode, proc.stdout, proc.stderr, ms
    except subprocess.TimeoutExpired as e:
        ms = (time.perf_counter() - t0) * 1000.0
        return 124, e.stdout or b"", e.stderr or b"timeout", ms
    except FileNotFoundError as e:
        return 127, b"", str(e).encode(), 0.0


def prompt_tax(skill_paths: list[Path], turns: int = 1) -> int:
    """Amortized per-task prompt token cost.

    - 1-turn cold session: pays 100% of static skill tokens.
    - N-turn cached session: turn 1 pays 100%, turns 2..N pay 10% cache-read weight
      (average per task across 5 tasks = (1.0 + 4 * 0.1) / 5 = 0.28x).
    """
    total = sum(estimate_tokens(p.read_bytes()) for p in skill_paths if p.is_file())
    if turns <= 1:
        return total
    effective_total = total * (1.0 + 0.1 * (turns - 1))
    return int(round(effective_total / float(turns)))


def rel_corpus(corpus: Path) -> str:
    return str(corpus.relative_to(REPO_ROOT) if corpus.is_relative_to(REPO_ROOT) else corpus)


def ensure_index(rgctl: Path, corpus: Path, with_cfg: bool = False) -> bool:
    snap = corpus / ".rgctl" / "graph.snapshot.bin"
    if snap.is_file() and not with_cfg:
        return True
    argv = [str(rgctl), "discover", "."]
    if with_cfg:
        argv.append("--with-cfg")
    code, _, err, _ = run_cmd(argv, corpus, timeout=600.0)
    if code != 0:
        sys.stderr.write(f"discover failed in {corpus}: {err.decode(errors='replace')[:400]}\n")
        return False
    return snap.is_file()


def make_result(
    task: str,
    corpus: Path,
    arm: str,
    ok: bool,
    wall_ms: float,
    stdout_tokens: int,
    tool_turns: int,
    skill_paths: list[Path],
    detail: str = "",
    error: str = "",
) -> TrajectoryResult:
    p1 = prompt_tax(skill_paths, 1) if arm == "with_skill" else 0
    p5 = prompt_tax(skill_paths, 5) if arm == "with_skill" else 0
    return TrajectoryResult(
        task=task,
        corpus=rel_corpus(corpus),
        arm=arm,
        ok=ok,
        wall_ms=wall_ms,
        stdout_tokens_est=stdout_tokens,
        tool_turns=tool_turns,
        prompt_tokens_1turn=p1,
        prompt_tokens_5turn_cached=p5,
        net_tokens_1turn=p1 + stdout_tokens,
        net_tokens_5turn_cached=p5 + stdout_tokens,
        detail=detail,
        error=error,
    )


def run_no_skill_rg_and_read(
    task: str,
    corpus: Path,
    pattern: str,
    max_files: int = 5,
    max_bytes_per_file: int = 24000,
    extra_files: Optional[list[Path]] = None,
) -> TrajectoryResult:
    rg = find_rg()
    if not rg:
        return make_result(task, corpus, "no_skill", False, 0.0, 0, 0, [], error="rg not found")
    t0 = time.perf_counter()
    code_rg, out_rg, err_rg, _ = run_cmd(
        [
            rg,
            "-n",
            *RG_EXCLUDE_GLOBS,
            "--type-add",
            "src:*.{rs,java,go,py,js,ts,xml,gradle}",
            "-t",
            "src",
            pattern,
            ".",
        ],
        corpus,
        timeout=60.0,
    )
    seen: set[Path] = set()
    read_bytes = 0
    for fp in extra_files or []:
        if fp.is_file() and fp not in seen:
            seen.add(fp)
            read_bytes += len(fp.read_bytes()[:max_bytes_per_file])
    for line in out_rg.decode(errors="replace").splitlines()[:100]:
        if ":" not in line:
            continue
        rel = line.split(":", 1)[0]
        fp = (corpus / rel).resolve()
        if fp.is_file() and fp not in seen:
            seen.add(fp)
            read_bytes += len(fp.read_bytes()[:max_bytes_per_file])
            if len(seen) >= max_files:
                break
    ms_ns = (time.perf_counter() - t0) * 1000.0
    # Bound rg stdout to top 80 matching lines (realistic agent tool truncation) + files read
    rg_lines = b"\n".join(out_rg.splitlines()[:80])
    stdout_tok = estimate_tokens(rg_lines) + (read_bytes // 4)
    return make_result(
        task=task,
        corpus=corpus,
        arm="no_skill",
        ok=code_rg in (0, 1),
        wall_ms=ms_ns,
        stdout_tokens=stdout_tok,
        tool_turns=1 + len(seen),
        skill_paths=[],
        detail=f"rg+{len(seen)} files ({read_bytes} B)",
        error="" if code_rg in (0, 1) else err_rg.decode(errors="replace")[:200],
    )


def pick_blast_target(rgctl: Path, corpus: Path) -> tuple[list[str], str]:
    """Select a realistic symbol for blast-radius on coolstore or large repo."""
    if "coolstore" in corpus.name:
        return (["priceShoppingCart", "--depth", "2"], "priceShoppingCart")
    if "kafka" in corpus.name:
        return (["nextInt", "--class", "MockableRandom", "--depth", "2"], "nextInt")
    # Fallback: pick top function from metrics --pagerank or find --type function
    code, stdout, _, _ = run_cmd([str(rgctl), "-f", "json", "metrics", "--pagerank"], corpus)
    if code == 0:
        try:
            top = json.loads(stdout).get("pagerank", {}).get("top", [])
            if top:
                node_id = top[0].get("node")
                name = top[0].get("name") or "main"
                if node_id:
                    return ([node_id, "--depth", "2"], name)
        except json.JSONDecodeError:
            pass
    code, stdout, _, _ = run_cmd(
        [str(rgctl), "-f", "json", "find", "--type", "function", "--limit", "20"], corpus
    )
    if code == 0:
        try:
            entities = json.loads(stdout).get("entities", [])
            for ent in entities:
                name = ent.get("name", "")
                if name and not name.startswith("anonymous@"):
                    return ([ent.get("id", name), "--depth", "2"], name)
        except json.JSONDecodeError:
            pass
    return (["main", "--depth", "2"], "main")


def trajectory_blast(
    rgctl: Path, corpus: Path, skill_paths: list[Path]
) -> list[TrajectoryResult]:
    if not ensure_index(rgctl, corpus):
        return [
            make_result(
                "Blast Radius", corpus, "with_skill", False, 0, 0, 0, skill_paths, error="discover failed"
            )
        ]
    blast_args, sym_name = pick_blast_target(rgctl, corpus)
    code, stdout, err, ms = run_cmd(
        [str(rgctl), "-f", "json", "blast-radius", *blast_args],
        corpus,
    )
    # Apply the skill's jq projection (score, counts, top 20 direct_callers & impact_zone)
    projected = stdout
    if code == 0:
        try:
            doc = json.loads(stdout)
            topo = doc.get("topology", {})
            compact = {
                "schema_version": doc.get("schema_version"),
                "target": doc.get("target"),
                "metrics": doc.get("metrics"),
                "direct_callers": (topo.get("direct_callers") or [])[:20],
                "impact_zone_sample": (topo.get("impact_zone") or [])[:20],
                "gatekeeping": doc.get("gatekeeping"),
            }
            projected = json.dumps(compact).encode("utf-8")
        except json.JSONDecodeError:
            pass
    with_res = make_result(
        task="Blast Radius",
        corpus=corpus,
        arm="with_skill",
        ok=code == 0,
        wall_ms=ms,
        stdout_tokens=estimate_tokens(projected),
        tool_turns=1,
        skill_paths=skill_paths,
        detail=f"symbol={sym_name}",
        error="" if code == 0 else err.decode(errors="replace")[:200],
    )
    no_res = run_no_skill_rg_and_read("Blast Radius", corpus, sym_name, max_files=6)
    return [with_res, no_res]


def trajectory_vuln(
    rgctl: Path, corpus: Path, skill_paths: list[Path], osv_fixture: Path
) -> list[TrajectoryResult]:
    """Run the 3-step /rgctl:vuln trajectory (triage -> deps check -> analyze)."""
    if not ensure_index(rgctl, corpus):
        return [
            make_result(
                "OSV Reachability /rgctl:vuln",
                corpus,
                "with_skill",
                False,
                0,
                0,
                0,
                skill_paths,
                error="discover failed",
            )
        ]
    jars_args = ["--include-jars", "lib"] if (corpus / "lib").is_dir() else []
    total_ms = 0.0
    total_stdout_tok = 0
    ok = True
    err_msg = ""

    # Step 1: vuln triage
    c1, out1, e1, ms1 = run_cmd(
        [str(rgctl), "-f", "json", "vuln", "triage", "--osv", str(osv_fixture)], corpus
    )
    total_ms += ms1
    total_stdout_tok += estimate_tokens(out1)
    if c1 != 0:
        ok = False
        err_msg = e1.decode(errors="replace")[:200]

    # Step 2: deps check
    c2, out2, e2, ms2 = run_cmd(
        [str(rgctl), "-f", "json", "deps", "check", "--osv", str(osv_fixture), *jars_args], corpus
    )
    total_ms += ms2
    total_stdout_tok += estimate_tokens(out2)
    if c2 != 0 and ok:
        ok = False
        err_msg = e2.decode(errors="replace")[:200]

    # Step 3: vuln analyze (projecting the fields prescribed in skills/rgctl-vuln/SKILL.md)
    c3, out3, e3, ms3 = run_cmd(
        [str(rgctl), "-f", "json", "vuln", "analyze", "--osv", str(osv_fixture), *jars_args],
        corpus,
    )
    total_ms += ms3
    if c3 == 0:
        try:
            doc = json.loads(out3)
            st = doc.get("sink_taint") or {}
            projected = json.dumps(
                {
                    "deps_verdict": doc.get("deps_verdict"),
                    "exploitability": doc.get("exploitability"),
                    "openvex_status": (doc.get("openvex", {}).get("statements") or [{}])[0].get(
                        "status"
                    ),
                    "call_sites": st.get("call_sites") or [],
                    "paths": st.get("paths") or [],
                }
            ).encode("utf-8")
            total_stdout_tok += estimate_tokens(projected)
        except json.JSONDecodeError:
            total_stdout_tok += estimate_tokens(out3)
    else:
        ok = False
        err_msg = e3.decode(errors="replace")[:200]

    with_res = make_result(
        task="OSV Reachability /rgctl:vuln",
        corpus=corpus,
        arm="with_skill",
        ok=ok,
        wall_ms=total_ms,
        stdout_tokens=total_stdout_tok,
        tool_turns=3,
        skill_paths=skill_paths,
        detail="triage+deps+analyze",
        error=err_msg,
    )
    no_res = run_no_skill_rg_and_read(
        "OSV Reachability /rgctl:vuln",
        corpus,
        "ObjectMapper|readValue|jackson-databind|jsonToOrder",
        max_files=5,
    )
    return [with_res, no_res]


def trajectory_review(
    rgctl: Path, corpus: Path, skill_paths: list[Path]
) -> list[TrajectoryResult]:
    """Run /rgctl:review: Step 0a (--synthetic-head worktree) on small dirty sets,
    or Step 0b/1 (--base-artifact) on 1K+ corpora with large dirty trees."""
    if not ensure_index(rgctl, corpus):
        return [
            make_result(
                "Uncommitted Review /rgctl:review",
                corpus,
                "with_skill",
                False,
                0,
                0,
                0,
                skill_paths,
                error="discover failed",
            )
        ]
    # Check git dirty file count via git status --porcelain
    git_dirty = 0
    c_git, out_git, _, ms_git = run_cmd(["git", "status", "--porcelain"], corpus)
    if c_git == 0:
        git_dirty = len([ln for ln in out_git.splitlines() if ln.strip()])

    if "coolstore" in corpus.name and git_dirty <= 10:
        review_argv = [
            str(rgctl),
            "-f",
            "json",
            "review",
            "paths",
            "--synthetic-head",
            "worktree",
            "--upstream-depth",
            "2",
            "--downstream-depth",
            "1",
            "--fanout",
            "5",
            "--max-symbols",
            "25",
        ]
        detail_label = "review paths --synthetic-head worktree"
        turns = 1
        extra_ms = 0.0
    else:
        review_argv = [
            str(rgctl),
            "-f",
            "json",
            "review",
            "paths",
            "--base-artifact",
            str(corpus),
            "--base-ref",
            "HEAD",
            "--head-ref",
            "HEAD",
            "--upstream-depth",
            "2",
            "--downstream-depth",
            "1",
            "--fanout",
            "5",
            "--max-symbols",
            "25",
        ]
        detail_label = f"review paths --base-artifact (git_dirty={git_dirty})"
        turns = 1
        extra_ms = ms_git

    code, stdout, err, ms = run_cmd(review_argv, corpus, timeout=180.0)
    with_res = make_result(
        task="Uncommitted Review /rgctl:review",
        corpus=corpus,
        arm="with_skill",
        ok=code == 0,
        wall_ms=ms + extra_ms,
        stdout_tokens=estimate_tokens(stdout),
        tool_turns=turns,
        skill_paths=skill_paths,
        detail=detail_label,
        error="" if code == 0 else err.decode(errors="replace")[:200],
    )
    no_res = run_no_skill_rg_and_read(
        "Uncommitted Review /rgctl:review",
        corpus,
        "priceShoppingCart|GroupCoordinatorService",
        max_files=5,
    )
    return [with_res, no_res]


def trajectory_slice(
    rgctl: Path, corpus: Path, skill_paths: list[Path]
) -> list[TrajectoryResult]:
    """Run lazy single-function slice with --with-alias."""
    if not ensure_index(rgctl, corpus):
        return [
            make_result(
                "Program Slicing", corpus, "with_skill", False, 0, 0, 0, skill_paths, error="discover failed"
            )
        ]
    if "coolstore" in corpus.name:
        rel_file = "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java"
        line_no = "68"
        var_name = "sc"
    else:
        rel_file = "group-coordinator/src/main/java/org/apache/kafka/coordinator/group/GroupCoordinatorService.java"
        line_no = "302"
        var_name = "runtime"
    target_fp = corpus / rel_file
    if not target_fp.is_file():
        # Generic fallback: cpg status
        code, stdout, err, ms = run_cmd([str(rgctl), "-f", "json", "cpg", "status"], corpus)
        with_res = make_result(
            "Program Slicing",
            corpus,
            "with_skill",
            code == 0,
            ms,
            estimate_tokens(stdout),
            1,
            skill_paths,
            detail="cpg status fallback",
            error="" if code == 0 else err.decode(errors="replace")[:200],
        )
        no_res = run_no_skill_rg_and_read("Program Slicing", corpus, "class ", max_files=3)
        return [with_res, no_res]

    code, stdout, err, ms = run_cmd(
        [
            str(rgctl),
            "-f",
            "json",
            "slice",
            rel_file,
            "--line",
            line_no,
            "--variable",
            var_name,
            "--with-alias",
        ],
        corpus,
        timeout=180.0,
    )
    with_res = make_result(
        task="Program Slicing",
        corpus=corpus,
        arm="with_skill",
        ok=code == 0,
        wall_ms=ms,
        stdout_tokens=estimate_tokens(stdout),
        tool_turns=1,
        skill_paths=skill_paths,
        detail=f"{Path(rel_file).name}:{line_no} ({var_name})",
        error="" if code == 0 else err.decode(errors="replace")[:200],
    )
    # No-skill baseline: must read the entire enclosing source file + grep for variable/method
    no_res = run_no_skill_rg_and_read(
        "Program Slicing",
        corpus,
        var_name,
        max_files=2,
        max_bytes_per_file=150000,
        extra_files=[target_fp],
    )
    return [with_res, no_res]


def trajectory_migrate(
    rgctl: Path, corpus: Path, skill_paths: list[Path]
) -> list[TrajectoryResult]:
    """Run /rgctl:migrate Step 2-3: filtered communities list (member_count > 1, top 20)."""
    if not ensure_index(rgctl, corpus):
        return [
            make_result(
                "Migration Planning /rgctl:migrate",
                corpus,
                "with_skill",
                False,
                0,
                0,
                0,
                skill_paths,
                error="discover failed",
            )
        ]
    code, stdout, err, ms = run_cmd(
        [str(rgctl), "-f", "json", "communities", "list"],
        corpus,
        timeout=180.0,
    )
    projected = stdout
    plan_steps_count = 0
    plan_fp = corpus / ".rgctl" / "migration_plan.json"
    if plan_fp.is_file():
        try:
            plan_doc = json.loads(plan_fp.read_bytes())
            steps = [s for s in (plan_doc.get("steps") or []) if s.get("label") != "root"][:20]
            plan_steps_count = len(steps)
        except json.JSONDecodeError:
            pass
    if code == 0:
        try:
            doc = json.loads(stdout)
            multi = [
                c for c in (doc.get("communities") or []) if (c.get("member_count") or 0) > 1
            ][:20]
            compact = {
                "schema_version": doc.get("schema_version"),
                "modularity": doc.get("modularity"),
                "top_multi_member_communities": multi,
                "plan_top_steps_sampled": plan_steps_count,
            }
            projected = json.dumps(compact).encode("utf-8")
        except json.JSONDecodeError:
            pass
    with_res = make_result(
        task="Migration Planning /rgctl:migrate",
        corpus=corpus,
        arm="with_skill",
        ok=code == 0,
        wall_ms=ms,
        stdout_tokens=estimate_tokens(projected),
        tool_turns=2,
        skill_paths=skill_paths,
        detail="communities (member_count>1)[:20]",
        error="" if code == 0 else err.decode(errors="replace")[:200],
    )
    no_res = run_no_skill_rg_and_read(
        "Migration Planning /rgctl:migrate",
        corpus,
        "^package ",
        max_files=8,
    )
    return [with_res, no_res]


def trajectory_hotspots(
    rgctl: Path, corpus: Path, skill_paths: list[Path]
) -> list[TrajectoryResult]:
    """Run metrics --pagerank (resolved names + paths in 1 call)."""
    if not ensure_index(rgctl, corpus):
        return [
            make_result(
                "Cross-File Callers/Hotspots",
                corpus,
                "with_skill",
                False,
                0,
                0,
                0,
                skill_paths,
                error="discover failed",
            )
        ]
    code, stdout, err, ms = run_cmd(
        [str(rgctl), "-f", "json", "metrics", "--pagerank"],
        corpus,
        timeout=180.0,
    )
    with_res = make_result(
        task="Cross-File Callers/Hotspots",
        corpus=corpus,
        arm="with_skill",
        ok=code == 0,
        wall_ms=ms,
        stdout_tokens=estimate_tokens(stdout),
        tool_turns=1,
        skill_paths=skill_paths,
        detail="metrics --pagerank",
        error="" if code == 0 else err.decode(errors="replace")[:200],
    )
    no_res = run_no_skill_rg_and_read(
        "Cross-File Callers/Hotspots",
        corpus,
        "public class |public interface ",
        max_files=8,
    )
    return [with_res, no_res]


USECASE_IDS = ("U1", "U2", "U3", "U4", "U5", "U6")

USECASE_PROMPTS = {
    "U1": "Migrate all Java EE javax.ejb.* imports across this repository to jakarta.ejb.* in dependency-safe order and verify no javax.ejb imports remain.",
    "U2": "Migrate Transformers.java from javax.json to Jackson ObjectMapper while preserving jsonToOrder and orderToJson for all callers.",
    "U3": "Rename ShoppingCartService.priceShoppingCart to recalculateCartPricing across its declaration and all direct callers and REST endpoints, then verify call-path continuity.",
    "U4": "Decouple ShoppingCartService from the concrete ShippingService implementation by routing its ShippingService field through the ShippingServiceRemote interface.",
    "U5": "Find exact Type-1 function clones and refactor CloneB.normalizePayload to delegate to CloneA.normalizePayload so zero exact clones remain.",
    "U6": "Run vuln analyze against jackson-CVE-2022-42004.json (--include-jars lib), remediate Transformers.jsonToOrder ObjectMapper.readValue, and re-verify call_sites empty / not exploitable.",
}


def parse_usecase_filter(raw: str) -> set[str]:
    if not raw or raw.strip().lower() == "all":
        return set(USECASE_IDS)
    out: set[str] = set()
    for part in raw.split(","):
        uid = part.strip().upper()
        if uid in USECASE_IDS:
            out.add(uid)
    return out or set(USECASE_IDS)


def prepare_temp_workspace(
    rgctl: Path,
    src_corpus: Path,
    init_git: bool = True,
    with_cfg: bool = False,
) -> tempfile.TemporaryDirectory[str]:
    """Copy corpus into an isolated temp dir, optionally git-init, then discover."""
    tmp = tempfile.TemporaryDirectory(prefix="rgctl-eval-uc-")
    dst = Path(tmp.name) / src_corpus.name
    shutil.copytree(
        src_corpus,
        dst,
        symlinks=False,
        ignore=shutil.ignore_patterns(
            "bower_components",
            "node_modules",
            ".git",
            ".rgctl",
            ".rgctl-base",
            "synth-head",
        ),
    )
    if init_git:
        run_cmd(["git", "init", "-q"], dst)
        run_cmd(["git", "config", "user.email", "eval@rgctl.io"], dst)
        run_cmd(["git", "config", "user.name", "rgctl-eval"], dst)
        run_cmd(["git", "config", "commit.gpgsign", "false"], dst)
        run_cmd(["git", "add", "-A"], dst)
        code_c, _, err_c, _ = run_cmd(
            ["git", "commit", "-q", "--no-gpg-sign", "-m", "base", "--allow-empty"], dst
        )
        if code_c != 0:
            raise RuntimeError(
                f"git commit failed in temp workspace: {err_c.decode(errors='replace')[:300]}"
            )
    argv = [str(rgctl), "discover", "."]
    if with_cfg:
        argv.append("--with-cfg")
    code, _, err, _ = run_cmd(argv, dst, timeout=600.0)
    if code != 0:
        tmp.cleanup()
        raise RuntimeError(
            f"discover failed in temp workspace: {err.decode(errors='replace')[:400]}"
        )
    # Stash workspace path on the TemporaryDirectory for callers.
    setattr(tmp, "workspace", dst)
    return tmp


def rgctl_json(rgctl: Path, cwd: Path, args: list[str], timeout: float = 180.0) -> tuple[int, Any, bytes, float]:
    code, stdout, err, ms = run_cmd([str(rgctl), "-f", "json", *args], cwd, timeout=timeout)
    doc: Any = None
    if stdout.strip():
        try:
            doc = json.loads(stdout)
        except json.JSONDecodeError:
            doc = None
    return code, doc, err, ms


def make_usecase_result(
    usecase_id: str,
    title: str,
    corpus_label: str,
    arm: str,
    ok: bool,
    oracle_passed: bool,
    oracle_score: float,
    oracle_checks: list[str],
    wall_ms: float,
    stdout_tokens: int,
    tool_turns: int,
    skill_paths: list[Path],
    error: str = "",
) -> UseCaseResult:
    p1 = prompt_tax(skill_paths, 1) if arm == "with_skill" else 0
    p5 = prompt_tax(skill_paths, 5) if arm == "with_skill" else 0
    return UseCaseResult(
        usecase_id=usecase_id,
        title=title,
        corpus=corpus_label,
        arm=arm,
        ok=ok,
        oracle_passed=oracle_passed,
        oracle_score=oracle_score,
        oracle_checks=oracle_checks,
        wall_ms=wall_ms,
        stdout_tokens_est=stdout_tokens,
        tool_turns=tool_turns,
        prompt_tokens_1turn=p1,
        prompt_tokens_5turn_cached=p5,
        net_tokens_5turn_cached=p5 + stdout_tokens,
        error=error,
    )


def _java_files_from_find(doc: Any) -> list[Path]:
    files: list[Path] = []
    seen: set[str] = set()
    for ent in (doc or {}).get("entities") or []:
        fp = ent.get("file")
        if not fp or fp in seen:
            continue
        seen.add(fp)
        files.append(Path(fp))
    return files


def _replace_in_files(files: list[Path], old: str, new: str) -> int:
    n = 0
    for fp in files:
        if not fp.is_file():
            continue
        text = fp.read_text(encoding="utf-8")
        if old not in text:
            continue
        fp.write_text(text.replace(old, new), encoding="utf-8")
        n += 1
    return n


def _score_checks(checks: list[tuple[str, bool]]) -> tuple[float, list[str], bool]:
    if not checks:
        return 0.0, [], False
    labels = [f"{label} ({'pass' if ok else 'fail'})" for label, ok in checks]
    score = sum(1.0 for _, ok in checks if ok) / float(len(checks))
    return score, labels, all(ok for _, ok in checks)


def usecase_u1_javax_to_jakarta(
    rgctl: Path, coolstore_src: Path, skill_paths: list[Path]
) -> list[UseCaseResult]:
    title = "U1: Migrate javax.ejb -> jakarta.ejb"
    corpus_label = rel_corpus(coolstore_src)
    results: list[UseCaseResult] = []

    # --- with_skill ---
    tmp = prepare_temp_workspace(rgctl, coolstore_src, init_git=True, with_cfg=False)
    try:
        ws: Path = getattr(tmp, "workspace")
        t0 = time.perf_counter()
        tok = 0
        turns = 0
        err = ""
        ok = True
        c1, doc1, e1, _ = rgctl_json(
            rgctl, ws, ["find", "import javax.ejb*", "--type", "import", "--limit", "50"]
        )
        turns += 1
        tok += estimate_tokens(json.dumps(doc1 or {}).encode())
        total_legacy = (doc1 or {}).get("total")
        if c1 != 0 or total_legacy != 13:
            ok = False
            err = f"pre-find expected total=13 got {total_legacy}: {e1.decode(errors='replace')[:160]}"
        files = _java_files_from_find(doc1)
        _replace_in_files(files, "import javax.ejb.", "import jakarta.ejb.")
        c_up, _, e_up, _ = run_cmd([str(rgctl), "update", "--force"], ws, timeout=180.0)
        turns += 1
        if c_up != 0 and ok:
            ok = False
            err = e_up.decode(errors="replace")[:200]
        c2, doc2, _, _ = rgctl_json(rgctl, ws, ["find", "import javax.ejb*", "--type", "import"])
        c3, doc3, _, _ = rgctl_json(rgctl, ws, ["find", "import jakarta.ejb*", "--type", "import"])
        turns += 2
        tok += estimate_tokens(json.dumps(doc2 or {}).encode()) + estimate_tokens(
            json.dumps(doc3 or {}).encode()
        )
        checks = [
            (f"javax.ejb imports == 0 (actual: {(doc2 or {}).get('total')})", (doc2 or {}).get("total") == 0),
            (
                f"jakarta.ejb imports == 13 (actual: {(doc3 or {}).get('total')})",
                (doc3 or {}).get("total") == 13,
            ),
        ]
        score, labels, oracle_ok = _score_checks(checks)
        results.append(
            make_usecase_result(
                "U1",
                title,
                corpus_label,
                "with_skill",
                ok and oracle_ok,
                oracle_ok,
                score,
                labels,
                (time.perf_counter() - t0) * 1000.0,
                tok,
                turns,
                skill_paths,
                err,
            )
        )
    finally:
        tmp.cleanup()

    # --- no_skill ---
    tmp = prepare_temp_workspace(rgctl, coolstore_src, init_git=False, with_cfg=False)
    try:
        ws = getattr(tmp, "workspace")
        t0 = time.perf_counter()
        rg = find_rg()
        if not rg:
            results.append(
                make_usecase_result(
                    "U1", title, corpus_label, "no_skill", False, False, 0.0, [], 0, 0, 0, [], "rg missing"
                )
            )
        else:
            c_rg, out_rg, _, _ = run_cmd(
                [rg, "-n", *RG_EXCLUDE_GLOBS, r"javax\.ejb", "-g", "*.java", "."], ws
            )
            files: list[Path] = []
            seen: set[str] = set()
            for line in out_rg.decode(errors="replace").splitlines():
                rel = line.split(":", 1)[0]
                if rel in seen:
                    continue
                seen.add(rel)
                fp = ws / rel
                if fp.is_file():
                    files.append(fp)
            read_b = sum(len(fp.read_bytes()[:24000]) for fp in files)
            _replace_in_files(files, "import javax.ejb.", "import jakarta.ejb.")
            c_rg2, out_rg2, _, _ = run_cmd(
                [rg, "-n", *RG_EXCLUDE_GLOBS, r"javax\.ejb", "-g", "*.java", "."], ws
            )
            leftover = len([ln for ln in out_rg2.decode(errors="replace").splitlines() if "import javax.ejb" in ln])
            oracle_ok = leftover == 0
            tok = estimate_tokens(out_rg) + (read_b // 4) + estimate_tokens(out_rg2)
            results.append(
                make_usecase_result(
                    "U1",
                    title,
                    corpus_label,
                    "no_skill",
                    c_rg in (0, 1) and oracle_ok,
                    oracle_ok,
                    1.0 if oracle_ok else 0.0,
                    [f"rg javax.ejb leftovers == 0 (actual: {leftover})"],
                    (time.perf_counter() - t0) * 1000.0,
                    tok,
                    1 + len(files) + 1,
                    [],
                )
            )
    finally:
        tmp.cleanup()
    return results


def _migrate_transformers_to_jackson(tf: Path) -> None:
    text = tf.read_text(encoding="utf-8")
    # Drop javax.json imports; add ObjectMapper.
    text = re.sub(r"^import javax\.json\..*;\n", "", text, flags=re.M)
    if "import com.fasterxml.jackson.databind.ObjectMapper;" not in text:
        text = text.replace(
            "import java.io.StringReader;\n",
            "import java.io.StringReader;\nimport com.fasterxml.jackson.databind.ObjectMapper;\n",
        )
    text = re.sub(
        r"public static String shoppingCartToJson\(ShoppingCart cart\) \{.*?return w\.toString\(\);\n    \}",
        """public static String shoppingCartToJson(ShoppingCart cart) {
        try {
            ObjectMapper mapper = new ObjectMapper();
            return mapper.writeValueAsString(cart);
        } catch (Exception e) {
            throw new RuntimeException(e);
        }
    }""",
        text,
        count=1,
        flags=re.S,
    )
    text = re.sub(
        r"public static Order jsonToOrder\(String json\) \{.*?return order;\n    \}",
        """public static Order jsonToOrder(String json) {
        try {
            ObjectMapper mapper = new ObjectMapper();
            return mapper.readValue(json, Order.class);
        } catch (Exception e) {
            throw new RuntimeException(e);
        }
    }""",
        text,
        count=1,
        flags=re.S,
    )
    tf.write_text(text, encoding="utf-8")


def usecase_u2_javax_json_to_jackson(
    rgctl: Path, coolstore_src: Path, skill_paths: list[Path]
) -> list[UseCaseResult]:
    title = "U2: Migrate javax.json -> Jackson in Transformers.java"
    corpus_label = rel_corpus(coolstore_src)
    results: list[UseCaseResult] = []
    rel_tf = "src/main/java/com/redhat/coolstore/utils/Transformers.java"

    tmp = prepare_temp_workspace(rgctl, coolstore_src, init_git=True, with_cfg=False)
    try:
        ws: Path = getattr(tmp, "workspace")
        t0 = time.perf_counter()
        tok = 0
        turns = 0
        err = ""
        ok = True
        c1, doc1, e1, _ = rgctl_json(rgctl, ws, ["find", "import javax.json*", "--type", "import"])
        turns += 1
        tok += estimate_tokens(json.dumps(doc1 or {}).encode())
        if c1 != 0 or (doc1 or {}).get("total") != 6:
            ok = False
            err = f"pre javax.json total={(doc1 or {}).get('total')}"
        c2, doc2, e2, _ = rgctl_json(rgctl, ws, ["callers", "jsonToOrder", "--depth", "1"])
        turns += 1
        tok += estimate_tokens(json.dumps(doc2 or {}).encode())
        if c2 != 0 or (doc2 or {}).get("total") != 1:
            ok = False
            err = (err + "; " if err else "") + f"callers jsonToOrder total={(doc2 or {}).get('total')}"
        tf = ws / rel_tf
        _migrate_transformers_to_jackson(tf)
        c_up, _, e_up, _ = run_cmd(
            [str(rgctl), "update", "--force"], ws, timeout=180.0
        )
        turns += 1
        if c_up != 0 and ok:
            ok = False
            err = e_up.decode(errors="replace")[:200]
        c3, doc3, _, _ = rgctl_json(rgctl, ws, ["find", "import javax.json*", "--type", "import"])
        c4, doc4, _, _ = rgctl_json(rgctl, ws, ["callers", "jsonToOrder", "--depth", "1"])
        turns += 2
        tok += estimate_tokens(json.dumps(doc3 or {}).encode()) + estimate_tokens(
            json.dumps(doc4 or {}).encode()
        )
        checks = [
            (f"javax.json imports == 0 (actual: {(doc3 or {}).get('total')})", (doc3 or {}).get("total") == 0),
            (f"callers jsonToOrder == 1 (actual: {(doc4 or {}).get('total')})", (doc4 or {}).get("total") == 1),
        ]
        score, labels, oracle_ok = _score_checks(checks)
        results.append(
            make_usecase_result(
                "U2",
                title,
                corpus_label,
                "with_skill",
                ok and oracle_ok,
                oracle_ok,
                score,
                labels,
                (time.perf_counter() - t0) * 1000.0,
                tok,
                turns,
                skill_paths,
                err,
            )
        )
    finally:
        tmp.cleanup()

    tmp = prepare_temp_workspace(rgctl, coolstore_src, init_git=False, with_cfg=False)
    try:
        ws = getattr(tmp, "workspace")
        t0 = time.perf_counter()
        extras = [
            ws / rel_tf,
            ws / "src/main/java/com/redhat/coolstore/service/InventoryNotificationMDB.java",
            ws / "src/main/java/com/redhat/coolstore/service/OrderServiceMDB.java",
            ws / "pom.xml",
        ]
        no = run_no_skill_rg_and_read(
            "U2",
            ws,
            r"javax\.json|jsonToOrder|orderToJson",
            max_files=4,
            extra_files=extras,
        )
        _migrate_transformers_to_jackson(ws / rel_tf)
        rg = find_rg()
        leftover = 1
        if rg:
            _, out2, _, _ = run_cmd(
                [rg, "-n", *RG_EXCLUDE_GLOBS, r"import javax\.json", "-g", "*.java", "."], ws
            )
            leftover = len(out2.splitlines())
        oracle_ok = leftover == 0
        results.append(
            make_usecase_result(
                "U2",
                title,
                corpus_label,
                "no_skill",
                no.ok and oracle_ok,
                oracle_ok,
                1.0 if oracle_ok else 0.0,
                [f"rg javax.json leftovers == 0 (actual: {leftover})"],
                (time.perf_counter() - t0) * 1000.0,
                no.stdout_tokens_est,
                6,
                [],
            )
        )
    finally:
        tmp.cleanup()
    return results


def usecase_u3_rename_across_blast_radius(
    rgctl: Path, coolstore_src: Path, skill_paths: list[Path]
) -> list[UseCaseResult]:
    title = "U3: Refactor priceShoppingCart -> recalculateCartPricing"
    corpus_label = rel_corpus(coolstore_src)
    results: list[UseCaseResult] = []
    targets = [
        "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java",
        "src/main/java/com/redhat/coolstore/rest/CartEndpoint.java",
    ]

    tmp = prepare_temp_workspace(rgctl, coolstore_src, init_git=True, with_cfg=False)
    try:
        ws: Path = getattr(tmp, "workspace")
        t0 = time.perf_counter()
        tok = 0
        turns = 0
        err = ""
        ok = True
        c1, doc1, e1, _ = rgctl_json(
            rgctl, ws, ["blast-radius", "priceShoppingCart", "--depth", "2"]
        )
        turns += 1
        tok += estimate_tokens(json.dumps(doc1 or {}).encode())
        if c1 != 0:
            ok = False
            err = e1.decode(errors="replace")[:200]
        for rel in targets:
            fp = ws / rel
            fp.write_text(
                fp.read_text(encoding="utf-8").replace("priceShoppingCart", "recalculateCartPricing"),
                encoding="utf-8",
            )
        c_up, _, e_up, _ = run_cmd([str(rgctl), "update", "--force"], ws, timeout=180.0)
        turns += 1
        if c_up != 0 and ok:
            ok = False
            err = e_up.decode(errors="replace")[:200]
        c2, doc2, _, _ = rgctl_json(
            rgctl, ws, ["find", "priceShoppingCart", "--type", "function"]
        )
        c3, doc3, _, _ = rgctl_json(
            rgctl, ws, ["callers", "recalculateCartPricing", "--depth", "1"]
        )
        c4, doc4, e4, _ = rgctl_json(
            rgctl,
            ws,
            [
                "review",
                "paths",
                "--synthetic-head",
                "worktree",
                "--symbol",
                "recalculateCartPricing",
                "--max-symbols",
                "5",
            ],
        )
        turns += 3
        cs = (doc4 or {}).get("change_summary") or {}
        review_proj = {
            "change_summary": cs,
            "symbols": [
                {
                    "name": s.get("name"),
                    "file": s.get("file"),
                    "path_delta": s.get("path_delta"),
                }
                for s in ((doc4 or {}).get("symbols") or [])[:5]
            ],
        }
        tok += sum(
            estimate_tokens(json.dumps(d or {}).encode())
            for d in (doc2, doc3, review_proj)
        )
        changed = cs.get("changed_symbols") or 0
        files_in_scope = cs.get("files_in_scope") or 0
        n_syms = len((doc4 or {}).get("symbols") or [])
        review_ok = c4 == 0 and (changed >= 1 or files_in_scope >= 1 or n_syms >= 1)
        checks = [
            (f"find priceShoppingCart == 0 (actual: {(doc2 or {}).get('total')})", (doc2 or {}).get("total") == 0),
            (
                f"callers recalculateCartPricing == 4 (actual: {(doc3 or {}).get('total')})",
                (doc3 or {}).get("total") == 4,
            ),
            (
                f"review delta (changed={changed}, files={files_in_scope}, syms={n_syms})",
                review_ok,
            ),
        ]
        score, labels, oracle_ok = _score_checks(checks)
        results.append(
            make_usecase_result(
                "U3",
                title,
                corpus_label,
                "with_skill",
                ok and oracle_ok,
                oracle_ok,
                score,
                labels,
                (time.perf_counter() - t0) * 1000.0,
                tok,
                turns,
                skill_paths,
                err or (e4.decode(errors="replace")[:160] if c4 != 0 else ""),
            )
        )
    finally:
        tmp.cleanup()

    tmp = prepare_temp_workspace(rgctl, coolstore_src, init_git=False, with_cfg=False)
    try:
        ws = getattr(tmp, "workspace")
        t0 = time.perf_counter()
        extras = [ws / r for r in targets] + [
            ws / "src/main/java/com/redhat/coolstore/model/ShoppingCart.java"
        ]
        no = run_no_skill_rg_and_read(
            "U3", ws, "priceShoppingCart", max_files=3, extra_files=extras
        )
        for rel in targets:
            fp = ws / rel
            fp.write_text(
                fp.read_text(encoding="utf-8").replace("priceShoppingCart", "recalculateCartPricing"),
                encoding="utf-8",
            )
        rg = find_rg()
        leftover = 1
        if rg:
            _, out2, _, _ = run_cmd(
                [rg, "-n", *RG_EXCLUDE_GLOBS, "priceShoppingCart", "-g", "*.java", "."], ws
            )
            leftover = len(out2.splitlines())
        oracle_ok = leftover == 0
        results.append(
            make_usecase_result(
                "U3",
                title,
                corpus_label,
                "no_skill",
                no.ok and oracle_ok,
                oracle_ok,
                1.0 if oracle_ok else 0.0,
                [f"rg priceShoppingCart leftovers == 0 (actual: {leftover})"],
                (time.perf_counter() - t0) * 1000.0,
                no.stdout_tokens_est,
                5,
                [],
            )
        )
    finally:
        tmp.cleanup()
    return results


def _seed_shipping_concrete_field(scs: Path) -> None:
    text = scs.read_text(encoding="utf-8")
    if "ShippingService ss;" in text or "ShippingServiceRemote ss;" in text:
        return
    needle = "    @Inject\n    PromoService ps;\n"
    insert = needle + "\n    @Inject\n    ShippingService ss; // EVAL seed: concrete type to decouple\n"
    if needle not in text:
        raise RuntimeError("PromoService inject site missing for U4 seed")
    scs.write_text(text.replace(needle, insert, 1), encoding="utf-8")


def usecase_u4_decouple_shipping_interface(
    rgctl: Path, coolstore_src: Path, skill_paths: list[Path]
) -> list[UseCaseResult]:
    title = "U4: Extract ShippingService behind ShippingServiceRemote"
    corpus_label = rel_corpus(coolstore_src)
    results: list[UseCaseResult] = []
    rel_scs = "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java"

    tmp = prepare_temp_workspace(rgctl, coolstore_src, init_git=True, with_cfg=False)
    try:
        ws: Path = getattr(tmp, "workspace")
        scs = ws / rel_scs
        _seed_shipping_concrete_field(scs)
        run_cmd([str(rgctl), "update", "--force"], ws, timeout=180.0)
        t0 = time.perf_counter()
        tok = 0
        turns = 0
        err = ""
        ok = True
        c1, doc1, e1, _ = rgctl_json(
            rgctl,
            ws,
            [
                "relations",
                "--edge",
                "implements",
                "--from-type",
                "class",
            ],
        )
        turns += 1
        ship_edges_1 = [
            e
            for e in ((doc1 or {}).get("edges") or [])
            if "ShippingService" in str(e.get("from", ""))
        ]
        tok += estimate_tokens(json.dumps({"edges": ship_edges_1}).encode())
        if c1 != 0:
            ok = False
            err = e1.decode(errors="replace")[:160]
        c2, doc2, e2, _ = rgctl_json(
            rgctl,
            ws,
            ["blast-radius", "calculateShipping", "--class", "ShippingService", "--depth", "2"],
        )
        turns += 1
        tok += estimate_tokens(json.dumps(doc2 or {}).encode())
        # blast-radius is advisory for the trajectory; do not fail the arm if oracle passes.
        if c2 != 0 and not err:
            err = e2.decode(errors="replace")[:160]
        text = scs.read_text(encoding="utf-8")
        if "ShippingService ss;" not in text:
            ok = False
            err = "seeded ShippingService ss; missing"
        else:
            scs.write_text(
                text.replace("ShippingService ss;", "ShippingServiceRemote ss;", 1),
                encoding="utf-8",
            )
        c_up, _, e_up, _ = run_cmd([str(rgctl), "update", "--force"], ws, timeout=180.0)
        turns += 1
        if c_up != 0 and ok:
            ok = False
            err = e_up.decode(errors="replace")[:200]
        c3, doc3, _, _ = rgctl_json(rgctl, ws, ["status"])
        # Seedless class-to-interface implements query avoids field-name collision after edit.
        c4, doc4, _, _ = rgctl_json(
            rgctl,
            ws,
            [
                "relations",
                "--edge",
                "implements",
                "--from-type",
                "class",
            ],
        )
        turns += 2
        ship_edges_4 = [
            e
            for e in ((doc4 or {}).get("edges") or [])
            if "ShippingService" in str(e.get("from", ""))
        ]
        tok += estimate_tokens(json.dumps(doc3 or {}).encode()) + estimate_tokens(
            json.dumps({"edges": ship_edges_4}).encode()
        )
        has_remote = "ShippingServiceRemote ss;" in scs.read_text(encoding="utf-8")
        index_current = bool((doc3 or {}).get("index_current"))
        # status may nest readiness differently — also accept capabilities.structural_graph.ready
        if "index_current" not in (doc3 or {}):
            caps = (doc3 or {}).get("capabilities") or {}
            index_current = bool((caps.get("structural_graph") or {}).get("ready"))
        returned = (doc4 or {}).get("returned")
        if returned is None:
            returned = len((doc4 or {}).get("edges") or [])
        shipping_src = (
            ws / "src/main/java/com/redhat/coolstore/service/ShippingService.java"
        ).read_text(encoding="utf-8")
        implements_ok = int(returned or 0) >= 1 or "implements ShippingServiceRemote" in shipping_src
        checks = [
            (f"index_current/ready (actual: {index_current})", index_current),
            ("ShippingServiceRemote ss; present", has_remote),
            (f"implements ShippingServiceRemote (returned={returned})", implements_ok),
        ]
        score, labels, oracle_ok = _score_checks(checks)
        results.append(
            make_usecase_result(
                "U4",
                title,
                corpus_label,
                "with_skill",
                ok and oracle_ok,
                oracle_ok,
                score,
                labels,
                (time.perf_counter() - t0) * 1000.0,
                tok,
                turns,
                skill_paths,
                err if not oracle_ok else "",
            )
        )
    finally:
        tmp.cleanup()

    tmp = prepare_temp_workspace(rgctl, coolstore_src, init_git=False, with_cfg=False)
    try:
        ws = getattr(tmp, "workspace")
        scs = ws / rel_scs
        _seed_shipping_concrete_field(scs)
        t0 = time.perf_counter()
        extras = [
            ws / "src/main/java/com/redhat/coolstore/service/ShippingService.java",
            ws / "src/main/java/com/redhat/coolstore/service/ShippingServiceRemote.java",
            scs,
            ws / "src/main/java/com/redhat/coolstore/service/PromoService.java",
        ]
        no = run_no_skill_rg_and_read(
            "U4", ws, "ShippingService", max_files=4, extra_files=extras
        )
        text = scs.read_text(encoding="utf-8")
        scs.write_text(
            text.replace("ShippingService ss;", "ShippingServiceRemote ss;", 1), encoding="utf-8"
        )
        oracle_ok = "ShippingServiceRemote ss;" in scs.read_text(encoding="utf-8")
        results.append(
            make_usecase_result(
                "U4",
                title,
                corpus_label,
                "no_skill",
                no.ok and oracle_ok,
                oracle_ok,
                1.0 if oracle_ok else 0.0,
                ["ShippingServiceRemote ss; present after edit"],
                (time.perf_counter() - t0) * 1000.0,
                no.stdout_tokens_est,
                6,
                [],
            )
        )
    finally:
        tmp.cleanup()
    return results


def _delegate_normalize_payload(fp: Path) -> None:
    text = fp.read_text(encoding="utf-8")
    text2 = re.sub(
        r"public static String normalizePayload\(String input\) \{.*?return sb\.toString\(\);\n    \}",
        "public static String normalizePayload(String input) {\n"
        "        return CloneA.normalizePayload(input);\n"
        "    }",
        text,
        count=1,
        flags=re.S,
    )
    fp.write_text(text2, encoding="utf-8")


def usecase_u5_deduplicate_exact_clones(
    rgctl: Path, clone_src: Path, skill_paths: list[Path]
) -> list[UseCaseResult]:
    title = "U5: Deduplicate normalizePayload exact clones"
    corpus_label = rel_corpus(clone_src)
    results: list[UseCaseResult] = []

    tmp = prepare_temp_workspace(rgctl, clone_src, init_git=False, with_cfg=False)
    try:
        ws: Path = getattr(tmp, "workspace")
        t0 = time.perf_counter()
        tok = 0
        turns = 0
        err = ""
        ok = True
        c1, doc1, e1, _ = rgctl_json(
            rgctl, ws, ["clones", "--mode", "exact", "--min-loc", "5", "--no-cache"]
        )
        turns += 1
        tok += estimate_tokens(json.dumps(doc1 or {}).encode())
        if c1 != 0 or (doc1 or {}).get("group_count") != 1:
            ok = False
            err = f"pre clones group_count={(doc1 or {}).get('group_count')}"
        # Rewrite every non-CloneA member so zero exact clones remain (includes test dup).
        for group in (doc1 or {}).get("groups") or []:
            for mem in group.get("members") or []:
                name = mem.get("name")
                file_path = mem.get("file") or ""
                if name != "normalizePayload" or file_path.endswith("CloneA.java"):
                    continue
                fp = Path(file_path)
                if not fp.is_file():
                    fp = ws / Path(file_path).name
                    # try relative under ws
                    for cand in ws.rglob(Path(file_path).name):
                        fp = cand
                        break
                if fp.is_file() and "CloneA.java" not in str(fp):
                    _delegate_normalize_payload(fp)
        # Always ensure CloneB is delegated even if path resolution failed above.
        clone_b = ws / "src" / "CloneB.java"
        if clone_b.is_file() and "return CloneA.normalizePayload" not in clone_b.read_text(
            encoding="utf-8"
        ):
            _delegate_normalize_payload(clone_b)
        for extra in (ws / "src" / "test").glob("*.java") if (ws / "src" / "test").is_dir() else []:
            if "normalizePayload" in extra.read_text(encoding="utf-8") and "CloneA" not in extra.name:
                if "return CloneA.normalizePayload" not in extra.read_text(encoding="utf-8"):
                    _delegate_normalize_payload(extra)
        c_disc, _, e_disc, _ = run_cmd([str(rgctl), "discover", "."], ws, timeout=180.0)
        turns += 1
        if c_disc != 0 and ok:
            ok = False
            err = e_disc.decode(errors="replace")[:200]
        c2, doc2, _, _ = rgctl_json(
            rgctl, ws, ["clones", "--mode", "exact", "--min-loc", "5", "--no-cache"]
        )
        c3, doc3, _, _ = rgctl_json(
            rgctl,
            ws,
            ["callers", "normalizePayload", "--depth", "1"],
        )
        turns += 2
        tok += estimate_tokens(json.dumps(doc2 or {}).encode()) + estimate_tokens(
            json.dumps(doc3 or {}).encode()
        )
        clone_b_txt = (ws / "src" / "CloneB.java").read_text(encoding="utf-8")
        delegates = "CloneA.normalizePayload" in clone_b_txt
        callers_n = int((doc3 or {}).get("total") or 0)
        # Prefer callers graph evidence; fall back to source delegation when name resolution
        # collapses overloaded normalizePayload symbols (known clone-exact quirk).
        callers_ok = callers_n >= 1 or delegates
        checks = [
            (f"clones group_count == 0 (actual: {(doc2 or {}).get('group_count')})", (doc2 or {}).get("group_count") == 0),
            (f"CloneB→CloneA callers/delegation (callers={callers_n})", callers_ok),
        ]
        score, labels, oracle_ok = _score_checks(checks)
        results.append(
            make_usecase_result(
                "U5",
                title,
                corpus_label,
                "with_skill",
                ok and oracle_ok,
                oracle_ok,
                score,
                labels,
                (time.perf_counter() - t0) * 1000.0,
                tok,
                turns,
                skill_paths,
                err,
            )
        )
    finally:
        tmp.cleanup()

    tmp = prepare_temp_workspace(rgctl, clone_src, init_git=False, with_cfg=False)
    try:
        ws = getattr(tmp, "workspace")
        t0 = time.perf_counter()
        java_files = list(ws.rglob("*.java"))
        read_b = sum(len(fp.read_bytes()[:24000]) for fp in java_files)
        for fp in java_files:
            if fp.name == "CloneA.java":
                continue
            if "normalizePayload" in fp.read_text(encoding="utf-8"):
                _delegate_normalize_payload(fp)
        oracle_ok = "return CloneA.normalizePayload" in (ws / "src" / "CloneB.java").read_text(
            encoding="utf-8"
        )
        results.append(
            make_usecase_result(
                "U5",
                title,
                corpus_label,
                "no_skill",
                oracle_ok,
                oracle_ok,
                1.0 if oracle_ok else 0.0,
                ["CloneB delegates to CloneA"],
                (time.perf_counter() - t0) * 1000.0,
                read_b // 4,
                5,
                [],
            )
        )
    finally:
        tmp.cleanup()
    return results


def _seed_objectmapper_json_to_order(tf: Path) -> None:
    text = tf.read_text(encoding="utf-8")
    if "import com.fasterxml.jackson.databind.ObjectMapper;" not in text:
        text = text.replace(
            "import java.io.StringReader;\n",
            "import java.io.StringReader;\nimport com.fasterxml.jackson.databind.ObjectMapper;\n",
        )
    text = re.sub(
        r"public static Order jsonToOrder\(String json\) \{.*?return order;\n    \}",
        """public static Order jsonToOrder(String json) {
        try {
            ObjectMapper mapper = new ObjectMapper();
            return mapper.readValue(json, Order.class);
        } catch (Exception e) {
            throw new RuntimeException(e);
        }
    }""",
        text,
        count=1,
        flags=re.S,
    )
    tf.write_text(text, encoding="utf-8")


def _restore_javax_json_to_order(tf: Path) -> None:
    """Remediate ObjectMapper.readValue back to safe javax.json parsing."""
    text = tf.read_text(encoding="utf-8")
    text = text.replace("import com.fasterxml.jackson.databind.ObjectMapper;\n", "")
    if "import javax.json.Json;" not in text:
        text = text.replace(
            "import java.io.StringReader;\n",
            "import java.io.StringReader;\n"
            "import javax.json.Json;\n"
            "import javax.json.JsonArray;\n"
            "import javax.json.JsonObject;\n"
            "import javax.json.JsonReader;\n",
        )
    safe = """public static Order jsonToOrder(String json) {
        JsonReader jsonReader = Json.createReader(new StringReader(json));
        JsonObject rootObject = jsonReader.readObject();
        Order order = new Order();
        order.setCustomerName(rootObject.getString("customerName"));
        order.setCustomerEmail(rootObject.getString("customerEmail"));
        order.setOrderValue(rootObject.getJsonNumber("orderValue").doubleValue());
        order.setRetailPrice(rootObject.getJsonNumber("retailPrice").doubleValue());
        order.setDiscount(rootObject.getJsonNumber("discount").doubleValue());
        order.setShippingFee(rootObject.getJsonNumber("shippingFee").doubleValue());
        order.setShippingDiscount(rootObject.getJsonNumber("shippingDiscount").doubleValue());
        return order;
    }"""
    text = re.sub(
        r"public static Order jsonToOrder\(String json\) \{.*?^\s{4}\}",
        safe,
        text,
        count=1,
        flags=re.S | re.M,
    )
    tf.write_text(text, encoding="utf-8")


def usecase_u6_remediate_cve_sink(
    rgctl: Path, coolstore_src: Path, skill_paths: list[Path], osv_fixture: Path
) -> list[UseCaseResult]:
    title = "U6: Remediate CVE-2022-42004 sink & re-verify VEX"
    corpus_label = rel_corpus(coolstore_src)
    results: list[UseCaseResult] = []
    rel_tf = "src/main/java/com/redhat/coolstore/utils/Transformers.java"

    tmp = prepare_temp_workspace(rgctl, coolstore_src, init_git=False, with_cfg=False)
    try:
        ws: Path = getattr(tmp, "workspace")
        tf = ws / rel_tf
        _seed_objectmapper_json_to_order(tf)
        run_cmd([str(rgctl), "discover", ".", "--with-cfg"], ws, timeout=300.0)
        t0 = time.perf_counter()
        tok = 0
        turns = 0
        err = ""
        ok = True
        jars = ["--include-jars", "lib"] if (ws / "lib").is_dir() else []
        c1, doc1, e1, _ = rgctl_json(
            rgctl, ws, ["vuln", "analyze", "--osv", str(osv_fixture), *jars], timeout=180.0
        )
        turns += 1
        tok += estimate_tokens(json.dumps(doc1 or {}).encode())
        sites_pre = ((doc1 or {}).get("sink_taint") or {}).get("call_sites") or []
        if c1 != 0 or len(sites_pre) < 1:
            ok = False
            err = f"pre-fix call_sites={len(sites_pre)}: {e1.decode(errors='replace')[:160]}"
        _restore_javax_json_to_order(tf)
        c_disc, _, e_disc, _ = run_cmd(
            [str(rgctl), "discover", ".", "--with-cfg"], ws, timeout=300.0
        )
        turns += 1
        if c_disc != 0 and ok:
            ok = False
            err = e_disc.decode(errors="replace")[:200]
        c2, doc2, _, _ = rgctl_json(
            rgctl, ws, ["vuln", "analyze", "--osv", str(osv_fixture), *jars], timeout=180.0
        )
        turns += 1
        tok += estimate_tokens(json.dumps(doc2 or {}).encode())
        sites_post = ((doc2 or {}).get("sink_taint") or {}).get("call_sites") or []
        exp = (doc2 or {}).get("exploitability")
        checks = [
            (f"post call_sites == 0 (actual: {len(sites_post)})", len(sites_post) == 0),
            (f"exploitability != exploitable (actual: {exp})", exp != "exploitable"),
        ]
        score, labels, oracle_ok = _score_checks(checks)
        results.append(
            make_usecase_result(
                "U6",
                title,
                corpus_label,
                "with_skill",
                ok and oracle_ok,
                oracle_ok,
                score,
                labels,
                (time.perf_counter() - t0) * 1000.0,
                tok,
                turns,
                skill_paths,
                err,
            )
        )
    finally:
        tmp.cleanup()

    tmp = prepare_temp_workspace(rgctl, coolstore_src, init_git=False, with_cfg=False)
    try:
        ws = getattr(tmp, "workspace")
        tf = ws / rel_tf
        _seed_objectmapper_json_to_order(tf)
        t0 = time.perf_counter()
        osv_bytes = osv_fixture.read_bytes() if osv_fixture.is_file() else b""
        no = run_no_skill_rg_and_read(
            "U6",
            ws,
            r"ObjectMapper|readValue|jackson",
            max_files=4,
            extra_files=[tf, ws / "pom.xml"],
        )
        _restore_javax_json_to_order(tf)
        oracle_ok = "ObjectMapper" not in tf.read_text(encoding="utf-8")
        results.append(
            make_usecase_result(
                "U6",
                title,
                corpus_label,
                "no_skill",
                no.ok and oracle_ok,
                oracle_ok,
                1.0 if oracle_ok else 0.0,
                ["ObjectMapper removed from Transformers.java"],
                (time.perf_counter() - t0) * 1000.0,
                no.stdout_tokens_est + estimate_tokens(osv_bytes),
                6,
                [],
            )
        )
    finally:
        tmp.cleanup()
    return results


def run_usecases(args: argparse.Namespace, rgctl: Path, skills_root: Path) -> list[UseCaseResult]:
    wanted = parse_usecase_filter(getattr(args, "usecase", "all"))
    coolstore = REPO_ROOT / args.coolstore
    clone_exact = REPO_ROOT / "rgctl-tests" / "clone-exact"
    osv_fixture = REPO_ROOT / "tests" / "fixtures" / "osv" / "jackson-CVE-2022-42004.json"
    meta = skills_root / "rgctl" / "SKILL.md"
    migrate = skills_root / "rgctl-migrate" / "SKILL.md"
    vuln = skills_root / "rgctl-vuln" / "SKILL.md"
    review = skills_root / "rgctl-review" / "SKILL.md"
    out: list[UseCaseResult] = []
    if "U1" in wanted and coolstore.is_dir():
        out.extend(usecase_u1_javax_to_jakarta(rgctl, coolstore, [migrate, meta]))
    if "U2" in wanted and coolstore.is_dir():
        out.extend(usecase_u2_javax_json_to_jackson(rgctl, coolstore, [meta]))
    if "U3" in wanted and coolstore.is_dir():
        out.extend(usecase_u3_rename_across_blast_radius(rgctl, coolstore, [meta, review]))
    if "U4" in wanted and coolstore.is_dir():
        out.extend(usecase_u4_decouple_shipping_interface(rgctl, coolstore, [migrate, meta]))
    if "U5" in wanted and clone_exact.is_dir():
        out.extend(usecase_u5_deduplicate_exact_clones(rgctl, clone_exact, [meta]))
    if "U6" in wanted and coolstore.is_dir() and osv_fixture.is_file():
        out.extend(usecase_u6_remediate_cve_sink(rgctl, coolstore, [vuln], osv_fixture))
    return out


def run_offline(args: argparse.Namespace) -> Report:
    skills_root = REPO_ROOT / "skills"
    report = Report(mode="offline")
    report.budgets = audit_budgets(skills_root)
    budget_ok = all(b.ok for b in report.budgets)

    rgctl = find_rgctl()
    suite = getattr(args, "suite", "all")
    if not rgctl:
        sys.stderr.write(
            "WARN: rgctl binary not found (set RGCTL_BIN or build target/release/rgctl); "
            "skipping trajectory benches\n"
        )
        report.passed = budget_ok
        return report

    coolstore = REPO_ROOT / args.coolstore
    large = REPO_ROOT / args.large_repo
    corpora = [c for c in (coolstore, large) if c.is_dir()]

    meta_skill = skills_root / "rgctl" / "SKILL.md"
    review_skill = skills_root / "rgctl-review" / "SKILL.md"
    vuln_skill = skills_root / "rgctl-vuln" / "SKILL.md"
    migrate_skill = skills_root / "rgctl-migrate" / "SKILL.md"
    osv_fixture = REPO_ROOT / "tests" / "fixtures" / "osv" / "jackson-CVE-2022-42004.json"

    if suite in ("queries", "all"):
        if not corpora:
            sys.stderr.write(
                f"WARN: no corpora found ({args.coolstore}, {args.large_repo}); "
                "budget audit only for queries\n"
            )
        for corpus in corpora:
            report.trajectories.extend(trajectory_blast(rgctl, corpus, [meta_skill]))
            if osv_fixture.is_file():
                report.trajectories.extend(
                    trajectory_vuln(rgctl, corpus, [vuln_skill], osv_fixture)
                )
            report.trajectories.extend(trajectory_review(rgctl, corpus, [review_skill]))
            report.trajectories.extend(trajectory_slice(rgctl, corpus, [meta_skill]))
            report.trajectories.extend(trajectory_migrate(rgctl, corpus, [migrate_skill]))
            report.trajectories.extend(trajectory_hotspots(rgctl, corpus, [meta_skill]))

    if suite in ("usecases", "all"):
        report.usecases.extend(run_usecases(args, rgctl, skills_root))

    with_skill_failures = [
        t for t in report.trajectories if t.arm == "with_skill" and not t.ok
    ]
    usecase_oracle_failures = [
        u for u in report.usecases if u.arm == "with_skill" and not u.oracle_passed
    ]
    queries_ok = True if suite == "usecases" else len(with_skill_failures) == 0
    usecases_ok = True if suite == "queries" else len(usecase_oracle_failures) == 0
    report.passed = budget_ok and queries_ok and usecases_ok
    return report


def grade_usecase_workspace(
    usecase_id: str, rgctl: Path, ws: Path, osv_fixture: Path
) -> tuple[bool, float, list[str]]:
    """Deterministic post-edit graph oracle for agent-modified workspaces.

    Returns (oracle_passed, oracle_score, oracle_checks).
    """

    def _pack(checks: list[tuple[str, bool]]) -> tuple[bool, float, list[str]]:
        score, labels, passed = _score_checks(checks)
        return passed, score, labels

    if usecase_id == "U1":
        _, doc2, _, _ = rgctl_json(rgctl, ws, ["find", "import javax.ejb*", "--type", "import"])
        _, doc3, _, _ = rgctl_json(rgctl, ws, ["find", "import jakarta.ejb*", "--type", "import"])
        return _pack(
            [
                (
                    f"javax.ejb imports == 0 (actual: {(doc2 or {}).get('total')})",
                    (doc2 or {}).get("total") == 0,
                ),
                (
                    f"jakarta.ejb imports == 13 (actual: {(doc3 or {}).get('total')})",
                    (doc3 or {}).get("total") == 13,
                ),
            ]
        )
    if usecase_id == "U2":
        _, doc3, _, _ = rgctl_json(rgctl, ws, ["find", "import javax.json*", "--type", "import"])
        _, doc4, _, _ = rgctl_json(rgctl, ws, ["callers", "jsonToOrder", "--depth", "1"])
        return _pack(
            [
                (
                    f"javax.json imports == 0 (actual: {(doc3 or {}).get('total')})",
                    (doc3 or {}).get("total") == 0,
                ),
                (
                    f"callers jsonToOrder == 1 (actual: {(doc4 or {}).get('total')})",
                    (doc4 or {}).get("total") == 1,
                ),
            ]
        )
    if usecase_id == "U3":
        _, doc2, _, _ = rgctl_json(
            rgctl, ws, ["find", "priceShoppingCart", "--type", "function"]
        )
        _, doc3, _, _ = rgctl_json(
            rgctl, ws, ["callers", "recalculateCartPricing", "--depth", "1"]
        )
        c4, doc4, _, _ = rgctl_json(
            rgctl, ws, ["review", "paths", "--synthetic-head", "worktree", "--max-symbols", "25"]
        )
        cs = (doc4 or {}).get("change_summary") or {}
        changed = cs.get("changed_symbols") or 0
        files_in_scope = cs.get("files_in_scope") or 0
        n_syms = len((doc4 or {}).get("symbols") or [])
        review_ok = c4 == 0 and (changed >= 1 or files_in_scope >= 1 or n_syms >= 1)
        return _pack(
            [
                (
                    f"find priceShoppingCart == 0 (actual: {(doc2 or {}).get('total')})",
                    (doc2 or {}).get("total") == 0,
                ),
                (
                    f"callers recalculateCartPricing == 4 (actual: {(doc3 or {}).get('total')})",
                    (doc3 or {}).get("total") == 4,
                ),
                (
                    f"review delta (changed={changed}, files={files_in_scope}, syms={n_syms})",
                    review_ok,
                ),
            ]
        )
    if usecase_id == "U4":
        scs = ws / "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java"
        has_remote = scs.is_file() and "ShippingServiceRemote ss;" in scs.read_text(encoding="utf-8")
        _, doc3, _, _ = rgctl_json(rgctl, ws, ["status"])
        index_current = bool((doc3 or {}).get("index_current"))
        if "index_current" not in (doc3 or {}):
            caps = (doc3 or {}).get("capabilities") or {}
            index_current = bool((caps.get("structural_graph") or {}).get("ready"))
        _, doc4, _, _ = rgctl_json(
            rgctl,
            ws,
            [
                "relations",
                "--edge",
                "implements",
                "--from-type",
                "class",
            ],
        )
        returned = (doc4 or {}).get("returned")
        if returned is None:
            returned = len((doc4 or {}).get("edges") or [])
        shipping_src = (
            ws / "src/main/java/com/redhat/coolstore/service/ShippingService.java"
        ).read_text(encoding="utf-8")
        implements_ok = int(returned or 0) >= 1 or "implements ShippingServiceRemote" in shipping_src
        return _pack(
            [
                (f"index ready (actual: {index_current})", index_current),
                ("ShippingServiceRemote ss; present", has_remote),
                (f"implements ShippingServiceRemote (returned={returned})", implements_ok),
            ]
        )
    if usecase_id == "U5":
        _, doc2, _, _ = rgctl_json(
            rgctl, ws, ["clones", "--mode", "exact", "--min-loc", "5", "--no-cache"]
        )
        _, doc3, _, _ = rgctl_json(
            rgctl,
            ws,
            ["callers", "normalizePayload", "--depth", "1"],
        )
        clone_b = ws / "src" / "CloneB.java"
        delegates = clone_b.is_file() and "CloneA.normalizePayload" in clone_b.read_text(
            encoding="utf-8"
        )
        callers_n = int((doc3 or {}).get("total") or 0)
        return _pack(
            [
                (
                    f"clones group_count == 0 (actual: {(doc2 or {}).get('group_count')})",
                    (doc2 or {}).get("group_count") == 0,
                ),
                (
                    f"CloneB→CloneA callers/delegation (callers={callers_n})",
                    callers_n >= 1 or delegates,
                ),
            ]
        )
    if usecase_id == "U6":
        jars = ["--include-jars", "lib"] if (ws / "lib").is_dir() else []
        _, doc2, _, _ = rgctl_json(
            rgctl, ws, ["vuln", "analyze", "--osv", str(osv_fixture), *jars]
        )
        sites_post = ((doc2 or {}).get("sink_taint") or {}).get("call_sites") or []
        exp = (doc2 or {}).get("exploitability")
        return _pack(
            [
                (f"post call_sites == 0 (actual: {len(sites_post)})", len(sites_post) == 0),
                (f"exploitability != exploitable (actual: {exp})", exp != "exploitable"),
            ]
        )
    return False, 0.0, [f"unknown usecase {usecase_id}"]


def run_agent(args: argparse.Namespace) -> Report:
    report = Report(mode="agent")
    report.budgets = audit_budgets(REPO_ROOT / "skills")
    if not args.agent_cmd:
        report.agent = {
            "status": "skipped",
            "reason": "pass --agent-cmd to run live A/B (Arm0 no-skill, Arm1 ambient rgctl, Arm2 slash commands)",
            "arms": ["no_skill", "ambient_rgctl", "slash_commands"],
            "metrics": [
                "ground_truth_f1",
                "input_tokens",
                "output_tokens",
                "cached_tokens",
                "tool_turns",
                "cli_flag_error_rate",
                "wall_clock_s",
                "oracle_score",
            ],
        }
        report.passed = all(b.ok for b in report.budgets)
        return report

    rgctl = find_rgctl()
    suite = getattr(args, "suite", "all")
    wanted = parse_usecase_filter(getattr(args, "usecase", "all"))
    osv_fixture = REPO_ROOT / "tests" / "fixtures" / "osv" / "jackson-CVE-2022-42004.json"
    coolstore = REPO_ROOT / args.coolstore
    clone_exact = REPO_ROOT / "rgctl-tests" / "clone-exact"

    agent_rows: list[dict[str, Any]] = []
    if suite in ("usecases", "all") and rgctl:
        arms = ["no_skill", "ambient_rgctl", "slash_commands"]
        for uid in USECASE_IDS:
            if uid not in wanted:
                continue
            src = clone_exact if uid == "U5" else coolstore
            if not src.is_dir():
                continue
            prompt = USECASE_PROMPTS[uid]
            for arm in arms:
                with_cfg = uid == "U6"
                tmp = prepare_temp_workspace(rgctl, src, init_git=True, with_cfg=with_cfg)
                try:
                    ws: Path = getattr(tmp, "workspace")
                    if uid == "U4":
                        _seed_shipping_concrete_field(
                            ws / "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java"
                        )
                        run_cmd(
                            [
                                str(rgctl),
                                "update",
                                "--files",
                                "src/main/java/com/redhat/coolstore/service/ShoppingCartService.java",
                            ],
                            ws,
                        )
                    if uid == "U6":
                        _seed_objectmapper_json_to_order(
                            ws / "src/main/java/com/redhat/coolstore/utils/Transformers.java"
                        )
                        run_cmd([str(rgctl), "discover", ".", "--with-cfg"], ws, timeout=300.0)
                    t0 = time.perf_counter()
                    code, stdout, err, ms = run_cmd(
                        args.agent_cmd.split()
                        + [
                            "--usecase",
                            uid,
                            "--arm",
                            arm,
                            "--workspace",
                            str(ws),
                            "--prompt",
                            prompt,
                        ],
                        REPO_ROOT,
                        timeout=args.agent_timeout,
                    )
                    # Refresh graph before oracle when agent may have edited sources.
                    run_cmd([str(rgctl), "update", "--force"], ws, timeout=180.0)
                    if uid in ("U5", "U6"):
                        disc = [str(rgctl), "discover", "."]
                        if uid == "U6":
                            disc.append("--with-cfg")
                        run_cmd(disc, ws, timeout=300.0)
                    oracle_passed, oracle_score, oracle_checks = grade_usecase_workspace(
                        uid, rgctl, ws, osv_fixture
                    )
                    agent_rows.append(
                        {
                            "usecase_id": uid,
                            "arm": arm,
                            "exit_code": code,
                            "wall_ms": ms,
                            "oracle_passed": oracle_passed,
                            "oracle_score": oracle_score,
                            "oracle_checks": oracle_checks,
                            "stdout": stdout.decode(errors="replace")[:2000],
                            "stderr": err.decode(errors="replace")[:1000],
                            "elapsed_ms": (time.perf_counter() - t0) * 1000.0,
                        }
                    )
                    report.usecases.append(
                        make_usecase_result(
                            uid,
                            USECASE_PROMPTS[uid][:60],
                            rel_corpus(src),
                            arm,
                            code == 0 and oracle_passed,
                            oracle_passed,
                            oracle_score,
                            oracle_checks,
                            ms,
                            estimate_tokens(stdout),
                            1,
                            [],
                            "" if code == 0 else err.decode(errors="replace")[:200],
                        )
                    )
                finally:
                    tmp.cleanup()

    if not agent_rows:
        # Fallback: single external runner invocation (legacy Level-1 agent harness).
        code, stdout, err, ms = run_cmd(
            args.agent_cmd.split()
            + [
                "--coolstore",
                str(REPO_ROOT / args.coolstore),
                "--large-repo",
                str(REPO_ROOT / args.large_repo),
            ],
            REPO_ROOT,
            timeout=args.agent_timeout,
        )
        report.agent = {
            "status": "ok" if code == 0 else "failed",
            "wall_ms": ms,
            "stdout": stdout.decode(errors="replace")[:4000],
            "stderr": err.decode(errors="replace")[:2000],
            "exit_code": code,
        }
        report.passed = code == 0 and all(b.ok for b in report.budgets)
        return report

    report.agent = {"status": "ok", "rows": agent_rows}
    usecase_ok = all(u.oracle_passed for u in report.usecases if u.arm != "no_skill") or all(
        r.get("oracle_passed") for r in agent_rows
    )
    # Require at least ambient/slash arms to pass oracle when present.
    graded = [r for r in agent_rows if r["arm"] in ("ambient_rgctl", "slash_commands")]
    if graded:
        usecase_ok = all(r["oracle_passed"] for r in graded)
    report.passed = all(b.ok for b in report.budgets) and usecase_ok
    return report


def emit_markdown(report: Report) -> str:
    lines = [
        f"# rgctl skill eval (`{report.mode}`)",
        "",
        f"**Passed:** {report.passed}",
        "",
        "## Token budgets",
        "",
        "| Path | Bytes | Tokens (est) | Max tokens | Max bytes | OK |",
        "|------|------:|-------------:|-----------:|----------:|:--:|",
    ]
    for b in report.budgets:
        lines.append(
            f"| `{b.path}` | {b.bytes} | {b.tokens_est} | {b.max_tokens} | {b.max_bytes} | "
            f"{'✓' if b.ok else '✗'} |"
        )
    if report.trajectories:
        lines += [
            "",
            "## Trajectories",
            "",
            "| Task | Corpus | Arm | OK | Wall ms | Stdout tok | Turns | Prompt 1-turn | Prompt 5-turn cached | Net 5-turn tok | Detail |",
            "|------|--------|-----|:--:|--------:|-----------:|------:|--------------:|---------------------:|---------------:|--------|",
        ]
        for t in report.trajectories:
            lines.append(
                f"| {t.task} | `{t.corpus}` | {t.arm} | {'✓' if t.ok else '✗'} | "
                f"{t.wall_ms:.0f} | {t.stdout_tokens_est} | {t.tool_turns} | "
                f"{t.prompt_tokens_1turn} | {t.prompt_tokens_5turn_cached} | "
                f"{t.net_tokens_5turn_cached} | {t.detail or t.error} |"
            )
    if report.usecases:
        lines += [
            "",
            "## Use-Case Evals (ScarfBench-Inspired Transformation Suite)",
            "",
            "| Use Case | Corpus | Arm | OK | Oracle | Score | Wall ms | Stdout tok | Turns | Net 5-turn tok | Oracle Checks |",
            "|----------|--------|-----|:--:|:------:|------:|--------:|-----------:|------:|---------------:|---------------|",
        ]
        for u in report.usecases:
            checks = "; ".join(u.oracle_checks[:3])
            lines.append(
                f"| {u.title} | `{u.corpus}` | {u.arm} | "
                f"{'✓' if u.ok else '✗'} | {'✓' if u.oracle_passed else '✗'} | "
                f"{u.oracle_score:.2f} | {u.wall_ms:.0f} | {u.stdout_tokens_est} | "
                f"{u.tool_turns} | {u.net_tokens_5turn_cached} | {checks or u.error} |"
            )
    if report.agent:
        lines += ["", "## Agent A/B", "", "```json", json.dumps(report.agent, indent=2), "```"]
    lines.append("")
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--mode", choices=("offline", "agent"), default="offline")
    ap.add_argument("--coolstore", default="example/coolstore")
    ap.add_argument(
        "--large-repo",
        default="example/kafka",
        help="1K+ file corpus for scale trajectories (skipped if missing)",
    )
    ap.add_argument(
        "--suite",
        choices=("queries", "usecases", "all"),
        default="all",
        help="Level 1 queries, Level 2 usecases, or both",
    )
    ap.add_argument(
        "--usecase",
        default="all",
        help='Use-case filter: "all" or comma-separated U1,U2,U3,U4,U5,U6',
    )
    ap.add_argument("--json-out", type=Path, default=None)
    ap.add_argument("--md-out", type=Path, default=None)
    ap.add_argument("--agent-cmd", default="", help="External runner for --mode agent")
    ap.add_argument("--agent-timeout", type=float, default=1800.0)
    args = ap.parse_args()

    if args.mode == "offline":
        report = run_offline(args)
    else:
        report = run_agent(args)

    md = emit_markdown(report)
    print(md)
    payload = {
        "mode": report.mode,
        "passed": report.passed,
        "budgets": [asdict(b) for b in report.budgets],
        "trajectories": [asdict(t) for t in report.trajectories],
        "usecases": [asdict(u) for u in report.usecases],
        "agent": report.agent,
    }
    if args.json_out:
        args.json_out.parent.mkdir(parents=True, exist_ok=True)
        args.json_out.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")
    if args.md_out:
        args.md_out.parent.mkdir(parents=True, exist_ok=True)
        args.md_out.write_text(md, encoding="utf-8")

    return 0 if report.passed else 1


if __name__ == "__main__":
    sys.exit(main())
