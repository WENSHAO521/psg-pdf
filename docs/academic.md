# PSG Academic PDF

PSG Academic PDF typesets articles that a journal has already accepted. It turns the manuscript
into a finished PDF that follows the journal's template and carries the formal publication
metadata. Submission, peer review and editorial management are out of scope.

## Architecture

```text
apps/psg-academic-pdf      egui/eframe window + headless commands (cli.rs)
  ├── app.rs               project, template, metadata, settings, compile, preview, logs, export
  ├── preview.rs           renders the PDF with pdfcraft-render on a worker thread
  ├── tex_setup.rs         downloads and installs/updates PSG's own TeX in the background
  └── cli.rs               check-tex · install-tex · update-tex · repair-biber · templates · new · compile
crates/academic            (psg-academic, no UI, layer 4 in xtask/src/layers.rs)
  ├── managed.rs           PSG's own TeX (TinyTeX): layout, unpacking, tlmgr updates,
  │                        installing the packages a document is missing (academic-tex.md)
  ├── tex.rs               finds pdflatex, latexmk, biber (PSG's TeX first, then the PATH), pandoc
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

A broken template is skipped and listed in the Template Library and the activity log, or on standard error.

## The data folder

Projects made with New project, the recent projects list and the app's settings (language,
theme, update channel, window layout) live in the data folder: eframe's per-user folder for PSG
Academic PDF by default, or `PSGAcademicPDFData` beside the exe in the portable zip. Settings ▸
Data folder shows it and can move it:

- **Change…** picks a folder. An empty folder is used as it is; a folder that already holds
  files gets a `PSG Academic PDF` folder inside it. A folder inside the current one is refused.
- The move copies everything and checks each file's size before changing anything. A failure
  (a full disk, a drive pulled out) removes what was copied and leaves the old folder as it was.
  A project that already exists at the new place stops the move before anything is copied; only
  the app's own settings files there are replaced.
- Then the new place is written to `data-location.txt` in the default folder and the old copies
  are removed (folders only when empty). The open project is saved first and opened again from
  the new place; the recent projects list follows the projects that moved.
- **Use the default folder** moves everything back and removes `data-location.txt`.
- `data-location.txt` lives in the default folder, which installers and updates never touch, so
  the choice survives updates. If the chosen folder can't be used at start-up (a disconnected
  drive), that run uses the default folder and the activity log says why; the choice is kept.
- The log stays in the old folder for the rest of that run; the next run logs in the new one.
  Projects made with "New project in folder…" or opened from elsewhere are not moved.

## Start-up and the log file

Each run writes `logs/psg-academic-pdf.log` in the data folder (eframe's per-user folder for
PSG Academic PDF, or `PSGAcademicPDFData` beside the exe in the portable zip); the previous run's
log is kept as `psg-academic-pdf.1.log`. Panics are written there too, with a backtrace.

The window starts with the GPU renderer (wgpu) under the same safeguards as PSG PDF: on Windows
only Direct3D 12 and OpenGL are tried, never Vulkan, and the device gets the adapter's real
texture limit. If wgpu can't start, the window opens again with OpenGL. If no renderer starts, a
message box says why and where the log is. `PSG_RENDERER=gl` starts with OpenGL directly
(`wgpu` uses wgpu only, without the fallback).

## Status

Implemented and tested:

- TeX detection (pdflatex, latexmk, biber, pandoc) with versions.
- Sandboxed latexmk/pdfLaTeX/Biber compilation with time limit, cancel, error/warning
  diagnostics, latexmk output and TeX log views.
- PDF preview through the shared PdfCraft renderer; export; hand-off to PSG PDF.
- Template manifest format, the built-in `psg-article` template, on-disk template discovery.
- Publication metadata form, validation (DOI, ISSN and ORCID check digits, ISO dates) and
  escaping into `psg-metadata.tex`.
- Projects: create, open, import a LaTeX manuscript, add bibliography and figure files.
- Headless commands: `check-tex`, `templates`, `new`, `compile`, and `check` (the checks made
  before compiling, exit status 1 on an error).

Phase two:

- **Manuscript ▸ Source** (phase four: Editor, Split or PDF, with a draggable, remembered split and
  an outline of the `\section` headings): the manuscript body and the project's `.bib` files in an editor with
  line numbers, next to the PDF. Lines with a problem are marked; Ctrl+S / ⌘S saves, and the
  file is saved before every compilation.
- **Problems** lists the checks made before compiling and the compiler's diagnostics; "Go to
  line" opens the file at the line.
- **Before compiling** (`Project::preflight`, `crates/academic/src/scripts.rs`): Chinese,
  Japanese, Korean, Cyrillic, Greek, Arabic, Hebrew, Thai and Indic text in the body or the
  metadata is named with its line, with what the limitation is and what to do. The pdfLaTeX
  templates load T1 Latin Modern only, so these scripts cannot be typeset; this replaces TeX's
  "Unicode character … not set up for use with LaTeX".
- **References** (`crates/academic/src/bib.rs`): the `.bib` entries, and checks for duplicate
  keys, missing required fields, malformed DOIs and years, citations without an entry and
  entries nothing cites.
- **Interface languages**: English, Simplified and Traditional Chinese, Japanese, Korean and
  German (`i18n/psg-academic/<code>.tsv`), following the system language or the choice in
  Settings. Chinese, Japanese and Korean interface text uses the craft-fonts faces, so a build
  without `CRAFT_FONTS_DIR` shows boxes for them (release builds set it). Messages produced by
  TeX, and the generated details of the checks, stay in English; their hints are translated.

Experimental:

- DOCX import through Pandoc (`--sandbox`). It converts text, headings, lists, tables, equations
  and images. It does not map Word styles or Word citations.

Not implemented yet:

- XeLaTeX/LuaLaTeX (CJK manuscripts); only pdfLaTeX is supported.
- Journal-specific templates beyond the sample; template editing in the app.
- JATS XML, Crossref deposit metadata, PDF/A output.
- SyncTeX (clicking the PDF to reach the source, or the reverse).
