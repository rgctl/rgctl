#!/usr/bin/env bash
# Bump the lockstep workspace version and optionally scaffold release notes / open a Release PR.
#
# Usage:
#   ./scripts/bump-version.sh patch              # 0.4.21 -> 0.4.22 (TOMLs + README + release notes)
#   ./scripts/bump-version.sh minor              # 0.4.21 -> 0.5.0
#   ./scripts/bump-version.sh 0.4.22             # set exact version
#   ./scripts/bump-version.sh patch --pr         # also create branch release/vX.Y.Z, commit, push & open PR
#
# Updates:
#   - [workspace.package] version (crates use version.workspace = true)
#   - path crate version= pins under [workspace.dependencies]
#   - Cargo.lock workspace crate versions
#   - README.md latest-release links
#   - docs/releases/vX.Y.Z.md (scaffolded if missing)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

ARG="${1:-}"
OPEN_PR=0
if [[ "${2:-}" == "--pr" ]]; then
  OPEN_PR=1
fi

if [[ -z "$ARG" ]]; then
  echo "usage: $0 <patch|minor|major|X.Y.Z> [--pr]" >&2
  exit 1
fi

read_workspace_version() {
  python3 - <<'PY'
import re
from pathlib import Path
text = Path("Cargo.toml").read_text()
m = re.search(r'(?ms)^\[workspace\.package\].*?^version = "([^"]+)"', text)
if not m:
    raise SystemExit("could not find [workspace.package] version")
print(m.group(1))
PY
}

prev="$(read_workspace_version)"

compute_new_version() {
  python3 - "$prev" "$ARG" <<'PY'
import re, sys
prev, arg = sys.argv[1], sys.argv[2]
if re.match(r'^[0-9]+\.[0-9]+\.[0-9]+([.-].*)?$', arg):
    print(arg)
    raise SystemExit(0)
m = re.match(r'^([0-9]+)\.([0-9]+)\.([0-9]+)', prev)
if not m:
    raise SystemExit(f"cannot parse current version: {prev}")
maj, min_, pat = map(int, m.groups())
if arg == "patch":
    print(f"{maj}.{min_}.{pat + 1}")
elif arg == "minor":
    print(f"{maj}.{min_ + 1}.0")
elif arg == "major":
    print(f"{maj + 1}.0.0")
else:
    raise SystemExit(f"error: unknown bump '{arg}' (use patch|minor|major|X.Y.Z)")
PY
}

new="$(compute_new_version)"

if [[ "$OPEN_PR" == "1" ]]; then
  if [[ -n "$(git status --porcelain)" ]]; then
    echo "error: working tree is not clean; commit or stash changes before --pr" >&2
    exit 1
  fi
  git checkout -b "release/v${new}"
fi

if cargo set-version -h >/dev/null 2>&1; then
  cargo set-version --workspace "$new"
else
  python3 - "$prev" "$new" <<'PY'
import re, sys
from pathlib import Path
prev, new = sys.argv[1], sys.argv[2]
p = Path("Cargo.toml")
text = p.read_text()
text2 = re.sub(
    r'(?ms)(^\[workspace\.package\].*?^version = )"[^"]+"',
    rf'\1"{new}"',
    text,
    count=1,
)
p.write_text(text2)
PY
fi

# Keep [workspace.dependencies] path pins in lockstep (needed for publish metadata).
python3 - "$prev" "$new" <<'PY'
import re, sys
from pathlib import Path
prev, new = sys.argv[1], sys.argv[2]
path = Path("Cargo.toml")
text = path.read_text()

def repl_block(match: re.Match) -> str:
    return match.group(0).replace(f'version = "{prev}"', f'version = "{new}"')

text2, n = re.subn(
    r'(?ms)^\[workspace\.dependencies\]\n.*?(?=^\[|\Z)',
    repl_block,
    text,
    count=1,
)
if n != 1:
    print("warning: could not locate [workspace.dependencies] block to sync", file=sys.stderr)
else:
    path.write_text(text2)
    print(f"synced [workspace.dependencies] path versions {prev} -> {new}")
PY

# Sync workspace package versions in Cargo.lock without hitting network.
if [[ -f Cargo.lock ]]; then
  python3 - "$prev" "$new" <<'PY'
import re, sys
from pathlib import Path
prev, new = sys.argv[1], sys.argv[2]
lock = Path("Cargo.lock")
text = lock.read_text()
updated = re.sub(
    rf'(name = "rgctl[^"]*"\nversion = )"{re.escape(prev)}"',
    rf'\1"{new}"',
    text,
)
if updated != text:
    lock.write_text(updated)
    print(f"synced Cargo.lock rgctl* crate versions {prev} -> {new}")
PY
fi

echo "workspace version: $prev -> $new"

# Keep README latest-release link in sync when present.
if [[ -f README.md ]]; then
  python3 - "$prev" "$new" <<'PY'
import sys
from pathlib import Path
prev, new = sys.argv[1], sys.argv[2]
path = Path("README.md")
text = path.read_text()
updated = text.replace(f"v{prev}", f"v{new}").replace(
    f"docs/releases/v{prev}.md", f"docs/releases/v{new}.md"
)
if updated != text:
    path.write_text(updated)
    print(f"updated README.md release links {prev} -> {new}")
PY
fi

# Scaffold docs/releases/vX.Y.Z.md if missing.
NOTES_FILE="docs/releases/v${new}.md"
if [[ ! -f "$NOTES_FILE" ]]; then
  mkdir -p docs/releases
  LAST_TAG="$(git describe --tags --abbrev=0 --match 'v*' 2>/dev/null || echo "")"
  if [[ -n "$LAST_TAG" ]]; then
    COMMITS="$(git log "${LAST_TAG}..HEAD" --no-merges --pretty=format:'- %s (%h)' || true)"
  else
    COMMITS="$(git log -n 20 --no-merges --pretty=format:'- %s (%h)' || true)"
  fi
  cat > "$NOTES_FILE" <<EOF
# rgctl v${new}

## Highlights

${COMMITS:-- Maintenance and performance improvements.}
EOF
  echo "scaffolded $NOTES_FILE"
fi

if [[ "$OPEN_PR" == "1" ]]; then
  REMOTE="origin"
  if git remote get-url upstream >/dev/null 2>&1; then
    REMOTE="upstream"
  fi
  git add Cargo.toml Cargo.lock README.md "$NOTES_FILE"
  git commit -s -m "chore(release): v${new}"
  git push -u "$REMOTE" "release/v${new}"
  gh pr create --repo rgctl/rgctl \
    --base main \
    --head "release/v${new}" \
    --title "chore(release): v${new}" \
    --body "Automated release bump \`v${prev}\` → \`v${new}\`. Merging this PR into \`main\` automatically tags \`v${new}\` and publishes the GitHub Release binaries."
  exit 0
fi

echo "next: review git diff, then open a Release PR (or run with --pr):"
echo "  $0 $new --pr"
