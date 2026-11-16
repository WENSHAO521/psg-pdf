//! Notifications: short messages that stack in the bottom-right corner of the window. Information
//! and successes fade after a few seconds; warnings and errors stay until dismissed, so nothing
//! that needs action disappears unread.

use egui::{Align2, CornerRadius, RichText, Stroke, vec2};
use psg_brand::{Tone, motion, radius, space};

use crate::palette::Palette;

/// One notification.
#[derive(Clone, Debug, PartialEq)]
pub struct Notice {
    pub tone: Tone,
    pub text: String,
    /// When it was posted (egui time, seconds).
    pub posted: f64,
}

impl Notice {
    /// Whether this notice stays until dismissed.
    pub fn sticky(&self) -> bool {
        matches!(self.tone, Tone::Warning | Tone::Danger)
    }
}

/// The notifications of a window.
#[derive(Debug, Default)]
pub struct Notices {
    items: Vec<Notice>,
}

/// At most this many notices show at once; older ones make room.
pub const MAX_SHOWN: usize = 4;

impl Notices {
    /// Post `text` in `tone` at time `now` (`ctx.input(|i| i.time)`). An identical notice that is
    /// still showing is refreshed instead of repeated.
    pub fn push(&mut self, tone: Tone, text: impl Into<String>, now: f64) {
        let text = text.into();
        self.items.retain(|n| !(n.tone == tone && n.text == text));
        self.items.push(Notice { tone, text, posted: now });
        if self.items.len() > MAX_SHOWN {
            let drop = self.items.len() - MAX_SHOWN;
            self.items.drain(..drop);
        }
    }

    /// The notices showing, oldest first.
    pub fn items(&self) -> &[Notice] {
        &self.items
    }

    /// Remove the notices that have faded by `now`.
    pub fn expire(&mut self, now: f64) {
        self.items.retain(|n| n.sticky() || now - n.posted < f64::from(motion::NOTICE_SECS));
    }

    /// Draw the notices; pressing a notice's ✕ dismisses it.
    pub fn show(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.expire(now);
        if self.items.is_empty() {
            return;
        }
        let p = Palette::of_ctx(ctx);
        let mut dismiss = None;
        egui::Area::new(egui::Id::unique("psg-notices"))
            .anchor(Align2::RIGHT_BOTTOM, vec2(-space::L, -(psg_brand::size::STATUS_BAR + space::M)))
            .order(egui::Order::Foreground)
            .interactable(true)
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                for (i, n) in self.items.iter().enumerate().rev() {
                    let c = p.tone(n.tone);
                    egui::Frame::NONE
                        .fill(p.card)
                        .stroke(Stroke::new(1.0, p.border))
                        .corner_radius(CornerRadius::same(radius::CARD))
                        .shadow(egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: p.shadow })
                        .inner_margin(egui::Margin::symmetric(space::M as i8, space::S as i8))
                        .show(ui, |ui| {
                            ui.set_width(356.0);
                            ui.horizontal(|ui| {
                                let (edge, _) = ui.allocate_exact_size(vec2(4.0, 20.0), egui::Sense::hover());
                                ui.painter().rect_filled(edge, CornerRadius::same(2), c.solid);
                                ui.scope(|ui| {
                                    ui.set_max_width(300.0);
                                    ui.add(egui::Label::new(RichText::new(&n.text).color(p.text)).wrap());
                                });
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                    let close =
                                        ui.add(egui::Button::new(RichText::new("×").color(p.text_muted)).frame(false)).on_hover_text("Dismiss");
                                    if close.clicked() {
                                        dismiss = Some(i);
                                    }
                                });
                            });
                        });
                    ui.add_space(space::S);
                }
            });
        if let Some(i) = dismiss
            && i < self.items.len()
        {
            self.items.remove(i);
        }
        if self.items.iter().any(|n| !n.sticky()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
    }
}
