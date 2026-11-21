# PSG Academic PDF guide

PSG Academic PDF prepares the final PDF of an **accepted** article: it fills a journal template
with the manuscript and its publication metadata, typesets it with TeX on your computer (installed
for you) and shows the result beside the source. It has no submission, peer-review or editorial
workflow. For how it works inside (sandboxing, templates, what is implemented), see
[academic.md](academic.md).

## TeX: installed for you

You don't install TeX. The first time PSG Academic PDF starts, it downloads its own TeX (TinyTeX,
a small TeX Live, about 55–75 MB) in the background and adds what the templates need, including
`latexmk`, Biber and BibLaTeX, and then the Chinese and Japanese fonts the templates use (Noto Serif
and Noto Sans CJK, Traditional and Simplified Chinese, about 60 MB, OFL-1.1). Settings ▸ TeX installation (also on the Dashboard) shows the
progress. After that:

- it updates itself about once a week, and **Update TeX now** updates it at once;
- when a document uses a package it doesn't have yet, the package is installed during the
  compilation and the document is compiled again;
- if TeX Live starts a new yearly release, it reinstalls itself from the newest TinyTeX.

Until it is installed, a TeX Live or MiKTeX already on the computer is used if there is one. The
first install needs an internet connection. `pandoc` is optional (the `.docx` import) and is not
installed for you. See [academic-tex.md](academic-tex.md) for where the files
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
| **Template Library** | The journal templates as cards; the selected one is used for new projects. **Add a template (.zip)…** and **Add a template folder…** add a journal's own LaTeX template (see below). |
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
   footnotes, or one of the three converted from SciFlow, below), then **Import manuscript…** or **New project** on the Dashboard. Opening a `.tex`
   or `.docx` file with PSG Academic PDF (double-click, Open with, or a drop on the window) starts
   a new project around it from the selected template. This creates a project folder from the
   template, filled with sample metadata.
2. **Import ▸ Manuscript (.tex)…** replaces the body. If the file is a complete LaTeX document,
   only its body is kept, because the template supplies the preamble and the title block.
   **Word (.docx)…** imports a Word manuscript through the local Pandoc and reads its structure:
   the title, authors (with their affiliation marks, `王小明¹,*`), affiliations, abstract and
   keywords go into the publication metadata, whether Word's Title and Author styles were used or
   they were typed as the first paragraphs ("摘要：", "Keywords:"); a picture with a caption
   paragraph beside it ("圖 1：…", "Figure 1.") becomes a numbered figure, and a table takes its
   caption; headings that carry a typed number ("一、", "1.2") are not numbered again. The
   activity log names what it found and anything the template can't typeset as it is (characters
   its fonts lack, EMF or TIFF figures). Add bibliographies and figures with
   **Import ▸ Bibliography / figures…**.
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

`typeset` does it all in one step, from a Word or LaTeX manuscript to a PDF, with the journal's
own details (volume, DOI, dates: any field of the metadata form) from a TOML or JSON file laid
over what the manuscript gave. `--json` prints what happened for a script: the step that failed
and why, the metadata used, the TeX packages installed and every problem with its file and line.

```sh
psg-academic-pdf typeset paper.docx --template psg-article --metadata issue.toml --out article.pdf --json
```

After typesetting, `source-at` and `page-of` jump between the PDF and the source the way
double-clicking the preview and **Show in PDF** do in the window (pages count from 1, points from
the page's top-left corner):

```sh
psg-academic-pdf source-at ./my-article --page 2 --x 150 --y 300 --json   # {"file":"body.tex","line":42}
psg-academic-pdf page-of ./my-article --file body.tex --line 42 --json    # {"page":2,"y":300.4}
```

```toml
# issue.toml
journal = "Journal of Things"
volume = "7"
issue = "2"
doi = "10.1234/jot.2026.007"
received = "2026-09-01"
accepted = "2026-11-01"
```

## A journal's own template

**Template Library ▸ Add a template (.zip)…** (or **Add a template folder…**, or
`psg-academic-pdf add-template journal.zip --name "Journal of Things"`) adds the LaTeX template a
journal hands out, as it is. PSG Academic PDF finds its main file (the `.tex` with
`\documentclass` and `\begin{document}`), the engine it needs (XeLaTeX for `ctex`, `xeCJK` or
`fontspec`, otherwise pdfLaTeX) and its bibliography tool (Biber for BibLaTeX, BibTeX for
`\bibliography`). The text between the title block and the bibliography becomes the body that a
manuscript replaces, and the template's `\title`, `\author` (with `\affil`, `\address` and the
like), abstract and keywords are filled from the publication metadata. It shows what it found and
what it could not find, so check that before using the template. Added templates are kept in the
`templates` folder of the data folder; adding one with the same name replaces it.

A template whose names have spaces or non-ASCII characters loses those files (they are named in
the list of things to check), since TeX can't be relied on to read them: rename them and add the
template again. A pdfLaTeX template given a Chinese or Japanese manuscript is typeset with XeLaTeX
and `xeCJK` instead, and a `ctex` or `xeCJK` template that names no Chinese font uses the Noto CJK
faces, so the same document looks the same on every computer.

## SciFlow templates

SciFlow, an open-source (MIT) publishing platform, describes a journal's PDF in a template
configuration, `configuration.sfo.yml`, instead of LaTeX. PSG Academic PDF reads one and writes a
template on the PSG class from it, so Word manuscripts are typeset with it in one step like any
other template: the page size and margins, single- or double-sided pages, the text size, a
sans-serif or serif face, widows and orphans, the citation style (its Zotero/CSL id mapped to the
closest BibLaTeX style: `apa`, `ieee`, `chicago-note-bibliography`, `modern-language-association`,
`vancouver`, `american-chemical-society`, `nature`; others to numbered or author-year citations),
the table of contents, the journal name and article type, and the theme colour from
`metaData.schema.yml`. SciFlow's HTML, EPUB and reader settings, Handlebars headers and footers,
custom CSS and logos are left out, and each one left out is listed after adding the template.

Three of SciFlow's generic templates come built in: **SciFlow Journal Article** (A4, single-sided,
11 pt sans-serif, APA), **SciFlow Report with Contents** (the same, with a two-level table of
contents) and **SciFlow Monograph** (17 × 24 cm, double-sided, contents on its own page). To add
another, use **Add a template (.zip)…** or **Add a template folder…** (or `add-template`) on a
SciFlow template folder: when it has no LaTeX main file but has a `configuration.sfo.yml`, the
configuration is converted. The citation style can be changed afterwards in the template's
`main.tex` (`\psgbiblatexoptions`).

## Languages

The interface is in English, Simplified Chinese, Traditional Chinese, Japanese, Korean and
German. It follows the system language; **Settings** lets you choose another.

The PSG templates typeset with XeLaTeX: Latin-script text with Latin Modern, Chinese and Japanese
with Noto Serif CJK (Traditional Chinese faces, with the Simplified Chinese ones for characters
only Simplified Chinese writes). When the article's language is Chinese or Japanese, the title
block, figures, tables and references are labelled in it (摘要, 關鍵詞, 圖, 表, 參考文獻). Before
compiling, PSG Academic PDF names any text the template's engine and fonts can't set (Korean,
Cyrillic, Greek, Arabic, Hebrew, Thai or Indic text), with the line.

## Experimental

- **Word citations** inserted by Zotero, Mendeley or EndNote become LaTeX citations
  (`\parencite`/`\textcite` with BibLaTeX, `\cite` with BibTeX). Their entries are added to
  the template's bibliography file under keys like `smith2020`, and the reference list Word
  printed is left out, since the template prints its own. Citations typed as plain text, such as
  "[1]" or "(Smith, 2020)", stay as text, and so does a template without a bibliography.
