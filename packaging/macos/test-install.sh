#!/usr/bin/env bash
# Installs both PSG products from their DMGs as a user would (drag to an Applications folder),
# checks their signatures and identities, runs them, and removes them one at a time:
#
#   - each DMG holds exactly one app and the Applications link;
#   - the two apps have their own names, bundle ids and executables, so they coexist;
#   - `codesign --verify` passes (ad-hoc or Developer ID); with SIGNED=1, Gatekeeper (spctl)
#     accepts them too, which needs a Developer ID and notarization;
#   - removing either app leaves the other working.
#
# The 0.18.x "PSG PDF Suite" DMG held both apps under the same names and bundle ids, so the new
# DMGs replace them in place: there is nothing to migrate on macOS (docs/products.md).
#
# Usage: packaging/macos/test-install.sh <folder with both products' DMGs>
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
install_dmg psg-academic-pdf "PSG Academic PDF" com.panoramascholarly.psgacademicpdf "PSG Academic PDF"
"$APPS/PSG Academic PDF.app/Contents/MacOS/PSG Academic PDF" templates | grep -q '^psg-article ' || fail "PSG Academic PDF lists no templates"
[[ ! -e "$APPS/PSG PDF.app/Contents/MacOS/PSG Academic PDF" ]] || fail "PSG PDF.app contains PSG Academic PDF"
[[ ! -e "$APPS/PSG Academic PDF.app/Contents/MacOS/PSG PDF" ]] || fail "PSG Academic PDF.app contains PSG PDF"

step "Removing PSG Academic PDF leaves PSG PDF"
rm -rf "$APPS/PSG Academic PDF.app"
check_app psg-pdf "PSG PDF" com.panoramascholarly.psgpdf "PSG PDF"

step "Removing PSG PDF leaves PSG Academic PDF"
install_dmg psg-academic-pdf "PSG Academic PDF" com.panoramascholarly.psgacademicpdf "PSG Academic PDF"
rm -rf "$APPS/PSG PDF.app"
check_app psg-academic-pdf "PSG Academic PDF" com.panoramascholarly.psgacademicpdf "PSG Academic PDF"

rm -rf "$APPS"
echo "install tests passed"
