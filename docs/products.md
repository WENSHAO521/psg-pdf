# PSG products

PSG PDF is built from this Rust workspace on the PdfCraft engine in `crates/`, with its own
installers, identity, updates and uninstaller. Until 0.18 it shipped together with PSG Academic PDF
as the **PSG PDF Suite**, one package per platform; from 0.19.0 on the two shipped separately.

**PSG Academic PDF is retired** (2026-11). It is no longer built, packaged, install-tested or
released, and copies already installed get no more updates: its in-app updater finds no newer
`psg-academic-pdf-*` files. Its identities below stay reserved, so no future product reuses them.

## Where each product's identity lives

[`packaging/products.toml`](../packaging/products.toml) holds every identity value: names, app
ids, Windows upgrade codes, install folders and ProgIDs, Linux package names, release file stems,
icons, brand colours, file associations and update channels. Packaging scripts and installer
sources use those values, and `cargo xtask products check` (part of `cargo xtask ci`, and the
first job of the release workflow) fails when any file disagrees with the manifest or when two
products share a value that must be unique.

```sh
cargo xtask products check                      # every packaging file agrees with the manifest
cargo xtask products matrix                     # the build workflow's platform matrix (every product per leg)
cargo xtask products ids                        # every product id
cargo xtask products get psg-pdf nfpm           # one value, for scripts
```

Changing an identity value breaks upgrades of installed copies: an MSI with a new UpgradeCode
installs beside the old one instead of replacing it, a renamed Linux package doesn't upgrade the
old one, and the in-app updater stops finding releases under a new stem. Don't, unless the change
comes with a migration like the one below.

| | PSG PDF | PSG Academic PDF (retired, reserved) |
|---|---|---|
| Programs | `psg-pdf`, `pdfcraft-cli` | `psg-academic-pdf` |
| App id (Linux, macOS bundle id) | `com.panoramascholarly.psgpdf` | `com.panoramascholarly.psgacademicpdf` |
| Windows UpgradeCode | `C984AB08-…` (the suite's, continued) | `B43E2C11-…` (its own) |
| Windows install folder | `Panorama Scholarly Group\PSG PDF` | `Panorama Scholarly Group\PSG Academic PDF` |
| Windows ProgID | `PSGPDF.Document` (`.pdf`) | `PSGAcademicPDF.Manuscript` (`.tex`, `.docx`) |
| macOS | `PSG PDF.app` | `PSG Academic PDF.app` |
| Linux package | `psg-pdf` (the suite's name, continued) | `psg-academic-pdf` |
| Release files | `psg-pdf-<version>-<os>-<arch>.<ext>` | `psg-academic-pdf-<version>-<os>-<arch>.<ext>` |
| Data | its own settings, logs and recovery folders | its own window state and projects folder |

File associations are offered, never forced: each installer registers its product under "Open
with" and leaves the default program alone.

## Installers

| Platform | Built by | PSG PDF gets |
|---|---|---|
| Windows | `packaging/windows/package.ps1 -Product <id>` | a dual-scope MSI (per user or per machine) and a portable zip |
| macOS | `packaging/macos/package.sh --product <id>` | a DMG with the one app and an Applications link |
| Linux | `packaging/linux/package.sh --product <id>` | deb, rpm, AppImage (with zsync delta updates) and tar.gz |

The portable zip keeps everything beside the program (`portable.txt` switches portable mode on),
in `PSGPDFData`.

## Updates

PSG PDF publishes GitHub releases (stable `vX.Y.Z` from `main`, beta `vX.Y.Z-beta.N` from `beta`;
see [releasing.md](releasing.md)). The in-app updater (craft-update) only takes files whose name
starts with its product's stem followed by a version (`psg-pdf-*`), and the AppImage carries the
same zsync update pattern. Older releases also hold PSG Academic PDF's files under their own stem.
Each release lists its files' checksums in `SHA256SUMS.txt` (which the updaters read) and per product in
`SHA256SUMS-<product>.txt`.

## Migrating from the suite

Every suite release is below 0.19.0 (`legacy_suite.split_version`); every independent release is
at or above it, so packaging rules can tell them apart by version.

**Windows.** The new PSG PDF MSI keeps the suite's UpgradeCode, so installing it, or updating from
the suite's own updater, replaces the suite like any upgrade: settings stay, and the suite's copy of
PSG Academic PDF is removed with it.

**Linux.** The suite's deb and rpm were named `psg-pdf` and also installed PSG Academic PDF's files.
The new `psg-pdf` package upgrades it in place and removes those files.

**macOS.** The suite DMG held PSG PDF.app under the same bundle name and id the new DMG uses, so
dragging the new app to Applications replaces the suite's copy. There is nothing else to migrate.

**Data.** Projects, settings and logs are per product and were already separate in the suite, so
they are kept.

## What the release workflow checks

`.github/workflows/build.yaml` ([ci.md](ci.md)), in every full run (pushes to `beta` and `main`,
pull requests with the `full-ci` label or packaging changes, manual `full` and `release-check`):

- the plan job: the identity check (`cargo xtask products check`) and the build matrix, one job
  per platform that builds every product in turn;
- each build: the products' programs start, every file is named after exactly one product and goes
  into that product's own artifact, signatures verify (Authenticode on Windows; codesign, and
  Gatekeeper with a Developer ID, on macOS), and a SHA-256 list travels with the files;
- on the same runner: PSG PDF installed, run and uninstalled (`packaging/<os>/test-install.*`: deb
  on Ubuntu, rpm in a Fedora container, MSI, DMG); on Windows and Linux also a migration from the last suite release
  (`LEGACY_SUITE_TAG`). The artifacts are uploaded only once this passed;
- the release: only after the gate and the Linux and Windows install tests (and, for a stable
  release, macOS) pass.

A beta publishes what built when the macOS runners don't start; a stable release needs every
platform.
