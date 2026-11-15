#!/usr/bin/env bash
# PSG Academic PDF from the command line, end to end with the installed TeX Live: the tools are
# found, a project is made from the built-in template, and it compiles to a non-empty PDF. Runs
# after the tests in the Linux gate of .github/workflows/build.yaml, which installs TeX Live,
# latexmk, Biber and Pandoc.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
app() { cargo run --locked -q -p psg-academic-pdf -- "$@"; }
app check-tex
app templates
app new "$work/article"
app compile "$work/article" --out "$work/article.pdf"
test -s "$work/article.pdf"
echo "academic smoke test: $(wc -c <"$work/article.pdf") byte PDF"
