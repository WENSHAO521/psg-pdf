<p align="center">
  <img src="assets/app-icon/hicolor/128x128/apps/com.panoramascholarly.psgpdf.png" width="96" height="96" alt="PSG PDF icon">
  &nbsp;&nbsp;
  <img src="assets/app-icon-academic/hicolor/128x128/apps/com.panoramascholarly.psgacademicpdf.png" width="96" height="96" alt="PSG Academic PDF icon">
</p>

<h1 align="center">PSG PDF Suite</h1>

<p align="center">
  <b>PSG PDF</b> and <b>PSG Academic PDF</b>, native desktop PDF software from Panorama Scholarly Group.<br>
  Rust · egui/eframe · offline · Windows, macOS and Linux
</p>

The suite has two separate desktop programs that share one PDF engine. That engine is
[PdfCraft](https://github.com/storytold/pdfcraft) (MIT OR Apache-2.0), kept in this repository's
`crates/` and not duplicated.

| Program | Binary | What it does |
|---|---|---|
| **PSG PDF** | `psg-pdf` (`apps/psg-pdf`) | The general PDF app. It keeps every PdfCraft feature: reading, annotation, editing, organizing pages, combining, splitting, converting, forms, signing, redaction, OCR and printing. |
| **PSG Academic PDF** | `psg-academic-pdf` (`apps/psg-academic-pdf`) | Production typesetting for **accepted** articles. It combines a journal template, publication metadata and the manuscript, compiles them with **pdfLaTeX** through **latexmk** and **BibLaTeX/Biber** from the local TeX Live, then lets you preview and export the PDF. It has no submission, peer-review or editorial workflow. |

Both programs run fully offline. Neither one uploads anything, and PSG PDF does not check for
updates at startup.

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
templates/academic/         journal templates (psg-article is compiled into the app)
assets/app-icon/            PSG PDF icon (original; packaging/psg-icons.py)
assets/app-icon-academic/   PSG Academic PDF icon (original; packaging/psg-icons.py)
docs/academic.md            PSG Academic PDF: architecture, security, templates
docs/upstream/              the upstream PdfCraft README, kept for reference
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

Japanese interface text and writing Japanese into PDFs need the optional `CRAFT_FONTS_DIR` build
input, as in PdfCraft (see AGENTS.md §1.4). Without it everything else works.

### TeX for PSG Academic PDF

PSG Academic PDF uses the TeX installation that is already on the machine and never downloads one.
It needs `pdflatex`, `latexmk` and `biber` on the `PATH`. `pandoc` is optional and is used only for
the experimental `.docx` import.

| System | Install |
|---|---|
| Debian/Ubuntu | `sudo apt install texlive-latex-base texlive-latex-recommended texlive-latex-extra texlive-fonts-recommended lmodern latexmk biber texlive-bibtex-extra` |
| macOS | MacTeX (includes latexmk and biber) |
| Windows | TeX Live (includes latexmk and biber). MiKTeX also works, but see the security note in docs/academic.md. |

Run `psg-academic-pdf check-tex` to see what was found.

## Using PSG Academic PDF

1. Choose a journal template and press **New project**. This creates a project folder from the
   template, filled with sample metadata.
2. Import the manuscript with **Manuscript (.tex)…**. If the file is a complete LaTeX document, only
   its body is kept, because the template supplies the preamble and the title block. You can
   also try **Word (.docx)…**, an experimental import through the local Pandoc. Add
   bibliographies and figures with **Bibliography / figures…**.
3. Fill in the **publication metadata**: journal, volume, issue, pages, DOI, title, authors,
   affiliations, ORCID iDs, abstract, keywords, dates, licence and copyright. Problems are shown
   under the form.
4. Press **▶ Compile**. latexmk runs pdfLaTeX and Biber in the project folder, with a time limit
   that you can cancel. When it succeeds the PDF opens in **PDF preview**, drawn by the shared
   PdfCraft renderer. Errors appear under **Problems** with file and line, and the full
   latexmk output and TeX log have their own tabs.
5. **Export PDF…** saves the result. **Open in PSG PDF** opens it in the general app for review.

The same steps work without a window, for scripts:

```sh
psg-academic-pdf templates
psg-academic-pdf new ./my-article --template psg-article
psg-academic-pdf compile ./my-article --manuscript paper.tex --timeout 120 --out article.pdf
```

## Testing

```sh
cargo test -p psg-academic          # typesetting core; the TeX tests skip when TeX is missing
cargo test -p psg-academic-pdf      # academic app
cargo test -p psg-pdf               # PSG PDF app
cargo test -p pdfcraft-ui-egui      # shared interface (long)
cargo xtask layers                  # dependency layering
cargo xtask assets                  # asset attribution (ATTRIBUTION.toml)
```

## Licence and attribution

The code is licensed MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE), the same as PdfCraft. See
NOTICE. PSG PDF is a modified version of PdfCraft by the ArtCraft team. As PdfCraft's brand terms
require of modified versions, the ArtCraft name, wordmark and logos have been removed. Every
asset is listed in ATTRIBUTION.toml, and ATTRIBUTION.md is generated from it.
