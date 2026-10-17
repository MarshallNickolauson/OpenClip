//! The timeline: an overview of the whole talk on top, the selected clip's detail below
//! (waveform + words, caption blocks, framing path with keyframes).

use eframe::egui::{self, pos2, vec2, Align2, Color32, Id, Pos2, Rect, Sense, Shape, Stroke};

use crate::app::App;
use crate::editor::fmt;
use crate::state::Editor;
use crate::theme::*;

const LABEL_W: f32 = 62.0;
const TR_RULER: f32 = 22.0;
const TR_WORDS: f32 = 66.0;
const TR_CAPS: f32 = 26.0;
const TR_FRAME: f32 = 50.0;

pub fn fit_view(ed: &mut Editor) {
    let Some(c) = ed.sel_clip() else { return };
    let len = (c.end_ms - c.start_ms) as f64;
    ed.view = ((c.start_ms as f64 - len * 0.12 - 1500.0).max(0.0), (c.end_ms as f64 + len * 0.12 + 1500.0).min(ed.duration()));
}
pub fn wide_view(ed: &mut Editor) {
    let Some(c) = ed.sel_clip() else { return };
    let mid = (c.start_ms + c.end_ms) as f64 / 2.0;
    ed.view = ((mid - 60000.0).max(0.0), (mid + 60000.0).min(ed.duration()));
}
fn zoom_view(ed: &mut Editor, f: f64, around: Option<f64>) {
    let a = around.unwrap_or((ed.view.0 + ed.view.1) / 2.0);
    let span = ((ed.view.1 - ed.view.0) * f).clamp(4000.0, ed.duration().max(4000.0));
    let r = (a - ed.view.0) / (ed.view.1 - ed.view.0);
    let t0 = (a - span * r).clamp(0.0, (ed.duration() - span).max(0.0));
    ed.view = (t0, t0 + span);
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Drag {
    None,
    Seek,
    Start,
    End,
    Key(usize),
    OvSelect,
    OvNew(f64, f64),
}

impl App {
    pub fn timeline_ui(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, rect: Rect) {
        fill(ui, rect, 0, PANEL);
        ui.painter().hline(rect.x_range(), rect.top() + 0.5, Stroke::new(1.0, LINE));

        // splitter
        let sp = Rect::from_min_size(pos2(rect.left(), rect.top() - 4.0), vec2(rect.width(), 9.0));
        let sresp = ui.interact(sp, Id::new("split_v"), Sense::click_and_drag());
        let hot = sresp.hovered() || sresp.dragged();
        if hot {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        fill(ui, Rect::from_center_size(pos2(rect.center().x, rect.top() + 4.5), vec2(44.0, 3.0)), 2, if hot { ACCENT2 } else { LINE2 });
        if sresp.dragged() {
            let ed = self.ed.as_mut().unwrap();
            ed.tl_h = (ed.tl_h - sresp.drag_delta().y).clamp(200.0, 560.0);
        }
        if sresp.double_clicked() {
            self.ed.as_mut().unwrap().tl_h = 300.0;
        }

        // tools row
        let tools = Rect::from_min_size(pos2(rect.left() + 16.0, rect.top()), vec2(rect.width() - 32.0, 40.0));
        let mut row = Row::new(tools, 6.0);
        let sid = self.ed.as_ref().unwrap().sel.clone();
        for (i, (t, tip, start)) in [("Set in", "Set clip start at playhead ([)", true), ("Set out", "Set clip end at playhead (])", false)].iter().enumerate() {
            let sz = pill_size(ui, t, true);
            let r = row.left(sz.x, sz.y);
            if pill(ui, Id::new(("setio", i)), r, t, false, true, sid.is_some()).on_hover_text(*tip).clicked() {
                let ed = self.ed.as_mut().unwrap();
                let t = ed.t;
                if let Some(id) = &sid {
                    ed.trim_to(id, *start, t);
                }
            }
        }
        let hint = "Drag the clip edges to trim · click the framing track to add a keyframe · ⌘-scroll to zoom";
        let mut hr = Row::new(tools, 6.0);
        hr.x = row.x + 4.0;
        label(ui, pos2(hr.x, tools.center().y), Align2::LEFT_CENTER, hint, font(11.5), MUTED);
        let mut rr = Row::new(tools, 4.0);
        let wide = pill_size(ui, "Context", true);
        let r = rr.right(wide.x, wide.y);
        if pill(ui, Id::new("tl_wide"), r, "Context", false, true, true).on_hover_text("Show the surrounding talk").clicked() {
            wide_view(self.ed.as_mut().unwrap());
        }
        let fit = pill_size(ui, "Fit", true);
        let r = rr.right(fit.x, fit.y);
        if pill(ui, Id::new("tl_fit"), r, "Fit", false, true, true).on_hover_text("Fit the clip").clicked() {
            fit_view(self.ed.as_mut().unwrap());
        }
        let r = rr.right(32.0, 32.0);
        if icon_button(ui, &self.icons, Id::new("tl_zin"), r, "zoom_in", false, "Zoom in").clicked() {
            zoom_view(self.ed.as_mut().unwrap(), 1.0 / 1.5, None);
        }
        let r = rr.right(32.0, 32.0);
        if icon_button(ui, &self.icons, Id::new("tl_zout"), r, "zoom_out", false, "Zoom out").clicked() {
            zoom_view(self.ed.as_mut().unwrap(), 1.5, None);
        }

        let ov = Rect::from_min_size(pos2(rect.left() + 16.0, tools.bottom()), vec2(rect.width() - 32.0, 46.0));
        let det = Rect::from_min_max(pos2(rect.left() + 16.0, ov.bottom()), pos2(rect.right() - 16.0, rect.bottom() - 8.0));
        self.overview(ctx, ui, ov);
        self.detail(ctx, ui, det);
    }

    fn overview(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, rect: Rect) {
        let p = ui.painter().with_clip_rect(rect);
        let ed = self.ed.as_ref().unwrap();
        let dur = ed.duration();
        if dur <= 0.0 {
            return;
        }
        let w = rect.width();
        p.rect_filled(Rect::from_min_max(pos2(rect.left(), rect.top() + 4.0), pos2(rect.right(), rect.bottom() - 4.0)), 0.0, PANEL2);
        if let Some(e) = &ed.energy {
            let n = w.floor() as usize;
            let mid = rect.center().y;
            let col = hex(0x33466a);
            for x in 0..n {
                let a = x * e.v.len() / n;
                let b = ((x + 1) * e.v.len() / n).max(a + 1).min(e.v.len());
                let m = e.v[a..b].iter().copied().max().unwrap_or(0) as f32;
                let hh = (m / 255.0) * (rect.height() - 12.0);
                p.rect_filled(Rect::from_min_size(pos2(rect.left() + x as f32, mid - hh / 2.0), vec2(1.0, hh.max(1.0))), 0.0, col);
            }
        }
        for c in ed.live() {
            let x0 = rect.left() + (c.start_ms as f64 / dur) as f32 * w;
            let x1 = (rect.left() + (c.end_ms as f64 / dur) as f32 * w).max(x0 + 3.0);
            let col = if Some(&c.id) == ed.sel.as_ref() { alpha(CLIP, 0.95) } else if c.picked { alpha(ACCENT2, 0.8) } else { alpha(MUTED, 0.55) };
            p.rect_filled(Rect::from_min_max(pos2(x0, rect.top() + 2.0), pos2(x1, rect.bottom() - 2.0)), 0.0, col);
        }
        let vx0 = rect.left() + (ed.view.0 / dur) as f32 * w;
        let vx1 = rect.left() + (ed.view.1 / dur) as f32 * w;
        p.rect_stroke(Rect::from_min_max(pos2(vx0 + 0.5, rect.top() + 1.5), pos2(vx0.max(vx1 - 1.0) + 3.0, rect.bottom() - 1.5)), 0.0, Stroke::new(1.5, Color32::WHITE), egui::StrokeKind::Inside);
        let px = rect.left() + (ed.t / dur) as f32 * w;
        p.rect_filled(Rect::from_min_size(pos2(px - 1.0, rect.top()), vec2(2.0, rect.height())), 0.0, ACCENT2);
        let in_new = ctx.data(|d| d.get_temp::<Drag>(Id::new("ov_drag")));
        if let Some(Drag::OvNew(a, b)) = in_new {
            let (lo, hi) = (a.min(b), a.max(b));
            let r = Rect::from_min_max(pos2(rect.left() + (lo / dur) as f32 * w, rect.top() + 2.0), pos2(rect.left() + (hi / dur) as f32 * w, rect.bottom() - 2.0));
            p.rect_filled(r, 0.0, alpha(CLIP, 0.35));
            p.rect_stroke(r, 0.0, Stroke::new(1.0, CLIP), egui::StrokeKind::Inside);
        }

        // interaction
        let resp = ui.interact(rect, Id::new("overview"), Sense::click_and_drag());
        let ed = self.ed.as_mut().unwrap();
        let t_of = |x: f32| ((x - rect.left()) / w) as f64 * dur;
        let pressed = ctx.input(|i| i.pointer.primary_pressed()) && resp.hovered();
        let down = ctx.input(|i| i.pointer.primary_down());
        let pos = ctx.input(|i| i.pointer.interact_pos());
        let mut mode = ctx.data(|d| d.get_temp::<Drag>(Id::new("ov_drag"))).unwrap_or(Drag::None);
        if pressed {
            if let Some(pp) = pos {
                let t = t_of(pp.x);
                let hit = ed.live().iter().find(|c| t >= c.start_ms as f64 && t <= c.end_ms as f64).map(|c| c.id.clone());
                let shift = ctx.input(|i| i.modifiers.shift);
                if let (Some(id), false) = (hit, shift) {
                    mode = Drag::OvSelect;
                    let auto = ed.auto_play;
                    let _ = ed;
                    self.select_clip(Some(id), auto);
                } else {
                    mode = Drag::OvNew(t, t);
                }
            }
        } else if down {
            if let (Drag::OvNew(a, _), Some(pp)) = (mode, pos) {
                mode = Drag::OvNew(a, t_of(pp.x).clamp(0.0, dur));
            }
        } else if mode != Drag::None {
            if let Drag::OvNew(a, b) = mode {
                let ed = self.ed.as_mut().unwrap();
                let (lo, hi) = (a.min(b), a.max(b));
                if hi - lo >= 4000.0 {
                    let id = ed.new_clip(ed.snap_start(lo), ed.snap_end(hi));
                    self.select_clip(Some(id), false);
                } else {
                    let span = ed.view.1 - ed.view.0;
                    let t0 = (a - span / 2.0).clamp(0.0, (dur - span).max(0.0));
                    ed.view = (t0, t0 + span);
                }
            }
            mode = Drag::None;
        }
        ctx.data_mut(|d| d.insert_temp(Id::new("ov_drag"), mode));
    }

    fn detail(&mut self, ctx: &egui::Context, ui: &mut egui::Ui, rect: Rect) {
        let p = ui.painter().with_clip_rect(rect);
        let ed = self.ed.as_ref().unwrap();
        let Some(c) = ed.sel_clip().cloned() else {
            p.text(rect.center(), Align2::CENTER_CENTER, "Select a clip to edit it on the timeline", font(12.5), MUTED);
            return;
        };
        let w = rect.width();
        let track_w = w - LABEL_W - 8.0;
        let (v0, v1) = ed.view;
        let span = (v1 - v0).max(1.0);
        let x_of = |t: f64| rect.left() + LABEL_W + ((t - v0) / span) as f32 * track_w;
        let t_of = |x: f32| v0 + ((x - rect.left() - LABEL_W) / track_w) as f64 * span;
        let y_ruler = rect.top() + 2.0;
        let y_words = rect.top() + TR_RULER + 6.0;
        let y_caps = y_words + TR_WORDS + 6.0;
        let y_frame = y_caps + TR_CAPS + 6.0;

        // labels
        for (t, y, h) in [("WORDS", y_words, TR_WORDS), ("CAPTIONS", y_caps, TR_CAPS), ("FRAMING", y_frame, TR_FRAME)] {
            p.text(pos2(rect.left() + 2.0, y + h / 2.0), Align2::LEFT_CENTER, t, font(10.0), MUTED);
        }
        // ruler
        let steps = [250.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 15000.0, 30000.0, 60000.0, 120000.0, 300000.0];
        let step = steps.iter().copied().find(|s| (*s / span) as f32 * track_w > 70.0).unwrap_or(600000.0);
        let mut t = (v0 / step).ceil() * step;
        while t < v1 {
            let x = x_of(t);
            p.vline(x + 0.5, egui::Rangef::new(y_ruler + 12.0, y_ruler + 18.0), Stroke::new(1.0, LINE2));
            p.text(pos2(x, y_ruler + 5.0), Align2::CENTER_CENTER, fmt(t, step < 1000.0), font(10.5), MUTED);
            t += step;
        }
        // words track
        let wr = Rect::from_min_size(pos2(rect.left() + LABEL_W, y_words), vec2(track_w, TR_WORDS));
        p.rect_filled(wr, 0.0, PANEL2);
        let xa = x_of(c.start_ms as f64);
        let xb = x_of(c.end_ms as f64);
        if let Some(e) = &ed.energy {
            let mut x = wr.left();
            while x < wr.right() {
                let i = ((t_of(x) / e.step_ms as f64).max(0.0) as usize).min(e.v.len() - 1);
                let hh = e.v[i] as f32 / 255.0 * (TR_WORDS - 14.0);
                let col = if x >= xa && x <= xb { alpha(ACCENT2, 0.55) } else { Color32::from_rgba_unmultiplied(110, 116, 150, 90) };
                p.rect_filled(Rect::from_min_size(pos2(x, wr.center().y - hh / 2.0), vec2(1.5, hh.max(1.0))), 0.0, col);
                x += 2.0;
            }
        }
        p.rect_filled(Rect::from_min_max(wr.min, pos2(xa.max(wr.left()), wr.bottom())), 0.0, alpha(BG, 0.55));
        p.rect_filled(Rect::from_min_max(pos2(xb.min(wr.right()), wr.top()), wr.max), 0.0, alpha(BG, 0.55));
        // silences jump cuts will remove
        if c.jump_cut {
            if let Some(caps) = &ed.caps {
                for k in 0..caps.segments.len().saturating_sub(1) {
                    let (x0, x1) = (x_of(caps.segments[k].1 as f64), x_of(caps.segments[k + 1].0 as f64));
                    p.rect_filled(Rect::from_min_size(pos2(x0, wr.top()), vec2((x1 - x0).max(1.5), TR_WORDS)), 0.0, alpha(CLIP, 0.35));
                }
            }
        }
        let px_per_ms = track_w as f64 / span;
        if px_per_ms * 150.0 > 1.5 {
            let w0 = ed.word_at(v0).saturating_sub(1);
            for i in w0..ed.words.len() {
                let wd = &ed.words[i];
                if wd.1 as f64 > v1 {
                    break;
                }
                let x = x_of(wd.1 as f64);
                let ww = ((wd.2 - wd.1) as f64 * px_per_ms) as f32;
                let inside = wd.1 as u64 + 1 >= c.start_ms && wd.2 as u64 <= c.end_ms + 1;
                let hid = c.edits.get(&i.to_string()).map_or(false, |t| t.is_empty());
                let col = if inside { if hid { alpha(CLIP, 0.25) } else { alpha(ACCENT, 0.35) } } else { Color32::from_rgba_unmultiplied(60, 66, 100, 100) };
                p.rect_filled(Rect::from_min_size(pos2(x + 0.5, wr.bottom() - 22.0), vec2((ww - 1.0).max(1.0), 18.0)), 0.0, col);
                if ww > 18.0 {
                    let clip = p.with_clip_rect(Rect::from_min_size(pos2(x + 2.0, wr.top()), vec2((ww - 3.0).max(0.0), TR_WORDS)).intersect(rect));
                    clip.text(pos2(x + 3.0, wr.bottom() - 13.0), Align2::LEFT_CENTER, &wd.0, font(11.0), if inside { hex(0xf0f1ff) } else { hex(0x7c82a5) });
                }
            }
        }
        // trim handles
        for (x, left) in [(xa, true), (xb, false)] {
            p.rect_filled(Rect::from_min_size(pos2(x - if left { 0.0 } else { 3.0 }, y_words), vec2(3.0, TR_WORDS)), 0.0, CLIP);
            let tri = if left { vec![pos2(x, y_words), pos2(x + 9.0, y_words), pos2(x, y_words + 11.0)] } else { vec![pos2(x, y_words), pos2(x - 9.0, y_words), pos2(x, y_words + 11.0)] };
            p.add(Shape::convex_polygon(tri, CLIP, Stroke::NONE));
        }
        // captions track
        p.rect_filled(Rect::from_min_size(pos2(rect.left() + LABEL_W, y_caps), vec2(track_w, TR_CAPS)), 0.0, PANEL2);
        if let (Some(caps), true) = (&ed.caps, c.captions) {
            for ch in &caps.chunks {
                if (ch.t1 as f64) < v0 || ch.t0 as f64 > v1 {
                    continue;
                }
                let x = x_of(ch.t0 as f64);
                let ww = (x_of(ch.t1 as f64) - x - 1.0).max(2.0);
                p.rect_filled(Rect::from_min_size(pos2(x, y_caps + 3.0), vec2(ww, TR_CAPS - 6.0)), 0.0, alpha(OK, 0.35));
                if ww > 30.0 {
                    let clip = p.with_clip_rect(Rect::from_min_size(pos2(x, y_caps), vec2(ww, TR_CAPS)).intersect(rect));
                    let text = ch.words.iter().map(|q| q.0.as_str()).collect::<Vec<_>>().join(" ");
                    clip.text(pos2(x + 4.0, y_caps + TR_CAPS / 2.0), Align2::LEFT_CENTER, text, font(10.5), hex(0xd8ffee));
                }
            }
        } else {
            p.text(pos2(rect.left() + LABEL_W + 8.0, y_caps + TR_CAPS / 2.0), Align2::LEFT_CENTER, if c.captions { "…" } else { "Captions off for this clip" }, font(11.0), MUTED);
        }
        // framing track
        let fr = Rect::from_min_size(pos2(rect.left() + LABEL_W, y_frame), vec2(track_w, TR_FRAME));
        p.rect_filled(fr, 0.0, PANEL2);
        for s in Shape::dashed_line(&[pos2(fr.left(), fr.center().y), pos2(fr.right(), fr.center().y)], Stroke::new(1.0, LINE2), 3.0, 4.0) {
            p.add(s);
        }
        let fy = |cx: f32| y_frame + 5.0 + cx * (TR_FRAME - 10.0);
        if let Some(path) = &ed.path {
            if path.cx.len() > 1 {
                let line: Vec<Pos2> = path.cx.iter().enumerate().map(|(i, cx)| pos2(x_of(path.t0 as f64 + (i as u32 * path.step) as f64), fy(*cx))).collect();
                let top: Vec<Pos2> = path.cx.iter().enumerate().map(|(i, cx)| pos2(x_of(path.t0 as f64 + (i as u32 * path.step) as f64), fy(cx - path.crop_w / 2.0))).collect();
                let bot: Vec<Pos2> = path.cx.iter().enumerate().map(|(i, cx)| pos2(x_of(path.t0 as f64 + (i as u32 * path.step) as f64), fy(cx + path.crop_w / 2.0))).collect();
                let mut mesh = egui::epaint::Mesh::default();
                for i in 0..top.len() {
                    mesh.colored_vertex(top[i], alpha(ACCENT2, 0.12));
                    mesh.colored_vertex(bot[i], alpha(ACCENT2, 0.12));
                    if i > 0 {
                        let k = (i * 2) as u32;
                        mesh.add_triangle(k - 2, k - 1, k);
                        mesh.add_triangle(k - 1, k + 1, k);
                    }
                }
                p.add(Shape::mesh(mesh));
                p.add(Shape::line(line, Stroke::new(2.0, ACCENT2)));
            }
        }
        for k in &c.framing.keys {
            let c0 = pos2(x_of(k.t as f64), fy(k.x));
            p.circle_filled(c0, 5.0, CLIP);
            p.circle_stroke(c0, 5.0, Stroke::new(1.0, Color32::WHITE));
        }
        // playhead
        let px = x_of(ed.t);
        if px >= rect.left() + LABEL_W && px <= rect.right() {
            p.rect_filled(Rect::from_min_size(pos2(px - 1.0, rect.top()), vec2(2.0, rect.height())), 0.0, ACCENT2);
            p.add(Shape::convex_polygon(vec![pos2(px - 6.0, rect.top()), pos2(px + 6.0, rect.top()), pos2(px, rect.top() + 8.0)], ACCENT2, Stroke::NONE));
        }

        // ───── interaction (pointer down / move / up, as the web timeline did)
        let resp = ui.interact(rect, Id::new("detail"), Sense::click_and_drag());
        let hover = ctx.input(|i| i.pointer.hover_pos());
        let near = |x: f32, t: f64| (x - x_of(t)).abs() < 8.0;
        if let Some(hp) = hover.filter(|h| rect.contains(*h)) {
            let in_words = hp.y >= y_words && hp.y < y_caps;
            if in_words && (near(hp.x, c.start_ms as f64) || near(hp.x, c.end_ms as f64)) {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            } else if hp.y >= y_frame {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
            }
        }
        // wheel
        if resp.hovered() {
            let (sd, cmd) = ctx.input(|i| (i.smooth_scroll_delta, i.modifiers.command));
            let ed = self.ed.as_mut().unwrap();
            if cmd && sd.y != 0.0 {
                let at = hover.map(|h| t_of(h.x));
                zoom_view(ed, if sd.y < 0.0 { 1.12 } else { 1.0 / 1.12 }, at);
            } else if sd.x != 0.0 || sd.y != 0.0 {
                let d = if sd.x.abs() > sd.y.abs() { sd.x } else { sd.y };
                let span = ed.view.1 - ed.view.0;
                let dt = -(d as f64 / w as f64) * span;
                let t0 = (ed.view.0 + dt).clamp(0.0, (ed.duration() - span).max(0.0));
                ed.view = (t0, t0 + span);
            }
        }
        let pressed = ctx.input(|i| i.pointer.primary_pressed()) && resp.hovered();
        let down = ctx.input(|i| i.pointer.primary_down());
        let pos = ctx.input(|i| i.pointer.interact_pos());
        let mut mode = ctx.data(|d| d.get_temp::<Drag>(Id::new("det_drag"))).unwrap_or(Drag::None);
        let id = c.id.clone();
        if pressed {
            if let Some(pp) = pos {
                let in_words = pp.y >= y_words && pp.y < y_caps;
                if in_words && near(pp.x, c.start_ms as f64) {
                    mode = Drag::Start;
                } else if in_words && near(pp.x, c.end_ms as f64) {
                    mode = Drag::End;
                } else if pp.y >= y_frame {
                    let keys = &c.framing.keys;
                    if let Some(k) = keys.iter().position(|k| near(pp.x, k.t as f64)) {
                        if ctx.input(|i| i.modifiers.alt) || ctx.input(|i| i.pointer.button_double_clicked(egui::PointerButton::Primary)) {
                            let ed = self.ed.as_mut().unwrap();
                            ed.edit_clip(&id, "key", |x| {
                                x.framing.keys.remove(k);
                            });
                            mode = Drag::None;
                        } else {
                            mode = Drag::Key(k);
                        }
                    } else {
                        let t = t_of(pp.x).clamp(c.start_ms as f64, c.end_ms as f64).round() as u64;
                        let cx = ((pp.y - y_frame - 5.0) / (TR_FRAME - 10.0)).clamp(0.05, 0.95);
                        let ed = self.ed.as_mut().unwrap();
                        ed.edit_clip(&id, "key", |x| {
                            x.framing.keys.push(openclip_core::framing::Key { t, x: cx });
                            x.framing.keys.sort_by_key(|k| k.t);
                        });
                        let idx = self.ed.as_ref().unwrap().sel_clip().map_or(0, |c| c.framing.keys.iter().position(|k| k.t == t).unwrap_or(0));
                        mode = Drag::Key(idx);
                    }
                } else {
                    mode = Drag::Seek;
                    let ed = self.ed.as_mut().unwrap();
                    self.player.seek(ed, t_of(pp.x));
                }
            }
        } else if down && mode != Drag::None {
            if let Some(pp) = pos {
                let t = t_of(pp.x);
                let shift = ctx.input(|i| i.modifiers.shift);
                let dur = self.ed.as_ref().unwrap().duration();
                match mode {
                    Drag::Seek => {
                        let ed = self.ed.as_mut().unwrap();
                        self.player.seek(ed, t);
                    }
                    Drag::Start => {
                        let ed = self.ed.as_mut().unwrap();
                        let v = if shift { t } else { ed.snap_start(t) };
                        ed.trim_to(&id, true, v);
                    }
                    Drag::End => {
                        let ed = self.ed.as_mut().unwrap();
                        let v = if shift { t } else { ed.snap_end(t) };
                        ed.trim_to(&id, false, v.min(dur));
                    }
                    Drag::Key(i) => {
                        let cx = ((pp.y - y_frame - 5.0) / (TR_FRAME - 10.0)).clamp(0.05, 0.95);
                        let tt = t.clamp(c.start_ms as f64, c.end_ms as f64).round() as u64;
                        let ed = self.ed.as_mut().unwrap();
                        ed.edit_clip(&id, "key", |x| {
                            if let Some(k) = x.framing.keys.get_mut(i) {
                                k.x = cx;
                                k.t = tt;
                            }
                        });
                    }
                    _ => {}
                }
            }
        } else if !down && mode != Drag::None {
            if let Drag::Key(_) = mode {
                let ed = self.ed.as_mut().unwrap();
                ed.edit_clip(&id, "key", |x| x.framing.keys.sort_by_key(|k| k.t));
            }
            mode = Drag::None;
        }
        ctx.data_mut(|d| d.insert_temp(Id::new("det_drag"), mode));
    }
}
