#!/usr/bin/env bash
# PSG Academic PDF installs its own TeX: the built program downloads TinyTeX into an empty
# folder, adds the template packages, and typesets the built-in template, including a package
# the document asks for that TinyTeX-1 doesn't have (installed during the compilation). Runs
# after the install tests in each platform leg of .github/workflows/build.yaml
# (docs/academic-tex.md).
#
#   packaging/ci/academic-tex-smoke.sh <platform>
#
# It downloads from TinyTeX's GitHub releases and a CTAN mirror.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
platform="${1:?usage: academic-tex-smoke.sh <platform>}"
ext=""
[[ "$platform" == windows-* ]] && ext=".exe"
app="$(compgen -G "${CARGO_TARGET_DIR:-target}/release/psg-academic-pdf$ext" || compgen -G "${CARGO_TARGET_DIR:-target}/*/release/psg-academic-pdf$ext" | head -n1)" \
  || { echo "::error::psg-academic-pdf was not built"; exit 1; }
work="${RUNNER_TEMP:-$(mktemp -d)}/psg-tex-smoke"
rm -rf "$work"
mkdir -p "$work"
# Never the per-user folder of whoever runs this.
export PSG_ACADEMIC_TEX_DIR="$work/tex"
"$app" install-tex
"$app" check-tex
"$app" new "$work/article"
# A package outside TinyTeX-1's list, so the compilation has to install it.
perl -pi -e 's/^\\begin\{document\}/\\usepackage{lipsum}\n\\begin{document}/' "$work/article/main.tex"
printf '\n\\lipsum[1]\n' >>"$work/article/body.tex"
"$app" compile "$work/article" --out "$work/article.pdf"
test -s "$work/article.pdf"
"$app" update-tex
echo "academic TeX smoke test: $(wc -c <"$work/article.pdf") byte PDF with PSG's own TeX"
