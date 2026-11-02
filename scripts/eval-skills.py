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
from typing import Any, Callable, Optional

REPO_ROOT = Path(__file__).resolve().parents[1]

# Prompt-token ceilings (approx tokens ≈ bytes/4 when tiktoken unavailable).
META_SKILL_MAX_TOKENS = 5500
META_SKILL_MAX_BYTES = 22 * 1024
SLASH_SKILL_MAX_TOKENS = 2200
SLASH_SKILL_MAX_BYTES = 9 * 1024

SLASH_SKILLS = ("rgctl-review", "rgctl-vuln", "rgctl-migrate")


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


def read_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


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
    """Approximate prompt tokens: skill text once + (turns-1) cached reuse tax = 0."""
    total = sum(estimate_tokens(p.read_bytes()) for p in skill_paths if p.is_file())
    if turns <= 1:
        return total
    # Cached multi-turn: count full prompt once; subsequent turns pay ~0 for static skill body.
    return total


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


def trajectory_blast(
    rgctl: Path, corpus: Path, skill_paths: list[Path]
) -> list[TrajectoryResult]:
    out: list[TrajectoryResult] = []
    # With-skill
    if not ensure_index(rgctl, corpus):
        return [
            TrajectoryResult(
                "Blast Radius",
                str(corpus),
                "with_skill",
                False,
                0,
                0,
                0,
                0,
                0,
                error="discover failed",
            )
        ]
    # Pick a symbol via find
    code, stdout, _, ms_find = run_cmd(
        [str(rgctl), "-f", "json", "find", "--type", "function", "--limit", "5"],
        corpus,
    )
    symbol = "main"
    if code == 0:
        try:
            doc = json.loads(stdout)
            hits = doc.get("results") or doc.get("symbols") or doc.get("hits") or []
            if hits and isinstance(hits, list):
                first = hits[0]
                symbol = first.get("name") or first.get("qualified_name") or symbol
        except json.JSONDecodeError:
            pass
    code, stdout, err, ms = run_cmd(
        [str(rgctl), "-f", "json", "blast-radius", symbol, "--depth", "2", "--limit", "20"],
        corpus,
    )
    p1 = prompt_tax(skill_paths, 1)
    p5 = prompt_tax(skill_paths, 5)
    out.append(
        TrajectoryResult(
            task="Blast Radius",
            corpus=str(corpus.relative_to(REPO_ROOT) if corpus.is_relative_to(REPO_ROOT) else corpus),
            arm="with_skill",
            ok=code == 0,
            wall_ms=ms + ms_find,
            stdout_tokens_est=estimate_tokens(stdout),
            tool_turns=2,
            prompt_tokens_1turn=p1,
            prompt_tokens_5turn_cached=p5,
            detail=f"symbol={symbol}",
            error="" if code == 0 else err.decode(errors="replace")[:200],
        )
    )
    # No-skill baseline: rg + read a few matching files
    rg = find_rg()
    if rg:
        t0 = time.perf_counter()
        code_rg, out_rg, err_rg, _ = run_cmd(
            [rg, "-n", "--type-add", "src:*.{rs,java,go,py,js,ts}", "-t", "src", symbol],
            corpus,
            timeout=60.0,
        )
        # Read up to 3 hit files
        files_read = 0
        read_bytes = 0
        for line in out_rg.decode(errors="replace").splitlines()[:30]:
            if ":" not in line:
                continue
            rel = line.split(":", 1)[0]
            fp = corpus / rel
            if fp.is_file():
                data = fp.read_bytes()[:8000]
                read_bytes += len(data)
                files_read += 1
                if files_read >= 3:
                    break
        ms_ns = (time.perf_counter() - t0) * 1000.0
        out.append(
            TrajectoryResult(
                task="Blast Radius",
                corpus=str(corpus.relative_to(REPO_ROOT) if corpus.is_relative_to(REPO_ROOT) else corpus),
                arm="no_skill",
                ok=True,
                wall_ms=ms_ns,
                stdout_tokens_est=estimate_tokens(out_rg) + estimate_tokens(b"x" * read_bytes),
                tool_turns=1 + files_read,
                prompt_tokens_1turn=0,
                prompt_tokens_5turn_cached=0,
                detail=f"rg+{files_read} files",
                error="" if code_rg in (0, 1) else err_rg.decode(errors="replace")[:200],
            )
        )
    else:
        out.append(
            TrajectoryResult(
                task="Blast Radius",
                corpus=str(corpus.relative_to(REPO_ROOT) if corpus.is_relative_to(REPO_ROOT) else corpus),
                arm="no_skill",
                ok=False,
                wall_ms=0,
                stdout_tokens_est=0,
                tool_turns=0,
                prompt_tokens_1turn=0,
                prompt_tokens_5turn_cached=0,
                error="rg not found",
            )
        )
    return out


def trajectory_simple(
    task: str,
    rgctl: Path,
    corpus: Path,
    skill_paths: list[Path],
    with_argv: list[str],
    no_skill_pattern: str,
    need_cfg: bool = False,
    tool_turns: int = 1,
) -> list[TrajectoryResult]:
    out: list[TrajectoryResult] = []
    rel = str(corpus.relative_to(REPO_ROOT) if corpus.is_relative_to(REPO_ROOT) else corpus)
    if not ensure_index(rgctl, corpus, with_cfg=need_cfg):
        return [
            TrajectoryResult(
                task, rel, "with_skill", False, 0, 0, 0, 0, 0, error="discover failed"
            )
        ]
    code, stdout, err, ms = run_cmd([str(rgctl), *with_argv], corpus, timeout=180.0)
    p1 = prompt_tax(skill_paths, 1)
    p5 = prompt_tax(skill_paths, 5)
    out.append(
        TrajectoryResult(
            task=task,
            corpus=rel,
            arm="with_skill",
            ok=code == 0,
            wall_ms=ms,
            stdout_tokens_est=estimate_tokens(stdout),
            tool_turns=tool_turns,
            prompt_tokens_1turn=p1,
            prompt_tokens_5turn_cached=p5,
            error="" if code == 0 else err.decode(errors="replace")[:200],
        )
    )
    rg = find_rg()
    if rg:
        t0 = time.perf_counter()
        code_rg, out_rg, err_rg, _ = run_cmd(
            [rg, "-n", no_skill_pattern, str(corpus)],
            REPO_ROOT,
            timeout=60.0,
        )
        files_read = 0
        read_bytes = 0
        for line in out_rg.decode(errors="replace").splitlines()[:40]:
            # rg path:line:… when searching absolute; handle both
            parts = line.split(":", 2)
            if len(parts) < 2:
                continue
            candidate = Path(parts[0])
            if not candidate.is_file():
                candidate = corpus / parts[0]
            if candidate.is_file():
                read_bytes += len(candidate.read_bytes()[:8000])
                files_read += 1
                if files_read >= 3:
                    break
        ms_ns = (time.perf_counter() - t0) * 1000.0
        out.append(
            TrajectoryResult(
                task=task,
                corpus=rel,
                arm="no_skill",
                ok=True,
                wall_ms=ms_ns,
                stdout_tokens_est=estimate_tokens(out_rg) + (read_bytes // 4),
                tool_turns=1 + files_read,
                prompt_tokens_1turn=0,
                prompt_tokens_5turn_cached=0,
                detail=f"rg+{files_read} files",
                error="" if code_rg in (0, 1) else err_rg.decode(errors="replace")[:200],
            )
        )
    else:
        out.append(
            TrajectoryResult(
                task, rel, "no_skill", False, 0, 0, 0, 0, 0, error="rg not found"
            )
        )
    return out


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
        # 2 OSV Reachability (/rgctl:vuln) — only if fixture exists
        if osv_fixture.is_file():
            report.trajectories.extend(
                trajectory_simple(
                    "OSV Reachability /rgctl:vuln",
                    rgctl,
                    corpus,
                    [vuln_skill],
                    ["-f", "json", "vuln", "triage", "--osv", str(osv_fixture)],
                    "ObjectMapper|readValue|jackson",
                    need_cfg=False,
                    tool_turns=1,
                )
            )
        # 3 Uncommitted Review — synthetic-head may fail without dirty tree; still measure CLI
        report.trajectories.extend(
            trajectory_simple(
                "Uncommitted Review /rgctl:review",
                rgctl,
                corpus,
                [review_skill],
                ["-f", "json", "status"],
                "TODO|FIXME",
                tool_turns=1,
            )
        )
        # 4 Program Slicing — status/cpg as cheap proxy when no known file:line
        report.trajectories.extend(
            trajectory_simple(
                "Program Slicing",
                rgctl,
                corpus,
                [meta_skill],
                ["-f", "json", "cpg", "status"],
                "function|class ",
                tool_turns=1,
            )
        )
        # 5 Migration Planning
        report.trajectories.extend(
            trajectory_simple(
                "Migration Planning /rgctl:migrate",
                rgctl,
                corpus,
                [migrate_skill],
                ["-f", "json", "communities", "list"],
                "package |module ",
                tool_turns=1,
            )
        )
        # 6 Cross-File Callers / Hotspots
        report.trajectories.extend(
            trajectory_simple(
                "Cross-File Callers/Hotspots",
                rgctl,
                corpus,
                [meta_skill],
                ["-f", "json", "metrics", "--pagerank"],
                "public |fn ",
                tool_turns=1,
            )
        )

    # Trajectories are informational on missing corpora / empty graphs; budget is the hard gate.
    traj_hard_fail = any(
        t.arm == "with_skill" and t.task == "Blast Radius" and not t.ok and "discover failed" in t.error
        for t in report.trajectories
    )
    report.passed = budget_ok and not traj_hard_fail
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
            "| Task | Corpus | Arm | OK | Wall ms | Stdout tok | Turns | Prompt 1-turn | Prompt 5-turn cached | Detail |",
            "|------|--------|-----|:--:|--------:|-----------:|------:|--------------:|---------------------:|--------|",
        ]
        for t in report.trajectories:
            lines.append(
                f"| {t.task} | `{t.corpus}` | {t.arm} | {'✓' if t.ok else '✗'} | "
                f"{t.wall_ms:.0f} | {t.stdout_tokens_est} | {t.tool_turns} | "
                f"{t.prompt_tokens_1turn} | {t.prompt_tokens_5turn_cached} | "
                f"{t.detail or t.error} |"
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
