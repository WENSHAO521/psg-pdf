#!/usr/bin/env bash
# Build, sign and (optionally) notarize the macOS release artifacts:
#
#   $DIST/psg-pdf-<version>-macos-<arch>.dmg           PSG PDF.app on a drag-to-Applications DMG
#   $DIST/psg-academic-pdf-<version>-macos-<arch>.dmg  PSG Academic PDF.app on its own DMG
#   $DIST/pdfcraft-cli-<version>-macos-<arch>.zip      the headless CLI
#
# Usage: packaging/macos/package.sh [--arch universal|aarch64|x86_64] [--skip-build]
#
# Signing (env):
#   MACOS_SIGN_IDENTITY   codesign identity (name or SHA-1). Default "-" = ad-hoc (local testing;
#                         Gatekeeper will reject the result on other Macs). In CI, import-cert.sh sets it.
#   MACOS_KEYCHAIN        keychain holding the identity (optional)
# Notarization (env; all three needed, and a real identity):
#   APPLE_ID, APPLE_PASSWORD (app-specific password), APPLE_TEAM_ID
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/macos"

ARCH=universal
SKIP_BUILD=0
while [[ $# -gt 0 ]]; do
  case "$1" in
    --arch) ARCH="$2"; shift 2 ;;
    --skip-build) SKIP_BUILD=1; shift ;;
    -h | --help) sed -n '2,16p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done
case "$ARCH" in
  universal) TARGETS=(aarch64-apple-darwin x86_64-apple-darwin) ;;
  aarch64) TARGETS=(aarch64-apple-darwin) ;;
  x86_64) TARGETS=(x86_64-apple-darwin) ;;
  *) echo "unknown --arch $ARCH" >&2; exit 2 ;;
esac

# Keep in sync with LSMinimumSystemVersion in Info.plist.in.
export MACOSX_DEPLOYMENT_TARGET=11.0
IDENTITY="${MACOS_SIGN_IDENTITY:--}"
SHORT_VERSION="${VERSION%%-*}"
WORK="$CARGO_TARGET_DIR/macos-package"
APP="$WORK/PSG PDF.app"
DMG="$DIST/psg-pdf-$VERSION-macos-$ARCH.dmg"
ACADEMIC_APP="$WORK/PSG Academic PDF.app"
ACADEMIC_DMG="$DIST/psg-academic-pdf-$VERSION-macos-$ARCH.dmg"
CLI_ZIP="$DIST/pdfcraft-cli-$VERSION-macos-$ARCH.zip"

NOTARIZE=0
if [[ "$IDENTITY" = "-" ]]; then
  warn "macOS: ad-hoc signing (no MACOS_SIGN_IDENTITY); artifacts are not notarized"
elif [[ -n "${APPLE_ID:-}" ]] && [[ -n "${APPLE_PASSWORD:-}" ]] && [[ -n "${APPLE_TEAM_ID:-}" ]]; then
  NOTARIZE=1
else
  warn "macOS: APPLE_ID / APPLE_PASSWORD / APPLE_TEAM_ID incomplete; signed but not notarized"
fi

echo "==> PSG PDF Suite $VERSION for macOS ($ARCH), identity: $IDENTITY, notarize: $NOTARIZE"

# ---- build -------------------------------------------------------------------------------------
if [[ "$SKIP_BUILD" = 0 ]]; then
  args=()
  for t in "${TARGETS[@]}"; do args+=(--target "$t"); done
  (cd "$ROOT" && cargo build --release --locked -p psg-pdf -p pdfcraft-cli "${args[@]}")
fi

rm -rf "$WORK"
mkdir -p "$WORK/bin"
for bin in psg-pdf psg-academic-pdf pdfcraft-cli; do
  inputs=()
  for t in "${TARGETS[@]}"; do inputs+=("$CARGO_TARGET_DIR/$t/release/$bin"); done
  lipo -create -output "$WORK/bin/$bin" "${inputs[@]}"
  lipo -info "$WORK/bin/$bin"
done

# ---- signing helpers ---------------------------------------------------------------------------
sign() {
  # Hardened runtime + secure timestamp for a real identity; ad-hoc can't be timestamped.
  local ts=(--timestamp)
  [[ "$IDENTITY" = "-" ]] && ts=(--timestamp=none)
  local kc=()
  [[ -n "${MACOS_KEYCHAIN:-}" ]] && kc=(--keychain "$MACOS_KEYCHAIN")
  codesign --force --sign "$IDENTITY" ${kc[@]+"${kc[@]}"} "${ts[@]}" "$@"
}

# `stapler staple` and `stapler validate` both fetch the ticket from Apple's CloudKit, and a dropped
# connection there (NSURLErrorDomain -1005, "Error 68") fails them although notarization worked.
# Both are idempotent, so retry with a pause; a missing or bad ticket still fails every try.
stapler_retry() {
  local try
  for try in 1 2 3 4; do
    xcrun stapler "$1" "$2" && return 0
    [[ "$try" = 4 ]] && break
    echo "stapler $1 failed (try $try of 4); retrying in $((try * 20)) s" >&2
    sleep $((try * 20))
  done
  echo "error: stapler $1 $(basename "$2") failed 4 times" >&2
  exit 1
}

notarize() {
  local file="$1" out id status
  echo "==> notarizing $(basename "$file") (this can take a few minutes)"
  out="$(xcrun notarytool submit "$file" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" \
    --team-id "$APPLE_TEAM_ID" --wait --timeout 1h --output-format json)" || true
  echo "$out"
  id="$(printf '%s' "$out" | plutil -extract id raw -o - - 2>/dev/null || true)"
  status="$(printf '%s' "$out" | plutil -extract status raw -o - - 2>/dev/null || true)"
  if [[ "$status" != "Accepted" ]]; then
    if [[ -n "$id" ]]; then
      xcrun notarytool log "$id" --apple-id "$APPLE_ID" --password "$APPLE_PASSWORD" --team-id "$APPLE_TEAM_ID" || true
    fi
    echo "error: notarization of $(basename "$file") failed (status: ${status:-unknown})" >&2
    exit 1
  fi
}

# ---- app bundles -------------------------------------------------------------------------------
# Usage: assemble <bundle> <binary> <display name> <icns> <Info.plist template>
# Executable and icon carry the display name (CFBundleExecutable / CFBundleIconFile).
assemble() {
  local app="$1" bin="$2" name="$3" icns="$4" plist="$5"
  echo "==> assembling $app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "$WORK/bin/$bin" "$app/Contents/MacOS/$name"
  cp "$icns" "$app/Contents/Resources/$name.icns"
  # Required embedded-font notices are independent of optional font inputs.
  cp "$ROOT/NOTICE" "$app/Contents/Resources/NOTICE"
  # The standard About panel (app menu > About) shows Credits.rtf under the version.
  cp "$HERE/Credits.rtf" "$app/Contents/Resources/Credits.rtf"
  copy_font_licences "$app/Contents/Resources"
  sed -e "s/@VERSION@/$VERSION/g" -e "s/@SHORT_VERSION@/$SHORT_VERSION/g" \
    -e "s/@BUILD_SHA@/${PDFCRAFT_BUILD_SHA:-unknown}/g" \
    "$plist" >"$app/Contents/Info.plist"
  plutil -lint "$app/Contents/Info.plist"
  printf 'APPL????' >"$app/Contents/PkgInfo"
}

# Usage: seal <bundle> <display name>: sign inside-out (nested code first, then the bundle itself,
# no --deep on the final signature), then notarize and staple when configured.
seal() {
  local app="$1" name="$2"
  sign --options runtime --entitlements "$HERE/entitlements.plist" "$app/Contents/MacOS/$name"
  sign --options runtime --entitlements "$HERE/entitlements.plist" "$app"
  codesign --verify --strict --deep --verbose=2 "$app"
  if [[ "$NOTARIZE" = 1 ]]; then
    ditto -c -k --keepParent "$app" "$WORK/notarize.zip"
    notarize "$WORK/notarize.zip"
    rm -f "$WORK/notarize.zip"
    stapler_retry staple "$app"
    stapler_retry validate "$app"
    spctl --assess --type execute -vvv "$app"
  fi
}

assemble "$APP" psg-pdf "PSG PDF" "$ROOT/assets/app-icon/psg-pdf.icns" "$HERE/Info.plist.in"
# OCR models: the app finds them at Contents/MacOS/../Resources/models. Before signing, so the
# bundle's seal covers them.
stage_models "$APP/Contents/Resources/models"
seal "$APP" "PSG PDF"

assemble "$ACADEMIC_APP" psg-academic-pdf "PSG Academic PDF" "$ROOT/assets/app-icon-academic/psg-academic-pdf.icns" "$HERE/Info-academic.plist.in"
# Journal templates beside the executable (templates/academic), where the app looks for them;
# the PSG Research Article template is also compiled in.
mkdir -p "$ACADEMIC_APP/Contents/MacOS/templates"
cp -R "$ROOT/templates/academic" "$ACADEMIC_APP/Contents/MacOS/templates/academic"
seal "$ACADEMIC_APP" "PSG Academic PDF"

# ---- DMGs --------------------------------------------------------------------------------------
# `hdiutil create -srcfolder` writes the files as they are. `makehybrid -hfs` gave every file a
# com.apple.FinderInfo (a Finder location of -1,-1), which `codesign --verify --strict` rejects as
# "Finder information, or similar detritus" on the app in the image and on a copy installed from it,
# so MDM and compliance checks reported a correctly notarized app as damaged. `create`
# attaches a device while it copies, which fails now and then on CI runners ("Resource busy"), so
# it gets a few tries. The volume name has no version: .DS_Store finds the background through an
# alias that includes it.
# Usage: hdiutil_retry <what> <hdiutil args…>; tries five times with growing pauses.
hdiutil_retry() {
  local what="$1" attempt
  shift
  for attempt in 1 2 3 4 5; do
    if hdiutil "$@"; then
      return 0
    fi
    [[ "$attempt" = 5 ]] && { echo "hdiutil $what failed 5 times" >&2; return 1; }
    warn "hdiutil $what failed (attempt $attempt of 5); retrying in $((attempt * 10))s"
    sleep $((attempt * 10))
  done
}

# Usage: make_dmg <bundle> <volume name> <dmg> <background.tiff> <layout .DS_Store>
# Window layout: background, icon size and positions (packaging/macos/dmg/README.md).
make_dmg() {
  local app="$1" volume="$2" dmg="$3" background="$4" layout="$5" stage mount
  echo "==> building $dmg"
  stage="$WORK/dmg-stage"
  rm -rf "$stage"
  mkdir -p "$stage/.background"
  ditto "$app" "$stage/$(basename "$app")"
  ln -s /Applications "$stage/Applications"
  cp "$background" "$stage/.background/background.tiff"
  cp "$layout" "$stage/.DS_Store"
  rm -f "$dmg"
  hdiutil_retry create create -srcfolder "$stage" -volname "$volume" -fs HFS+ -format UDZO -imagekey zlib-level=9 -ov "$dmg"
  sign "$dmg"
  codesign --verify --strict --verbose=2 "$dmg"
  # The app inside the image must still pass --strict, so a makehybrid-style regression can't ship.
  mount="$WORK/dmg-check"
  rm -rf "$mount"
  mkdir -p "$mount"
  hdiutil_retry attach attach -nobrowse -readonly -mountpoint "$mount" "$dmg" >/dev/null
  if ! codesign --verify --strict --deep --verbose=2 "$mount/$(basename "$app")"; then
    hdiutil_retry detach detach "$mount" >/dev/null || true
    exit 1
  fi
  hdiutil_retry detach detach "$mount" >/dev/null
  if [[ "$NOTARIZE" = 1 ]]; then
    notarize "$dmg"
    stapler_retry staple "$dmg"
    stapler_retry validate "$dmg"
    spctl --assess --type open --context context:primary-signature -vvv "$dmg"
  fi
}

make_dmg "$APP" "PSG PDF" "$DMG" "$HERE/dmg/background.tiff" "$HERE/dmg/dmg-layout.DS_Store"
make_dmg "$ACADEMIC_APP" "PSG Academic PDF" "$ACADEMIC_DMG" "$HERE/dmg/academic-background.tiff" "$HERE/dmg/academic-layout.DS_Store"

# ---- CLI ---------------------------------------------------------------------------------------
echo "==> building $CLI_ZIP"
CLI_DIR="$WORK/pdfcraft-cli-$VERSION-macos-$ARCH"
mkdir -p "$CLI_DIR"
cp "$WORK/bin/pdfcraft-cli" "$CLI_DIR/"
copy_docs "$CLI_DIR"
sign --options runtime "$CLI_DIR/pdfcraft-cli"
codesign --verify --strict --verbose=2 "$CLI_DIR/pdfcraft-cli"
rm -f "$CLI_ZIP"
ditto -c -k --keepParent "$CLI_DIR" "$CLI_ZIP"
# A bare Mach-O can't carry a stapled ticket; Gatekeeper looks the notarization up online.
if [[ "$NOTARIZE" = 1 ]]; then notarize "$CLI_ZIP"; fi

"$WORK/bin/pdfcraft-cli" --version
echo "==> done"
ls -lh "$DMG" "$ACADEMIC_DMG" "$CLI_ZIP"
