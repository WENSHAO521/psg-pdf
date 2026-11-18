# PSG brand and design system

PSG PDF and PSG Academic PDF are two products of one family. They share a publisher, a logo
language, type, shapes and spacing, and they differ in one thing people see first: colour. This
page is the guide. The values live in code in [`crates/brand`](../crates/brand/src/lib.rs)
(`psg-brand`), which both apps use, and the installer identity lives in
[`packaging/products.toml`](../packaging/products.toml). Tests keep the two in step.

## The products

| | PSG PDF | PSG Academic PDF |
|---|---|---|
| Role | Professional PDF reading, editing and management | Scholarly typesetting and publication-grade PDF production |
| One line (installers, shortcuts) | Reading, organizing and protecting PDFs | Typesetting accepted articles with pdfLaTeX |
| Brand colour | Deep blue `#1E4384` | Academic purple `#5B2A78` |
| Character | Simple, modern, professional | Refined, rigorous, academic |
| Mark | Gold line-art page with a folded corner in a rounded hexagon | Letter P whose stem is a pen nib and whose bowl is a rolled page, gold keyline and corner flourishes |

### Names

- Write the names in full and exactly: **PSG PDF**, **PSG Academic PDF**. Never "PSG-PDF",
  "PSGPDF" or "Academic PDF" on its own in anything a user reads.
- The publisher is **Panorama Scholarly Group**; **PSG** is its short form.
- The old combined installer was called **PSG PDF Suite**. It is gone; the name appears only where
  installs of it are recognised and migrated (`packaging/products.toml`, `[legacy_suite]`).
- **PdfCraft** is named only as the upstream engine, with its licence: "Based on PdfCraft by the
  ArtCraft team (MIT OR Apache-2.0)." PdfCraft's licence requires modified versions to drop the
  ArtCraft name, wordmark and logos as branding, so they never appear as ours.

## Logo

One editable SVG master, [`assets/brand/psg-logo-master.svg`](../assets/brand/psg-logo-master.svg),
draws both product logos; [`assets/brand/README.md`](../assets/brand/README.md) explains its
groups and how to regenerate every icon. Do not redraw the marks.

- Every platform icon is generated from the master by `packaging/psg-icons.py`: Windows `.ico`
  (16 to 256 px), macOS `.icns`, Linux hicolor PNGs (16 to 512 px) and SVG, the window icon, and
  the DMG backgrounds.
- Icons of 48 px and smaller use the master's compact variant: the same design with heavier
  strokes, never a filled or different mark.
- The tile is opaque (deep brand gradient) on a transparent canvas with rounded corners, so the
  icon reads on light and dark docks, taskbars and Start menus alike.
- Keep clear space of at least a quarter of the icon's width around it. Don't recolour, rotate,
  outline or put it on a busy picture.

## Colour

| Token | PSG PDF | PSG Academic PDF | Use |
|---|---|---|---|
| Brand | `#1E4384` | `#5B2A78` | Icon tile, product name, accent in the light theme |
| Brand deep | `#0B1D45` | `#26103A` | Dark end of the icon tile |
| Accent, light theme | `#1E4384` | `#5B2A78` | Primary buttons, focus, active tab, links |
| Accent, dark theme | `#4472C4` | `#9A55C0` | The same, in the dark theme |
| Accent text, dark theme | `#A9C1EC` | `#CDB0E2` | Links and accent text on dark panels |
| Selected, light / dark | `#DCE5F3` / `#24365A` | `#E6DAEE` / `#3A2848` | Selected rows |
| Gold (shared) | `#D8AE5C` | `#D8AE5C` | Logo line work, the release badge |
| Cream (shared) | `#F5EFE1` | `#F5EFE1` | Logo paper |

Neutral surfaces are shared: white panels on `#F1F1F3` in the light theme, `#26262A` panels on
`#19191C` in the dark one. Text on an accent fill is white in both themes.

Contrast is tested (`cargo test -p psg-brand`): accent text meets WCAG AA (4.5:1) on every panel
and on its soft wash, white on the accent fill meets 4.5:1, and accents used as non-text marks
meet 3:1. The two products' accents are tested to stay clearly apart.

## Type

- Interface: **Inter** (OFL) in both products, from the PSG Desktop Design System
  ([design-system.md](design-system.md)). Chinese, Japanese and Korean come from the craft-fonts
  faces in release builds.
- Code, logs and TeX source: **JetBrains Mono** (OFL) in both products.
- Sizes (points): 12 small, 13 body, 15 heading, 20 title, 28 display.
- No font is taken from the operating system into a PDF, a package or the repository
  ([AGENTS.md](../AGENTS.md) §1.4).

## Shape and spacing

- Corner radius: 4 px for badges, 6 px for buttons, fields, menus and toolbar controls, 8 px for
  cards and panels, 10 px for dialogs.
- Spacing scale: 4, 8, 12, 16, 24, 32 px. Margins and gaps take one of these.
- Toolbars: icon buttons are Lucide outline icons at one size per toolbar; labels are sentence
  case ("Export PDF…", not "Export Pdf…"); an ellipsis means the command asks something first.

## Release badge

A build that is not a stable release says so, the same way everywhere:

| Where | Stable | Beta |
|---|---|---|
| Window title | PSG PDF | PSG PDF Beta |
| About window, home page, Academic top bar | name, version `0.19.0` | name, gold **Beta** badge, version `0.19.0 Beta 3` |
| Installer and file names | `psg-pdf-0.19.0-…` | `psg-pdf-0.19.0-beta.3-…` |
| GitHub release | PSG PDF & PSG Academic PDF 0.19.0 | PSG PDF & PSG Academic PDF 0.19.0 Beta 3, marked pre-release |

The badge is drawn in the shared gold in both products. A build with another pre-release or a
local build label says **Development build**. The kind of build comes from the version CI gives
it (`X.Y.Z-beta.N` for a beta), so a local build of `0.19.0` shows no badge. Windows MSI versions
are numeric, so the MSI itself carries `0.19.0`; its file name carries the beta number.

Both beta and stable releases include Windows, macOS and Linux installers for both products
([releasing.md](releasing.md)).

## Where it is applied

The interface itself (components, layout, notifications) is the PSG Desktop Design System:
[design-system.md](design-system.md).

| Surface | PSG PDF | PSG Academic PDF |
|---|---|---|
| Interface accent and selection | `crates/ui-egui/src/theme.rs` (from `crates/desktop-ui`) | `crates/desktop-ui` via `apps/psg-academic-pdf/src/theme.rs` |
| Name, version, badge | `crates/ui-egui/src/brand.rs` | `apps/psg-academic-pdf/src/theme.rs` |
| Update dialog accent | `crates/ui-egui/src/lib.rs` | `apps/psg-academic-pdf/src/updates.rs` |
| Windows exe icon and version info | `apps/psg-pdf/build.rs` | `apps/psg-academic-pdf/build.rs` |
| Installer identity | `packaging/products.toml` | `packaging/products.toml` |
| Linux software centre colours and screenshots | `packaging/linux/*.metainfo.xml.in` | `packaging/linux/*.metainfo.xml.in` |
| Release title and notes | `packaging/ci/release-notes.sh` | `packaging/ci/release-notes.sh` |
