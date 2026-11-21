#!/usr/bin/env bash
# Installs, runs and removes PSG PDF's Linux package, as a user would:
#
#   1. PSG PDF installs, owns its files and runs, and removing it leaves nothing behind;
#   2. with a 0.18.x "PSG PDF Suite" package given: upgrading from it keeps PSG PDF and removes
#      the suite's PSG Academic PDF files (that product is retired; docs/products.md).
#
# Usage: packaging/linux/test-install.sh deb|rpm <folder with the packages> [<suite package>]
#
# deb on Debian/Ubuntu (CI: the Ubuntu runner, with sudo); rpm on Fedora (CI: a fedora container,
# as root). Changes the system's packages: run it on a throwaway machine or container.
set -euo pipefail
FMT="${1:?deb or rpm}"
DIR="${2:?folder with the packages}"
SUITE="${3:-}"
SUDO=""
[[ "$(id -u)" = 0 ]] || SUDO=sudo

pkg() { compgen -G "$DIR/$1-[0-9]*-linux-*.$FMT" | head -n1 || { echo "error: no $1 .$FMT in $DIR" >&2; exit 1; }; }
PDF="$(realpath "$(pkg psg-pdf)")"
echo "PSG PDF: $PDF"

step() { echo; echo "==> $*"; }
fail() { echo "::error::$*" >&2; exit 1; }
# Weak dependencies stay out, as on a minimal system.
install_pkgs() {
  case "$FMT" in
    deb) $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends "$@" ;;
    rpm) $SUDO dnf install -y --setopt=install_weak_deps=False "$@" ;;
  esac
}
remove_pkgs() {
  case "$FMT" in
    deb) $SUDO env DEBIAN_FRONTEND=noninteractive apt-get purge -y "$@" ;;
    rpm) $SUDO dnf remove -y "$@" ;;
  esac
}
owner() {
  case "$FMT" in
    deb) dpkg-query -S "$1" 2>/dev/null | cut -d: -f1 ;;
    rpm) rpm -qf --qf '%{NAME}\n' "$1" 2>/dev/null ;;
  esac
}
owned_by() { # <file> <package>
  [[ -e "$1" ]] || fail "$1 is missing"
  local o
  o="$(owner "$1")"
  [[ "$o" = "$2" ]] || fail "$1 belongs to '${o:-nothing}', expected $2"
}
absent() { [[ ! -e "$1" ]] || fail "$1 should not be installed"; }

pdf_files=(/usr/bin/psg-pdf /usr/bin/pdfcraft-cli
  /usr/share/applications/com.panoramascholarly.psgpdf.desktop
  /usr/share/icons/hicolor/256x256/apps/com.panoramascholarly.psgpdf.png
  /usr/share/mime/packages/com.panoramascholarly.psgpdf.xml)
# The suite's PSG Academic PDF program, which the upgrade from the suite must remove.
academic_files=(/usr/bin/psg-academic-pdf)
check_pdf() {
  for f in "${pdf_files[@]}"; do owned_by "$f" psg-pdf; done
  psg-pdf --version
  pdfcraft-cli --version
}

step "PSG PDF alone"
install_pkgs "$PDF"
check_pdf
for f in "${academic_files[@]}"; do absent "$f"; done

step "Removing PSG PDF"
remove_pkgs psg-pdf
for f in "${pdf_files[@]}"; do absent "$f"; done

if [[ -n "$SUITE" ]]; then
  SUITE="$(realpath "$SUITE")"
  step "Migrating from the PSG PDF Suite ($SUITE)"
  install_pkgs "$SUITE"
  owned_by /usr/bin/psg-academic-pdf psg-pdf
  # PSG PDF continues the suite's package: the upgrade keeps PSG PDF and drops the rest.
  install_pkgs "$PDF"
  check_pdf
  for f in "${academic_files[@]}"; do absent "$f"; done
  remove_pkgs psg-pdf
fi

step "Nothing is left behind"
for f in "${pdf_files[@]}" "${academic_files[@]}"; do absent "$f"; done
echo "install tests passed"
