#!/usr/bin/env bash
# Installs PSG PDF from its DMG as a user would (drag to an Applications folder), checks its
# signature and identity, and runs it:
#
#   - the DMG holds exactly one app and the Applications link;
#   - the app has its own name, bundle id and executable;
#   - `codesign --verify` passes (ad-hoc or Developer ID); with SIGNED=1, Gatekeeper (spctl)
#     accepts it too, which needs a Developer ID and notarization.
#
# The 0.18.x "PSG PDF Suite" DMG held PSG PDF.app under the same name and bundle id, so the new
# DMG replaces it in place: there is nothing to migrate on macOS (docs/products.md).
#
# Usage: packaging/macos/test-install.sh <folder with the DMG>
set -euo pipefail
DIR="${1:?folder with the DMGs}"
APPS="$(mktemp -d)/Applications"
mkdir -p "$APPS"
fail() { echo "::error::$*" >&2; exit 1; }
step() { echo; echo "==> $*"; }

install_dmg() { # <stem> <app name> <bundle id> <executable>
  local dmg mount apps
  dmg="$(compgen -G "$DIR/$1-[0-9]*-macos-*.dmg" | head -n1)" || fail "no $1 DMG in $DIR"
  step "$2 from $(basename "$dmg")"
  mount="$(mktemp -d)"
  hdiutil attach -nobrowse -readonly -noautoopen -mountpoint "$mount" "$dmg" >/dev/null
  apps="$(find "$mount" -maxdepth 1 -name '*.app' | wc -l | tr -d ' ')"
  [[ "$apps" = 1 ]] || { hdiutil detach "$mount" >/dev/null; fail "$dmg holds $apps apps, expected only $2.app"; }
  [[ -d "$mount/$2.app" ]] || { hdiutil detach "$mount" >/dev/null; fail "$dmg has no $2.app"; }
  [[ -L "$mount/Applications" ]] || { hdiutil detach "$mount" >/dev/null; fail "$dmg has no Applications link"; }
  ditto "$mount/$2.app" "$APPS/$2.app"
  hdiutil detach "$mount" >/dev/null
  check_app "$@"
}

check_app() { # <stem> <app name> <bundle id> <executable>
  local app="$APPS/$2.app" id exe
  [[ -d "$app" ]] || fail "$app is missing"
  id="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$app/Contents/Info.plist")"
  [[ "$id" = "$3" ]] || fail "$2's bundle id is $id, expected $3"
  # package.sh names the executable after the app (CFBundleExecutable), not after the cargo binary.
  exe="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")"
  [[ "$exe" = "$4" ]] || fail "$2's CFBundleExecutable is $exe, expected $4"
  [[ -x "$app/Contents/MacOS/$4" ]] || fail "$2 has no executable $4"
  codesign --verify --deep --strict --verbose=2 "$app"
  if [[ "${SIGNED:-0}" = 1 ]]; then
    spctl --assess --type execute --verbose=2 "$app"
  fi
  "$app/Contents/MacOS/$4" --version
}

install_dmg psg-pdf "PSG PDF" com.panoramascholarly.psgpdf "PSG PDF"
[[ ! -e "$APPS/PSG PDF.app/Contents/MacOS/PSG Academic PDF" ]] || fail "PSG PDF.app contains PSG Academic PDF"

rm -rf "$APPS"
echo "install tests passed"
