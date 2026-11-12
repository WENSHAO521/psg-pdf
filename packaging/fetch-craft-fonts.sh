#!/usr/bin/env bash
# Check out the craft-fonts faces at the commit pinned in craft-fonts.rev, so a build embeds them:
# Japanese, Korean and Hebrew text and the UI symbol fallback (AGENTS.md §1.4).
#
# Usage: packaging/fetch-craft-fonts.sh [dir]     (default: target/craft-fonts)
# Then:  export CRAFT_FONTS_DIR="$(pwd)/target/craft-fonts"
#
# Without CRAFT_FONTS_DIR the apps build and run with no face for those scripts.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="${1:-$ROOT/target/craft-fonts}"
REV="$(tr -d '[:space:]' <"$ROOT/craft-fonts.rev")"
URL="${CRAFT_FONTS_URL:-https://github.com/storytold/craft-fonts}"
mkdir -p "$DIR"
cd "$DIR"
[[ -d .git ]] || git init -q
if [[ "$(git rev-parse -q --verify HEAD 2>/dev/null || true)" != "$REV" ]]; then
  git fetch -q --depth 1 "$URL" "$REV"
  git checkout -q --detach FETCH_HEAD
fi
[[ "$(git rev-parse HEAD)" = "$REV" ]] || { echo "error: $DIR is not at $REV" >&2; exit 1; }
DIR="$(pwd)"
echo "craft-fonts $REV in $DIR"
echo "export CRAFT_FONTS_DIR=\"$DIR\""
