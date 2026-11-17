# PSG Desktop Design System

PSG PDF and PSG Academic PDF are built from one design system, so they look like one family and
behave the same way, while each keeps a workspace made for its job. The brand rules (names, logo,
colours) are in [brand.md](brand.md); this page is about the interface.

## Where it lives

| Layer | Crate | What it holds |
|---|---|---|
| Tokens | [`crates/brand`](../crates/brand/src/lib.rs) (`psg-brand`) | Colours of both themes (`surfaces`, `Product::accent`, `Tone`), radii, spacing, type sizes, control and layout sizes, motion. Plain data: no UI dependency, so packaging, the website and tests read it too. |
| egui implementation | [`crates/desktop-ui`](../crates/desktop-ui/src/lib.rs) (`psg-desktop-ui`) | The egui style of both themes, the shared fonts, and the components. |
| PSG PDF | `crates/ui-egui` | Its own detailed egui style (`theme.rs`) written from the same tokens and fonts, and its PDF workspace. |
| PSG Academic PDF | `apps/psg-academic-pdf` | `psg_desktop_ui::style::install` with the academic accent, and its typesetting workspace built from the components. |

Nothing visual is maintained twice: both products take every neutral colour, tone, radius,
spacing step, font file and size from these two crates. Only the accent differs.

## Colour

- **Accent**: PSG PDF deep blue `#1E4384`, PSG Academic PDF academic purple `#5B2A78` (dark
  theme `#4472C4` / `#9A55C0`). Primary buttons, focus rings, the selected navigation entry,
  links, progress.
- **Neutrals** (`psg_brand::surfaces`): titlebar, chrome, panel, canvas, card, border, divider,
  text, muted text, faint text, icon, hover, pressed, field — identical in both products.
- **Tones** (`psg_brand::Tone`): Neutral, Info (the product accent), Success, Warning, Danger.
  Each has a text colour, a soft wash and a solid mark. Badges, banners, notifications, status
  dots and the problem list use them.
- Tests (`cargo test -p psg-brand`) hold every text colour to WCAG AA (4.5:1) on the surfaces it
  is drawn on, and solid marks to 3:1.

## Type

Noto Serif CJK TC (思源宋體; Regular for text and medium labels, SemiBold for headings) for the
interface in every language, in both products, and JetBrains Mono for code, logs and TeX source.
Release builds get the serif face from `packaging/fetch-psg-fonts.sh`, which
`packaging/fetch-craft-fonts.sh` runs; it is never committed (AGENTS.md §1.4). Inter (Regular,
Medium, SemiBold) and the craft-fonts CJK, Hangul and Arabic faces follow as fallbacks, and Inter
is the interface face of a build without that input (tests, the web build). Sizes: 12 small, 13 body, 15 heading, 20 title, 28 display. Each font file is
embedded once (`psg_desktop_ui::fonts`).

## Components (`psg_desktop_ui::widgets`, `layout`, `notice`)

| Component | Use |
|---|---|
| `button` (`Primary`, `Secondary`, `Ghost`, `Danger`) | One primary action per screen. A disabled button says on hover what it is waiting for. |
| `chip`, `status_dot`, `release_badge` | Status, counts, tags; the gold release badge. |
| `card`, `selectable_card` | Grouped content; choosable items (templates, recent projects). |
| `page_header`, `section_header`, `overline`, `muted` | The type hierarchy of a page. |
| `empty_state` | What a page is for and the actions that start it, instead of a blank pane. |
| `nav_item` + `layout::nav_rail` | The workspace navigation rail: a Lucide icon and label per area, with problem counts; collapses to the icons (label as tooltip) on narrow windows. |
| `icons` | The Lucide icons the shared components draw, from `assets/icons/`, tinted per theme. Never use Unicode glyphs as icons. |
| `segmented` | Mutually exclusive views ("Editor · Split · PDF"). |
| `banner` | A message across a page in a tone, with an optional action. |
| `form_grid`, `form_label`, `text_field` | Two-column forms with the shared spacing. |
| `progress` | Determinate or running progress in the accent. |
| `layout::toolbar`, `status_bar`, `side_panel`, `split`, `pane_header` | The workspace frame: toolbar, status bar, resizable panels, a two-pane split that is dragged, double-clicked to reset, and moved with the arrow keys. |
| `notice::Notices` | Notifications in the window corner. Information and successes fade after six seconds; warnings and errors stay until dismissed. |
| `keys::command` | Shortcuts written per platform: `⌘S` on macOS, `Ctrl+S` elsewhere. |

## Behaviour both products share

- Pages cap their width with `ui.set_max_width(ui.available_width().min(N))`: `set_max_width` alone can widen a page past the window.
- `chip` is measured before it is placed, so a wrapping row moves a chip to the next line instead of widening the card.

- **Theme**: follows the system's light or dark setting live; Light when the desktop says nothing.
  A manual choice (System, Light, Dark) is offered in Settings / Preferences.
- **Language**: follows the system language when it is one of the interface's languages, with a
  manual choice saved per product; CJK fallback faces are ordered for the chosen language.
- **Scale**: everything is sized in logical pixels, so per-monitor DPI and Ctrl/⌘ + / − / 0
  interface zoom scale the whole window.
- **Keyboard**: every control is reachable with Tab, the focused control shows the accent focus
  ring, and the main actions have shortcuts.
- **Accessibility**: components report their role, label and selection to AccessKit.
- **Never fake a feature**: a planned feature may appear only as a disabled control that says it
  is planned, never as a button that does nothing.

## Adding a component

Add it to `crates/desktop-ui`, reading colours only through `Palette` and sizes only from
`psg_brand`, give it an AccessKit role and label, and add a test in `crates/desktop-ui/src/tests.rs`.
Then use it from both products where it fits, rather than copying it into one of them.
