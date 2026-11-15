#!/usr/bin/env bash
# Check the signatures of what packaging/ci/build-products.sh made (.github/workflows/build.yaml):
# Authenticode on Windows (every MSI and every program in the staged install trees); codesign on
# macOS, plus Gatekeeper (spctl) when a Developer ID signed and notarized it. Without signing
# secrets the files are unsigned (Windows) or ad-hoc signed (macOS): that is reported in the job
# summary and passes, so pull requests and forks still build; with them, a bad signature fails.
#
#   packaging/ci/verify-signatures.sh <platform> <signed: 0|1> [product…]
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
platform="${1:?usage: verify-signatures.sh <platform> <signed> [product…]}"
signed="${2:?}"
shift 2
products=("$@")
if [[ ${#products[@]} -eq 0 ]]; then read -r -a products <<<"$(cargo xtask products ids 2>/dev/null)"; fi
UPLOAD="${UPLOAD:-$ROOT/dist/upload}"
summary="${GITHUB_STEP_SUMMARY:-/dev/stderr}"

for product in "${products[@]}"; do
  name="$(cargo xtask products get "$product" name 2>/dev/null)"
  case "$platform" in
    windows-x64)
      # shellcheck disable=SC2016 # PowerShell, not shell, expands these
      PRODUCT="$product" DIR="$UPLOAD/$product" SIGNED="$signed" pwsh -NoProfile -NonInteractive -Command '
        $ErrorActionPreference = "Stop"
        $files = @(Get-ChildItem -LiteralPath $env:DIR -Filter "*.msi") +
          @(Get-ChildItem -Recurse -LiteralPath "target/windows-package/$env:PRODUCT" -Filter "*.exe" -ErrorAction SilentlyContinue)
        if ($files.Count -eq 0) { throw "no MSI or program of $env:PRODUCT to check" }
        foreach ($f in $files) {
          $sig = Get-AuthenticodeSignature -LiteralPath $f.FullName
          Write-Host "$($f.Name): $($sig.Status) $($sig.SignerCertificate.Subject)"
          if ($env:SIGNED -eq "1" -and $sig.Status -ne "Valid") { throw "$($f.Name) is not validly signed ($($sig.Status))" }
        }'
      [[ "$signed" == 1 ]] || echo "Windows: $name is not Authenticode-signed (no signing certificate)." >>"$summary"
      ;;
    macos-universal)
      app="${CARGO_TARGET_DIR:-target}/macos-package/$product/$name.app"
      codesign --verify --deep --strict --verbose=2 "$app"
      for dmg in "$UPLOAD/$product"/*.dmg; do codesign --verify --verbose=2 "$dmg" || [[ "$signed" == 0 ]]; done
      if [[ "$signed" == 1 ]]; then
        spctl --assess --type execute --verbose=2 "$app"
        for dmg in "$UPLOAD/$product"/*.dmg; do spctl --assess --type open --context context:primary-signature --verbose=2 "$dmg"; done
      else
        echo "macOS: $name is ad-hoc signed and not notarized (no Developer ID)." >>"$summary"
      fi
      ;;
    *) ;; # Linux packages carry no signature (the checksums are published).
  esac
done
