#!/usr/bin/env bash
# Check out the craft-i18n interface catalogs at the commit pinned in craft-i18n.rev, so a build
# embeds the translations and the tests that need them run instead of being skipped.
#
# Usage: packaging/fetch-craft-i18n.sh [dir]     (default: target/craft-i18n)
# Then:  export CRAFT_I18N_DIR="$(pwd)/target/craft-i18n"
#
# Without CRAFT_I18N_DIR the apps build and run in English only (see docs/localization.md).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DIR="${1:-$ROOT/target/craft-i18n}"
REV="$(tr -d '[:space:]' <"$ROOT/craft-i18n.rev")"
URL="${CRAFT_I18N_URL:-https://github.com/storytold/craft-i18n}"
mkdir -p "$DIR"
cd "$DIR"
[[ -d .git ]] || git init -q
if [[ "$(git rev-parse -q --verify HEAD 2>/dev/null || true)" != "$REV" ]]; then
  git fetch -q --depth 1 "$URL" "$REV"
  git checkout -q --detach FETCH_HEAD
fi
[[ "$(git rev-parse HEAD)" = "$REV" ]] || { echo "error: $DIR is not at $REV" >&2; exit 1; }
DIR="$(pwd)"
echo "craft-i18n $REV in $DIR"
echo "export CRAFT_I18N_DIR=\"$DIR\""
