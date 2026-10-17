//! The 9:16 preview: an AVPlayer plays the source (audio + hardware decode) and every frame is
//! cropped on the GPU to the same camera window the exporter uses, then shown as a texture.

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

use crate::native::Player;
use crate::state::Editor;

#[derive(Default)]
pub struct PlayerView {
    native: Option<Player>,
    src: String,
    pub tex: Option<TextureHandle>,
    buf: Vec<u8>,
    last_crop: [i64; 4],
    last_size: (usize, usize),
    pub failed: bool,
    seeks: u32,
}

impl PlayerView {
    pub fn open(&mut self, path: &str) {
        if self.src == path && self.native.is_some() {
            return;
        }
        self.src = path.to_string();
        self.native = Player::new(path).ok();
        self.failed = self.native.is_none();
        self.tex = None;
        self.last_size = (0, 0);
    }
    pub fn close(&mut self) {
        self.native = None;
        self.src.clear();
        self.tex = None;
    }
    pub fn is_open(&self) -> bool {
        self.native.is_some()
    }
    pub fn playing(&self) -> bool {
        self.native.as_ref().map_or(false, |p| p.playing())
    }
    pub fn time(&self) -> f64 {
        self.native.as_ref().map_or(0.0, |p| p.time())
    }
    pub fn play(&mut self, ed: &Editor) {
        let (Some(p), Some(c)) = (&self.native, ed.sel_clip()) else { return };
        let t = p.time();
        if t < c.start_ms as f64 - 50.0 || t >= c.end_ms as f64 - 60.0 {
            p.seek(c.start_ms as f64);
        }
        p.play();
    }
    pub fn pause(&mut self) {
        if let Some(p) = &self.native {
            p.pause();
        }
    }
    pub fn toggle(&mut self, ed: &Editor) {
        if self.playing() {
            self.pause();
        } else {
            self.play(ed);
        }
    }
    /// Seek inside the selected clip.
    pub fn seek(&mut self, ed: &mut Editor, ms: f64) {
        let ms = match ed.sel_clip() {
            Some(c) => ms.clamp(c.start_ms as f64, c.end_ms as f64),
            None => ms,
        };
        if let Some(p) = &self.native {
            p.seek(ms);
            self.seeks = 90;
        }
        ed.t = ms;
        self.last_crop = [0; 4];
    }
    pub fn seek_raw(&mut self, ms: f64) {
        if let Some(p) = &self.native {
            p.seek(ms);
            self.seeks = 90;
        }
        self.last_crop = [0; 4];
    }

    /// Advance clip playback (loop, jump cuts) and produce the frame for this egui frame.
    pub fn tick(&mut self, ctx: &egui::Context, ed: &mut Editor, size_px: (usize, usize)) {
        let Some(p) = &self.native else { return };
        let t = p.time();
        let playing = p.playing();
        if let Some(c) = ed.sel_clip().cloned() {
            let (s, e) = (c.start_ms as f64, c.end_ms as f64);
            if playing {
                let mut hop: Option<f64> = None;
                if c.jump_cut {
                    if let Some(caps) = &ed.caps {
                        let segs = &caps.segments;
                        if segs.len() > 1 && t >= s && t < e && !segs.iter().any(|g| t >= g.0 as f64 - 15.0 && t < g.1 as f64) {
                            hop = Some(segs.iter().find(|g| g.0 as f64 > t).map_or(e, |g| g.0 as f64));
                        }
                    }
                }
                match hop {
                    Some(h) if h < e - 30.0 => p.seek(h),
                    _ if t >= e - 30.0 || hop.is_some() => {
                        if ed.looping {
                            p.seek(s);
                        } else {
                            p.pause();
                            p.seek(e);
                        }
                    }
                    _ if t < s - 400.0 => p.seek(s),
                    _ => {}
                }
            }
        }
        ed.t = t;

        // frame
        let (cx, cy, cw, ch) = ed.crop_at(t);
        let key = [(cx * 1e4) as i64, (cy * 1e4) as i64, (cw * 1e4) as i64, (ch * 1e4) as i64];
        let moved = key != self.last_crop || size_px != self.last_size;
        let want = playing || moved || self.seeks > 0 || self.tex.is_none();
        if !want {
            return;
        }
        if self.seeks > 0 {
            self.seeks -= 1;
        }
        if p.frame((cx, cy, cw, ch), size_px.0, size_px.1, &mut self.buf, moved || self.seeks > 0 || self.tex.is_none()) {
            let img = ColorImage::from_rgba_unmultiplied([size_px.0, size_px.1], &self.buf);
            match &mut self.tex {
                Some(t) if self.last_size == size_px => t.set(img, TextureOptions::LINEAR),
                _ => self.tex = Some(ctx.load_texture("preview", img, TextureOptions::LINEAR)),
            }
            self.last_crop = key;
            self.last_size = size_px;
            self.seeks = 0;
        }
        if playing || self.seeks > 0 {
            ctx.request_repaint();
        } else if self.tex.is_none() {
            ctx.request_repaint_after(std::time::Duration::from_millis(60));
        }
    }
}
