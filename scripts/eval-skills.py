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
import shutil
import subprocess
import sys
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
class Report:
    mode: str
    budgets: list[BudgetResult] = field(default_factory=list)
    trajectories: list[TrajectoryResult] = field(default_factory=list)
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


def run_offline(args: argparse.Namespace) -> Report:
    skills_root = REPO_ROOT / "skills"
    report = Report(mode="offline")
    report.budgets = audit_budgets(skills_root)
    budget_ok = all(b.ok for b in report.budgets)

    rgctl = find_rgctl()
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
    if not corpora:
        sys.stderr.write(
            f"WARN: no corpora found ({args.coolstore}, {args.large_repo}); "
            "budget audit only\n"
        )
        report.passed = budget_ok
        return report

    meta_skill = skills_root / "rgctl" / "SKILL.md"
    review_skill = skills_root / "rgctl-review" / "SKILL.md"
    vuln_skill = skills_root / "rgctl-vuln" / "SKILL.md"
    migrate_skill = skills_root / "rgctl-migrate" / "SKILL.md"

    osv_fixture = REPO_ROOT / "tests" / "fixtures" / "osv" / "jackson-CVE-2022-42004.json"
    for corpus in corpora:
        # 1 Blast Radius
        report.trajectories.extend(trajectory_blast(rgctl, corpus, [meta_skill]))
        # 2 OSV Reachability (/rgctl:vuln)
        if osv_fixture.is_file():
            report.trajectories.extend(
                trajectory_vuln(rgctl, corpus, [vuln_skill], osv_fixture)
            )
        # 3 Uncommitted Review (/rgctl:review)
        report.trajectories.extend(trajectory_review(rgctl, corpus, [review_skill]))
        # 4 Program Slicing
        report.trajectories.extend(trajectory_slice(rgctl, corpus, [meta_skill]))
        # 5 Migration Planning (/rgctl:migrate)
        report.trajectories.extend(trajectory_migrate(rgctl, corpus, [migrate_skill]))
        # 6 Cross-File Callers / Hotspots
        report.trajectories.extend(trajectory_hotspots(rgctl, corpus, [meta_skill]))

    with_skill_failures = [
        t for t in report.trajectories if t.arm == "with_skill" and not t.ok
    ]
    report.passed = budget_ok and len(with_skill_failures) == 0
    return report


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
            ],
        }
        report.passed = all(b.ok for b in report.budgets)
        return report
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
