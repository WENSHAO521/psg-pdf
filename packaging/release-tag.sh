#!/usr/bin/env bash
# Print the tag a CI build of this commit publishes under, or nothing when it publishes none
# (.github/workflows/psg-release.yml). The version is [workspace.package] version in Cargo.toml.
#
#   beta  vX.Y.Z-beta.<run number>, a pre-release, on every push. Once vX.Y.Z is out, a beta is
#         one patch ahead (vX.Y.(Z+1)-beta.N) so it sorts above it, and a beta whose tree is the
#         release's own (main merged back into beta) publishes nothing.
#   main  vX.Y.Z once a `chore(release): vX.Y.Z` commit has given CHANGELOG.md its section and
#         no vX.Y.Z tag exists yet. The publish job creates the tag (cloud sessions can't push one).
#
# Needs GITHUB_REF and, for beta, GITHUB_RUN_NUMBER. BETA_BRANCH overrides the branch name.
set -euo pipefail
# shellcheck source=env.sh
. "$(dirname "${BASH_SOURCE[0]}")/env.sh"

ref="${GITHUB_REF:?}"
beta_branch="${BETA_BRANCH:-beta}"
version="$(workspace_version)"
if [[ ! "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "error: Cargo.toml has version $version; releases need X.Y.Z (the tag adds -beta.N)" >&2
  exit 1
fi
tagged() { git -C "$ROOT" ls-remote --exit-code --tags origin "refs/tags/$1" >/dev/null; }

case "$ref" in
  refs/heads/main)
    if grep -q "^## v$version " "$ROOT/CHANGELOG.md" && ! tagged "v$version"; then
      echo "v$version"
    fi
    ;;
  "refs/heads/$beta_branch")
    if tagged "v$version"; then
      git -C "$ROOT" fetch --quiet --no-tags --depth=1 origin "refs/tags/v$version:refs/tags/v$version"
      if [[ "$(git -C "$ROOT" rev-parse 'HEAD^{tree}')" == "$(git -C "$ROOT" rev-parse "v$version^{tree}")" ]]; then
        exit 0
      fi
      IFS='.' read -r major minor patch <<<"$version"
      version="$major.$minor.$((patch + 1))"
    fi
    echo "v$version-beta.${GITHUB_RUN_NUMBER:?}"
    ;;
esac
