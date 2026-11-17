# PSG Academic PDF's own TeX

PSG Academic PDF installs and maintains its own TeX, so nobody has to install or find one. The
code is `crates/academic/src/managed.rs` (layout, unpacking, `tlmgr`) and
`apps/psg-academic-pdf/src/tex_setup.rs` (download, background jobs).

## What is installed, from where

- **TinyTeX-1** from the newest release of
  [rstudio/tinytex-releases](https://github.com/rstudio/tinytex-releases): TeX Live's
  infrastructure with about a hundred common LaTeX packages.
  - Windows: `TinyTeX-1-windows.exe` (a self-extracting archive, run with `-y -o<folder>`).
    TeX Live's Windows programs are x86-64; Windows on Arm runs them emulated.
  - macOS: `TinyTeX-1-darwin.tar.xz`. Linux: `TinyTeX-1-linux-x86_64.tar.xz` or `-arm64`.
    Unpacked with the system's `tar`.
- The download is checked against the SHA-256 that GitHub publishes for the release file (the
  asset's `digest`). A file without one is not downloaded. It goes through craft-update's HTTPS
  client, the one the in-app updates use (system certificate store, resumable).
- Then `tlmgr install` adds `managed::REQUIRED_PACKAGES` (latexmk, biber, biblatex, csquotes, …)
  from a CTAN mirror (`tlmgr option repository ctan`).

Nothing of TeX is in PSG's installers or repository. TeX Live is GPL and LPPL software, which is
outside the asset licence allowlist; PSG doesn't redistribute it, the person's computer downloads
it from its publishers. Bundling it into the installers was considered (offline install, but
55–75 MB more per installer and PSG redistributing GPL/LPPL programs) and not chosen
(2026-11-17).

## When

| When | What |
|---|---|
| The window starts and PSG's TeX isn't installed | Install, in the background. Typesetting with a TeX already on the PATH works meanwhile. |
| The window starts and the last update is 7 days old | `tlmgr update --self --all`. |
| `tlmgr` refuses a cross-release update (a new TeX Live year) | Reinstall from the newest TinyTeX, then put back the packages added for documents. |
| A compilation with PSG's TeX stops at ``File `x.sty' not found`` (or `.cls`, `.def`, `.bbx`, …) | `tlmgr search --global --file /x.sty`, install the package holding exactly that file (at most 12), compile once more. |
| Settings ▸ TeX installation ▸ **Install TeX** / **Update TeX now** | The same, by hand. |

Each automatic job runs once per start. A failed update leaves the installed TeX working and is
only noted in the activity log; a failed install is shown and can be retried. Typesetting waits
while PSG's TeX is being installed or updated.

## Where

| System | Folder |
|---|---|
| Windows | `%LOCALAPPDATA%\PSGAcademicTeX`; `%ProgramData%\PSGAcademicTeX` when the first has spaces or non-ASCII characters (a user name like `華陽`), which TeX Live can't run from |
| macOS | `~/Library/PSGAcademicTeX` |
| Linux | `$XDG_DATA_HOME/psg-academic-tex` (`~/.local/share/psg-academic-tex`) |
| Windows portable zip | `PSGAcademicPDFData\tex` beside the program, if that path has no spaces or non-ASCII characters |

`PSG_ACADEMIC_TEX_DIR` names another folder (CI, tests, or someone who wants it elsewhere). The
folder holds `TinyTeX/` and `psg-tex.toml` (release, install and update times, packages added for
documents). Uninstalling the app leaves it, like the projects; deleting the folder removes it,
and the app installs it again on its next start.

Nothing is added to the system `PATH` or registry. The compilation puts PSG's `bin` folder first
on latexmk's `PATH` and keeps every sandbox measure in [academic.md](academic.md).

## Requirements and limits

- The first install needs the internet (GitHub and a CTAN mirror) and about 300 MB of disk.
- On macOS and Linux, `latexmk` is a Perl script and needs the system `perl` (macOS has it; most
  Linux desktops do). Windows TinyTeX brings its own Perl.
- Unpacking needs `tar` with xz support on macOS and Linux.
- Other systems (FreeBSD, 32-bit) get no TinyTeX: install TeX Live, and PSG Academic PDF uses it
  from the `PATH` as before.

## Testing

- `cargo test -p psg-academic managed` covers the layout, unpacking a fake bundle, replacing a
  tree, the weekly schedule and the log and `tlmgr search` parsing.
- `packaging/ci/academic-tex-smoke.sh <platform>` runs the real thing with a built program:
  installs TinyTeX into a temporary folder, typesets the template with a package TinyTeX-1 lacks,
  and updates. It needs the network.
