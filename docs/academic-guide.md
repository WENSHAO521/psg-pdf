# PSG Academic PDF guide

PSG Academic PDF prepares the final PDF of an **accepted** article: it fills a journal template
with the manuscript and its publication metadata, typesets it with TeX on your computer (installed
for you) and shows the result beside the source. It has no submission, peer-review or editorial
workflow. For how it works inside (sandboxing, templates, what is implemented), see
[academic.md](academic.md).

## TeX: installed for you

You don't install TeX. The first time PSG Academic PDF starts, it downloads its own TeX (TinyTeX,
a small TeX Live, about 55–75 MB) in the background and adds what the templates need, including
`latexmk`, Biber and BibLaTeX. Settings ▸ TeX installation (also on the Dashboard) shows the
progress. After that:

- it updates itself about once a week, and **Update TeX now** updates it at once;
- when a document uses a package it doesn't have yet, the package is installed during the
  compilation and the document is compiled again;
- if TeX Live starts a new yearly release, it reinstalls itself from the newest TinyTeX.

Until it is installed, a TeX Live or MiKTeX already on the computer is used if there is one. The
first install needs an internet connection. `pandoc` is optional (the experimental `.docx`
import) and is not installed for you. See [academic-tex.md](academic-tex.md) for where the files
go and how to remove them.

Use `psg-academic-pdf install-tex` to install TeX, `update-tex` to update it, and
`check-tex` to check the tools. Run `psg-academic-pdf repair-biber` to reinstall Biber and
biblatex in PSG's managed TeX.

## The workspace

The window has a toolbar with the four main actions — **Import**, **Auto Typeset**, **Preview**
and **Export** — a navigation rail on the left, and a status bar that shows the TeX installation,
the engine (always pdfLaTeX) and what happened last. Notifications appear in the bottom-right
corner; warnings and errors stay until you dismiss them.

| Area | What it is for |
|---|---|
| **Dashboard** | Start a project (import a manuscript, new project, open), recent projects, the current project at a glance, and the TeX installation. |
| **Manuscript** | The source editor beside the PDF (drag the divider to any ratio; it is remembered), the article outline, title and abstract, authors and affiliations, publication details, and references. Use the page number and previous and next page buttons to move through the preview, and Back and Forward to retrace jumps. |
| **Template Library** | The journal templates as cards; the selected one is used for new projects. Importing templates from ZIP files and checking compatibility are planned and shown as such. |
| **Typesetting** | The task centre: run or cancel, this session's runs, the project's time limit, and the latexmk output, TeX log and activity log beside the PDF. |
| **Problems** | Errors and warnings from the checks made before typesetting and from the last run, with hints and **Go to line**. |
| **Export** | A checklist for the current revision, the PDF's details, **Export PDF…** and **Open in PSG PDF**. After editing the manuscript or its publication details, typeset again before exporting. Projects typeset by an older version require one new typeset to verify the PDF. |
| **Settings** | Language, theme (follow system, light, dark), interface size, TeX installation, workspace folder, data folder, updates and About. |

Shortcuts: Ctrl+S / ⌘S saves, Ctrl+R / ⌘R typesets, Ctrl+E / ⌘E opens Export, Ctrl+O / ⌘O opens a
project, and Ctrl+1 … 7 / ⌘1 … 7 switch areas.

Use **Choose workspace…** on the Dashboard or in Settings to pick where new projects go. The
choice applies to new projects and manuscript imports. **New in folder…** can also create one
project in a different folder. It uses the selected folder if empty, or creates a project
subfolder when there are already files there. If saving fails when changing projects or closing
the window, the current draft stays open so the save can be retried.

## Making an article

1. Choose a journal template in the **Template Library** (**PSG Research Article**, numbered
   citations, or **PSG Humanities and Social Sciences Article**, author-year citations and
   footnotes), then **Import manuscript…** or **New project** on the Dashboard. Opening a `.tex`
   or `.docx` file with PSG Academic PDF (double-click, Open with, or a drop on the window) starts
   a new project around it from the selected template. This creates a project folder from the
   template, filled with sample metadata.
2. **Import ▸ Manuscript (.tex)…** replaces the body. If the file is a complete LaTeX document,
   only its body is kept, because the template supplies the preamble and the title block. You can
   also try **Word (.docx)…**, an experimental import through the local Pandoc; afterwards the
   activity log names anything pdfLaTeX can't typeset as it is (characters its fonts lack, EMF or
   TIFF figures). Add bibliographies and figures with **Import ▸ Bibliography / figures…**.
3. Fill in the publication details under **Manuscript**: **Title & abstract**, **Authors**
   (with affiliations, ORCID iDs and the corresponding author) and **Publication** (journal,
   volume, issue, pages, DOI, dates, licence and copyright). Problems are shown above the forms.
   They are saved with the project and before every run.
4. Press **Auto Typeset**. latexmk runs pdfLaTeX and Biber in the project folder, with a time limit
   that you can cancel; on Windows it runs without a console window, and its output appears in
   **Typesetting**. When it succeeds the PDF appears beside the manuscript, drawn by the shared
   PdfCraft renderer. Errors appear under **Problems** with file and line and, for the common
   ones, what usually causes them and how to fix them.
5. **Export** checks the result; **Export PDF…** saves it and **Open in PSG PDF** opens it in the
   general app for review.

The same steps work without a window, for scripts. `psg-academic-pdf check <project>` runs the
checks made before compiling and exits with status 1 when one is an error:

```sh
psg-academic-pdf templates
psg-academic-pdf new ./my-article --template psg-article
psg-academic-pdf compile ./my-article --manuscript paper.tex --timeout 120 --out article.pdf
```

## Languages

The interface is in English, Simplified Chinese, Traditional Chinese, Japanese, Korean and
German. It follows the system language; **Settings** lets you choose another.

The pdfLaTeX templates typeset Latin-script text. Before compiling, PSG Academic PDF names any
Chinese, Japanese, Korean, Cyrillic, Greek, Arabic, Hebrew, Thai or Indic text it finds, with the
line, because those scripts need XeLaTeX or LuaLaTeX, which are not supported yet.

## Experimental

- **Word (.docx) import** goes through the local Pandoc (`--sandbox`). It converts text,
  headings, lists, tables, equations and images. It does not map Word styles or Word citations,
  so check the result before typesetting.
