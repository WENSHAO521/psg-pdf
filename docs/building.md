# Building and testing PSG PDF and PSG Academic PDF

This page is for people who build the two products from source, change them or release them.
People who only want to use them should download an installer from the
[releases page](https://github.com/WENSHAO521/psg-pdf/releases).

Both products are built from one Rust workspace and share one PDF engine, the
PdfCraft engine (MIT OR Apache-2.0) in `crates/`.
Rules for contributors and agents are in [AGENTS.md](../AGENTS.md).

## Repository layout

```text
Cargo.toml                  workspace: crates/*, apps/*, xtask
apps/
  psg-pdf/                  PSG PDF desktop app (formerly apps/pdfcraft)
  psg-academic-pdf/         PSG Academic PDF desktop app + headless commands
  pdfcraft-cli/             headless CLI and MCP server (shared engine)
  pdfcraft-web/             web build of the viewer
  pdfcraft-ocr-worker/      OCR helper process
crates/
  academic/                 psg-academic: TeX detection, sandboxed latexmk compilation,
                            log diagnostics, templates, publication metadata, projects,
                            DOCX import interface (Pandoc, experimental)
  engine/ render/ cos/ …    the PdfCraft PDF engine, shared by both programs
  ui-egui/                  PSG PDF's user interface (brand.rs holds the PSG name)
templates/academic/         journal templates (both are also compiled into the app)
assets/brand/               the editable PSG logo master (one SVG for both products)
crates/brand/               psg-brand: the PSG design system (names, colours, release badge)
assets/app-icon/            PSG PDF icons, generated from the master (packaging/psg-icons.py)
assets/app-icon-academic/   PSG Academic PDF icons, generated from the master
packaging/products.toml     each product's identity: ids, upgrade codes, packages, associations
docs/products.md            the products, their installers and migrating from the old suite
docs/academic.md            PSG Academic PDF: architecture, security, templates
```

## Building

You need Rust 1.95 or newer (`rust-toolchain` is not pinned; the workspace sets `rust-version`).
On Linux, eframe also needs the usual X11/Wayland development packages.

```sh
# PSG PDF (the general app)
cargo build --release -p psg-pdf
./target/release/psg-pdf [file.pdf]

# PSG Academic PDF
cargo build --release -p psg-academic-pdf
./target/release/psg-academic-pdf

# both, plus the CLI (the default members)
cargo build --release
```

Interface translations come from the craft-i18n catalogs, an optional build input. Without them
PSG PDF is English only and the tests that need a translated interface say they are skipped.
To embed them, check them out at the pinned commit and point the build at them:

```sh
packaging/fetch-craft-i18n.sh                 # clones storytold/craft-i18n at craft-i18n.rev into target/craft-i18n
export CRAFT_I18N_DIR="$PWD/target/craft-i18n"
```

The catalogs are written for PdfCraft; the build embeds copies with the app name replaced, so the
translated interface says PSG PDF (crates/ui-egui/build.rs).

Japanese interface text and writing Japanese into PDFs need the optional `CRAFT_FONTS_DIR` build
input, as in PdfCraft (see AGENTS.md §1.4). Without it everything else works.

The TeX tools PSG Academic PDF needs at run time are listed in the
[PSG Academic PDF guide](academic-guide.md#what-it-needs-tex).

## Two independent products

PSG PDF and PSG Academic PDF are separate products with separate installers. Each has its own
identity, recorded once in [`packaging/products.toml`](../packaging/products.toml) and checked
against every packaging file by `cargo xtask products check`. They never share or overwrite each
other's files, settings or updates. See [products.md](products.md).

| | PSG PDF | PSG Academic PDF |
|---|---|---|
| Cargo package / binary | `psg-pdf` (and `pdfcraft-cli`) | `psg-academic-pdf` |
| App ID (Linux desktop, macOS bundle) | `com.panoramascholarly.psgpdf` | `com.panoramascholarly.psgacademicpdf` |
| Windows install folder | `Panorama Scholarly Group\PSG PDF` | `Panorama Scholarly Group\PSG Academic PDF` |
| Linux package | `psg-pdf` | `psg-academic-pdf` |
| Opens | PDF (offered, never forced) | `.tex` and `.docx` manuscripts (each starts a new project) |
| Update channel | release files named `psg-pdf-…` | release files named `psg-academic-pdf-…` |
| Brand colour | deep blue `#1e4384` | academic purple `#5b2a78` ([brand.md](brand.md)) |

| Platform | PSG PDF | PSG Academic PDF |
|---|---|---|
| Windows | `psg-pdf-<v>-windows-x64.msi`, `…-portable.zip` | `psg-academic-pdf-<v>-windows-x64.msi`, `…-portable.zip` |
| macOS | `psg-pdf-<v>-macos-universal.dmg` | `psg-academic-pdf-<v>-macos-universal.dmg` |
| Linux | `psg-pdf-<v>-linux-x86_64.deb` / `.rpm` / `.AppImage` / `.tar.gz` | `psg-academic-pdf-<v>-linux-x86_64.deb` / `.rpm` / `.AppImage` / `.tar.gz` |
| Script | `packaging/<os>/package.* --product psg-pdf` | `packaging/<os>/package.* --product psg-academic-pdf` |

PSG Academic PDF's **Open in PSG PDF** finds a separately installed PSG PDF (and falls back to the
system's PDF viewer without it). Installs of the old combined **PSG PDF Suite** (0.18.x) are
migrated safely: see [products.md](products.md#migrating-from-the-suite).

## Testing

Set `CRAFT_I18N_DIR` (above) to run the interface tests that need the translations too.

```sh
cargo test -p psg-academic          # typesetting core; the TeX tests skip when TeX is missing
cargo test -p psg-academic-pdf      # academic app
cargo test -p psg-pdf               # PSG PDF app
cargo test -p pdfcraft-ui-egui      # shared interface (long)
cargo xtask layers                  # dependency layering
cargo xtask products check          # the two products' identities agree everywhere
cargo xtask assets                  # asset attribution (ATTRIBUTION.toml)
```

`.github/workflows/build.yaml` ("PSG PDF — Build & Release") is the repository's only workflow
([ci.md](ci.md)). Every pull request runs the quick checks: `cargo xtask ci` (format,
clippy, tests, cargo-deny, assets, layers) with the academic tests against a real TeX Live,
latexmk, Biber and Pandoc. Pushes to `beta` and `main`, and pull requests that change the
packaging or carry the `full-ci` label, also build both products for Linux, Windows and macOS (one
job per platform), verify signatures and checksums, install both products alone and side by side,
uninstall them one at a time, and migrate a 0.18 suite install (Windows and Linux). It publishes
only from pushes to `beta` and `main` ([releasing.md](releasing.md)).

## Releases

Pull requests go into `beta`. Every push to `beta` publishes a pre-release
(`vX.Y.Z-beta.N`) with both products' installers for macOS, Windows and Linux; merging `beta` into
`main` with a `chore(release): vX.Y.Z` commit publishes the stable release. Commit subjects are
Conventional Commits, and `CHANGELOG.md` is generated from them. See
[releasing.md](releasing.md).
