#!/usr/bin/env bash
# Decide what one run of .github/workflows/build.yaml does, from the event that started it.
# Prints KEY=VALUE lines (the workflow appends them to $GITHUB_OUTPUT); docs/ci.md has the table.
#
#   mode      quick   format, clippy, tests, lint (pull requests, pushes to other branches)
#             full    quick + Windows tests + both products' installers on every platform + the
#                     install, upgrade, uninstall and migration tests (beta, main, and pull
#                     requests with the `full-ci` label or changes to packaging/, the workflow or
#                     the app icons)
#             none    nothing: a push to a branch whose open pull request runs the same checks,
#                     or a scheduled run (only the quality jobs below)
#   quality   1 runs the long checks: fuzzing and the macOS tests (schedule, manual release-check)
#   sonar, nix, house_rules, packaging   1 runs that check (by event and changed files)
#   tag, version   the release this run publishes (packaging/release-tag.sh), or empty
#
# Reads the GitHub Actions environment (GITHUB_EVENT_NAME, GITHUB_REF, GITHUB_REPOSITORY,
# GITHUB_RUN_NUMBER) and, from the workflow: INPUT_MODE (manual runs), PR_LABELS (JSON array),
# BASE_SHA / BEFORE_SHA (the commit to diff against), GH_TOKEN (to look for an open pull request).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

event="${GITHUB_EVENT_NAME:?}"
ref="${GITHUB_REF:?}"
repo="${GITHUB_REPOSITORY:-}"
OWN_REPO="${OWN_REPO:-WENSHAO521/psg-pdf}"
branch="${ref#refs/heads/}"

# Files this run changes, one per line; "*" when unknown (then every path-filtered check runs).
changed_files() {
  local base=""
  case "$event" in
    pull_request) base="${BASE_SHA:-}" ;;
    push) base="${BEFORE_SHA:-}" ;;
  esac
  if [[ -z "$base" || "$base" =~ ^0+$ ]] || ! git cat-file -e "$base^{commit}" 2>/dev/null; then
    echo "*"
    return
  fi
  git diff --name-only "$base" HEAD
}
touched() { # <extended regex>: whether a changed file matches
  grep -Eq "^\*\$|$1" <<<"$files"
}

mode=quick quality=0 sonar=0 nix=0 house_rules=0 packaging=0
files="$(changed_files)"
case "$event" in
  pull_request)
    if grep -q '"full-ci"' <<<"${PR_LABELS:-[]}"; then mode=full; fi
    # A change to how the installers are made is install-tested before it reaches beta.
    if touched '^(packaging/|\.github/workflows/|assets/app-icon)'; then mode=full; fi
    sonar=1
    ;;
  push)
    case "$branch" in
      main | beta) mode=full ;;
      *)
        # A pull request from this branch already runs these checks on the same commit.
        if [[ -n "${GH_TOKEN:-}" && -n "$repo" ]] &&
          [[ "$(gh pr list --repo "$repo" --head "$branch" --state open --json number --jq length 2>/dev/null || echo 0)" != 0 ]]; then
          mode=none
        fi
        ;;
    esac
    [[ "$branch" == main ]] && sonar=1
    ;;
  schedule)
    mode=none quality=1 sonar=0 nix=1
    ;;
  workflow_dispatch)
    case "${INPUT_MODE:-quick}" in
      quick) mode=quick ;;
      full) mode=full ;;
      release-check) mode=full quality=1 sonar=1 nix=1 house_rules=1 packaging=1 ;;
      quality) mode=none quality=1 nix=1 ;;
      *) echo "error: unknown mode ${INPUT_MODE}; use quick, full, release-check or quality" >&2; exit 2 ;;
    esac
    ;;
  *) mode=none ;;
esac

if [[ "$mode" != none ]]; then
  # The same path filters the separate workflows had.
  touched '^(flake\.nix|flake\.lock|nix/|Cargo\.lock|\.github/workflows/)' && nix=1
  touched '^(AGENTS\.md|CLAUDE\.md|\.github/workflows/)' && house_rules=1
  touched '^(\.github/workflows/|packaging/|assets/app-icon)' && packaging=1
fi
# Installers are only built in full runs; their scripts are linted there as well.
[[ "$mode" == full ]] && packaging=1

tag=""
if [[ "$event" == push && "$repo" == "$OWN_REPO" && "$mode" == full ]]; then
  tag="$(packaging/release-tag.sh)"
fi

cat <<EOF
mode=$mode
quality=$quality
sonar=$sonar
nix=$nix
house_rules=$house_rules
packaging=$packaging
tag=$tag
version=${tag#v}
EOF
