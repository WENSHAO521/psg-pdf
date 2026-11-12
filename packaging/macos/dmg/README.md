# macOS DMG window

What Finder shows when the PSG PDF Suite DMG opens: a 660 × 400 pt background with both apps,
PSG PDF and PSG Academic PDF, and the `Applications` link in a row. `package.sh` copies these files
into the image; nothing here is generated at build time, so the DMG builds with a plain
`hdiutil create -srcfolder` (no Finder scripting on CI, nothing extra installed in the signing job).

| File | What |
|---|---|
| `background.svg` | The background, written by `generate.py`: a navy (`#1f3a68`) title band with a gold accent (`#d9a441`) over Paper, and an arrow towards `Applications`. Its text (Inter, JetBrains Mono, from `assets/fonts`) is outlined with usvg, so rendering it needs no fonts. |
| `background.tiff` | The background at 1x (660 × 400 px, 72 dpi) and 2x (1320 × 800 px, 144 dpi) in one HiDPI TIFF (16-colour palette, Deflate, sRGB). Goes to `.background/background.tiff`. |
| `dmg-layout.DS_Store` | Finder's view settings for the volume: window size, icon size 128, `PSG PDF.app` at (130, 228), `PSG Academic PDF.app` at (300, 228), `Applications` at (530, 228), and the background. Goes to `.DS_Store` in the image; named so it isn't mistaken for (or ignored like) a Finder-generated `.DS_Store`. |
| `generate.py` | Writes all of the above from the template and layout in the script. |

## The volume name has no version

The mounted volume is called `PSG PDF Suite`, without a version. The layout points at the
background through an alias that includes the volume name, and Finder resolves it by that name:
with a versioned name the window keeps its size and icon positions but shows no background. The DMG
file name (`psg-pdf-suite-<version>-macos-<arch>.dmg`) still carries the version.

## Rules

- **Finder draws the icon labels in black in light and dark mode** when a window has a background,
  so the area under the icons stays light (Paper).
- **Nothing goes inside the icon boxes:** artwork keeps clear of each 128 pt icon box and of the
  label strip under it.

## Regenerate

Edit `generate.py` (template, words, colours or layout), then run, on any OS:

```sh
cargo install resvg usvg --locked
pip install 'pillow>=12' ds_store==1.3.3 mac_alias==2.2.3
python3 packaging/macos/dmg/generate.py
cargo xtask assets --write   # the outputs' hashes are recorded in ATTRIBUTION.toml
```

All outputs are byte-for-byte reproducible and carry nothing from the machine that ran it: the SVG
renders without any fonts, and `dmg-layout.DS_Store` is written from scratch, its background alias
holding only the volume name and `/.background/background.tiff`. The window is 660 × 432 with
Finder's 32 pt title bar; the content area is 660 × 400.

The approach (not the artwork) follows PdfCraft's `packaging/macos/dmg/`.
