//! UI glyphs, rendered once from SVG by `scripts/make-ui-icons.swift` and tinted at draw time.

use std::collections::HashMap;

use eframe::egui::{self, Color32, Rect, TextureHandle, TextureOptions, Ui};

macro_rules! icon_list {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_bytes!(concat!("../assets/icons/", $name, ".png")) as &[u8])),*]
    };
}
const ALL: &[(&str, &[u8])] = icon_list![
    "undo", "redo", "play", "pause", "skip_back", "skip_next", "plus", "x", "check", "back", "upload", "clip", "caption", "frame",
    "zoom_in", "zoom_out", "folder", "drop", "xlogo", "github", "chev_down"
];

pub struct Icons {
    tex: HashMap<&'static str, TextureHandle>,
}

impl Icons {
    pub fn new(ctx: &egui::Context) -> Self {
        let mut tex = HashMap::new();
        for (name, bytes) in ALL {
            if let Ok(img) = image::load_from_memory(bytes) {
                let rgba = img.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let ci = egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw());
                tex.insert(*name, ctx.load_texture(format!("icon-{name}"), ci, TextureOptions::LINEAR));
            }
        }
        Self { tex }
    }
    pub fn paint(&self, ui: &Ui, name: &str, rect: Rect, tint: Color32) {
        if let Some(t) = self.tex.get(name) {
            ui.painter().image(t.id(), rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), tint);
        }
    }
}
