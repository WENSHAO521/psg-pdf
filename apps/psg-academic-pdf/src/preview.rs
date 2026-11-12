//! The PDF preview: pages drawn by the shared PdfCraft renderer (`pdfcraft-render`) on a worker
//! thread and shown as textures.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use pdfcraft_render::{PageRenderer, RenderConfig, RenderRequest};

/// The most pages rendered for the preview. Longer documents show this many and say so.
pub const MAX_PREVIEW_PAGES: usize = 300;

enum Message {
    Count(usize),
    Page { index: usize, width: u32, height: u32, rgba: Vec<u8> },
    Failed { index: usize, error: String },
    Done,
}

/// One page in the preview.
pub enum PageSlot {
    Pending,
    Ready(egui::TextureHandle, egui::Vec2),
    Failed(String),
}

#[derive(Default)]
pub struct Preview {
    pub pages: Vec<PageSlot>,
    pub total_pages: usize,
    pub rendering: bool,
    pub error: Option<String>,
    rx: Option<Receiver<Message>>,
    generation: u64,
}

impl Preview {
    pub fn clear(&mut self) {
        *self = Preview { generation: self.generation + 1, ..Preview::default() };
    }

    /// Start rendering `bytes` at `scale` device pixels per point.
    pub fn load(&mut self, bytes: Vec<u8>, scale: f32, ctx: &egui::Context) {
        self.clear();
        let (tx, rx) = channel();
        self.rx = Some(rx);
        self.rendering = true;
        let ctx = ctx.clone();
        let spawned = std::thread::Builder::new().name("psg-preview".into()).spawn(move || render_all(bytes, scale, &tx, &ctx));
        if let Err(e) = spawned {
            self.rendering = false;
            self.error = Some(format!("could not start the preview: {e}"));
        }
    }

    /// Take the pages the worker has finished.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.rx else { return };
        let mut done = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                Message::Count(n) => {
                    self.total_pages = n;
                    self.pages = (0..n.min(MAX_PREVIEW_PAGES)).map(|_| PageSlot::Pending).collect();
                    if n == 0 {
                        self.error = Some("The PDF has no pages, or it could not be read.".into());
                    }
                }
                Message::Page { index, width, height, rgba } => {
                    let image = egui::ColorImage::from_rgba_premultiplied([width as usize, height as usize], &rgba);
                    let texture = ctx.load_texture(format!("psg-preview-{}-{index}", self.generation), image, egui::TextureOptions::LINEAR);
                    if let Some(slot) = self.pages.get_mut(index) {
                        *slot = PageSlot::Ready(texture, egui::vec2(width as f32, height as f32));
                    }
                }
                Message::Failed { index, error } => {
                    if let Some(slot) = self.pages.get_mut(index) {
                        *slot = PageSlot::Failed(error);
                    }
                }
                Message::Done => done = true,
            }
        }
        if done {
            self.rendering = false;
            self.rx = None;
        }
    }
}

fn render_all(bytes: Vec<u8>, scale: f32, tx: &Sender<Message>, ctx: &egui::Context) {
    let mut renderer = PageRenderer::new(Arc::new(bytes), RenderConfig::default().on_screen());
    let count = renderer.page_count();
    if tx.send(Message::Count(count)).is_err() {
        return;
    }
    ctx.request_repaint();
    for index in 0..count.min(MAX_PREVIEW_PAGES) {
        let page = renderer.render(RenderRequest { page: index, scale, ..RenderRequest::default() });
        let msg = match page.error {
            Some(error) => Message::Failed { index, error },
            None => Message::Page { index, width: page.width, height: page.height, rgba: page.rgba.to_vec() },
        };
        // The receiver is gone when a newer compilation replaced this preview.
        if tx.send(msg).is_err() {
            return;
        }
        ctx.request_repaint();
    }
    let _ = tx.send(Message::Done);
    ctx.request_repaint();
}
