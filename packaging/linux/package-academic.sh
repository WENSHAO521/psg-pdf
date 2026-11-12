#!/usr/bin/env bash
# Build and package PSG Academic PDF for Linux, independently of PSG PDF:
#
#   $DIST/psg-academic-pdf-<version>-linux-<arch>.tar.gz   plain FHS-style tree (bin/, share/)
#   $DIST/psg-academic-pdf-<version>-linux-<arch>.deb      with nfpm on PATH
#   $DIST/psg-academic-pdf-<version>-linux-<arch>.rpm      with nfpm on PATH
#
# The package holds the app, its desktop entry, AppStream metadata, icons and the journal
# templates (share/psg-academic-pdf/templates, where the app looks beside ../share). It does not
# bundle TeX: the deb/rpm recommend TeX Live, latexmk and Biber, and suggest Pandoc.
#
# Usage: packaging/linux/package-academic.sh [--skip-build] [--formats "tar deb rpm"]
set -euo pipefail
# shellcheck source=../env.sh
. "$(dirname "${BASH_SOURCE[0]}")/../env.sh"
HERE="$ROOT/packaging/linux"
APP_ID=com.panoramascholarly.psgacademicpdf

SKIP_BUILD=0
FORMATS="tar deb rpm"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) SKIP_BUILD=1; shift ;;
    --formats) FORMATS="$2"; shift 2 ;;
    -h | --help) sed -n '2,13p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
done

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64) DEB_ARCH=amd64 ;;
  aarch64 | arm64) ARCH=aarch64; DEB_ARCH=arm64 ;;
  *) echo "unsupported architecture $ARCH" >&2; exit 2 ;;
esac
export PDFCRAFT_MAINTAINER="${PDFCRAFT_MAINTAINER:-PSG PDF maintainers <84624193+WENSHAO521@users.noreply.github.com>}"
LABEL_SUFFIX="${PDFCRAFT_BUILD_LABEL:+-$PDFCRAFT_BUILD_LABEL}"
BASENAME="psg-academic-pdf-$VERSION$LABEL_SUFFIX-linux-$ARCH"
echo "==> PSG Academic PDF $VERSION$LABEL_SUFFIX for Linux $ARCH ($FORMATS)"

if [[ "$SKIP_BUILD" = 0 ]]; then
  (cd "$ROOT" && cargo build --release --locked -p psg-academic-pdf)
fi
BIN="$CARGO_TARGET_DIR/release"
WORK="$CARGO_TARGET_DIR/linux-package-academic"
STAGE="$WORK/root"
rm -rf "$WORK"

install -Dm755 "$BIN/psg-academic-pdf" "$STAGE/usr/bin/psg-academic-pdf"
strip "$STAGE/usr/bin/psg-academic-pdf" 2>/dev/null || true
install -Dm644 "$HERE/$APP_ID.desktop" "$STAGE/usr/share/applications/$APP_ID.desktop"
mkdir -p "$STAGE/usr/share/metainfo"
sed -e "s/@VERSION@/$VERSION/g" -e "s/@DATE@/$PDFCRAFT_BUILD_DATE/g" \
  "$HERE/$APP_ID.metainfo.xml.in" >"$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
mkdir -p "$STAGE/usr/share/icons"
cp -R "$ROOT/assets/app-icon-academic/hicolor" "$STAGE/usr/share/icons/"
mkdir -p "$STAGE/usr/share/psg-academic-pdf"
cp -R "$ROOT/templates/academic" "$STAGE/usr/share/psg-academic-pdf/templates"
mkdir -p "$STAGE/usr/share/doc/psg-academic-pdf"
copy_docs "$STAGE/usr/share/doc/psg-academic-pdf"
cp "$ROOT/docs/academic.md" "$STAGE/usr/share/doc/psg-academic-pdf/"

if command -v desktop-file-validate >/dev/null; then
  desktop-file-validate "$STAGE/usr/share/applications/$APP_ID.desktop"
fi
if command -v appstreamcli >/dev/null; then
  appstreamcli validate --no-net --explain "$STAGE/usr/share/metainfo/$APP_ID.metainfo.xml"
fi

has() {
  case " $FORMATS " in *" $1 "*) return 0 ;; *) return 1 ;; esac
}

if has tar; then
  mkdir -p "$WORK/tar"
  cp -R "$STAGE/usr" "$WORK/tar/$BASENAME"
  tar -C "$WORK/tar" -czf "$DIST/$BASENAME.tar.gz" "$BASENAME"
  echo "wrote $DIST/$BASENAME.tar.gz"
fi

if has deb || has rpm; then
  command -v nfpm >/dev/null || { echo "error: nfpm not found (https://nfpm.goreleaser.com/docs/install/)" >&2; exit 1; }
  export VERSION
  export RELEASE="1${PDFCRAFT_BUILD_LABEL:+.$PDFCRAFT_BUILD_LABEL}"
  export NFPM_ARCH="$DEB_ARCH"
  sed "s|\${STAGE}|$STAGE|g" "$HERE/nfpm-academic.yaml" >"$WORK/nfpm.yaml"
  for fmt in deb rpm; do
    has "$fmt" || continue
    nfpm package -f "$WORK/nfpm.yaml" -p "$fmt" -t "$WORK/"
    mv "$WORK"/*."$fmt" "$DIST/$BASENAME.$fmt"
    echo "wrote $DIST/$BASENAME.$fmt"
  done
fi

"$STAGE/usr/bin/psg-academic-pdf" --version
echo "==> done"
