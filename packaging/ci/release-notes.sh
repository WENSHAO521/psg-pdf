#!/usr/bin/env bash
# The notes of release <tag> (.github/workflows/build.yaml, job "Release"), in Markdown on stdout:
# the changes from CHANGELOG.md, then each product's downloads, listed separately, with a note for
# any platform this build did not produce (a beta may publish without macOS).
#
#   packaging/ci/release-notes.sh <tag> <folder with the release files>
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
TAG="${1:?usage: release-notes.sh <tag> <folder>}"
FILES="${2:?}"

echo "PSG ${TAG}: PSG PDF and PSG Academic PDF, two independent products. Install either"
echo "or both; each has its own installers, updates and uninstaller."
echo
if [[ "$TAG" == *-beta.* ]]; then
  echo "This is a beta: a test build of the \`beta\` branch, not a stable release."
  echo
  echo "## Changes since the last release"
  echo
  cargo xtask changelog notes --unreleased 2>/dev/null
else
  echo "## Changes"
  echo
  cargo xtask changelog notes --version "${TAG#v}" 2>/dev/null
fi
echo
echo "## Downloads"
echo
platform() { # <glob> <line>: the line when a file matches, else a note that it is missing
  if compgen -G "$FILES/$1" >/dev/null; then echo "- $2"; else echo "- ${2%%:*}: not in this build"; fi
}
for product in $(cargo xtask products ids 2>/dev/null); do
  stem="$(cargo xtask products get "$product" asset_stem 2>/dev/null)"
  echo "### $(cargo xtask products get "$product" name 2>/dev/null): $(cargo xtask products get "$product" tagline 2>/dev/null)"
  echo
  platform "$stem-[0-9]*-macos-universal.dmg" "macOS 11+ (Apple silicon and Intel): \`$stem-*-macos-universal.dmg\`"
  platform "$stem-[0-9]*-windows-x64.msi" "Windows 10+ x64: \`$stem-*-windows-x64.msi\` (installer) or \`$stem-*-windows-x64-portable.zip\`"
  platform "$stem-[0-9]*-linux-x86_64.deb" "Linux x86_64: \`$stem-*.deb\`, \`.rpm\`, \`.AppImage\` or \`.tar.gz\`"
  echo "- Checksums: \`SHA256SUMS-$stem.txt\`"
  echo
done
echo "PSG Academic PDF needs a TeX installation (pdflatex, latexmk, biber); see the README."
echo "\`SHA256SUMS.txt\` lists every file's SHA-256."
echo
echo "### Coming from the PSG PDF Suite (0.18)"
echo
echo "The new PSG PDF replaces the suite and keeps your settings. If you used PSG Academic PDF,"
echo "install it as well: your projects stay where they were. On Windows, install PSG PDF first;"
echo "PSG Academic PDF's installer asks for that while the suite is still installed."
