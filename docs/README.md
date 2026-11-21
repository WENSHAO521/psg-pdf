# PSG PDF and PSG Academic PDF documentation

Documentation for the two products of Panorama Scholarly Group. For an overview and downloads,
start at the [project page](../README.md). The product website is [`index.html`](index.html) in this
folder, published at [pdf.panorama-sg.com](https://pdf.panorama-sg.com/) with GitHub Pages from `docs/`
on `main` (`CNAME` holds the domain); it is plain HTML and CSS with no build
step, and `.nojekyll` keeps Pages from processing these notes.

## Using the products

| Page | What it covers |
|---|---|
| [PSG Academic PDF guide](academic-guide.md) | What it needs (TeX), making an article step by step, the command-line form, languages |
| [Products](products.md) | Installers, install folders, updates, and migrating from the old PSG PDF Suite |
| [Interface language](localization.md) | Choosing PSG PDF's interface language |
| [Batch conversion, OCR and compression](cli-batch.md) | Running `pdfcraft-cli batch` over many files |
| [Windows printing](windows-printing.md) | How PSG PDF prints on Windows |

## Brand

| Page | What it covers |
|---|---|
| [PSG brand and design system](brand.md) | Names, logos, colours, type, shapes, the Beta badge |
| [Logo master](../assets/brand/README.md) | The editable SVG and how every icon is generated from it |

## Building, releasing and contributing

| Page | What it covers |
|---|---|
| [Building and testing](building.md) | Repository layout, building both products, optional inputs, tests |
| [PSG Academic PDF internals](academic.md) | Architecture, sandboxed compilation, templates, status |
| [CI/CD](ci.md) | The single `build.yaml` workflow: checks, builds, install tests, release |
| [Releasing](releasing.md) | Branches, commit messages, beta and stable releases, signing |
| [Development notes](development.md) | Environment variables, logs, renderer fallbacks |
| [Contributor credits](contributors.md) | How the About window credits contributors |
| [Licensing self-check](licensing-self-check.md) | Checking the source's licences offline |
| [AGENTS.md](../AGENTS.md) | Rules for every contributor and agent |

## The shared PDF engine

Both products share the PDF engine of PdfCraft, the
open-source project they are based on. These notes come from it and describe the engine and PSG
PDF's interface in depth; they keep PdfCraft's name where they describe its design and history.

- [Architecture](architecture.md), [MCP conventions](mcp.md), [page display](page-display.md)
- Status: [gaps](gaps.md), [roadmap](roadmap.md), [feature parity](target-app-parity.md),
  [UI parity](ui-parity.md), [file formats](file-format-parity.md),
  [PDF specification](pdf-spec-parity.md), [localization](localization-parity.md),
  [hardware](hardware-parity.md)
- Editing: [existing text](editing-existing-text.md), [images](editing-images.md),
  [several objects](editing-multiple-objects.md), [document properties](document-properties-sessions.md),
  [word count](word-count.md), [form performance](form-performance.md)
- Platform: [Windows renderer start-up](windows-renderer-startup.md),
  [egui 0.37 migration](egui-037-migration-plan.md), [integration notes](integration-20261030.md)
- [Workflow research](research/pdf-workflows/README.md)
