# PSG logo master

[`psg-logo-master.svg`](psg-logo-master.svg) is the one editable source of both product logos. Every
platform icon (Windows `.ico`, macOS `.icns`, Linux hicolor PNGs and SVG, the window icon) and, through
them, the macOS DMG backgrounds are generated from it. Edit this file, never the generated ones.

| Group | Product | Field |
|---|---|---|
| `<g id="psg-pdf">` | PSG PDF | deep blue `#1e4384` → `#0b1d45` |
| `<g id="psg-academic-pdf">` | PSG Academic PDF | academic purple `#5b2a78` → `#26103a` |

Each group is a 1024 × 1024 icon (the master places them side by side). Inside a group:

- elements with `class="detail"` are drawn at 64 px and above (fine line work, ornament, watermark);
- elements with `class="compact"` replace them at 48 px and below, so small icons stay legible.

The shared colours are cream `#f5efe1` and gold `#d8ae5c`. The group ids, the icon folders and the brand
colours are recorded in [`packaging/products.toml`](../../packaging/products.toml); `cargo xtask products
check` fails if a product's group is missing from the master.

## Regenerate

```sh
cargo install resvg --locked                 # once
python3 packaging/psg-icons.py               # every icon of both products
python3 packaging/macos/dmg/generate.py      # the DMG backgrounds
cargo xtask assets --write && cargo xtask assets
```

**Provenance:** contributor-original, drawn as SVG for this project (no fonts, no third-party artwork,
nothing traced). Licence: `MIT OR Apache-2.0`, like the repository ([LICENSE.txt](../app-icon/LICENSE.txt)).
