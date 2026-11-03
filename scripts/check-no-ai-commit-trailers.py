#!/usr/bin/env python3
"""Fail if commits contain AI agent co-author / assisted / generated trailers.

Intended for PR CI. Uses ``git log`` only (no git-filter-repo).

Detects tool-injected attribution across common coding agents (Cursor, Claude,
Copilot, Codex, Aider, Devin, Windsurf, Gemini/Jules, Continue, Cline/Roo,
Amazon Q, Cody, Replit, Sweep, Goose, OpenHands, Antigravity, …) via:

- trailers: Co-Authored-By / Assisted-By / Generated-By / Generated-With / …
- known agent emails / noreply domains in author, committer, or trailer
- common “Generated with …” / bot banner lines in the commit body

Examples:
  ./scripts/check-no-ai-commit-trailers.py --range origin/main..HEAD
  ./scripts/check-no-ai-commit-trailers.py --range "$BASE_SHA..$HEAD_SHA"
  ./scripts/check-no-ai-commit-trailers.py --all
  ./scripts/check-no-ai-commit-trailers.py --list-agents
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

# ---------------------------------------------------------------------------
# Agent catalog — names (matched in trailers / banners) and emails / domains
# (matched in author, committer, and trailer addresses).
# ---------------------------------------------------------------------------

# Display-name / product tokens (used inside attribution trailers).
# Each token is wrapped with word-ish boundaries when compiled to avoid
# matching inside ordinary words (e.g. Amp ⊂ example).
AGENT_NAME_TOKENS: tuple[str, ...] = (
    # Anthropic
    r"Claude(?:\s+Code)?",
    r"Anthropic",
    r"Claude\s+(?:Sonnet|Opus|Haiku)(?:\s+[\d.]+)?",
    # Cursor / Anysphere
    r"Cursor(?:\s+Agent)?",
    r"cursoragent",
    r"Anysphere",
    # GitHub Copilot
    r"GitHub\s+Copilot",
    r"Copilot(?:\s+(?:Chat|Agent|SWE|Edits))?",
    # OpenAI
    r"OpenAI",
    r"ChatGPT",
    r"Codex",
    r"GPT-?\d(?:\.\d+)?(?:-[\w-]+)?",
    r"o\d(?:-[\w]+)?",  # o1, o3-mini, …
    # Google
    r"Gemini(?:\s+(?:Code\s+Assist|CLI))?",
    r"Google\s+(?:AI|Gemini|Jules)",
    r"Bard",
    r"Jules",
    # Aider
    r"aider",
    # Cognition / Devin
    r"Devin",
    r"Cognition",
    # Windsurf / Codeium
    r"Windsurf",
    r"Cascade",
    r"Codeium",
    r"Exafunction",
    # Continue / Cline / Roo / Amp / Augment
    r"Continue(?:\.dev)?",
    r"Cline",
    r"Roo(?:\s*Code)?",
    r"RooCode",
    r"Amp(?:Code)?",
    r"Augment(?:\s+Code)?",
    # Amazon
    r"Amazon\s+Q(?:\s+Developer)?",
    r"CodeWhisperer",
    # Sourcegraph / JetBrains / GitLab / Tabnine
    r"Cody",
    r"Sourcegraph",
    r"JetBrains\s+AI(?:\s+Assistant)?",
    r"GitLab\s+Duo",
    r"Tabnine",
    r"Supermaven",
    # Replit / Sweep / Mentat / Factory / Goose / OpenHands
    r"Replit(?:\s+Agent)?",
    r"Sweep(?:\s+AI|\s+Bot)?",
    r"Mentat",
    r"Factory(?:\.ai)?",
    r"Droid",
    r"Goose",
    r"OpenHands",
    r"OpenDevin",
    r"SWE[- ]?agent",
    r"SWE[- ]?bench",
    # Others commonly seen in commits / PRs
    r"Antigravity",
    r"Blackbox(?:\s+AI)?",
    r"Phind",
    r"Pieces(?:\s+OS)?",
    r"PearAI",
    r"Bolt\.new",
    r"StackBlitz",
    r"v0(?:\.dev)?",
    r"Lovable",
    r"Warp\s+AI",
    r"Zed\s+AI",
    r"Diffblue",
    r"Qodo(?:\s+Gen)?",
    r"Codium(?:AI)?",
    r"Mutahunter",
    r"Bito",
    r"Codeium\s+Chat",
    r"Smol\s+Developer",
    r"GPT\s+Engineer",
    r"MetaGPT",
    r"AutoGPT",
    r"BabyAGI",
    r"Cursor\s+Tab",
    r"Cursor\s+Composer",
)

# Emails / domains that identify agents (author, committer, or trailer).
# No inline (?i) — composed into larger regexes; compile with re.I.
AGENT_EMAIL_PATTERNS: tuple[tuple[str, str], ...] = (
    ("Anthropic", r"(?:noreply|claude)@anthropic\.com\b"),
    ("Cursor", r"cursoragent@|[\w.+-]*@cursor\.com\b|@anysphere\.co\b"),
    ("GitHub Copilot", r"copilot(?:\[bot\])?@|[\w.+-]*\+copilot(?:\[bot\])?@users\.noreply\.github\.com\b"),
    ("OpenAI / Codex", r"[\w.+-]*@(?:openai\.com|chatgpt\.com)\b|codex(?:-bot)?@"),
    ("Aider", r"[\w.+-]*@aider\.(?:chat|dev)\b|noreply@aider\."),
    ("Devin / Cognition", r"[\w.+-]*@(?:devin\.ai|cognition\.ai|cognition\.labs)\b"),
    ("Windsurf / Codeium", r"[\w.+-]*@(?:windsurf(?:ai)?\.com|codeium\.com|exafunction\.com)\b"),
    ("Continue", r"[\w.+-]*@continue\.dev\b"),
    ("Google Gemini / Jules", r"jules(?:\[bot\])?@|gemini(?:-?code|-?bot)?@|[\w.+-]*\+jules(?:\[bot\])?@users\.noreply\.github\.com\b"),
    ("Amazon Q", r"amazon-?q(?:-?developer)?@|codewhisperer@|[\w.+-]*\+amazon-?q@"),
    ("Sourcegraph Cody", r"[\w.+-]*@sourcegraph\.com\b|cody(?:\[bot\])?@sourcegraph"),
    ("Replit", r"[\w.+-]*@replit\.com\b|replit-?agent@"),
    ("Sweep", r"sweep(?:-?ai|-?bot)?@users\.noreply\.github\.com\b|[\w.+-]*@sweep\.dev\b"),
    ("Factory / Droid", r"[\w.+-]*@factory\.ai\b|droid@factory"),
    ("Goose", r"goose(?:-?bot)?@block\.|goose\[bot\]@"),
    ("OpenHands / OpenDevin", r"openhands(?:\[bot\])?@|opendevin(?:\[bot\])?@|[\w.+-]*@all-hands\.dev\b"),
    ("SWE-agent", r"swe-?agent(?:\[bot\])?@"),
    ("Antigravity", r"[\w.+-]*@antigravity\."),
    ("Tabnine", r"[\w.+-]*@tabnine\.com\b"),
    ("Augment", r"[\w.+-]*@augmentcode\.com\b"),
    ("Cline / Roo", r"[\w.+-]*@roocode\.|roocode(?:\[bot\])?@"),
    ("v0 / Vercel", r"[\w.+-]*@v0\.dev\b"),
    ("Lovable", r"[\w.+-]*@lovable\.(?:dev|so)\b"),
    (
        "Generic AI noreply bot",
        r"(?:ai|agent)[-_.]?noreply@|noreply[-_.]?(?:ai|agent)@",
    ),
)

# Trailer keys that carry attribution (case-insensitive, optional spaces around -).
_TRAILER_KEYS = (
    r"Co-Authored-By",
    r"Assisted-By",
    r"Generated-By",
    r"Generated-With",
    r"Generated-With-AI",
    r"AI-Assisted-By",
    r"AI-Generated-By",
    r"Attributed-To",
    r"Powered-By",
    r"Made-With",
    r"Made-By",
)

_TRAILER_KEY_RE = "(?:" + "|".join(_TRAILER_KEYS) + ")"
# Bound each alternative so short tokens (Amp, Bard, …) cannot match mid-word.
_AGENT_NAME_RE = "(?:(?:^|[^A-Za-z0-9_])(?:{alts})(?=$|[^A-Za-z0-9_]))".format(
    alts="|".join(AGENT_NAME_TOKENS)
)

# Banner / body lines that are not always formal trailers.
_BANNER_PATTERNS: tuple[tuple[str, str], ...] = (
    (
        "Claude Code banner",
        r"^(?:[\U0001F300-\U0001F9FF]\s*)?Generated with\s+(?:\[)?Claude Code.*$",
    ),
    (
        "Cursor / agent banner",
        r"^(?:[\U0001F300-\U0001F9FF]\s*)?(?:Generated|Made|Written|Committed)\s+with\s+"
        r"(?:\[)?(?:Cursor|Copilot|Codex|ChatGPT|Aider|Devin|Windsurf|Cascade|Gemini|"
        r"Jules|Continue|Cline|Goose|OpenHands|Claude).*$",
    ),
    (
        "Generic AI-generated banner",
        r"^(?:[\U0001F300-\U0001F9FF]\s*)?(?:This\s+)?(?:commit|change|PR|diff)\s+(?:was\s+)?"
        r"(?:AI[- ](?:generated|assisted)|generated\s+by\s+AI|assisted\s+by\s+AI).*$",
    ),
)


def _compile_patterns() -> tuple[
    list[tuple[str, re.Pattern[str]]],
    list[tuple[str, re.Pattern[str]]],
]:
    flags = re.IGNORECASE | re.MULTILINE
    message: list[tuple[str, re.Pattern[str]]] = []

    # Any attribution trailer whose value mentions a known agent product/name.
    message.append(
        (
            "AI agent attribution trailer (known tool name)",
            re.compile(
                rf"^{_TRAILER_KEY_RE}:\s*.*{_AGENT_NAME_RE}.*$",
                flags,
            ),
        )
    )

    # Any attribution trailer whose address is a known agent email/domain.
    email_alt = "|".join(f"(?:{pat})" for _, pat in AGENT_EMAIL_PATTERNS)
    message.append(
        (
            "AI agent attribution trailer (known agent email)",
            re.compile(
                rf"^{_TRAILER_KEY_RE}:\s*.*<(?:{email_alt})>.*$",
                flags,
            ),
        )
    )

    for label, pat in _BANNER_PATTERNS:
        message.append((label, re.compile(pat, flags)))

    emails: list[tuple[str, re.Pattern[str]]] = [
        (f"{label} email/identity", re.compile(pat, re.IGNORECASE))
        for label, pat in AGENT_EMAIL_PATTERNS
    ]

    # Author/committer display names that are clearly coding-agent bot accounts.
    emails.append(
        (
            "AI/bot author display name",
            re.compile(
                r"\b(?:"
                r"copilot\[bot\]|cursoragent|claude\[bot\]|devin\[bot\]|"
                r"sweep\[bot\]|jules\[bot\]|openai\[bot\]|chatgpt\[bot\]|"
                r"aider\[bot\]|windsurf\[bot\]|codeium\[bot\]|goose\[bot\]|"
                r"openhands\[bot\]|factory\[bot\]|antigravity\[bot\]"
                r")\b",
                re.IGNORECASE,
            ),
        )
    )
    return message, emails


AI_MESSAGE_PATTERNS, AI_IDENTITY_PATTERNS = _compile_patterns()

DCO_SIGNOFF_RE = re.compile(
    r"^\s*Signed-off-by:\s+.+\s+<[^>\s]+@[^>\s]+>\s*$",
    re.IGNORECASE | re.MULTILINE,
)


@dataclass(frozen=True)
class Violation:
    sha: str
    author: str
    subject: str
    reasons: tuple[str, ...]
    evidence: tuple[str, ...]


def git_log(
    repo: Path,
    rev_range: str | None,
    all_refs: bool,
    *,
    no_merges: bool = False,
) -> str:
    fmt = "%H%x01%P%x01%an <%ae>%x01%cn <%ce>%x01%B%x00"
    cmd = ["git", "-C", str(repo), "log", f"--format={fmt}"]
    if no_merges:
        cmd.append("--no-merges")
    if all_refs:
        cmd.append("--all")
    elif rev_range:
        cmd.append(rev_range)
    else:
        cmd.append("HEAD")
    proc = subprocess.run(cmd, check=False, capture_output=True, text=True)
    if proc.returncode != 0:
        raise SystemExit(proc.stderr.strip() or f"git log failed ({proc.returncode})")
    return proc.stdout


def scan(
    raw: str,
    *,
    check_ai: bool = True,
    require_dco: bool = False,
) -> list[Violation]:
    violations: list[Violation] = []
    for block in raw.split("\0"):
        if not block.strip():
            continue
        parts = block.split("\x01", 4)
        if len(parts) < 5:
            continue
        sha, parents, author, committer, body = parts
        sha = sha.strip().lstrip("\n")
        is_merge = len(parents.strip().split()) > 1
        subject = next((ln for ln in body.splitlines() if ln.strip()), "")
        reasons: list[str] = []
        evidence: list[str] = []

        if check_ai:
            for label, rx in AI_MESSAGE_PATTERNS:
                for m in rx.finditer(body):
                    reasons.append(label)
                    line = m.group(0).splitlines()[0].strip()
                    if line not in evidence:
                        evidence.append(line)

            people = f"{author}\n{committer}"
            for label, rx in AI_IDENTITY_PATTERNS:
                if rx.search(people):
                    reasons.append(label)
                    hit = f"identity: {author} / {committer}"
                    if hit not in evidence:
                        evidence.append(hit)

        if require_dco and not is_merge and not DCO_SIGNOFF_RE.search(body):
            reasons.append("missing DCO sign-off (Signed-off-by: Name <email>)")
            evidence.append("run `git commit --amend -s` (or `git rebase --signoff`)")

        seen: set[str] = set()
        uniq_reasons: list[str] = []
        for r in reasons:
            if r not in seen:
                seen.add(r)
                uniq_reasons.append(r)

        if uniq_reasons:
            violations.append(
                Violation(
                    sha=sha,
                    author=author.strip(),
                    subject=subject.strip(),
                    reasons=tuple(uniq_reasons),
                    evidence=tuple(evidence),
                )
            )
    return violations


def list_agents() -> None:
    print("Tracked agent name tokens:")
    for tok in AGENT_NAME_TOKENS:
        print(f"  - {tok}")
    print("\nTracked agent email / domain patterns:")
    for label, pat in AGENT_EMAIL_PATTERNS:
        print(f"  - {label}: {pat}")
    print("\nAttribution trailer keys:")
    for key in _TRAILER_KEYS:
        print(f"  - {key}")


def parse_args(argv: list[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Fail if commits contain AI agent trailers or lack DCO sign-off."
    )
    p.add_argument(
        "-C",
        "--repo",
        default=".",
        help="Git repository path (default: .)",
    )
    scope = p.add_mutually_exclusive_group()
    scope.add_argument(
        "--range",
        dest="rev_range",
        metavar="REV_RANGE",
        help="Git revision range to scan (e.g. origin/main..HEAD)",
    )
    scope.add_argument(
        "--all",
        action="store_true",
        help="Scan all refs",
    )
    p.add_argument(
        "--require-dco",
        action="store_true",
        help="Also require DCO `Signed-off-by: Name <email>` on non-merge commits",
    )
    p.add_argument(
        "--dco-only",
        action="store_true",
        help="Check only DCO `Signed-off-by: Name <email>` on non-merge commits",
    )
    p.add_argument(
        "--list-agents",
        action="store_true",
        help="Print the agent catalog and exit",
    )
    p.add_argument(
        "--quiet-ok",
        action="store_true",
        help="Print nothing when there are no violations",
    )
    return p.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv if argv is not None else sys.argv[1:])
    if args.list_agents:
        list_agents()
        return 0

    check_ai = not args.dco_only
    require_dco = args.require_dco or args.dco_only

    repo = Path(args.repo).expanduser().resolve()
    raw = git_log(repo, args.rev_range, args.all, no_merges=args.dco_only)
    violations = scan(raw, check_ai=check_ai, require_dco=require_dco)

    if not violations:
        if not args.quiet_ok:
            scope = args.rev_range or ("--all" if args.all else "HEAD")
            if args.dco_only:
                print(f"OK: all non-merge commits have DCO sign-off in {scope}")
            elif require_dco:
                print(f"OK: no AI agent trailers and DCO sign-off present in {scope}")
            else:
                print(f"OK: no AI agent trailers in {scope}")
        return 0

    if args.dco_only:
        header = f"error: {len(violations)} commit(s) missing DCO sign-off:\n"
    elif require_dco:
        header = (
            f"error: {len(violations)} commit(s) failed commit policy "
            f"(AI attribution or missing DCO sign-off):\n"
        )
    else:
        header = (
            f"error: {len(violations)} commit(s) contain AI agent "
            f"created/assisted signals:\n"
        )
    print(header, file=sys.stderr)
    for v in violations:
        print(f"  {v.sha[:12]}  {v.author}", file=sys.stderr)
        print(f"    {v.subject[:120]}", file=sys.stderr)
        for reason in v.reasons:
            print(f"    - {reason}", file=sys.stderr)
        for ev in v.evidence:
            print(f"      {ev}", file=sys.stderr)
        print(file=sys.stderr)

    if args.dco_only:
        print(
            "Every non-merge commit must include a DCO sign-off trailer:\n"
            "  Signed-off-by: Your Name <your.email@example.com>\n"
            "Local tip: git commit --amend -s  or  git rebase --signoff origin/main",
            file=sys.stderr,
        )
    else:
        print(
            "Remove AI Co-Authored-By / Assisted-By / Generated-By (and similar) "
            "trailers and agent author emails, ensure `Signed-off-by:` (DCO) is present, then push again.\n"
            "Local tip: git commit --amend -s  or  interactive rebase on the PR branch.\n"
            "Disable agent co-author / attribution settings in your coding tool.\n"
            "Catalog: ./scripts/check-no-ai-commit-trailers.py --list-agents",
            file=sys.stderr,
        )
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
