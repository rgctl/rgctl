#!/usr/bin/env bash
# Record the user-guide CLI demo with Charm VHS.
#
# Review the tape first:
#   less docs/videos/user-guide-cli.tape
#   vhs validate docs/videos/user-guide-cli.tape
#
# Then record (from repo root):
#   ./docs/videos/record-user-guide-cli.sh
#
# Outputs:
#   docs/videos/user-guide-cli-no-captions.gif
#   docs/videos/user-guide-cli-no-captions.mp4
# Burn captions separately:
#   ./docs/videos/burn-user-guide-captions.sh  → user-guide-cli.mp4
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

TAPE="$ROOT/docs/videos/user-guide-cli.tape"
OUT_GIF="$ROOT/docs/videos/user-guide-cli-no-captions.gif"
OUT_MP4="$ROOT/docs/videos/user-guide-cli-no-captions.mp4"

if ! command -v vhs >/dev/null 2>&1; then
  echo "error: vhs not found (brew install vhs)" >&2
  exit 1
fi

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq not found (needed for demo JSON pipes)" >&2
  exit 1
fi

# Prefer local release build first; fall back to PATH.
if [[ -x "$ROOT/target/release/rgctl" ]]; then
  export PATH="$ROOT/target/release:$PATH"
elif command -v rgctl >/dev/null 2>&1; then
  :
else
  echo "error: rgctl not on PATH — run: cargo build --release --bin rgctl" >&2
  exit 1
fi

echo "==> rgctl: $(command -v rgctl)"
rgctl --version
echo "==> validating tape"
vhs validate "$TAPE"
echo "==> recording (this runs discover + the full walkthrough)"
vhs "$TAPE"
echo "==> wrote:"
ls -lh "$OUT_GIF" "$OUT_MP4" 2>/dev/null || ls -lh docs/videos/user-guide-cli.*
