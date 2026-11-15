#!/usr/bin/env bash
# Gather the files of a release (.github/workflows/build.yaml, job "Release"): check every file
# against the SHA-256 written on the runner that built it, make sure the platforms this release
# needs are there for both products, then write one checksum list per product and SHA256SUMS.txt
# over everything (the in-app updaters read that one).
#
#   packaging/ci/release-files.sh <tag> <folder>
#
# The folder holds the build artifacts, one sub-folder per <product>-<platform>. Each was uploaded
# only after that platform's install tests passed. A stable release needs every platform of every
# product; a beta needs Linux and Windows and publishes without macOS when its runner didn't
# finish (the notes say so). The files end up directly in <folder>.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
TAG="${1:?usage: release-files.sh <tag> <folder>}"
DIR="${2:?}"
read -r -a products <<<"$(cargo xtask products ids 2>/dev/null)"
declare -A stem
for product in "${products[@]}"; do stem[$product]="$(cargo xtask products get "$product" asset_stem 2>/dev/null)"; done
platforms=(linux-x86_64 windows-x64 macos-universal)
[[ "$TAG" == *-beta.* ]] && platforms=(linux-x86_64 windows-x64)
for product in "${products[@]}"; do
  for platform in "${platforms[@]}"; do
    [[ -f "$DIR/$product-$platform/SHA256SUMS-$product-$platform.txt" ]] ||
      { echo "::error::$product for $platform is missing (it did not build, or its install tests failed)"; exit 1; }
  done
done
shopt -s nullglob
for sub in "$DIR"/*/; do
  (cd "$sub" && for list in SHA256SUMS-*.txt; do sha256sum -c "$list"; done && rm -f SHA256SUMS-*.txt)
  mv "$sub"* "$DIR/"
  rmdir "$sub"
done
cd "$DIR"
for product in "${products[@]}"; do
  files=("${stem[$product]}"-[0-9]*)
  if [[ "$product" == psg-pdf ]]; then files+=(pdfcraft-cli-[0-9]*); fi
  sha256sum -- "${files[@]}" >"SHA256SUMS-${stem[$product]}.txt"
done
shopt -s extglob
sha256sum -- !(SHA256SUMS*) | tee SHA256SUMS.txt

# Sign SHA256SUMS.txt for the in-app updaters (docs/update-signing.md). The apps embed the public
# key in packaging/update-signing-key.txt; once it is there, a release without a valid signature
# couldn't update anyone, so it isn't published.
cd - >/dev/null
KEY_FILE=packaging/update-signing-key.txt
pub="$(tr -d '[:space:]' <"$KEY_FILE" 2>/dev/null || true)"
if [[ -z "$pub" ]]; then
  echo "::warning::$KEY_FILE is empty: the release is not signed for the in-app updaters"
  exit 0
fi
[[ "$pub" =~ ^[0-9a-fA-F]{64}$ ]] || { echo "::error::$KEY_FILE must hold 64 hex digits"; exit 1; }
[[ -n "${UPDATE_SIGNING_KEY:-}" ]] || { echo "::error::the UPDATE_SIGNING_KEY secret is missing, so the apps couldn't update to this release"; exit 1; }
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
umask 077
printf '%s\n' "$UPDATE_SIGNING_KEY" >"$work/key.pem"
openssl pkeyutl -sign -rawin -inkey "$work/key.pem" -in "$DIR/SHA256SUMS.txt" -out "$DIR/SHA256SUMS.txt.sig"
# Check it with the key the apps embed (SubjectPublicKeyInfo prefix for Ed25519, then the key).
# shellcheck disable=SC2059 # the format is the key as \x escapes, made just above
printf "$(printf '302a300506032b6570032100%s' "$pub" | sed 's/../\\x&/g')" >"$work/pub.der"
openssl pkeyutl -verify -rawin -pubin -keyform DER -inkey "$work/pub.der" -in "$DIR/SHA256SUMS.txt" -sigfile "$DIR/SHA256SUMS.txt.sig" >/dev/null ||
  { echo "::error::UPDATE_SIGNING_KEY doesn't match $KEY_FILE: the apps would refuse this release"; exit 1; }
echo "Signed SHA256SUMS.txt for the in-app updaters"
