# Changelog

Changes to PSG PDF and PSG Academic PDF, two independent products released at the same time
(until 0.18, together as the PSG PDF Suite). Each section is written by
`cargo xtask changelog release` from the Conventional Commits since the previous release, and may
be reworded by hand before the release commit. See docs/releasing.md.

History before the line below belongs to PdfCraft, the project both products are based on.

<!-- changelog-base: d2916decb3d948b7e404ee839d7542692bf18d92 -->

## v0.20.0 (2026-11-15)

**Features**

- **update** PSG PDF and PSG Academic PDF download and install their own updates: each release is signed with PSG's key, and an update installs only after its signature and checksum check out (b39c3c4c)
- **update** Get beta updates: an opt-in beta channel in both apps, confirmed a second time, that updates to the newest release, beta or stable (b39c3c4c)

**Bug Fixes**

- **website** The download buttons download the right file for each product and platform directly (3415365b)

## v0.19.0 (2026-11-15)

**Breaking Changes**

- **release** The PSG PDF Suite installers are replaced by separate PSG PDF and PSG Academic PDF installers. Upgrading PSG PDF replaces the suite; install PSG Academic PDF separately (your projects are kept). (354f3ad1)

**Features**

- **brand** New PSG PDF and PSG Academic PDF logos, generated for every platform from one SVG master (29375dae)
- **academic** Humanities journal template, DOCX import checks and fix hints for TeX problems (f5313388)
- **academic** Source and PDF side by side, bibliography checks, script checks and six interface languages (4dc8d0d1)
- **academic** Check a project's scripts and bibliography from the command line (4458bba4)
- **brand** One PSG design system for both apps, with a Beta badge (36fc320b)
- **release** Branded release pages that flag an incomplete beta (ce1640c1)

**Bug Fixes**

- **macos** The macOS PSG Academic PDF app finds its journal templates in the app bundle (d0f80d75)
- **academic** Refuse a Pandoc too old to convert sandboxed, and say which version is needed (66884ef4)
- **brand** Keep the approved hexagon and outlined page in the small app icons (31409da3)
- **sign** Draw typed signatures in Chinese, Japanese, Korean, Arabic and Hebrew (e6addee8)
- **windows** Show PSG Academic PDF's icon and publisher on its program (041031c5)
- **macos** Name the right product in each app's About panel (03545341)
- **packaging** PSG screenshots and brand colours in software centres (374f196d)

**Other Changes**

- **release** Every push to beta publishes a test build, and each release has a changelog (efd6c11e)
