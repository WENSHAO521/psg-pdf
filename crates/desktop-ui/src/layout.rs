//! Workspace layout: toolbar, navigation rail, status bar and a resizable two-pane split, with the
//! shared sizes, so both products' windows are built from the same frame.

use egui::{CornerRadius, CursorIcon, Frame, Margin, Sense, Stroke, Ui, vec2};
use psg_brand::{size, space};

use crate::palette::Palette;

/// True when the window is narrow enough that a workspace should collapse its navigation rail.
pub fn is_compact(ctx: &egui::Context) -> bool {
    ctx.content_rect().width() < size::COMPACT_WIDTH
}

/// The workspace toolbar across the top of the window: the chrome colour, the toolbar height,
/// a hairline under it.
pub fn toolbar<R>(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let p = Palette::of(ui);
    egui::Panel::top(id)
        .exact_size(size::TOOLBAR)
        .frame(Frame::NONE.fill(p.chrome).inner_margin(Margin::symmetric(space::M as i8, 0)).stroke(Stroke::new(1.0, p.divider)))
        .show(ui, |ui| ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), add).inner)
}

/// The status bar along the bottom of the window.
pub fn status_bar<R>(ui: &mut Ui, id: &str, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let p = Palette::of(ui);
    egui::Panel::bottom(id)
        .exact_size(size::STATUS_BAR)
        .frame(Frame::NONE.fill(p.titlebar).inner_margin(Margin::symmetric(space::M as i8, 0)).stroke(Stroke::new(1.0, p.divider)))
        .show(ui, |ui| {
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.style_mut().override_text_style = Some(egui::TextStyle::Small);
                ui.visuals_mut().override_text_color = Some(p.text_muted);
                add(ui)
            })
            .inner
        })
}

/// The navigation rail on the left: full width with labels, or compact with initials.
pub fn nav_rail<R>(ui: &mut Ui, id: &str, compact: bool, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let p = Palette::of(ui);
    egui::Panel::left(id)
        .resizable(false)
        .exact_size(if compact { size::NAV_RAIL_COMPACT } else { size::NAV_RAIL })
        .frame(Frame::NONE.fill(p.chrome).inner_margin(Margin::symmetric(0, space::S as i8)).stroke(Stroke::new(1.0, p.divider)))
        .show(ui, add)
}

/// A side panel that can be resized (between the shared limits) and collapsed. Its width is
/// remembered by egui with the window's other state.
pub fn side_panel<R>(ui: &mut Ui, id: &str, right: bool, default: f32, add: impl FnOnce(&mut Ui) -> R) -> egui::InnerResponse<R> {
    let p = Palette::of(ui);
    let panel = if right { egui::Panel::right(id) } else { egui::Panel::left(id) };
    panel
        .resizable(true)
        .default_size(default)
        .size_range(size::PANEL_MIN..=size::PANEL_MAX)
        .frame(Frame::NONE.fill(p.panel).inner_margin(Margin::same(space::M as i8)).stroke(Stroke::new(1.0, p.divider)))
        .show(ui, add)
}

/// The fraction of a split given to the first pane, kept between `min` and `1 - min`.
pub fn clamp_ratio(ratio: f32, min: f32) -> f32 {
    let min = min.clamp(0.05, 0.45);
    if ratio.is_finite() { ratio.clamp(min, 1.0 - min) } else { 0.5 }
}

/// Two panes side by side with a draggable divider between them. `ratio` is the first pane's
/// share of the width, changed by dragging and reset to an even split by double-clicking the
/// divider; each pane keeps at least `min` of the width. Keyboard: with the divider focused
/// (Tab), the arrow keys move it. `state` is lent to each pane in turn.
pub fn split<S: ?Sized>(
    ui: &mut Ui,
    id: &str,
    ratio: &mut f32,
    min: f32,
    state: &mut S,
    first: impl FnOnce(&mut Ui, &mut S),
    second: impl FnOnce(&mut Ui, &mut S),
) {
    let p = Palette::of(ui);
    let full = ui.available_rect_before_wrap();
    let handle = 6.0;
    *ratio = clamp_ratio(*ratio, min);
    let usable = (full.width() - handle).max(0.0);
    let left_w = (usable * *ratio).round();
    let left = egui::Rect::from_min_size(full.min, vec2(left_w, full.height()));
    let bar = egui::Rect::from_min_size(egui::pos2(left.right(), full.top()), vec2(handle, full.height()));
    let right = egui::Rect::from_min_max(egui::pos2(bar.right(), full.top()), full.max);
    let resp = ui.interact(bar, ui.scope_id().with(id).with("split"), Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Splitter, true, psg_i18n::ui_text("Resize panes")));
    if resp.dragged()
        && let Some(pos) = resp.interact_pointer_pos()
        && usable > 0.0
    {
        *ratio = clamp_ratio((pos.x - full.left()) / usable, min);
    }
    if resp.double_clicked() {
        *ratio = 0.5;
    }
    if resp.has_focus() {
        let step = ui.input(|i| {
            if i.key_pressed(egui::Key::ArrowLeft) {
                -0.02
            } else if i.key_pressed(egui::Key::ArrowRight) {
                0.02
            } else {
                0.0
            }
        });
        *ratio = clamp_ratio(*ratio + step, min);
    }
    let hot = resp.hovered() || resp.dragged() || resp.has_focus();
    if hot {
        ui.ctx().set_cursor_icon(CursorIcon::ResizeHorizontal);
    }
    let line = if hot { p.accent } else { p.divider };
    ui.painter().vline(bar.center().x, bar.y_range(), Stroke::new(if hot { 2.0 } else { 1.0 }, line));
    ui.scope_builder(egui::UiBuilder::new().max_rect(left).layout(*ui.layout()), |ui| {
        ui.set_clip_rect(left.intersect(ui.clip_rect()));
        first(ui, state);
    });
    ui.scope_builder(egui::UiBuilder::new().max_rect(right).layout(*ui.layout()), |ui| {
        ui.set_clip_rect(right.intersect(ui.clip_rect()));
        second(ui, state);
    });
    ui.advance_cursor_after_rect(full);
}

/// A pane header: a title on the left, controls on the right, a hairline under it.
pub fn pane_header(ui: &mut Ui, title: &str, controls: impl FnOnce(&mut Ui)) {
    let p = Palette::of(ui);
    ui.horizontal(|ui| {
        ui.set_min_height(size::CONTROL);
        ui.label(egui::RichText::new(title).font(crate::fonts::semibold(psg_brand::text::BODY)).color(p.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), controls);
    });
    crate::widgets::divider(ui);
}

/// The canvas behind a document or preview: the canvas colour, edge to edge.
pub fn canvas_frame() -> Frame {
    Frame::NONE.corner_radius(CornerRadius::ZERO).inner_margin(Margin::same(0))
}
