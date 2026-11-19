# Changelog

Changes to PSG PDF and PSG Academic PDF, two independent products released at the same time
(until 0.18, together as the PSG PDF Suite). Each section is written by
`cargo xtask changelog release` from the Conventional Commits since the previous release, and may
be reworded by hand before the release commit. See docs/releasing.md.

History before the line below belongs to PdfCraft, the project both products are based on.

<!-- changelog-base: d2916decb3d948b7e404ee839d7542692bf18d92 -->

## v0.20.1 (2026-11-19)

**Features**

- **ui** PSG PDF and PSG Academic PDF now share one design system: the same colours, type, controls and notifications, with each product's own accent. (521db56a)
- **pdf** The PSG PDF home puts Open file and recent documents first, and notifications share the new PSG look. (ba29f945)
- **academic** PSG Academic PDF has a new workspace: dashboard, manuscript forms with a resizable editor and preview, template library, task centre, problems list, recent projects and a dark theme. (61427877)
- **i18n** PSG PDF and PSG Academic PDF support ten interface languages (b89b9ec5, b1adb13c)
- **academic** Let people move the data folder from Settings (1f018f5e)
- **academic** Install TeX automatically and keep it up to date (ab9dbc0c)
- **ui** Both apps use LXGW WenKai for Chinese, Japanese and Latin-script languages, and Gowun Batang for Korean (e4f2a4d5, c96f05b9)
- **pdf** Let people add their own AI provider and API key for the AI assistant (1239cc73)
- **pdf** Export a PDF copy with text converted to outlines, and find available editing, protection and print tools more easily (7f6dd0a7)
- **pdf** Set presentation page transitions from Organize pages (44e2562e)
- **pdf** Preserve rotated text and overlapping artwork more reliably when exporting to Word, recognize small print on large scans, and search long printer lists (ffaf8c11)

**Bug Fixes**

- **academic** PSG Academic PDF fits small windows: an icon navigation rail and pages that no longer run off the edge. (9acef5b2)
- **update** Windows installs update themselves again (ae17f8c0)
- **academic** Open the window on more GPUs and log startup errors (7d577e4a)
- **pdf** Save over an existing PDF on macOS while preserving Finder tags and other extended attributes (3f54f454, e199b281)
- **pdf** Safer handling of damaged and hostile PDFs, more reliable redaction, signatures, forms and printing, and PSG PDF now starts when OPENSSL_ia32cap names a CPU feature the processor lacks (fixes from PdfCraft) (f0ca781b)
- **updates** Installed betas get the next beta instead of saying they're up to date (a1623f01)
- **ui** Link PSG's own websites instead of the upstream project (52748850)
- **ui** Align menu titles with the tool tabs, restore keyboard access letters, and show update download progress in both apps (85013b41, 4e106a8f)
- **ui** Align font controls and order the Simplified Chinese fonts correctly (9a273da8)
- **deps** Use Jiff for the apps' date and time handling (d787ff9a)
- **academic** Download the latest TinyTeX bundle directly from the official release for your platform and report when an installed TeX tool fails its check (a823c7c7)

**Other Changes**

- **website** Improve mobile navigation and make it easier to compare both apps and find the right installer (c9b018fe)

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
