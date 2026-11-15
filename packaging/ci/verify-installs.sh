#!/usr/bin/env bash
# Install, run, upgrade from the 0.18.x suite, and uninstall both PSG products' packages on the
# runner that just built them (.github/workflows/build.yaml, job "Build"). Running here instead
# of in separate jobs means each platform's checks start as soon as its own build is done,
# without waiting for the slowest platform or downloading the artifacts again.
#
#   packaging/ci/verify-installs.sh <platform>
#
#   linux-x86_64     deb on this Ubuntu runner (packaging/linux/test-install.sh), and rpm in a
#                    Fedora container (docker), each also migrating the suite's package
#   windows-x64      every MSI alone, side by side, uninstalled one at a time, and the migration
#                    from the suite MSI (packaging/windows/test-install.ps1)
#   macos-universal  each DMG alone and side by side, removed one at a time
#                    (packaging/macos/test-install.sh); SIGNED=1 also runs Gatekeeper
#
# Needs: the files in dist/upload/<product>/ from packaging/ci/build-products.sh, and GH_TOKEN for
# the suite release (LEGACY_SUITE_TAG). Without that release the migration step is skipped with a
# warning, as before. Changes the machine's installed software: CI runners only.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
platform="${1:?usage: verify-installs.sh <platform>}"
UPLOAD="${UPLOAD:-$ROOT/dist/upload}"
SUITE_TAG="${LEGACY_SUITE_TAG:-v0.18.1-beta.7}"
FEDORA_IMAGE="${FEDORA_IMAGE:-fedora:41}"

# Both products' files in one folder, as the install tests expect (hard links: no copies).
packages="$ROOT/dist/verify"
rm -rf "$packages" "$ROOT/dist/suite"
mkdir -p "$packages" "$ROOT/dist/suite"
cp -l "$UPLOAD"/*/* "$packages/" 2>/dev/null || cp "$UPLOAD"/*/* "$packages/"

suite() { # <pattern>: the suite's package, or nothing (with a warning)
  if gh release download "$SUITE_TAG" --repo "${GITHUB_REPOSITORY:-WENSHAO521/psg-pdf}" --dir "$ROOT/dist/suite" --pattern "$1" >&2; then
    compgen -G "$ROOT/dist/suite/$1" | head -n1
  else
    echo "::warning::$SUITE_TAG has no $1; the migration test is skipped" >&2
  fi
}

case "$platform" in
  linux-x86_64)
    echo "::group::deb (Ubuntu)"
    packaging/linux/test-install.sh deb "$packages" "$(suite 'psg-pdf-*-linux-x86_64.deb')"
    echo "::endgroup::"
    echo "::group::rpm ($FEDORA_IMAGE)"
    rpm_suite="$(suite 'psg-pdf-*-linux-x86_64.rpm')"
    docker run --rm -v "$ROOT:/src" -w /src "$FEDORA_IMAGE" bash -euo pipefail -c '
      dnf install -y -q --setopt=install_weak_deps=False findutils >/dev/null
      packaging/linux/test-install.sh rpm dist/verify ${1:+"/src/${1#"$2"/}"}' _ "$rpm_suite" "$ROOT"
    echo "::endgroup::"
    ;;
  windows-x64)
    msi="$(suite 'psg-pdf-*-windows-x64.msi')"
    pwsh -NoProfile -NonInteractive -File packaging/windows/test-install.ps1 -Dir "$packages" -SuiteMsi "$msi"
    ;;
  macos-universal)
    packaging/macos/test-install.sh "$packages"
    ;;
  *) echo "error: unknown platform $platform" >&2; exit 2 ;;
esac
echo "install tests passed on $platform"
