//! The shared components. Each one reads the [`Palette`] of the product the app installed, so the
//! same call draws a deep-blue control in PSG PDF and a purple one in PSG Academic PDF.

use egui::{Align, Align2, CornerRadius, Layout, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, vec2};
use psg_brand::{Tone, radius, size, space, text};

use crate::fonts;
use crate::palette::Palette;

/// The four kinds of button. A screen has at most one [`ButtonKind::Primary`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonKind {
    /// The main action of a screen: white text on the accent.
    Primary,
    /// Every other action: outlined.
    Secondary,
    /// Low-emphasis actions in toolbars and lists: accent text, no outline until hovered.
    Ghost,
    /// Actions that discard or delete: red text, outlined.
    Danger,
}

/// A button of `kind` labelled `label`, at the shared control height.
pub fn button(ui: &mut Ui, kind: ButtonKind, label: impl Into<String>) -> Response {
    ui.add(button_widget(ui, kind, label, size::CONTROL))
}

/// A large button for the call to action of a page or empty state.
pub fn button_large(ui: &mut Ui, kind: ButtonKind, label: impl Into<String>) -> Response {
    ui.add(button_widget(ui, kind, label, size::CONTROL_LARGE))
}

/// A button of `kind` that is greyed out and inert unless `enabled`. `why` explains, on hover,
/// what is missing while it is disabled.
pub fn button_enabled(ui: &mut Ui, enabled: bool, kind: ButtonKind, label: impl Into<String>, why: &str) -> Response {
    let resp = ui.add_enabled(enabled, button_widget(ui, kind, label, size::CONTROL));
    if why.is_empty() { resp } else { resp.on_disabled_hover_text(why) }
}

/// The egui button behind [`button`], for callers that need `add_enabled` or `add_sized`.
pub fn button_widget(ui: &Ui, kind: ButtonKind, label: impl Into<String>, height: f32) -> egui::Button<'static> {
    let p = Palette::of(ui);
    let label = label.into();
    let r = CornerRadius::same(radius::CONTROL);
    let min = vec2(height * 2.0, height);
    match kind {
        ButtonKind::Primary => egui::Button::new(RichText::new(label).font(fonts::semibold(text::BODY)).color(p.on_accent))
            .fill(p.accent)
            .stroke(Stroke::NONE)
            .corner_radius(r)
            .min_size(min),
        ButtonKind::Secondary => egui::Button::new(RichText::new(label).font(fonts::medium(text::BODY)).color(p.text))
            .fill(p.field)
            .stroke(Stroke::new(1.0, p.border))
            .corner_radius(r)
            .min_size(min),
        ButtonKind::Ghost => egui::Button::new(RichText::new(label).font(fonts::medium(text::BODY)).color(p.text))
            .frame(false)
            .corner_radius(r)
            .min_size(vec2(0.0, height)),
        ButtonKind::Danger => egui::Button::new(RichText::new(label).font(fonts::medium(text::BODY)).color(p.tone(Tone::Danger).text))
            .fill(p.field)
            .stroke(Stroke::new(1.0, p.tone(Tone::Danger).solid))
            .corner_radius(r)
            .min_size(min),
    }
}

/// [`button`] with [`ButtonKind::Primary`].
pub fn primary(ui: &mut Ui, label: impl Into<String>) -> Response {
    button(ui, ButtonKind::Primary, label)
}

/// [`button`] with [`ButtonKind::Secondary`].
pub fn secondary(ui: &mut Ui, label: impl Into<String>) -> Response {
    button(ui, ButtonKind::Secondary, label)
}

/// [`button`] with [`ButtonKind::Ghost`].
pub fn ghost(ui: &mut Ui, label: impl Into<String>) -> Response {
    button(ui, ButtonKind::Ghost, label)
}

/// A small rounded label in `tone`: a status, a count, a tag.
pub fn chip(ui: &mut Ui, tone: Tone, label: impl Into<String>) -> Response {
    let c = Palette::of(ui).tone(tone);
    egui::Frame::NONE
        .fill(c.soft)
        .corner_radius(CornerRadius::same(radius::SMALL))
        .inner_margin(egui::Margin::symmetric(6, 1))
        .show(ui, |ui| ui.label(RichText::new(label.into()).font(fonts::medium(text::SMALL)).color(c.text)))
        .inner
}

/// A coloured dot in `tone`, for a status beside a label.
pub fn status_dot(ui: &mut Ui, tone: Tone) -> Response {
    let c = Palette::of(ui).tone(tone);
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, c.solid);
    resp
}

/// The words [`psg_brand::Release::badge`] can return, marked so the apps' translation checks
/// require a row for each (the badge is drawn through [`psg_i18n::ui_text`]).
pub const BADGES: [&str; 2] = [psg_i18n::mark("Beta"), psg_i18n::mark("Development build")];

/// The release badge ("Beta", "Development build") for `version`, in the shared gold and the
/// interface language; nothing for a stable release.
pub fn release_badge(ui: &mut Ui, version: &str) -> Option<Response> {
    let label = psg_brand::Release::of(version).badge()?;
    let p = Palette::of(ui);
    Some(
        egui::Frame::NONE
            .fill(p.gold.gamma_multiply(0.18))
            .stroke(Stroke::new(1.0, p.gold))
            .corner_radius(CornerRadius::same(radius::SMALL))
            .inner_margin(egui::Margin::symmetric(6, 1))
            .show(ui, |ui| ui.label(RichText::new(psg_i18n::ui_text(label)).font(fonts::semibold(text::SMALL)).color(p.text)))
            .response,
    )
}

/// The product name in its accent colour, then the release badge for `version`.
pub fn product_name(ui: &mut Ui, version: &str) {
    let p = Palette::of(ui);
    ui.label(RichText::new(p.product.name()).font(fonts::semibold(text::HEADING)).color(p.accent_text));
    release_badge(ui, version);
}

/// A card: the content on the card colour with an outline and the card radius.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let p = Palette::of(ui);
    egui::Frame::NONE
        .fill(p.card)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(egui::Margin::same(space::L as i8))
        .show(ui, add)
}

/// A card that is one big button (a template, a recent project). `selected` draws the accent
/// outline and wash. Returns the response of the whole card.
pub fn selectable_card(ui: &mut Ui, selected: bool, width: f32, add: impl FnOnce(&mut Ui)) -> Response {
    let p = Palette::of(ui);
    let frame = egui::Frame::NONE
        .fill(if selected { p.accent_soft } else { p.card })
        .stroke(Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { p.accent } else { p.border }))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(egui::Margin::same(space::M as i8));
    let inner = frame.show(ui, |ui| {
        ui.set_width(width - 2.0 * space::M);
        // Cards are often laid out in a wrapping row; their content always stacks.
        ui.vertical(add);
    });
    let resp = ui.interact(inner.response.rect, inner.response.id.with("card"), Sense::click());
    if resp.hovered() && !selected {
        ui.painter().rect_stroke(inner.response.rect, CornerRadius::same(radius::CARD), Stroke::new(1.0, p.accent), StrokeKind::Inside);
    }
    if resp.has_focus() {
        ui.painter().rect_stroke(
            inner.response.rect.expand(2.0),
            CornerRadius::same(radius::CARD + 2),
            crate::style::focus_ring(&p),
            StrokeKind::Outside,
        );
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A page title with an optional one-line description under it.
pub fn page_header(ui: &mut Ui, title: &str, subtitle: Option<&str>) {
    let p = Palette::of(ui);
    ui.label(RichText::new(title).font(fonts::semibold(text::TITLE)).color(p.text));
    if let Some(s) = subtitle {
        ui.label(RichText::new(s).font(fonts::regular(text::BODY)).color(p.text_muted));
    }
    ui.add_space(space::M);
}

/// A section heading inside a page or panel.
pub fn section_header(ui: &mut Ui, title: &str) {
    let p = Palette::of(ui);
    ui.add_space(space::XS);
    ui.label(RichText::new(title).font(fonts::semibold(text::HEADING)).color(p.text));
    ui.add_space(space::XS);
}

/// A small upper-level label over a group of controls ("TEMPLATE", "ENGINE").
pub fn overline(ui: &mut Ui, label: &str) {
    let p = Palette::of(ui);
    ui.label(RichText::new(label.to_uppercase()).font(fonts::semibold(text::SMALL - 1.0)).color(p.text_muted));
}

/// Muted secondary text.
pub fn muted(ui: &mut Ui, text_: impl Into<String>) -> Response {
    let p = Palette::of(ui);
    ui.label(RichText::new(text_.into()).color(p.text_muted))
}

/// The empty state of a page or pane: a title, what to do, and the actions that do it.
pub fn empty_state(ui: &mut Ui, title: &str, body: &str, actions: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    ui.vertical_centered(|ui| {
        ui.add_space((ui.available_height() * 0.22).clamp(space::L, 160.0));
        ui.label(RichText::new(title).font(fonts::semibold(17.0)).color(p.text));
        ui.add_space(space::XS);
        ui.add(egui::Label::new(RichText::new(body).color(p.text_muted)).wrap());
        ui.add_space(space::L);
        ui.horizontal(|ui| {
            // Centre the row of actions: lay it out once to measure it, then pad.
            let id = ui.scope_id().with("empty-actions");
            let width: f32 = ui.ctx().data(|d| d.get_temp(id)).unwrap_or(0.0);
            ui.add_space(((ui.available_width() - width) * 0.5).max(0.0));
            let start = ui.cursor().min.x;
            actions(ui);
            let used = ui.min_rect().max.x - start;
            ui.ctx().data_mut(|d| d.insert_temp(id, used));
        });
    });
}

/// One entry of a navigation rail: a label, an optional count in `tone`, the accent bar when
/// `selected`. `compact` draws only the first letter (the rail collapsed on a narrow window).
pub fn nav_item(ui: &mut Ui, label: &str, selected: bool, count: Option<(usize, Tone)>, compact: bool) -> Response {
    let p = Palette::of(ui);
    let height = 34.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Tab, true, selected, label));
    let fill = if selected {
        p.accent_soft
    } else if resp.is_pointer_button_down_on() {
        p.pressed
    } else if resp.hovered() {
        p.hover
    } else {
        egui::Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect.shrink2(vec2(space::XS, 1.0)), CornerRadius::same(radius::CONTROL), fill);
    if selected {
        let bar = egui::Rect::from_min_size(rect.left_top() + vec2(space::XS, 8.0), vec2(3.0, height - 16.0));
        painter.rect_filled(bar, CornerRadius::same(2), p.accent);
    }
    let colour = if selected { p.accent_text } else { p.text };
    let font = if selected { fonts::semibold(text::BODY) } else { fonts::medium(text::BODY) };
    if compact {
        let initial: String = label.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
        painter.text(rect.center(), Align2::CENTER_CENTER, initial, fonts::semibold(text::HEADING), colour);
    } else {
        let left = rect.left() + space::L;
        let right_reserved = if count.is_some() { 36.0 } else { 0.0 };
        let galley = painter.layout(label.to_owned(), font, colour, rect.width() - space::L - space::S - right_reserved);
        let y = rect.center().y - galley.size().y * 0.5;
        painter.galley(egui::pos2(left, y), galley, colour);
    }
    if let Some((n, tone)) = count.filter(|(n, _)| *n > 0) {
        let c = p.tone(tone);
        let label = if n > 99 { "99+".to_owned() } else { n.to_string() };
        let galley = painter.layout_no_wrap(label, fonts::semibold(text::SMALL - 1.0), c.text);
        let w = (galley.size().x + 10.0).max(18.0);
        let centre =
            if compact { rect.right_top() + vec2(-w * 0.5 - 4.0, 9.0) } else { egui::pos2(rect.right() - space::S - w * 0.5, rect.center().y) };
        let pill = egui::Rect::from_center_size(centre, vec2(w, 16.0));
        painter.rect_filled(pill, CornerRadius::same(8), c.soft);
        painter.galley(pill.center() - galley.size() * 0.5, galley, c.text);
    }
    if resp.has_focus() {
        painter.rect_stroke(
            rect.shrink2(vec2(space::XS, 1.0)),
            CornerRadius::same(radius::CONTROL),
            crate::style::focus_ring(&p),
            StrokeKind::Inside,
        );
    }
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    if compact { resp.on_hover_text(label) } else { resp }
}

/// A row of mutually exclusive choices drawn as one control ("Editor · Split · Preview").
/// Returns true when the choice changed.
pub fn segmented<V: PartialEq + Copy>(ui: &mut Ui, current: &mut V, options: &[(V, &str)]) -> bool {
    let p = Palette::of(ui);
    let mut changed = false;
    egui::Frame::NONE.fill(p.hover).corner_radius(CornerRadius::same(radius::CONTROL)).inner_margin(egui::Margin::same(2)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (value, label) in options {
                let on = *current == *value;
                let text = RichText::new(*label).font(if on { fonts::semibold(text::BODY) } else { fonts::medium(text::BODY) }).color(if on {
                    p.accent_text
                } else {
                    p.text_muted
                });
                let b = egui::Button::new(text)
                    .fill(if on { p.card } else { egui::Color32::TRANSPARENT })
                    .stroke(if on { Stroke::new(1.0, p.border) } else { Stroke::NONE })
                    .corner_radius(CornerRadius::same(radius::CONTROL - 1))
                    .min_size(vec2(0.0, size::CONTROL - 4.0));
                let resp = ui.add(b);
                resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::RadioButton, true, on, *label));
                if resp.clicked() && !on {
                    *current = *value;
                    changed = true;
                }
            }
        });
    });
    changed
}

/// A banner across a page or pane in `tone`. With `action`, a button at its end; returns true
/// when that was pressed.
pub fn banner(ui: &mut Ui, tone: Tone, message: &str, action: Option<&str>) -> bool {
    let p = Palette::of(ui);
    let c = p.tone(tone);
    let mut pressed = false;
    egui::Frame::NONE
        .fill(c.soft)
        .stroke(Stroke::new(1.0, c.solid.gamma_multiply(0.5)))
        .corner_radius(CornerRadius::same(radius::CARD))
        .inner_margin(egui::Margin::symmetric(space::M as i8, space::S as i8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let (dot, _) = ui.allocate_exact_size(Vec2::splat(10.0), Sense::hover());
                ui.painter().circle_filled(dot.center(), 4.0, c.solid);
                let reserve = if action.is_some() { 120.0 } else { 0.0 };
                ui.scope(|ui| {
                    ui.set_max_width((ui.available_width() - reserve).max(80.0));
                    ui.add(egui::Label::new(RichText::new(message).color(c.text)).wrap());
                });
                if let Some(label) = action {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        pressed = ui.add(button_widget(ui, ButtonKind::Secondary, label, size::CONTROL - 4.0)).clicked();
                    });
                }
            });
        });
    pressed
}

/// A keyboard shortcut drawn as a key cap ("Ctrl+S").
pub fn kbd(ui: &mut Ui, keys: &str) -> Response {
    let p = Palette::of(ui);
    egui::Frame::NONE
        .fill(p.hover)
        .stroke(Stroke::new(1.0, p.border))
        .corner_radius(CornerRadius::same(radius::SMALL))
        .inner_margin(egui::Margin::symmetric(5, 0))
        .show(ui, |ui| ui.label(RichText::new(keys).font(fonts::mono(text::SMALL - 1.0)).color(p.text_muted)))
        .inner
}

/// A two-column form: labels on the left, fields on the right, the shared row spacing.
pub fn form_grid<R>(ui: &mut Ui, id: &'static str, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Grid::new(id).num_columns(2).spacing(vec2(space::M, space::S)).min_col_width(112.0).show(ui, add).inner
}

/// A form label in the muted colour, vertically centred on its field. It is never shortened: in a
/// [`form_grid`] the label column grows to the longest label, so a longer translation ("Taille de
/// l'interface") is read in full instead of cut at the column's minimum width.
pub fn form_label(ui: &mut Ui, label: &str) -> Response {
    let p = Palette::of(ui);
    ui.add(egui::Label::new(RichText::new(label).color(p.text_muted)).extend())
}

/// A single-line text field that fills the row, with a placeholder.
pub fn text_field<'a>(text_: &'a mut String, hint: &str) -> egui::TextEdit<'a> {
    egui::TextEdit::singleline(text_).hint_text(hint.to_owned()).desired_width(f32::INFINITY).margin(vec2(space::S, space::XS + 1.0))
}

/// A determinate (`Some`) or indeterminate (`None`) progress bar in the accent colour.
pub fn progress(ui: &mut Ui, fraction: Option<f32>) -> Response {
    let p = Palette::of(ui);
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(2), p.hover);
    let fill = match fraction {
        Some(f) => egui::Rect::from_min_size(rect.min, vec2(rect.width() * f.clamp(0.0, 1.0), rect.height())),
        None => {
            let t = ui.input(|i| i.time) as f32;
            let w = rect.width() * 0.3;
            let x = rect.left() + (t * 0.8).fract() * (rect.width() + w) - w;
            ui.ctx().request_repaint();
            egui::Rect::from_min_max(egui::pos2(x.max(rect.left()), rect.top()), egui::pos2((x + w).min(rect.right()), rect.bottom()))
        }
    };
    ui.painter().rect_filled(fill, CornerRadius::same(2), p.accent);
    resp
}

/// A hairline divider in the theme's divider colour.
pub fn divider(ui: &mut Ui) {
    let p = Palette::of(ui);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(rect.x_range(), rect.center().y, Stroke::new(1.0, p.divider));
}
