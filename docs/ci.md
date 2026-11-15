# CI/CD: one workflow, `PSG PDF — Build & Release`

Everything that checks, builds, tests and publishes PSG PDF and PSG Academic PDF runs from one
file, [`.github/workflows/build.yaml`](../.github/workflows/build.yaml), so the Actions tab lists
one workflow. Its shell lives in `packaging/ci/`, where it can be run and shellchecked locally.
Releases themselves are described in [releasing.md](releasing.md), and the two products'
identities and migration in [products.md](products.md).

## Stages

| Stage | Jobs | What runs |
|---|---|---|
| Check | `Check · plan, …` | `packaging/ci/plan.sh` (what this run does), `cargo xtask products check`, `cargo xtask changelog verify` / `lint`, git dependencies only from storytold (`git-sources.sh`), Windows font-notice regressions |
| | `Check` (shared gate) | `cargo xtask ci`: format, clippy `-D warnings`, every test (nextest), cargo-deny (advisories, licences, sources), assets, layers, products, parity, wasm. The Linux leg installs TeX Live, latexmk, Biber and Pandoc once, so the PSG Academic PDF TeX tests run there (`PSG_ACADEMIC_TEX_REQUIRED=1` fails instead of skipping) followed by `academic-smoke.sh` (template → PDF from the command line). Windows leg in full runs. |
| Build + Verify | `Build · <platform>` ×3 | One job per platform builds **both** products, one after the other, on one Cargo target directory and one cache (`build-products.sh`), checks signatures (`verify-signatures.sh`), then on the same runner installs each product alone and side by side, uninstalls one at a time, and upgrades from the 0.18.x suite (`verify-installs.sh`: deb on Ubuntu, rpm in a Fedora container, MSI, DMG). |
| Quality | `Quality · …` | SonarCloud, FreeBSD, Nix, house rules, packaging lint + Windows installation policy, macOS tests, fuzzing |
| Release | `Release · <tag>` | Checksums, notes, GitHub release (`release-files.sh`, `release-notes.sh`) |

The products stay separate throughout: each is built by its own packaging script with its own
`cargo build -p …`, every file must be named after exactly one product, and each product's files go
into their own artifact (`psg-pdf-<platform>`, `psg-academic-pdf-<platform>`) with their own
SHA-256 list. An artifact is uploaded only after that platform's install tests passed, and the
release takes only those.

## Triggers

| Event | Check | Build + Verify | Quality | Release |
|---|---|---|---|---|
| Pull request | Linux | with the `full-ci` label, or when it changes `packaging/`, the workflow or the app icons (+ Windows tests) | SonarCloud; FreeBSD, Nix, house rules, packaging lint when their files change | never |
| Push to another branch | Linux; nothing when an open pull request runs the same commit | – | by changed files | never |
| Push to `beta` | Linux + Windows | yes, signed when the secrets exist | by changed files; packaging lint | `vX.Y.Z-beta.N` (pre-release) |
| Push to `main` | Linux + Windows | yes, signed | SonarCloud, FreeBSD, by changed files | `vX.Y.Z` when `CHANGELOG.md` has its section |
| Schedule (03:17 UTC nightly) | – | – | fuzzing, macOS tests, FreeBSD, Nix | never |
| Manual, `quick` | Linux | – | by changed files | never |
| Manual, `full` | Linux + Windows | yes, unsigned | by changed files | never |
| Manual, `release-check` | Linux + Windows | yes, unsigned | all of them | never |
| Manual, `quality` | – | – | the nightly set | never |

The path filters are the ones the separate workflows had: FreeBSD on Cargo manifests and
`packaging/freebsd/`; Nix on `flake.*`, `nix/` and `Cargo.lock`; house rules on `AGENTS.md` and
`CLAUDE.md`; packaging lint on `packaging/` and the app icons; all of them when the workflow itself
changes. Release tags and their rules are unchanged (`packaging/release-tag.sh`): only a push to
`beta` or `main` of `WENSHAO521/psg-pdf` publishes. A beta number never goes below an earlier beta of
the same version, because the run counter restarted with the new workflow file.

**Release conditions.** The plan, the gate (Linux and Windows) and the builds must pass. A stable
release needs both products on all three platforms, each having passed its install tests. A beta
needs Linux and Windows and publishes without macOS when its runner never started; the notes say
what is missing.

**Concurrency.** A newer push to a pull request or feature branch cancels the older run. Runs on
`beta` and `main` always finish.

## Caches

- The gate uses the shared `rust-cache` of craft-common-actions (written by `main`).
- Each platform build has one cache, `build-<platform>`, shared by both products. The plan job
  has `plan` (xtask only). Both are written only by `beta` and `main`, so pull requests read them
  without evicting them with their own.
- TeX Live, latexmk, Biber and Pandoc are installed once per run, in the Linux gate (cached apt
  packages), instead of in a job of their own.

## Pinned actions

Third-party actions are pinned to a commit with the version in a comment
(`actions/checkout`, `dtolnay/rust-toolchain`, `Swatinem/rust-cache`, `actions/upload-artifact`,
`actions/download-artifact`). nfpm and the Pandoc/TeX packages are pinned by version (nfpm also by
SHA-256). The shared workflows of `storytold/craft-common-actions` are called at `@main` on purpose
(AGENTS.md, "CI workflows"): they are the suite's own, and their fixes reach every app.

## Before and after

Measured on PR #7 (5ee7e51, 15 Nov 2026), the last run of the old layout.

| | Before (9 workflow files) | After (`build.yaml`) |
|---|---|---|
| Workflows in the Actions list | 9 (CI, PSG release, SonarCloud, Packaging lint, Nix, FreeBSD, House rules, Fuzz, Nightly macOS) | 1 |
| Workflow runs per pull request | up to 7 | 1 |
| Jobs defined (excluding shared workflows' own) | 19 | 12 |
| Steps defined | 70 | 31 |
| YAML lines | 846 | about 425 (plus 445 lines of tested shell in `packaging/ci/`) |
| Jobs on an ordinary pull request | 15 in PSG release + gate + 4 more | plan + gate (+ SonarCloud, path-filtered checks) |
| Product builds per full run | 6 jobs (product × platform), each with its own Rust setup and cache | 3 jobs (one per platform), one setup and cache each |
| TeX Live installs per run | 1 (separate academic job) + a separate test compile | 0 extra: in the gate's Linux leg, sharing its build |
| Install tests start | after **all** 6 builds (waited for macOS, about 40 min) | right after their own platform's build, on the same runner, no artifact download |
| Release artifacts | 6, any of which could be published untested | 6, uploaded only after the install tests passed |

Wall-clock times of the old PSG release run on PR #7: Linux builds 3.5 and 17 min, Windows 10.5 and
20.5 min, macOS 11 and 40 min; install jobs started 41 minutes after the run began. The CI gate took
27 min and FreeBSD 44 min. In the new layout a pull request no longer builds installers (it did on
every push before), so its wall-clock time is the gate's, about 30 min, and its runner time drops by
the roughly 110 runner-minutes the six builds and the installs took. A full run's per-platform time
is about the sum of that platform's two old builds minus the shared engine compile; these figures
are estimates until the first runs of `build.yaml` report real numbers.

## Running the scripts locally

```sh
GITHUB_EVENT_NAME=pull_request GITHUB_REF=refs/pull/1/merge BASE_SHA=$(git merge-base HEAD origin/beta) packaging/ci/plan.sh
packaging/ci/git-sources.sh
PSG_ACADEMIC_TEX_REQUIRED=1 cargo test -p psg-academic && packaging/ci/academic-smoke.sh
packaging/ci/build-products.sh linux-x86_64        # both products' Linux packages into dist/upload/
packaging/ci/verify-installs.sh linux-x86_64       # changes installed packages: a throwaway machine only
```
