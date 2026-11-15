# PSG Academic PDF guide

PSG Academic PDF prepares the final PDF of an **accepted** article: it fills a journal template
with the manuscript and its publication metadata, typesets it with the TeX installation on your
computer and shows the result beside the source. It has no submission, peer-review or editorial
workflow. For how it works inside (sandboxing, templates, what is implemented), see
[academic.md](academic.md).

## What it needs: TeX

PSG Academic PDF uses the TeX installation that is already on the machine and never downloads one.
It needs `pdflatex`, `latexmk` and `biber` on the `PATH`. `pandoc` is optional and is used only for
the experimental `.docx` import.

| System | Install |
|---|---|
| Debian/Ubuntu | `sudo apt install texlive-latex-base texlive-latex-recommended texlive-latex-extra texlive-fonts-recommended lmodern latexmk biber texlive-bibtex-extra` |
| macOS | MacTeX (includes latexmk and biber) |
| Windows | TeX Live (includes latexmk and biber). MiKTeX also works, but see the security note in docs/academic.md. |

Run `psg-academic-pdf check-tex` to see what was found.

## Making an article

1. Choose a journal template (**PSG Research Article**, numbered citations, or **PSG Humanities
   and Social Sciences Article**, author-year citations and footnotes) and press **New project**.
   Opening a `.tex` or `.docx` file with PSG Academic PDF (double-click, Open with, or a drop on
   the window) starts a new project around it from the selected template. This creates a project folder from the
   template, filled with sample metadata.
2. Import the manuscript with **Manuscript (.tex)…**. If the file is a complete LaTeX document, only
   its body is kept, because the template supplies the preamble and the title block. You can
   also try **Word (.docx)…**, an experimental import through the local Pandoc; afterwards the
   Activity log names anything pdfLaTeX can't typeset as it is (characters its fonts lack, EMF or
   TIFF figures). Add
   bibliographies and figures with **Bibliography / figures…**.
3. Fill in the **publication metadata**: journal, volume, issue, pages, DOI, title, authors,
   affiliations, ORCID iDs, abstract, keywords, dates, licence and copyright. Problems are shown
   under the form.
4. Press **Compile**. latexmk runs pdfLaTeX and Biber in the project folder, with a time limit
   that you can cancel. When it succeeds the PDF opens in **PDF preview**, drawn by the shared
   PdfCraft renderer. Errors appear under **Problems** with file and line and, for the common
   ones, what usually causes them and how to fix them; the full
   latexmk output and TeX log have their own tabs.
5. **Export PDF…** saves the result. **Open in PSG PDF** opens it in the general app for review.

The same steps work without a window, for scripts. `psg-academic-pdf check <project>` runs the
checks made before compiling and exits with status 1 when one is an error:

```sh
psg-academic-pdf templates
psg-academic-pdf new ./my-article --template psg-article
psg-academic-pdf compile ./my-article --manuscript paper.tex --timeout 120 --out article.pdf
```

## Languages

The interface is in English, Simplified Chinese, Traditional Chinese, Japanese, Korean and
German. It follows the system language; the side panel lets you choose another.

The pdfLaTeX templates typeset Latin-script text. Before compiling, PSG Academic PDF names any
Chinese, Japanese, Korean, Cyrillic, Greek, Arabic, Hebrew, Thai or Indic text it finds, with the
line, because those scripts need XeLaTeX or LuaLaTeX, which are not supported yet.

## Experimental

- **Word (.docx) import** goes through the local Pandoc (`--sandbox`). It converts text,
  headings, lists, tables, equations and images. It does not map Word styles or Word citations,
  so check the result before typesetting.
