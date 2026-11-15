#!/usr/bin/env bash
# Installs, runs and removes the Linux packages of both PSG products, as a user would:
#
#   1. PSG PDF alone, then PSG Academic PDF beside it (they coexist; each owns its own files);
#   2. removing either leaves the other working;
#   3. with a 0.18.x "PSG PDF Suite" package given: upgrading from it keeps PSG PDF and hands the
#      suite's PSG Academic PDF files to the new psg-academic-pdf package (docs/products.md).
#
# Usage: packaging/linux/test-install.sh deb|rpm <folder with both products' packages> [<suite package>]
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
ACADEMIC="$(realpath "$(pkg psg-academic-pdf)")"
echo "PSG PDF: $PDF"
echo "PSG Academic PDF: $ACADEMIC"

step() { echo; echo "==> $*"; }
fail() { echo "::error::$*" >&2; exit 1; }
# Weak dependencies stay out: PSG Academic PDF recommends a TeX installation of several GB.
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
installed() {
  case "$FMT" in
    deb) dpkg-query -W -f '${Status}' "$1" 2>/dev/null | grep -q 'install ok installed' ;;
    rpm) rpm -q "$1" >/dev/null 2>&1 ;;
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
academic_files=(/usr/bin/psg-academic-pdf
  /usr/share/applications/com.panoramascholarly.psgacademicpdf.desktop
  /usr/share/icons/hicolor/256x256/apps/com.panoramascholarly.psgacademicpdf.png
  /usr/share/psg-academic-pdf/templates/psg-article/template.toml)
check_pdf() {
  for f in "${pdf_files[@]}"; do owned_by "$f" psg-pdf; done
  psg-pdf --version
  pdfcraft-cli --version
}
check_academic() {
  for f in "${academic_files[@]}"; do owned_by "$f" psg-academic-pdf; done
  psg-academic-pdf --version
  # Every packaged template is found through ../share/psg-academic-pdf/templates.
  local listed id
  listed="$(psg-academic-pdf templates)"
  for t in /usr/share/psg-academic-pdf/templates/*/template.toml; do
    id="$(basename "$(dirname "$t")")"
    grep -q "^$id " <<<"$listed" || fail "psg-academic-pdf does not list the packaged template $id"
  done
}

step "PSG PDF alone"
install_pkgs "$PDF"
check_pdf
for f in "${academic_files[@]}"; do absent "$f"; done

step "PSG Academic PDF beside PSG PDF"
install_pkgs "$ACADEMIC"
check_academic
check_pdf

step "Removing PSG Academic PDF leaves PSG PDF"
remove_pkgs psg-academic-pdf
for f in "${academic_files[@]}"; do absent "$f"; done
check_pdf

step "Removing PSG PDF leaves PSG Academic PDF"
install_pkgs "$ACADEMIC"
remove_pkgs psg-pdf
for f in "${pdf_files[@]}"; do absent "$f"; done
check_academic
remove_pkgs psg-academic-pdf

if [[ -n "$SUITE" ]]; then
  SUITE="$(realpath "$SUITE")"
  step "Migrating from the PSG PDF Suite ($SUITE)"
  install_pkgs "$SUITE"
  owned_by /usr/bin/psg-academic-pdf psg-pdf
  case "$FMT" in
    deb)
      # Academic first: it takes the suite's files over without touching PSG PDF.
      install_pkgs "$ACADEMIC"
      installed psg-pdf || fail "installing PSG Academic PDF removed the suite's PSG PDF"
      owned_by /usr/bin/psg-academic-pdf psg-academic-pdf
      psg-pdf --version
      install_pkgs "$PDF"
      ;;
    rpm)
      # rpm can't hand files over: both in one transaction.
      install_pkgs "$PDF" "$ACADEMIC"
      ;;
  esac
  check_pdf
  check_academic
  remove_pkgs psg-pdf psg-academic-pdf
fi

step "Nothing is left behind"
for f in "${pdf_files[@]}" "${academic_files[@]}"; do absent "$f"; done
echo "install tests passed"
