# PSG Academic PDF journal templates

Each subfolder is a template for one journal or article type. PSG Academic PDF copies the
template's files into a new project folder, writes the publication metadata into the template's
metadata file, puts the imported manuscript into its body file, and compiles its main file with
latexmk and the template's engine (XeLaTeX or pdfLaTeX).

## `template.toml`

```toml
id = "psg-article"                 # letters, digits, '-' and '_'; unique
name = "PSG Research Article"       # shown in the app
description = "…"
version = "1.0.0"
licence = "MIT OR Apache-2.0"
engine = "xelatex"                  # "xelatex" or "pdflatex"
bibliography = "biber"              # "biber", "bibtex" or "none"
main = "main.tex"                   # compiled by latexmk
body = "body.tex"                   # replaced by the imported manuscript body
metadata = "psg-metadata.tex"       # overwritten from the metadata form
files = ["main.tex", "psg-article.cls", "body.tex", "references.bib", "psg-metadata.tex"]
```

The rules:

- File names may use `A–Z a–z 0–9 . - _` and must not start with `.` or `-`. Files in
  `files` may be in subfolders (`figures/logo.pdf`, at most four deep); `main`, `body` and
  `metadata` are beside `template.toml`.
- `main`, `body` and `metadata` must be `.tex` files listed in `files`.
- The metadata file is generated. It calls the setters `\psgJournal`, `\psgISSN`, `\psgVolume`,
  `\psgIssue`, `\psgYear`, `\psgPages`, `\psgArticleNumber`, `\psgDOI`, `\psgArticleType`,
  `\psgTitle`, `\psgShortTitle`, `\psgAuthors`, `\psgAffiliations`, `\psgCorrespondence`,
  `\psgAbstract`, `\psgKeywords`, `\psgReceived`, `\psgRevised`, `\psgAccepted`,
  `\psgPublished`, `\psgLicence`, `\psgCopyright`, `\psgPdfTitle`, `\psgPdfAuthor` and
  `\psgPdfKeywords`, and uses `\psgAffMark` inside the author and affiliation lists. A template's
  class must define all of them. `psg-article.cls` is the reference. It also calls, each after a
  `\providecommand` so a template may ignore them, `\psgLanguage` (`zh-hant`, `zh-hans`, `ja`)
  and `\psgAuthorList{\psgAuthorItem{name}{1,*}…}` and
  `\psgAffiliationList{\psgAffiliationItem{1}{text}…}`, for templates that set each author and
  affiliation with its own command.
- A journal's own template, added in the app or with `psg-academic-pdf add-template`, gets these
  definitions in its main file; see `crates/academic/src/template_import.rs`.
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


## Templates converted from SciFlow

`sciflow-journal`, `sciflow-report` and `sciflow-monograph` are written by the SciFlow converter
(`crates/academic/src/sciflow.rs`) from SciFlow's generic templates in `sciflow-source/`
(`configuration.sfo.yml` and `metaData.schema.yml`, unmodified, MIT licensed by SciFlow GmbH; see
`sciflow-source/LICENSE.txt`). They share the PSG article's class, sample body and bibliography.
Don't edit them by hand: after changing the converter or those files, run
`PSG_WRITE_SCIFLOW_TEMPLATES=1 cargo test -p psg-academic --lib sciflow_templates` to write them
again; without it, that test checks they are up to date.
