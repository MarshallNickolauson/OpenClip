//! The right-hand inspector: Clip, Captions, Framing and Export tabs.

use eframe::egui::{self, epaint, pos2, vec2, Align2, Color32, Id, Pos2, Rect, Sense, Stroke, Ui};

use crate::app::App;
use crate::editor::{fmt, open_url, secs, wrap_lines};
use crate::home::dashed_rounded;
use crate::state::{Editor, Tab};
use crate::theme::*;

/// A column that stacks controls top to bottom inside the scrolling panel.
pub struct Col {
    pub x: f32,
    pub w: f32,
    pub y: f32,
    pub top: f32,
}

impl Col {
    fn rect(&self, h: f32) -> Rect {
        Rect::from_min_size(pos2(self.x, self.y), vec2(self.w, h))
    }
    fn h3(&mut self, ui: &Ui, text: &str, first: bool) {
        self.y += if first { 2.0 } else { 20.0 };
        section_title(ui, pos2(self.x, self.y), text);
        self.y += 10.5 + 10.0;
    }
    fn hint(&mut self, ui: &Ui, text: &str) {
        self.y += 8.0;
        for l in wrap_lines(ui, text, font(12.0), self.w, 12) {
            label(ui, pos2(self.x, self.y + 9.0), Align2::LEFT_CENTER, &l, font(12.0), MUTED);
            self.y += 18.0;
        }
        self.y += 8.0;
    }
    fn gap(&mut self, n: f32) {
        self.y += n;
    }
    /// Label (left) + value (right), then a slider.
    fn slider(&mut self, ui: &mut Ui, id: &str, label_t: &str, val: &str, v: &mut f32, min: f32, max: f32, step: f32) -> bool {
        self.y += 14.0;
        label(ui, pos2(self.x, self.y + 8.0), Align2::LEFT_CENTER, label_t, font(13.0), LABEL);
        label(ui, pos2(self.x + self.w, self.y + 8.0), Align2::RIGHT_CENTER, val, font(13.0), ACCENT2);
        self.y += 16.0 + 7.0;
        let r = Rect::from_min_size(pos2(self.x, self.y), vec2(self.w, 18.0));
        let ch = slider(ui, Id::new(("sl", id)), r, v, min, max, step);
        self.y += 18.0 + 14.0;
        ch
    }
    fn toggle_row(&mut self, ui: &mut Ui, id: &str, title: &str, sub: &str, on: bool) -> bool {
        self.y += 14.0;
        label(ui, pos2(self.x, self.y + 9.0), Align2::LEFT_CENTER, title, font(13.0), TEXT);
        let mut h = 18.0;
        if !sub.is_empty() {
            label(ui, pos2(self.x, self.y + 9.0 + 17.0), Align2::LEFT_CENTER, sub, font(12.0), MUTED);
            h = 36.0;
        }
        let clicked = toggle(ui, Id::new(("tg", id)), pos2(self.x + self.w - 36.0, self.y + (h - 20.0) / 2.0 + if sub.is_empty() { 0.0 } else { 0.0 }), on);
        self.y += h + 14.0;
        clicked
    }
    fn button(&mut self, ui: &mut Ui, id: &str, text: &str, danger: bool, w: Option<f32>, at_x: Option<f32>) -> bool {
        let rect = Rect::from_min_size(pos2(at_x.unwrap_or(self.x), self.y), vec2(w.unwrap_or(self.w), 34.0));
        let resp = ui.interact(rect, Id::new(("btn", id)), Sense::click());
        fill(ui, rect, 7, if resp.hovered() { RAISE } else { PANEL2 });
        if resp.hovered() {
            border(ui, rect, 7, LINE2);
        } else {
            dashed_rounded(ui, rect, 7.0, Stroke::new(1.0, LINE2));
        }
        label(ui, rect.center(), Align2::CENTER_CENTER, text, font(13.0), if danger { hex(0xff8a97) } else { hex(0xd0d4ea) });
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        resp.clicked()
    }
}

impl App {
    pub fn inspector_ui(&mut self, ctx: &egui::Context, ui: &mut Ui, rect: Rect) {
        fill(ui, rect, 0, PANEL);
        ui.painter().vline(rect.left() + 0.5, rect.y_range(), Stroke::new(1.0, LINE));
        // tab rail
        let rail = Rect::from_min_size(rect.min + vec2(1.0, 0.0), vec2(rect.width() - 1.0, 47.0));
        ui.painter().hline(rail.x_range(), rail.bottom() - 0.5, Stroke::new(1.0, LINE));
        let tabs = [(Tab::Clip, "clip", "Clip"), (Tab::Captions, "caption", "Captions"), (Tab::Framing, "frame", "Framing"), (Tab::Export, "upload", "Export")];
        let cur = self.ed.as_ref().unwrap().tab;
        let tw = (rail.width() - 20.0 - 6.0) / 4.0;
        for (i, (t, icon, tip)) in tabs.iter().enumerate() {
            let r = Rect::from_min_size(pos2(rail.left() + 10.0 + i as f32 * (tw + 2.0), rail.top() + 8.0), vec2(tw, 38.0));
            let resp = ui.interact(r, Id::new(("rail", i)), Sense::click());
            let on = cur == *t;
            if on {
                ui.painter().rect_filled(r, egui::CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, BG);
                ui.painter().hline(Rect::from_min_size(pos2(r.left() + r.width() * 0.2, r.bottom() - 2.0), vec2(r.width() * 0.6, 2.0)).x_range(), r.bottom() - 1.0, Stroke::new(2.0, ACCENT2));
            } else if resp.hovered() {
                ui.painter().rect_filled(r, egui::CornerRadius { nw: 7, ne: 7, sw: 0, se: 0 }, PANEL2);
            }
            self.icons.paint(ui, icon, Rect::from_center_size(r.center(), vec2(19.0, 19.0)), if on || resp.hovered() { Color32::WHITE } else { hex(0x7f86a6) });
            if resp.on_hover_text(*tip).clicked() {
                self.ed.as_mut().unwrap().tab = *t;
            }
        }
        let body = Rect::from_min_max(pos2(rect.left() + 1.0, rail.bottom()), rect.max);
        fill(ui, body, 0, BG);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(body));
        child.set_clip_rect(body.intersect(ui.clip_rect()));
        let tab = self.ed.as_ref().unwrap().tab;
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt(("insp_scroll", tab as u8)).show(&mut child, |ui| {
            let start = pos2(body.left() + 18.0, body.top() + 16.0);
            let mut col = Col { x: start.x, w: body.width() - 36.0, y: start.y, top: start.y };
            match tab {
                Tab::Clip => self.tab_clip(ctx, ui, &mut col),
                Tab::Captions => self.tab_captions(ui, &mut col),
                Tab::Framing => self.tab_framing(ui, &mut col),
                Tab::Export => self.tab_export(ctx, ui, &mut col),
            }
            ui.allocate_rect(Rect::from_min_max(pos2(body.left(), body.top()), pos2(body.right(), col.y + 24.0)), Sense::hover());
        });
    }

    fn tab_clip(&mut self, ctx: &egui::Context, ui: &mut Ui, col: &mut Col) {
        let Some(c) = self.ed.as_ref().unwrap().sel_clip().cloned() else {
            col.hint(ui, "Select a clip in the list to edit it.");
            return;
        };
        let id = c.id.clone();
        col.h3(ui, "Title", true);
        // title input
        let tr = col.rect(34.0);
        fill(ui, tr, 7, BG);
        border(ui, tr, 7, LINE2);
        let mut title = c.title.clone();
        let te = egui::TextEdit::singleline(&mut title).font(font(13.0)).frame(egui::Frame::NONE).margin(vec2(10.0, 8.0));
        let out = ui.put(tr, te);
        if out.changed() {
            self.ed.as_mut().unwrap().edit_clip(&id, "title", |x| x.title = title.clone());
        }
        col.y += 34.0;
        if !c.why.is_empty() && c.why != "null" {
            col.hint(ui, &c.why);
        }
        // jump cuts
        let full = (c.end_ms - c.start_ms) as f64;
        let eff = self.ed.as_ref().unwrap().eff_len(&c);
        let sub = if !c.jump_cut { "Remove pauses between words".to_string() } else if eff < full { format!("{} → {}", fmt(full, true), fmt(eff, true)) } else { "No long pauses in this clip".to_string() };
        if col.toggle_row(ui, "cuts", "Jump cut silences", &sub, c.jump_cut) {
            self.ed.as_mut().unwrap().edit_clip(&id, "cuts", |x| x.jump_cut = !x.jump_cut);
        }
        if c.jump_cut && col.button(ui, "cuts_all", "Turn on for all clips", false, None, None) {
            self.ed.as_mut().unwrap().mutate("", |p| p.clips.iter_mut().filter(|x| !x.dismissed).for_each(|x| x.jump_cut = true));
            self.toast("Jump cuts on for every clip");
        }
        if c.jump_cut {
            col.gap(34.0);
        }
        col.h3(ui, "Trim", false);
        // nudge rows
        for (is_start, name, ms) in [(true, "Starts", c.start_ms as f64), (false, "Ends", c.end_ms as f64)] {
            label(ui, pos2(col.x, col.y + 6.0), Align2::LEFT_TOP, name, font(11.0), MUTED);
            label(ui, pos2(col.x, col.y + 20.0), Align2::LEFT_TOP, &fmt(ms, true), bold(15.0), TEXT);
            let specs: [(&str, f64); 5] = [("−1s", -1000.0), ("−.1", -100.0), ("+.1", 100.0), ("+1s", 1000.0), ("⌖", 0.0)];
            let mut x = col.x + col.w;
            for (k, (t, d)) in specs.iter().enumerate().rev() {
                let w = text_w(ui, t, font(12.0)) + 16.0 + 2.0;
                x -= w;
                let r = Rect::from_min_size(pos2(x, col.y + 10.0), vec2(w, 24.0));
                x -= 4.0;
                let resp = ui.interact(r, Id::new(("nudge", is_start, k)), Sense::click());
                fill(ui, r, 6, if resp.hovered() { RAISE } else { PANEL2 });
                border(ui, r, 6, LINE);
                label(ui, r.center(), Align2::CENTER_CENTER, t, font(12.0), TEXT);
                if resp.clicked() {
                    let ed = self.ed.as_mut().unwrap();
                    let t = if *d == 0.0 { ed.t } else { ms + d };
                    ed.trim_to(&id, is_start, t);
                }
            }
            col.y += 42.0;
        }
        // sentence steppers (wrap like the web layout: three then one)
        let labels = [("◂ Sentence", true, -1), ("Sentence ▸", true, 1), ("◂ Sentence", false, -1), ("Sentence ▸", false, 1)];
        let (mut x, mut y) = (col.x, col.y + 6.0);
        for (i, (t, start, dir)) in labels.iter().enumerate() {
            let sz = pill_size(ui, t, true);
            if x + sz.x > col.x + col.w {
                x = col.x;
                y += sz.y + 6.0;
            }
            let r = Rect::from_min_size(pos2(x, y), sz);
            if pill(ui, Id::new(("sent", i)), r, t, false, true, true).clicked() {
                self.ed.as_mut().unwrap().step_sentence(&id, *start, *dir);
            }
            x += sz.x + 6.0;
        }
        col.y = y + 27.0 + 4.0;
        col.hint(ui, &format!("Top pair moves the start, bottom pair the end. Length: {}", fmt(full, true)));

        col.h3(ui, "Transcript", false);
        self.transcript(ctx, ui, col, &c);
        col.hint(ui, "Click a word to jump there · double-click to fix its spelling · ⌥-click to hide it from captions.");
        col.h3(ui, "Clip", false);
        let half = (col.w - 6.0) / 2.0;
        let (x0, x1) = (col.x, col.x + half + 6.0);
        let d = col.button(ui, "dup", "Duplicate", false, Some(half), Some(x0));
        let x = col.button(ui, "dismiss", "Dismiss", true, Some(half), Some(x1));
        col.y += 34.0;
        if d {
            if let Some(n) = self.ed.as_mut().unwrap().duplicate(&id) {
                self.select_clip(Some(n), false);
            }
        }
        if x {
            let next = self.ed.as_mut().unwrap().dismiss(&id);
            self.select_clip(next, false);
        }
    }

    fn transcript(&mut self, _ctx: &egui::Context, ui: &mut Ui, col: &mut Col, c: &openclip_core::project::Clip) {
        let ed = self.ed.as_ref().unwrap();
        let s0 = ed.sentence_at(c.start_ms as f64);
        let s1 = ed.sentence_at(c.end_ms as f64);
        let from = ed.sents.get(s0.saturating_sub(1)).map_or(0, |s| s.w0 as usize);
        let to = ed.sents.get((s1 + 1).min(ed.sents.len().saturating_sub(1))).map_or(0, |s| s.w1 as usize);
        let line_h = 25.0;
        let f = font(13.5);
        let (mut x, mut y) = (col.x, col.y);
        let cur_t = ed.t;
        let mut seek: Option<f64> = None;
        let mut hide: Option<usize> = None;
        let mut edit: Option<(usize, String)> = None;
        let mut commit: Option<(usize, Option<String>)> = None;
        let editing = ed.edit_word.clone();
        for i in from..=to.min(ed.words.len().saturating_sub(1)) {
            let w = &ed.words[i];
            let inside = w.1 as u64 >= c.start_ms.saturating_sub(1) && (w.1 as u64) < c.end_ms;
            let ov = c.edits.get(&i.to_string());
            let text = match ov {
                Some(t) if t.is_empty() => w.0.clone(),
                Some(t) => t.clone(),
                None => w.0.clone(),
            };
            let tw = text_w(ui, &text, f.clone()) + 2.0;
            let sp = 5.0;
            if x + tw > col.x + col.w && x > col.x {
                x = col.x;
                y += line_h;
            }
            if let Some((ei, buf)) = &editing {
                if *ei == i {
                    let r = Rect::from_min_size(pos2(x, y), vec2(120.0, line_h - 2.0));
                    let mut b = buf.clone();
                    let out = ui.put(r, egui::TextEdit::singleline(&mut b).font(font(13.0)).margin(vec2(4.0, 2.0)));
                    out.request_focus();
                    let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
                    if enter || out.lost_focus() && !esc {
                        commit = Some((i, Some(b.trim().to_string())));
                    } else if esc {
                        commit = Some((i, None));
                    } else if &b != buf {
                        edit = Some((i, b));
                    }
                    x += 124.0 + sp;
                    continue;
                }
            }
            let r = Rect::from_min_size(pos2(x - 1.0, y), vec2(tw, line_h - 3.0));
            let resp = ui.interact(r, Id::new(("tw", i)), Sense::click());
            let current = inside && (w.1 as f64) <= cur_t && cur_t < w.2 as f64 + 40.0;
            if current {
                fill(ui, r, 4, ACCENT);
            } else if resp.hovered() {
                fill(ui, r, 4, alpha(ACCENT2, 0.22));
            }
            let col_c = if current { Color32::WHITE } else if ov.map_or(false, |t| t.is_empty()) { alpha(TEXT, 0.5) } else if ov.is_some() { CLIP } else if inside { TEXT } else { hex(0x656b8c) };
            let tr = label(ui, pos2(x, y + (line_h - 3.0) / 2.0), Align2::LEFT_CENTER, &text, f.clone(), col_c);
            if ov.map_or(false, |t| t.is_empty()) {
                ui.painter().hline(tr.x_range(), tr.center().y, Stroke::new(1.0, col_c));
            }
            if resp.double_clicked() {
                edit = Some((i, text.clone()));
            } else if resp.clicked() {
                if ui.input(|i| i.modifiers.alt) {
                    hide = Some(i);
                } else {
                    seek = Some(w.1 as f64);
                }
            }
            x += tw + sp;
        }
        col.y = y + line_h;
        let ed = self.ed.as_mut().unwrap();
        if let Some(t) = seek {
            let ed2: &mut Editor = ed;
            self.player.seek(ed2, t);
        }
        let ed = self.ed.as_mut().unwrap();
        if let Some(i) = hide {
            let id = c.id.clone();
            ed.edit_clip(&id, "edit", |x| {
                if x.edits.get(&i.to_string()).map_or(false, |t| t.is_empty()) {
                    x.edits.remove(&i.to_string());
                } else {
                    x.edits.insert(i.to_string(), String::new());
                }
            });
        }
        if let Some(e) = edit {
            ed.edit_word = Some(e);
        }
        if let Some((i, v)) = commit {
            ed.edit_word = None;
            if let Some(v) = v {
                let id = c.id.clone();
                let orig = ed.words[i].0.clone();
                ed.edit_clip(&id, "edit", |x| {
                    if v == orig {
                        x.edits.remove(&i.to_string());
                    } else {
                        x.edits.insert(i.to_string(), v);
                    }
                });
            }
        }
    }

    fn tab_captions(&mut self, ui: &mut Ui, col: &mut Col) {
        let ed = self.ed.as_mut().unwrap();
        let st = ed.project.style.clone();
        if let Some(c) = ed.sel_clip().cloned() {
            if col.toggle_row(ui, "cap_on", "Captions on this clip", "Burned in at export", c.captions) {
                ed.edit_clip(&c.id, "caps", |x| x.captions = !x.captions);
            }
        }
        col.h3(ui, "Look (all clips)", col.y == col.top);
        let gap = 8.0;
        let tw = (col.w - 2.0 * gap) / 3.0;
        let th = tw / 1.2;
        let presets = [("pop", "POP", "Bold, outlined"), ("boxed", "Boxed", "On a dark box"), ("clean", "Clean", "Soft shadow")];
        for (i, (id, name, sub)) in presets.iter().enumerate() {
            let r = Rect::from_min_size(pos2(col.x + i as f32 * (tw + gap), col.y), vec2(tw, th));
            let resp = ui.interact(r, Id::new(("preset", i)), Sense::click());
            let mut mesh = epaint::Mesh::default();
            mesh.colored_vertex(r.left_top(), hex(0x253a5e));
            mesh.colored_vertex(r.right_top(), hex(0x1b2a45));
            mesh.colored_vertex(r.right_bottom(), hex(0x111827));
            mesh.colored_vertex(r.left_bottom(), hex(0x182236));
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(epaint::Shape::mesh(mesh));
            border(ui, r, 9, if st.preset == *id { ACCENT2 } else { Color32::TRANSPARENT });
            let f = match *id {
                "pop" => cap_font("Arial Black", false, 14.0),
                _ => cap_font("Helvetica Neue", true, 14.0),
            };
            if *id == "boxed" {
                let w = text_w(ui, name, f.clone()) + 12.0;
                fill(ui, Rect::from_center_size(pos2(r.center().x, r.center().y - 6.0), vec2(w, 20.0)), 5, alpha(Color32::BLACK, 0.72));
            }
            if *id == "pop" {
                for k in 0..8 {
                    let a = k as f32 / 8.0 * std::f32::consts::TAU;
                    ui.painter().text(pos2(r.center().x + a.cos(), r.center().y - 6.0 + a.sin()), Align2::CENTER_CENTER, name, f.clone(), Color32::BLACK);
                }
            }
            label(ui, pos2(r.center().x, r.center().y - 6.0), Align2::CENTER_CENTER, name, f, Color32::WHITE);
            label(ui, pos2(r.center().x, r.center().y + 12.0), Align2::CENTER_CENTER, sub, font(10.5), MUTED);
            if resp.clicked() {
                ed.mutate("style", |p| {
                    p.style.preset = id.to_string();
                    p.style.upper = None;
                });
            }
        }
        col.y += th;
        let mut size = st.size;
        if col.slider(ui, "cs", "Size", &format!("{:.2}×", size), &mut size, 0.6, 1.7, 0.05) {
            ed.mutate("cs", |p| p.style.size = size);
        }
        let mut y = st.y;
        if col.slider(ui, "cy", "Position", &format!("{}%", (y * 100.0).round() as i32), &mut y, 0.15, 0.92, 0.01) {
            ed.mutate("cy", |p| p.style.y = y);
        }
        let mut words = st.words as f32;
        if col.slider(ui, "cw", "Words per line", &format!("{}", words as i32), &mut words, 1.0, 6.0, 1.0) {
            ed.mutate("cw", |p| p.style.words = words as u8);
        }
        label(ui, pos2(col.x, col.y + 8.0), Align2::LEFT_CENTER, "Spoken-word colour", font(13.0), LABEL);
        col.y += 16.0 + 7.0;
        let accents = ["#ffd60a", "#ffffff", "#ff5d73", "#3ddc97", "#4cc9ff", "#c084fc"];
        let sw = (col.w - 5.0 * 7.0) / 6.0;
        for (i, a) in accents.iter().enumerate() {
            let r = Rect::from_min_size(pos2(col.x + i as f32 * (sw + 7.0), col.y), vec2(sw, sw / 1.3));
            let resp = ui.interact(r, Id::new(("accent", i)), Sense::click());
            fill(ui, r, 6, crate::editor::parse_hex(a));
            if st.accent == *a {
                border(ui, r, 6, Color32::WHITE);
                border(ui, r.shrink(1.0), 5, Color32::WHITE);
            }
            if resp.clicked() {
                ed.mutate("", |p| p.style.accent = a.to_string());
            }
        }
        col.y += sw / 1.3 + 14.0;
        let upper = st.upper.unwrap_or(st.preset == "pop");
        if col.toggle_row(ui, "upper", "Uppercase", "", upper) {
            ed.mutate("", |p| p.style.upper = Some(!upper));
        }
        col.hint(ui, "Captions come from the on-device transcript. Fix a word by double-clicking it in the Clip tab.");
    }

    fn tab_framing(&mut self, ui: &mut Ui, col: &mut Col) {
        let ed = self.ed.as_mut().unwrap();
        let Some(c) = ed.sel_clip().cloned() else {
            col.hint(ui, "Select a clip in the list to edit it.");
            return;
        };
        let f = c.framing.clone();
        let id = c.id.clone();
        let sub = if ed.project.faces { "Camera glides to keep the face centred" } else { "Face tracking is still running…" };
        if col.toggle_row(ui, "follow", "Follow the speaker", sub, f.follow) {
            ed.edit_clip(&id, "fr", |x| x.framing.follow = !x.framing.follow);
        }
        if f.follow {
            label(ui, pos2(col.x, col.y + 8.0), Align2::LEFT_CENTER, "Who to follow", font(13.0), LABEL);
            col.y += 16.0 + 7.0;
            let cur = match ed.project.subject {
                openclip_core::framing::Subject::Largest => 0,
                openclip_core::framing::Subject::Left => 1,
                openclip_core::framing::Subject::Right => 2,
            };
            if let Some(i) = seg(ui, Id::new("subject"), pos2(col.x, col.y), &["Closest", "Left", "Right"], cur, 12.0, 30.0, 13.0) {
                ed.mutate("", |p| p.subject = [openclip_core::framing::Subject::Largest, openclip_core::framing::Subject::Left, openclip_core::framing::Subject::Right][i]);
            }
            col.y += 30.0;
        }
        let mut sm = ed.project.smooth;
        if col.slider(ui, "smooth", "Camera smoothness", &format!("{:.2}s", sm), &mut sm, 0.2, 1.4, 0.05) {
            ed.mutate("smooth", |p| p.smooth = sm);
        }
        col.h3(ui, "This clip", false);
        let mut z = f.zoom;
        if col.slider(ui, "zoom", "Zoom", &format!("{:.2}×", z), &mut z, 1.0, 2.2, 0.05) {
            ed.edit_clip(&id, "fr", |x| x.framing.zoom = z);
        }
        let mut sh = f.shift;
        if col.slider(ui, "nudge", "Nudge", &format!("{}{:.2}", if sh >= 0.0 { "+" } else { "" }, sh), &mut sh, -0.6, 0.6, 0.02) {
            ed.edit_clip(&id, "fr", |x| x.framing.shift = sh);
        }
        col.h3(ui, "Keyframes", false);
        col.hint(ui, "Drag the picture in the preview, or click the framing track, to pin the crop at a moment. With keyframes the crop follows them instead of the face.");
        let mut remove = None;
        for (i, k) in f.keys.iter().enumerate() {
            let r = col.rect(28.0);
            fill(ui, r, 6, PANEL2);
            border(ui, r, 6, LINE);
            label(ui, pos2(r.left() + 9.0, r.center().y), Align2::LEFT_CENTER, &fmt(k.t as f64 - c.start_ms as f64, true), font(13.0), TEXT);
            label(ui, pos2(r.center().x, r.center().y), Align2::CENTER_CENTER, &format!("{}%", (k.x * 100.0).round() as i32), font(13.0), MUTED);
            let xr = Rect::from_center_size(pos2(r.right() - 16.0, r.center().y), vec2(20.0, 20.0));
            let resp = ui.interact(xr, Id::new(("keydel", i)), Sense::click());
            self.icons.paint(ui, "x", Rect::from_center_size(xr.center(), vec2(14.0, 14.0)), if resp.hovered() { hex(0xff8a97) } else { hex(0x9aa0bd) });
            if resp.clicked() {
                remove = Some(i);
            }
            col.y += 28.0 + 4.0;
        }
        if let Some(i) = remove {
            ed.edit_clip(&id, "fr", |x| {
                x.framing.keys.remove(i);
            });
        }
        col.gap(4.0);
        if col.button(ui, "reset_fr", "Reset framing", false, None, None) {
            ed.edit_clip(&id, "fr", |x| {
                x.framing.keys.clear();
                x.framing.shift = 0.0;
                x.framing.zoom = 1.0;
                x.framing.follow = true;
            });
        }
        col.y += 34.0;
    }

    fn tab_export(&mut self, _ctx: &egui::Context, ui: &mut Ui, col: &mut Col) {
        let ed = self.ed.as_mut().unwrap();
        col.h3(ui, "Output", true);
        label(ui, pos2(col.x, col.y + 8.0), Align2::LEFT_CENTER, "Folder", font(13.0), LABEL);
        col.y += 16.0 + 7.0;
        let dir = if ed.project.export.dir.is_empty() { "~/Movies/OpenClip".to_string() } else { ed.project.export.dir.clone() };
        let r = col.rect(36.0);
        fill(ui, r, 7, PANEL2);
        border(ui, r, 7, LINE);
        let shown = crate::home::ellipsize(ui, &dir, font(12.0), r.width() - 20.0);
        label(ui, pos2(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, &shown, font(12.0), hex(0xb9bfdc));
        col.y += 36.0 + 8.0;
        if col.button(ui, "choose_dir", "Choose folder…", false, None, None) {
            let start = if ed.project.export.dir.is_empty() { None } else { Some(ed.project.export.dir.clone()) };
            let mut dlg = rfd::FileDialog::new();
            if let Some(s) = start {
                dlg = dlg.set_directory(s);
            }
            if let Some(d) = dlg.pick_folder() {
                ed.mutate("", |p| p.export.dir = d.to_string_lossy().into_owned());
            }
        }
        col.y += 34.0 + 14.0;
        label(ui, pos2(col.x, col.y + 8.0), Align2::LEFT_CENTER, "Size", font(13.0), LABEL);
        col.y += 16.0 + 7.0;
        let cur = if ed.project.export.width == 720 { 0 } else { 1 };
        if let Some(i) = seg(ui, Id::new("size"), pos2(col.x, col.y), &["720 × 1280", "1080 × 1920"], cur, 12.0, 30.0, 13.0) {
            ed.mutate("", |p| p.export.width = if i == 0 { 720 } else { 1080 });
        }
        col.y += 30.0 + 14.0;
        label(ui, pos2(col.x, col.y + 8.0), Align2::LEFT_CENTER, "Codec", font(13.0), LABEL);
        col.y += 16.0 + 7.0;
        let cur = if ed.project.export.codec == "hevc" { 1 } else { 0 };
        if let Some(i) = seg(ui, Id::new("codec"), pos2(col.x, col.y), &["H.264", "HEVC (smaller)"], cur, 12.0, 30.0, 13.0) {
            ed.mutate("", |p| p.export.codec = if i == 1 { "hevc".into() } else { "h264".into() });
        }
        col.y += 30.0;
        let items: Vec<openclip_core::project::Clip> = ed.picked().into_iter().cloned().collect();
        let total: f64 = items.iter().map(|c| (c.end_ms - c.start_ms) as f64).sum();
        col.h3(ui, &format!("Picked · {} clip{} · {}", items.len(), if items.len() == 1 { "" } else { "s" }, fmt(total, false)), false);
        if items.is_empty() {
            col.hint(ui, "Tick the clips you want in the list on the left.");
        }
        let exporting = ed.exporting;
        let mut reveal: Option<String> = None;
        for c in &items {
            let ex = ed.export.get(&c.id).cloned();
            let mut h = 36.0;
            if ex.is_some() {
                h += 10.0;
            }
            if ex.as_ref().map_or(false, |e| e.state == "error") {
                h += 18.0;
            }
            if ex.as_ref().map_or(false, |e| e.state == "done") {
                h += 34.0;
            }
            let r = col.rect(h);
            fill(ui, r, 8, PANEL2);
            border(ui, r, 8, LINE);
            let t = crate::home::ellipsize(ui, &c.title, font(13.0), r.width() - 70.0);
            label(ui, pos2(r.left() + 10.0, r.top() + 18.0), Align2::LEFT_CENTER, &t, font(13.0), TEXT);
            label(ui, pos2(r.right() - 10.0, r.top() + 18.0), Align2::RIGHT_CENTER, &secs((c.end_ms - c.start_ms) as f64), font(13.0), MUTED);
            let mut y = r.top() + 36.0;
            if let Some(e) = &ex {
                let bar = Rect::from_min_size(pos2(r.left() + 10.0, y), vec2(r.width() - 20.0, 4.0));
                fill(ui, bar, 2, LINE2);
                fill(ui, Rect::from_min_size(bar.min, vec2(bar.width() * (e.pct / 100.0).clamp(0.0, 1.0), 4.0)), 2, if e.state == "done" { OK } else { ACCENT2 });
                y += 10.0;
                if e.state == "error" {
                    label(ui, pos2(r.left() + 10.0, y + 9.0), Align2::LEFT_CENTER, e.error.as_deref().unwrap_or("error"), font(12.0), DANGER);
                }
                if e.state == "done" {
                    let pr = Rect::from_min_size(pos2(r.left() + 10.0, y + 4.0), pill_size(ui, "Show in Finder", true));
                    if pill(ui, Id::new(("reveal", &c.id)), pr, "Show in Finder", false, true, true).clicked() {
                        reveal = e.path.clone();
                    }
                }
            }
            col.y += h + 6.0;
        }
        col.gap(8.0);
        if exporting {
            if col.button(ui, "cancel_export", "Cancel export", true, None, None) {
                self.cancel_export.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            col.y += 34.0;
        } else {
            let n = items.len();
            let r = col.rect(36.0);
            let label_t = format!("Export {}clip{}", if n > 0 { format!("{n} ") } else { String::new() }, if n == 1 { "" } else { "s" });
            if n == 0 {
                fill(ui, r, 7, alpha(ACCENT, 0.4));
                label(ui, r.center(), Align2::CENTER_CENTER, &label_t, bold(13.0), alpha(Color32::WHITE, 0.5));
            } else if primary(ui, Id::new("export_btn"), r, &label_t, None, "").clicked() {
                self.pending_export = true;
            }
            col.y += 36.0;
        }
        col.hint(ui, "Rendered on this Mac with the hardware encoder. 30 fps max, AAC audio.");
        if let Some(p) = reveal {
            open_url_reveal(&p);
        }
        let _ = (open_url, Pos2::ZERO);
    }
}

fn open_url_reveal(path: &str) {
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
}
