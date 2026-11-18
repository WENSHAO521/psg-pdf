#!/usr/bin/env bash
# Exercise the release inventory with synthetic packages. No signing key is needed.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."

temp="$(mktemp -d)"
trap 'rm -rf "$temp"' EXIT
unset UPDATE_SIGNING_KEY

make_inventory() {
  local tag="$1" root="$2" product platform suffix folder hashes
  local -a suffixes
  for product in psg-pdf psg-academic-pdf; do
    for platform in linux-x86_64 windows-x64 macos-universal; do
      case "$platform" in
        linux-x86_64) suffixes=(.deb .rpm .AppImage .tar.gz) ;;
        windows-x64) suffixes=(.msi -portable.zip) ;;
        macos-universal) suffixes=(.dmg) ;;
      esac
      folder="$root/$product-$platform"
      mkdir -p "$folder"
      for suffix in "${suffixes[@]}"; do
        printf 'installer fixture\n' >"$folder/$product-${tag#v}-$platform$suffix"
      done
      hashes="$(cd "$folder" && sha256sum -- ./* | sed 's|  \./|  |')"
      printf '%s\n' "$hashes" >"$folder/SHA256SUMS-$product-$platform.txt"
    done
  done
}

check_inventory() {
  local tag="$1" root="$2" expected="$3" output
  if output="$(packaging/ci/release-files.sh "$tag" "$root" 2>&1)"; then
    printf '%s\n' "release assembly unexpectedly succeeded without a signing key" >&2
    return 1
  fi
  if [[ "$output" != *"$expected"* ]]; then
    printf 'Expected %s in the release output.\n%s\n' "$expected" "$output" >&2
    return 1
  fi
}

for tag in v99.0.0-beta.1 v99.0.0; do
  root="$temp/$tag"
  make_inventory "$tag" "$root/missing-platform"
  rm -r -- "$root/missing-platform/psg-academic-pdf-macos-universal"
  check_inventory "$tag" "$root/missing-platform" "psg-academic-pdf for macos-universal is missing"
  [[ ! -e "$root/missing-platform/SHA256SUMS.txt" ]]

  make_inventory "$tag" "$root/missing-checksum"
  (
    cd "$root/missing-checksum/psg-pdf-macos-universal"
    printf 'fixture for a different asset\n' >other-file
    sha256sum other-file >SHA256SUMS-psg-pdf-macos-universal.txt
  )
  check_inventory "$tag" "$root/missing-checksum" "psg-pdf-${tag#v}-macos-universal.dmg is missing or absent"
  [[ ! -e "$root/missing-checksum/SHA256SUMS.txt" ]]

  make_inventory "$tag" "$root/complete"
  check_inventory "$tag" "$root/complete" 'UPDATE_SIGNING_KEY secret is missing'
  [[ "$(wc -l <"$root/complete/SHA256SUMS.txt")" -eq 14 ]]
done

echo 'Both channels require checksummed installers for both products on all three platforms.'
