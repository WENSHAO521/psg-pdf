# PSG Academic PDF journal templates

Each subfolder is a template for one journal or article type. PSG Academic PDF copies the
template's files into a new project folder, writes the publication metadata into the template's
metadata file, puts the imported manuscript into its body file, and compiles its main file with
latexmk and pdfLaTeX.

## `template.toml`

```toml
id = "psg-article"                 # letters, digits, '-' and '_'; unique
name = "PSG Research Article"       # shown in the app
description = "…"
version = "1.0.0"
licence = "MIT OR Apache-2.0"
engine = "pdflatex"                 # the only engine in phase one
bibliography = "biber"              # "biber" or "none"
main = "main.tex"                   # compiled by latexmk
body = "body.tex"                   # replaced by the imported manuscript body
metadata = "psg-metadata.tex"       # overwritten from the metadata form
files = ["main.tex", "psg-article.cls", "body.tex", "references.bib", "psg-metadata.tex"]
```

The rules:

- File names are plain names in the template folder. They may use `A–Z a–z 0–9 . - _`, must not
  start with `.` or `-`, and may not contain subfolders.
- `main`, `body` and `metadata` must be `.tex` files listed in `files`.
- The metadata file is generated. It calls the setters `\psgJournal`, `\psgISSN`, `\psgVolume`,
  `\psgIssue`, `\psgYear`, `\psgPages`, `\psgArticleNumber`, `\psgDOI`, `\psgArticleType`,
  `\psgTitle`, `\psgShortTitle`, `\psgAuthors`, `\psgAffiliations`, `\psgCorrespondence`,
  `\psgAbstract`, `\psgKeywords`, `\psgReceived`, `\psgRevised`, `\psgAccepted`,
  `\psgPublished`, `\psgLicence`, `\psgCopyright`, `\psgPdfTitle`, `\psgPdfAuthor` and
  `\psgPdfKeywords`, and uses `\psgAffMark` inside the author and affiliation lists. A template's
  class must define all of them. `psg-article.cls` is the reference.
- A template must compile offline with a standard TeX Live and must not need shell escape.
- Add every template file to `ATTRIBUTION.toml`. A template must be original or openly
  licensed (AGENTS.md §1.2).

## `psg-article`

The PSG Research Article is a single-column A4 research article. It uses Latin Modern (T1),
microtype, amsmath, graphicx, booktabs, longtable, xcolor, caption, fancyhdr, hyperref, and
BibLaTeX (`numeric-comp`) with Biber. Its files are compiled into PSG Academic PDF
(`crates/academic/src/template.rs`), like `psg-humanities`'s. The test
`the_builtin_templates_match_their_directories` keeps the built-in copies and these folders in step.

## `psg-humanities`

The PSG Humanities and Social Sciences Article: the same `psg-article.cls` with the `authoryear`
class option, which switches BibLaTeX to author-year citations (`\textcite`, `\parencite`,
`authoryear-comp`, references sorted by author and year). The sample body shows footnotes, block
quotations and compressed citation lists. Its copy of `psg-article.cls` must stay identical to
`psg-article`'s (a test checks it), so a fix to the class goes into both.

## Word manuscripts

`psg-article.cls` defines the commands Pandoc's LaTeX writer uses in a converted `.docx`
(`\tightlist`, `\passthrough`, `\pandocbounded`, and `\st`/`\ul` when TeX Live has no `soul`),
so a converted body compiles in any template built on it. A template with its own class must
define them too.

