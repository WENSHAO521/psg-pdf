#!/usr/bin/env bash
# Add PSG's interface face, Noto Serif CJK TC (思源宋體, OFL-1.1), to a craft-fonts checkout, so a
# build with CRAFT_FONTS_DIR draws the whole interface in it, in every language (AGENTS.md §1.4:
# no font file is committed here). The files come from the Noto CJK project's own repository at
# the Serif2.003 release tag and are checked against the SHA-256 recorded in ATTRIBUTION.toml.
#
# Usage: packaging/fetch-psg-fonts.sh <craft-fonts dir>
#   writes fonts/noto-serif-cjk-tc/{NotoSerifCJKtc-Regular.otf,NotoSerifCJKtc-SemiBold.otf,OFL.txt}
#   and fonts/psg-ui-manifest.txt (read by crates/fonts/build.rs and the packaging notices).
#
# packaging/fetch-craft-fonts.sh runs it, so every release build embeds the face.
set -euo pipefail
DIR="${1:?usage: fetch-psg-fonts.sh <craft-fonts dir>}"
TAG="Serif2.003" # notofonts/noto-cjk tag, commit 9b0f1436e455d902de067a2501422e5dc71ad16b
BASE="${PSG_FONTS_URL:-https://raw.githubusercontent.com/notofonts/noto-cjk/$TAG/Serif}"
FAMILY_DIR="fonts/noto-serif-cjk-tc"
# source path | file name | SHA-256
FILES="OTF/TraditionalChinese/NotoSerifCJKtc-Regular.otf|NotoSerifCJKtc-Regular.otf|234301038e76e7c35c43113785024700c4e4fe7bdce1d1fbbc42fca7e6683798
OTF/TraditionalChinese/NotoSerifCJKtc-SemiBold.otf|NotoSerifCJKtc-SemiBold.otf|422e868b6a983caa3a661f6580295fbaf00f26694087e86810fa58577e9be27a
LICENSE|OFL.txt|6a73f9541c2de74158c0e7cf6b0a58ef774f5a780bf191f2d7ec9cc53efe2bf2"

sha256() {
  if command -v sha256sum >/dev/null 2>&1; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi
}

mkdir -p "$DIR/$FAMILY_DIR"
while IFS='|' read -r src name want; do
  dest="$DIR/$FAMILY_DIR/$name"
  if [[ -f "$dest" && "$(sha256 "$dest")" == "$want" ]]; then continue; fi
  tmp="$dest.part"
  for attempt in 1 2 3 4; do
    if curl -fsSL --retry 3 -o "$tmp" "$BASE/$src"; then break; fi
    [[ "$attempt" -lt 4 ]] || { echo "error: downloading $BASE/$src failed" >&2; exit 1; }
    sleep $((attempt * 5))
  done
  got="$(sha256 "$tmp")"
  if [[ "$got" != "$want" ]]; then
    rm -f "$tmp"
    echo "error: $src has SHA-256 $got, expected $want" >&2
    exit 1
  fi
  mv "$tmp" "$dest"
done <<<"$FILES"

cat >"$DIR/fonts/psg-ui-manifest.txt" <<EOF
# PSG's interface face, added by psg-pdf's packaging/fetch-psg-fonts.sh (not part of craft-fonts).
# family | style | file | scripts | licence | notice
Noto Serif CJK TC | Regular | $FAMILY_DIR/NotoSerifCJKtc-Regular.otf | Hant,Hans,Jpan,Kore,Latn,Grek,Cyrl | OFL-1.1 | $FAMILY_DIR/OFL.txt
Noto Serif CJK TC | SemiBold | $FAMILY_DIR/NotoSerifCJKtc-SemiBold.otf | Hant,Hans,Jpan,Kore,Latn,Grek,Cyrl | OFL-1.1 | $FAMILY_DIR/OFL.txt
EOF
echo "Noto Serif CJK TC ($TAG) in $DIR/$FAMILY_DIR"
