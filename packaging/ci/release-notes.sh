#!/usr/bin/env bash
# The notes of release <tag> (.github/workflows/build.yaml, job "Release"), in Markdown on stdout:
# Each product has its own section with its downloads, followed by the changes and download checks.
#
#   packaging/ci/release-notes.sh <tag> <folder with the release files>
#   packaging/ci/release-notes.sh --title <tag> <folder>   the release's title, one line
#
# Titles read "PSG PDF & PSG Academic PDF 0.19.0" for a stable release and
# "PSG PDF & PSG Academic PDF 0.19.0 Beta 3" for a beta. release-files.sh has already validated
# the downloads for all three platforms in both channels.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
title_only=false
if [[ "${1:-}" == --title ]]; then title_only=true; shift; fi
TAG="${1:?usage: release-notes.sh [--title] <tag> <folder>}"
FILES="${2:?}"
[[ -d "$FILES" ]] || { echo "release folder is missing: $FILES" >&2; exit 1; }
REPO="${GITHUB_REPOSITORY:-WENSHAO521/psg-pdf}"

version="${TAG#v}"
if [[ "$version" =~ ^([0-9]+\.[0-9]+\.[0-9]+)-beta\.([0-9]+)$ ]]; then
  shown="${BASH_REMATCH[1]} Beta ${BASH_REMATCH[2]}"
  beta=true
else
  shown="$version"
  beta=false
fi

read -r -a products <<<"$(cargo xtask products ids 2>/dev/null)"
declare -A stem name tagline
for product in "${products[@]}"; do
  stem[$product]="$(cargo xtask products get "$product" asset_stem 2>/dev/null)"
  name[$product]="$(cargo xtask products get "$product" name 2>/dev/null)"
  tagline[$product]="$(cargo xtask products get "$product" tagline 2>/dev/null)"
done

title="PSG PDF & PSG Academic PDF $shown"
if $title_only; then
  echo "$title"
  exit 0
fi

icon() { # <product>: the product's 64 px icon at this tag
  local dir
  dir="$(cargo xtask products get "$1" icon_dir 2>/dev/null)"
  local app_id
  app_id="$(cargo xtask products get "$1" app_id 2>/dev/null)"
  echo "https://raw.githubusercontent.com/$REPO/$TAG/$dir/hicolor/64x64/apps/$app_id.png"
}

echo "# $title"
echo
if $beta; then
  echo "> [!NOTE]"
  echo "> **Beta.** A test build of the \`beta\` branch, not a stable release. Both apps show"
  echo "> **Beta** beside their name. Use it to try what is coming; keep the stable release for"
  echo "> work that matters."
  echo
fi
echo "Two independent desktop applications from Panorama Scholarly Group. Install either or both:"
echo "each has its own installer, updates and uninstaller, and neither needs the other."
echo
for product in "${products[@]}"; do
  s="${stem[$product]}"
  echo "## <img src=\"$(icon "$product")\" width=\"28\" height=\"28\" alt=\"\"> ${name[$product]}"
  echo
  echo "_${tagline[$product]}_"
  echo
  echo "| Platform | Download |"
  echo "|---|---|"
  echo "| Windows 10 and 11 (x64) | \`$s-$version-windows-x64.msi\` (installer) · \`$s-$version-windows-x64-portable.zip\` (portable) |"
  echo "| macOS 11 or later (Apple silicon and Intel) | \`$s-$version-macos-universal.dmg\` |"
  echo "| Linux (x86_64) | \`.deb\` (Debian, Ubuntu) · \`.rpm\` (Fedora, openSUSE) · \`.AppImage\` · \`.tar.gz\` |"
  echo
  echo "SHA-256 checksums: \`SHA256SUMS-$s.txt\`"
  echo
done
echo "PSG Academic PDF typesets with the TeX installation already on your computer (pdfLaTeX,"
echo "latexmk and Biber); see the [PSG Academic PDF guide](https://github.com/$REPO/blob/$TAG/docs/academic-guide.md)."
echo
if $beta; then
  echo "## Changes since the last release"
  echo
  cargo xtask changelog notes --unreleased 2>/dev/null
else
  echo "## Changes"
  echo
  cargo xtask changelog notes --version "$version" 2>/dev/null
fi
echo
echo "## Checking your download"
echo
echo "\`SHA256SUMS.txt\` lists the SHA-256 of every file in this release, and each product has its own"
echo "list. Whether this build was code-signed is shown in the release workflow's summary (docs/releasing.md)."
echo
echo "## Coming from the PSG PDF Suite (0.18)"
echo
echo "The new PSG PDF replaces the suite and keeps your settings. If you used PSG Academic PDF,"
echo "install it as well: your projects stay where they were. On Windows, install PSG PDF first;"
echo "PSG Academic PDF's installer asks for that while the suite is still installed."
