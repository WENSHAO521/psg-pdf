//! Organize pages, laid out as Acrobat's panel: the pages selected (typed, or picked by kind from
//! its menu); page options as a row of icon buttons (rotate both ways, delete, extract) and Insert,
//! Replace, Split and Bates numbering; page setup (boxes, crop, transitions, templates, printing);
//! and page labels and properties. It shows whenever the page grid does, so every way into
//! Organize (the tool, the Edit panel, the command, `--organize`) gets the same controls, and the
//! grid's own toolbar keeps only moving pages, their size, Save pages and Close.
//!
//! The page actions queue on the view (`pending_edit`, `pending_action`) exactly as the grid's
//! toolbar did, so they run, refuse and undo the same way.

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, vec2};
use pdfcraft_engine::{Edit, TRANSITION_DIRECTIONS, TRANSITION_STYLES, Transition, TransitionSpec};

use crate::canvas::ViewAction;
use crate::theme::{self, Tokens};
use crate::{LeftPanel, PdfCraftApp, icons, widgets};

/// Whether the left panel is Organize pages: the tool is chosen, or the active view shows the grid.
pub(crate) fn showing(app: &PdfCraftApp) -> bool {
    app.left == LeftPanel::Tool("organize") || app.active.and_then(|i| app.views.get(i)).is_some_and(|v| v.organize)
}

/// The panel of the workspace Organize was opened from, shown again once Organize closes (by
/// its Close, the grid's Close, or opening a page from the grid): Edit's panel under the Edit
/// tab, as Acrobat goes back to the tool you came from, and All tools otherwise.
pub(crate) fn workspace_panel(mode: crate::Mode) -> LeftPanel {
    match mode {
        crate::Mode::Edit => LeftPanel::Tool("edit"),
        crate::Mode::Convert => LeftPanel::Tool("export"),
        crate::Mode::Sign => LeftPanel::Tool("fill_sign"),
        _ => LeftPanel::AllTools,
    }
}

/// What a row does once clicked.
enum Act {
    Edit(Box<Edit>),
    View(ViewAction),
    Command(&'static str),
}

pub(crate) fn panel(app: &mut PdfCraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let Some((index, id)) = app.active_ids() else { return };
    let Some(doc) = app.session.get(id) else { return };
    let info = &doc.info;
    let editable = doc.allows_assembly();
    let n = info.pages.len();
    let mut act: Option<Act> = None;
    let mut back = false;
    let mut close = false;
    let Some(view) = app.views.get_mut(index) else { return };

    ui.horizontal(|ui| {
        if icons::button(ui, "chevron-left", 26.0, false, tl!("Back to all tools")).clicked() {
            back = true;
        }
        let title = tl!("Organize pages");
        let title_width = (ui.available_width() - 26.0 - ui.spacing().item_spacing.x).max(1.0);
        ui.add_sized([title_width, 26.0], egui::Label::new(egui::RichText::new(title).font(theme::semibold(15.5)).color(t.text)).truncate())
            .on_hover_text(title);
        if icons::button(ui, "x", 26.0, false, tl!("Close panel")).clicked() {
            close = true;
        }
    });
    ui.add_space(4.0);
    ui.separator();
    ui.add_space(6.0);

    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        ui.add_space(2.0);
        widgets::section_title(ui, tl!("Pages selected"));
        let current = view.current;
        let picked = view.organize_range.show(ui, &view.selected, n, current, |ui| {
            use pdfcraft_engine::{PageOrientation as O, PageParity as P, filter_pages};
            let all: Vec<usize> = (0..n).collect();
            let mut chosen = None;
            for (label, parity, orient) in [
                ("All pages", P::Both, O::Both),
                ("Even pages", P::Even, O::Both),
                ("Odd pages", P::Odd, O::Both),
                ("Landscape pages", P::Both, O::Landscape),
                ("Portrait pages", P::Both, O::Portrait),
            ] {
                if ui.button(tl!(label)).clicked() {
                    chosen = Some(filter_pages(info, &all, parity, orient));
                }
            }
            if ui.button(tl!("None")).clicked() {
                chosen = Some(Vec::new());
            }
            chosen
        });
        if let Some(pages) = picked {
            view.select_pages(&pages);
        }

        let targets = view.target_pages();
        let last = targets.last().copied().unwrap_or(0);
        widgets::section_title(ui, tl!("Page options"));
        ui.add_enabled_ui(editable, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                if icons::button(ui, "rotate-ccw", 34.0, false, tl!("Rotate counterclockwise")).clicked() {
                    act = Some(Act::Edit(Box::new(Edit::RotatePages { pages: targets.clone(), degrees: -90 })));
                }
                if icons::button(ui, "rotate-cw", 34.0, false, tl!("Rotate clockwise")).clicked() {
                    act = Some(Act::Edit(Box::new(Edit::RotatePages { pages: targets.clone(), degrees: 90 })));
                }
                // A document keeps at least one page.
                let can_delete = targets.len() < n;
                if ui.add_enabled_ui(can_delete, |ui| icons::button(ui, "trash-2", 34.0, false, tl!("Delete pages (Delete)"))).inner.clicked() {
                    act = Some(Act::Edit(Box::new(Edit::DeletePages { pages: targets.clone() })));
                }
                if icons::button(ui, "file-output", 34.0, false, tl!("Extract pages to a new document")).clicked() {
                    act = Some(Act::View(ViewAction::Extract));
                }
            });
            ui.add_space(2.0);
            // Insert ▸: a blank page, pages from a file, or copies of the selected pages.
            let insert = row(ui, t, "file-plus-2", tl!("Insert"), Availability::Ready, true);
            egui::Popup::menu(&insert).align(egui::RectAlign::RIGHT_START).gap(6.0).show(|ui| {
                if ui.button(tl!("Insert a blank page after the selection")).clicked() {
                    let crop = info.pages.get(last).map_or([0.0, 0.0, 612.0, 792.0], |p| p.crop);
                    let (w, h) = ((crop[2] - crop[0]).abs().max(1.0) as f64, (crop[3] - crop[1]).abs().max(1.0) as f64);
                    act = Some(Act::Edit(Box::new(Edit::InsertBlankPage { at: last.saturating_add(1), width: w, height: h })));
                }
                if ui.button(tl!("Insert pages from a file…")).clicked() {
                    act = Some(Act::View(ViewAction::InsertFromFile));
                }
                if ui.button(tl!("Duplicate pages")).clicked() {
                    act = Some(Act::Command("page.duplicate"));
                }
            });
            if row(ui, t, "replace", tl!("Replace"), Availability::Ready, false).clicked() {
                act = Some(Act::View(ViewAction::ReplacePages));
            }
            if row(ui, t, "scissors", tl!("Split"), Availability::Ready, false).clicked() {
                act = Some(Act::View(ViewAction::Split));
            }
        });
        if row(ui, t, "hash", tl!("Bates numbering"), Availability::Ready, false).clicked() {
            act = Some(Act::Command("edit.bates"));
        }
        divider(ui, t);
        for (icon, label, availability, command) in [
            ("square-dashed-mouse-pointer", "Set page boxes", Availability::Ready, Some("page.boxes")),
            ("crop", "Crop pages", Availability::Ready, Some("page.crop")),
            ("presentation", "Page transitions", Availability::Ready, Some("page.transitions")),
            ("file-text", "Page templates", Availability::Later, None),
            ("printer", "Print pages", Availability::Ready, Some("print.dialog")),
        ] {
            if row(ui, t, icon, tl!(label), availability, false).clicked()
                && let Some(command) = command
            {
                act = Some(Act::Command(command));
            }
        }
        divider(ui, t);
        if row(ui, t, "tag", tl!("Page labels"), Availability::Ready, false).clicked() {
            act = Some(Act::Command("page.number"));
        }
        if row(ui, t, "info", tl!("Page properties"), Availability::Ready, false).clicked() {
            act = Some(Act::Command("page.properties"));
        }
    });

    if close {
        view.organize = false;
        app.left = workspace_panel(app.mode);
    } else if back {
        view.organize = false;
        app.left = LeftPanel::AllTools;
    }
    match act {
        Some(Act::Edit(edit)) => view.pending_edit = Some(*edit),
        Some(Act::View(action)) => view.pending_action = Some(action),
        Some(Act::Command(command)) => app.run_command(command),
        None => {}
    }
    // Leaving the workspace needs the whole app, so it waits until the view is done with: leaving
    // Edit puts Edit text down and applies a paragraph still open on the page.
    if back && !close {
        app.enter_mode(crate::Mode::AllTools);
    }
}

pub(crate) struct TransitionDraft {
    pub doc: pdfcraft_engine::DocId,
    pub generation: u64,
    pub pages: Vec<usize>,
    pub spec: TransitionSpec,
    before: Option<TransitionSpec>,
}

impl TransitionDraft {
    pub(crate) fn new(doc: &pdfcraft_engine::Document, pages: Vec<usize>) -> Self {
        let transitions = &doc.info.transitions;
        let for_page = |&page: &usize| {
            let t = transitions.binary_search_by_key(&page, |t| t.page).ok().and_then(|i| transitions.get(i));
            TransitionSpec {
                display: t.and_then(|t| t.display),
                transition: t.and_then(|t| {
                    Some(Transition {
                        style: t.style?.name().into(),
                        duration: t.duration,
                        horizontal: t.horizontal,
                        inward: t.inward,
                        direction: t.direction,
                        scale: t.scale,
                        opaque: t.opaque,
                    })
                }),
            }
        };
        let mut existing = pages.iter().map(for_page);
        let before = existing.next().filter(|first| existing.all(|next| &next == first));
        let spec = before.clone().unwrap_or(TransitionSpec { transition: None, display: None });
        Self { doc: doc.id, generation: doc.edit_generation(), pages, spec, before }
    }
}

fn transition_label(style: &str) -> &str {
    match style {
        "Split" => tl!("Split"),
        "Blinds" => tl!("Blinds"),
        "Box" => tl!("Box"),
        "Wipe" => tl!("Wipe"),
        "Dissolve" => tl!("Dissolve"),
        "Glitter" => tl!("Glitter"),
        "R" => tl!("Replace"),
        "Fly" => tl!("Fly"),
        "Push" => tl!("Push"),
        "Cover" => tl!("Cover"),
        "Uncover" => tl!("Uncover"),
        "Fade" => tl!("Fade"),
        _ => tl!("None"),
    }
}

pub(crate) fn transition_body(ui: &mut egui::Ui, draft: &mut TransitionDraft) -> (bool, bool) {
    ui.label(egui::RichText::new(tl!("Page transitions")).font(theme::semibold(18.0)));
    ui.label(format!("{} ({})", tl!("Pages selected"), draft.pages.len()));
    if draft.before.is_none() {
        ui.label(tl!("The selected pages have different presentation settings. Applying sets the same values on all of them."));
    }
    ui.horizontal(|ui| {
        let current = draft.spec.transition.as_ref().map_or("", |t| t.style.as_str());
        let mut chosen = None;
        let label = ui.label(tl!("Effect"));
        craft_ui::ComboBox::from_id_salt("page-transition-effect")
            .selected_text(transition_label(current))
            .show_ui(ui, |ui| {
                for style in std::iter::once("").chain(TRANSITION_STYLES) {
                    if ui.selectable_label(style == current, transition_label(style)).clicked() {
                        chosen = Some(style);
                    }
                }
            })
            .response
            .labelled_by(label.id);
        if let Some(style) = chosen.filter(|style| *style != current) {
            draft.spec.transition = (!style.is_empty()).then(|| Transition::new(style));
        }
    });
    if let Some(transition) = draft.spec.transition.as_mut() {
        ui.horizontal(|ui| {
            let label = ui.label(tl!("Transition duration"));
            ui.add(egui::DragValue::new(&mut transition.duration).range(0.0..=10.0).clamp_existing_to_range(false).speed(0.1).suffix(" s"))
                .labelled_by(label.id);
        });
        if matches!(transition.style.as_str(), "Split" | "Blinds") {
            ui.horizontal(|ui| {
                ui.radio_value(&mut transition.horizontal, true, tl!("Horizontal"));
                ui.radio_value(&mut transition.horizontal, false, tl!("Vertical"));
            });
        }
        if matches!(transition.style.as_str(), "Split" | "Box" | "Fly") {
            ui.horizontal(|ui| {
                ui.radio_value(&mut transition.inward, true, tl!("Inward"));
                ui.radio_value(&mut transition.inward, false, tl!("Outward"));
            });
        }
        if matches!(transition.style.as_str(), "Wipe" | "Glitter" | "Fly" | "Cover" | "Uncover" | "Push") {
            ui.horizontal(|ui| {
                ui.label(tl!("Direction"));
                let text = |direction: Option<u16>| direction.map_or_else(|| tl!("None").to_string(), |d| format!("{d}°"));
                craft_ui::ComboBox::from_id_salt("page-transition-direction").selected_text(text(transition.direction)).show_ui(ui, |ui| {
                    for direction in TRANSITION_DIRECTIONS.into_iter().map(Some).chain((transition.style == "Fly").then_some(None)) {
                        ui.selectable_value(&mut transition.direction, direction, text(direction));
                    }
                });
            });
        }
        if transition.style == "Fly" {
            ui.horizontal(|ui| {
                let label = ui.label(tl!("Scale"));
                ui.add(egui::DragValue::new(&mut transition.scale).range(0.01..=10.0).clamp_existing_to_range(false).speed(0.01))
                    .labelled_by(label.id);
                ui.checkbox(&mut transition.opaque, tl!("Opaque background"));
            });
        }
    }
    ui.horizontal(|ui| {
        let mut timed = draft.spec.display.is_some();
        let toggle = ui.checkbox(&mut timed, tl!("Advance automatically"));
        if toggle.changed() {
            draft.spec.display = timed.then_some(5.0);
        }
        if let Some(seconds) = draft.spec.display.as_mut() {
            ui.add(egui::DragValue::new(seconds).range(0.0..=600.0).clamp_existing_to_range(false).speed(0.1).suffix(" s")).labelled_by(toggle.id);
        }
    });
    ui.add_space(12.0);
    let (mut apply, mut cancel) = (false, false);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let changed = !draft.pages.is_empty() && draft.before.as_ref() != Some(&draft.spec);
        apply = ui.add_enabled_ui(changed, |ui| widgets::pill_button(ui, tl!("Apply"), true)).inner.clicked();
        cancel = widgets::pill_button(ui, tl!("Cancel"), false).clicked();
    });
    (apply, cancel)
}

/// Page Properties ▸ Tab Order for the pages it opened on: Use Row Order, Use Column Order, Use
/// Document Structure and Unspecified, then OK and Cancel. OK is off until a choice is made.
/// Returns (OK, Cancel).
pub(crate) fn page_props_body(ui: &mut egui::Ui, d: &mut crate::PagePropsDraft) -> (bool, bool) {
    use pdfcraft_engine::TabOrder;
    ui.label(egui::RichText::new(tl!("Page Properties")).font(theme::semibold(18.0)));
    widgets::section_title(ui, tl!("Tab Order"));
    for (order, label) in [
        (TabOrder::Row, tl!("Use Row Order")),
        (TabOrder::Column, tl!("Use Column Order")),
        (TabOrder::Structure, tl!("Use Document Structure")),
        (TabOrder::Annotations, tl!("Unspecified")),
    ] {
        widgets::radio_value(ui, &mut d.tab_order, Some(order), label);
    }
    let (mut ok, mut cancel) = (false, false);
    ui.add_space(12.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ok = ui.add_enabled_ui(d.tab_order.is_some(), |ui| widgets::pill_button(ui, tl!("OK"), true)).inner.clicked();
        cancel = widgets::pill_button(ui, tl!("Cancel"), false).clicked();
    });
    (ok, cancel)
}

/// Whether a row works in this build.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Availability {
    Ready,
    /// Not in PdfCraft yet: shown muted, as Acrobat lists it, and says it is planned when hovered.
    Later,
}

/// One row of the panel: a monochrome icon and the label, as Acrobat draws them, with a corner
/// mark when it opens a menu.
fn row(ui: &mut egui::Ui, t: &Tokens, icon: &str, label: &str, availability: Availability, menu: bool) -> egui::Response {
    let ready = availability == Availability::Ready && ui.is_enabled();
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), if ready { Sense::click() } else { Sense::hover() });
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, ready, label));
    let press = theme::Press::track(ui, &resp);
    if ready {
        press.wash(ui, rect, 6, false);
    }
    let body = rect.translate(press.offset());
    let fg = if ready { t.text } else { t.text_faint };
    let icon_rect = Rect::from_min_size(body.min + vec2(6.0, 8.0), vec2(18.0, 18.0));
    icons::paint(ui, icon_rect, icon, 17.0, fg);
    let font = theme::regular(13.5);
    let width = crate::widgets::text_width(ui, label, &font);
    ui.painter().text(body.left_center() + vec2(34.0, 0.0), Align2::LEFT_CENTER, label, font, fg);
    if menu {
        // The small corner triangle Acrobat puts after a label that opens a menu.
        let tip = body.left_center() + vec2(34.0 + width + 6.0, 6.0);
        let tri = vec![tip, tip + vec2(-5.0, 0.0), tip + vec2(0.0, -5.0)];
        ui.painter().add(egui::Shape::convex_polygon(tri, fg, Stroke::NONE));
    }
    match availability {
        Availability::Ready => theme::hand(resp),
        Availability::Later => resp.on_hover_text(tl!("Planned")),
    }
}

fn divider(ui: &mut egui::Ui, t: &Tokens) {
    ui.add_space(6.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, CornerRadius::ZERO, t.divider);
    ui.add_space(6.0);
}
