# PSG Academic PDF

PSG Academic PDF typesets articles that a journal has already accepted. It turns the manuscript
into a finished PDF that follows the journal's template and carries the formal publication
metadata. Submission, peer review and editorial management are out of scope.

## Architecture

```text
apps/psg-academic-pdf      egui/eframe window + headless commands (cli.rs)
  ├── app.rs               project, template, metadata, settings, compile, preview, logs, export
  ├── preview.rs           renders the PDF with pdfcraft-render on a worker thread
  └── cli.rs               check-tex · templates · new · compile
crates/academic            (psg-academic, no UI, layer 4 in xtask/src/layers.rs)
  ├── tex.rs               finds pdflatex, latexmk, biber, pandoc on the PATH
  ├── process.rs           runs a program with a time limit, cancel flag, bounded capture,
  │                        and kills the whole process tree
  ├── compile.rs           the sandboxed latexmk command and CompileReport
  ├── diagnostics.rs       TeX .log and Biber .blg → errors/warnings with file and line
  ├── template.rs          template.toml manifests; built-in and on-disk templates
  ├── metadata.rs          PublicationMetadata → psg-metadata.tex (every value escaped)
  ├── project.rs           project folder, manuscript/asset import, export
  └── pandoc.rs            ManuscriptImporter trait; PandocImporter (DOCX, experimental)
```

Both programs use the PdfCraft engine. PSG Academic PDF links `pdfcraft-render` for its preview.
**Open in PSG PDF** passes the compiled PDF to the general app.

## Compilation and security

Every compilation runs in the project folder and only there:

```text
latexmk -norc -pdf -interaction=nonstopmode -halt-on-error -file-line-error
        "-pdflatex=pdflatex -no-shell-escape -interaction=nonstopmode -halt-on-error -file-line-error %O %S"
        main.tex
```

| Measure | What it prevents |
|---|---|
| `latexmk -norc` | A `latexmkrc` in the project (Perl code) is never read. |
| `-no-shell-escape`, `shell_escape=f` | `\write18` and `\ShellEscape` cannot run commands. |
| `openin_any=p`, `openout_any=p` | TeX may not read or write files by absolute path, in parent directories, or dot files. A document can't read `~/.ssh/…` or write outside the project. |
| `TEXINPUTS`, `TEXMFHOME`, `TEXMFCNF`, `PERL5OPT`, … removed | The user's environment can't add search paths or change TeX/Perl configuration. |
| Main file must be a plain name (`[A-Za-z0-9._-]`) inside the project | No option injection into latexmk, no path tricks. |
| Time limit (default 180 s, 5–3600 s) and Cancel | The whole process group (Unix) or process tree (Windows, `taskkill /T`) is killed. |
| Standard input closed | TeX can't stop and wait for input. |
| Output capture capped at 4 MB per stream; logs read up to 16 MB | A runaway log can't exhaust memory. |
| Imported files are copied in under sanitized names; template files can't be overwritten | No `../` names, no dot files such as `.latexmkrc`. |
| Metadata values are escaped for TeX | A title such as `\input{/etc/passwd}` is typeset as text. |

`crates/academic/tests/tex.rs` checks these protections against the real TeX installation. In
those tests shell escape is refused, an absolute `\input` outside the project fails, an endless
macro is stopped at the time limit, and an error is reported with its file and line.

The `openin_any`/`openout_any` limits are kpathsea settings. **TeX Live honours them.**
MiKTeX has its own security settings and does not honour them. With MiKTeX, shell escape is still
off, but restricting file reads depends on MiKTeX's configuration. TeX Live is the recommended
installation.

## Templates

A template is a folder with a `template.toml` manifest; see `templates/academic/README.md`.
`psg-article` is compiled into the program, so the app always has a working template. More
templates are read from:

1. the folder named by `PSG_TEMPLATES_DIR`;
2. `templates/academic/` beside the executable;
3. `../share/psg-academic-pdf/templates/` relative to the executable (Linux packages).

A broken template is skipped and listed in the Activity tab or on standard error.

## Status (phase one)

Implemented and tested:

- TeX detection (pdflatex, latexmk, biber, pandoc) with versions.
- Sandboxed latexmk/pdfLaTeX/Biber compilation with time limit, cancel, error/warning
  diagnostics, latexmk output and TeX log views.
- PDF preview through the shared PdfCraft renderer; export; hand-off to PSG PDF.
- Template manifest format, the built-in `psg-article` template, on-disk template discovery.
- Publication metadata form, validation (DOI, ISSN and ORCID check digits, ISO dates) and
  escaping into `psg-metadata.tex`.
- Projects: create, open, import a LaTeX manuscript, add bibliography and figure files.
- Headless commands: `check-tex`, `templates`, `new`, `compile`.

Experimental:

- DOCX import through Pandoc (`--sandbox`). It converts text, headings, lists, tables, equations
  and images. It does not map Word styles or Word citations.

Not implemented yet:

- XeLaTeX/LuaLaTeX (CJK manuscripts); only pdfLaTeX is supported.
- Journal-specific templates beyond the sample; template editing in the app.
- JATS XML, Crossref deposit metadata, PDF/A output.
- Jumping from a problem to the source line; an in-app source editor.
- A translated (e.g. Chinese) interface for PSG Academic PDF; it is in English.
- Installers for PSG Academic PDF; PSG PDF's packaging scripts were renamed but not run.
