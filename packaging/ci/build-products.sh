#!/usr/bin/env bash
# Build, package and check every PSG product on this runner's platform, one product after the
# other on one Cargo target directory (.github/workflows/build.yaml, job "Build"). The products
# share the PdfCraft engine, so the second build reuses what the first compiled; each product is
# still built with its own `cargo build -p …` (its own feature set) by its own packaging script.
#
#   packaging/ci/build-products.sh <platform> [product…]
#
#   platform  linux-x86_64 | macos-universal | windows-x64 (packaging/products.toml, PLATFORMS)
#   product   default: every product (`cargo xtask products ids`)
#
# Per product it runs the platform's packaging script, starts each of the product's programs
# (`--version`), and checks that every file it made is named after it (its update channel). Then
# it sorts the files into upload/<product>/ with a SHA-256 list, SHA256SUMS-<product>-<platform>.txt,
# so the workflow uploads one artifact per product and no file can end up in the other's.
#
# Environment: DIST (default dist/release), and the signing variables of the packaging scripts.
# Signatures are checked by packaging/ci/verify-signatures.sh afterwards.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

platform="${1:?usage: build-products.sh <platform> [product…]}"
shift
products=("$@")
xtask() { cargo xtask "$@" 2>/dev/null; }
if [[ ${#products[@]} -eq 0 ]]; then read -r -a products <<<"$(xtask products ids)"; fi
export DIST="${DIST:-$ROOT/dist/release}"
UPLOAD="${UPLOAD:-$ROOT/dist/upload}"
mkdir -p "$DIST" "$UPLOAD"

ext=""
case "$platform" in
  linux-x86_64 | macos-universal) ;;
  windows-x64) ext=".exe" ;;
  *) echo "error: unknown platform $platform" >&2; exit 2 ;;
esac

find_bin() { # the built program, wherever the packaging script's --target put it
  compgen -G "${CARGO_TARGET_DIR:-target}/release/$1$ext" || compgen -G "${CARGO_TARGET_DIR:-target}/*/release/$1$ext" | head -n1
}

# Which product a release file belongs to: its stem, or pdfcraft-cli, which ships with PSG PDF.
owner() {
  local name="$1" p stem
  for p in "${all_products[@]}"; do
    stem="$(xtask products get "$p" asset_stem)"
    [[ "$name" == "$stem"-[0-9]* ]] && { echo "$p"; return; }
  done
  [[ "$name" == pdfcraft-cli-[0-9]* ]] && { echo psg-pdf; return; }
  echo ""
}
read -r -a all_products <<<"$(xtask products ids)"
sha256() { if command -v sha256sum >/dev/null; then sha256sum -- "$@"; else shasum -a 256 -- "$@"; fi; }

for product in "${products[@]}"; do
  echo "::group::$product ($platform): package"
  case "$platform" in
    linux-x86_64) packaging/linux/package.sh --product "$product" --formats "appimage deb rpm tar cli" ;;
    macos-universal) packaging/macos/package.sh --product "$product" --arch universal ;;
    windows-x64) pwsh -NoProfile -NonInteractive -File packaging/windows/package.ps1 -Product "$product" -Arch x64 ;;
  esac
  echo "::endgroup::"

  echo "::group::$product ($platform): smoke test"
  for bin in $(xtask products get "$product" binaries); do
    path="$(find_bin "$bin")" || { echo "::error::$bin was not built"; exit 1; }
    "$path" --version
  done
  if [[ "$product" == psg-academic-pdf ]]; then "$(find_bin psg-academic-pdf)" templates; fi
  echo "::endgroup::"
done

# Every file belongs to exactly one product, and goes into that product's upload folder only.
shopt -s nullglob
for f in "$DIST"/*; do
  name="$(basename "$f")"
  p="$(owner "$name")"
  if [[ -z "$p" ]]; then echo "::error::$name is not named after any product (<asset_stem>-<version>-…)"; exit 1; fi
  case " ${products[*]} " in *" $p "*) ;; *) echo "::error::$name belongs to $p, which this run did not build"; exit 1 ;; esac
  mkdir -p "$UPLOAD/$p"
  mv "$f" "$UPLOAD/$p/"
done
for product in "${products[@]}"; do
  files=("$UPLOAD/$product"/*)
  [[ ${#files[@]} -gt 0 ]] || { echo "::error::$product made no files on $platform"; exit 1; }
  # shellcheck disable=SC2035 # sha256 passes `--` itself; ./ prefixes would end up in the list
  (cd "$UPLOAD/$product" && sha256 * | tee "SHA256SUMS-$product-$platform.txt")
done
