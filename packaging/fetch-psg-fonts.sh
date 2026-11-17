#!/usr/bin/env bash
# Add PSG's interface faces to a craft-fonts checkout, so a build with CRAFT_FONTS_DIR draws the
# interface in them (AGENTS.md §1.4: no font file is committed here). All are OFL-1.1, from their
# authors' own releases, checked against the SHA-256 recorded in ATTRIBUTION.toml:
#
# - LXGW WenKai TC 霞鶩文楷 TC (Regular, Medium), lxgw/LxgwWenkaiTC v1.530: Traditional Chinese,
#   Japanese and the Latin-script languages.
# - LXGW WenKai Lite 霞鶩文楷 Lite Medium, lxgw/LxgwWenKai-Lite v1.530: Simplified Chinese headings
#   (its Regular is already in craft-fonts).
# - Gowun Batang 고운바탕 (Regular, Bold), from Google Fonts' repository: Korean.
#
# Usage: packaging/fetch-psg-fonts.sh <craft-fonts dir>
#   writes the faces under fonts/<family>/ with their OFL.txt, and fonts/psg-ui-manifest.txt
#   (read by crates/fonts/build.rs and the packaging's font notices).
#
# packaging/fetch-craft-fonts.sh runs it, so every release build embeds the faces.
set -euo pipefail
DIR="${1:?usage: fetch-psg-fonts.sh <craft-fonts dir>}"
TC="https://github.com/lxgw/LxgwWenkaiTC/releases/download/v1.530"     # tag commit c52809f35e5f3a03e90d1c7bac9011adac48e7cf
TC_RAW="https://raw.githubusercontent.com/lxgw/LxgwWenkaiTC/v1.530"
LITE="https://github.com/lxgw/LxgwWenKai-Lite/releases/download/v1.530" # tag commit b712c83d98acf977bacfb7fb010207802808bda9
LITE_RAW="https://raw.githubusercontent.com/lxgw/LxgwWenKai-Lite/v1.530"
GOWUN="https://raw.githubusercontent.com/google/fonts/7af0d90b4d532ffa4b041937b560c6df4e7b1e95/ofl/gowunbatang"
# URL | destination under the input | SHA-256
FILES="$TC/LXGWWenKaiTC-Regular.ttf|fonts/lxgw-wenkai-tc/LXGWWenKaiTC-Regular.ttf|6662480a3f95b4a90c80b189c679617a48adce3a505118a6af63ce5a482b83fa
$TC/LXGWWenKaiTC-Medium.ttf|fonts/lxgw-wenkai-tc/LXGWWenKaiTC-Medium.ttf|649efd8516f42d36eacc05480d5dbd2713fb28fbd2bbee8ea76fc40875045c49
$TC_RAW/OFL.txt|fonts/lxgw-wenkai-tc/OFL.txt|1a13783bf4337242834b60309cdcdbeb566ae3f923576fdbba8b78db9d174def
$LITE/LXGWWenKaiLite-Medium.ttf|fonts/lxgw-wenkai-lite/LXGWWenKaiLite-Medium.ttf|9282063131640c9558a102560cf6d6f8ecc4bb441e74f2bcff032ce4786238d0
$LITE_RAW/OFL.txt|fonts/lxgw-wenkai-lite/OFL.txt|0333a85fe077b6849f8e417acc44e8bd879442ceb681e271acf93e2d57fd4e2e
$GOWUN/GowunBatang-Regular.ttf|fonts/gowun-batang/GowunBatang-Regular.ttf|466c593e7147412e748af4856d5ad14709b5a860bdf62b9c2546f2c5874e9849
$GOWUN/GowunBatang-Bold.ttf|fonts/gowun-batang/GowunBatang-Bold.ttf|dbfcaa646e5831e7478524924f02906f550285a5050699b4e38c9950b3ec4b94
$GOWUN/OFL.txt|fonts/gowun-batang/OFL.txt|49a57cc769fa9affd6eefb9070a61e3d3f6b757c97cafb15848bc6d1c81acc78"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

# The interface face of 2026-11-17's builds, replaced by the Kai faces: never embed it again.
rm -rf "$DIR/fonts/noto-serif-cjk-tc"
while IFS='|' read -r url rel want; do
  dest="$DIR/$rel"
  mkdir -p "$(dirname "$dest")"
  if [[ -f "$dest" && "$(sha256 "$dest")" == "$want" ]]; then continue; fi
  tmp="$dest.part"
  for attempt in 1 2 3 4; do
    if curl -fsSL --retry 3 -o "$tmp" "$url"; then break; fi
    [[ "$attempt" -lt 4 ]] || { echo "error: downloading $url failed" >&2; exit 1; }
    sleep $((attempt * 5))
  done
  got="$(sha256 "$tmp")"
  if [[ "$got" != "$want" ]]; then
    rm -f "$tmp"
    echo "error: $url has SHA-256 $got, expected $want" >&2
    exit 1
  fi
  mv "$tmp" "$dest"
done <<<"$FILES"

cat >"$DIR/fonts/psg-ui-manifest.txt" <<'EOF'
# PSG's interface faces, added by psg-pdf's packaging/fetch-psg-fonts.sh (not part of craft-fonts).
# family | style | file | scripts | licence | notice
LXGW WenKai TC | Regular | fonts/lxgw-wenkai-tc/LXGWWenKaiTC-Regular.ttf | Hant,Jpan,Latn | OFL-1.1 | fonts/lxgw-wenkai-tc/OFL.txt
LXGW WenKai TC | Medium | fonts/lxgw-wenkai-tc/LXGWWenKaiTC-Medium.ttf | Hant,Jpan,Latn | OFL-1.1 | fonts/lxgw-wenkai-tc/OFL.txt
LXGW WenKai Lite | Medium | fonts/lxgw-wenkai-lite/LXGWWenKaiLite-Medium.ttf | Hans | OFL-1.1 | fonts/lxgw-wenkai-lite/OFL.txt
Gowun Batang | Regular | fonts/gowun-batang/GowunBatang-Regular.ttf | Kore | OFL-1.1 | fonts/gowun-batang/OFL.txt
Gowun Batang | Bold | fonts/gowun-batang/GowunBatang-Bold.ttf | Kore | OFL-1.1 | fonts/gowun-batang/OFL.txt
EOF
echo "PSG interface faces (LXGW WenKai TC, LXGW WenKai Lite Medium, Gowun Batang) in $DIR/fonts"
