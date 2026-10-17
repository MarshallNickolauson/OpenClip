//! The editor screen: top bar, clip list and the 9:16 stage. The inspector and timeline live in
//! their own modules.

use eframe::egui::{self, epaint, pos2, vec2, Align2, Color32, Id, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui};

use crate::app::{After, App, Modal};
use crate::home::{dashed_rounded, ellipsize};
use crate::state::Editor;
use crate::theme::*;

pub fn fmt(ms: f64, tenths: bool) -> String {
    let ms = ms.max(0.0);
    let s = ms / 1000.0;
    let m = (s / 60.0).floor();
    let r = s - m * 60.0;
    let base = format!("{}:{:02}", m as u64, r.floor() as u64);
    if tenths { format!("{base}.{}", ((r % 1.0) * 10.0).floor() as u64) } else { base }
}
pub fn fmt_long(ms: f64) -> String {
    let s = (ms / 1000.0).floor() as u64;
    let h = s / 3600;
    if h > 0 { format!("{h}:{:02}:{:02}", (s / 60) % 60, s % 60) } else { fmt(ms, false) }
}
pub fn secs(ms: f64) -> String {
    format!("{}s", (ms / 1000.0).round() as u64)
}

impl App {
    pub fn editor_ui(&mut self, ctx: &egui::Context, ui: &mut Ui, full: Rect) {
        {
            let ed = self.ed.as_mut().unwrap();
            ed.refresh();
        }
        let tl_h = self.ed.as_ref().unwrap().tl_h.clamp(200.0, full.height() * 0.6);
        let top = Rect::from_min_size(full.min, vec2(full.width(), 48.0));
        let tl = Rect::from_min_max(pos2(full.left(), full.bottom() - tl_h), full.max);
        let main = Rect::from_min_max(pos2(full.left(), top.bottom()), pos2(full.right(), tl.top()));
        let clips = Rect::from_min_size(main.min, vec2(330.0, main.height()));
        let side = Rect::from_min_max(pos2(main.right() - 340.0, main.top()), main.max);
        let stage = Rect::from_min_max(pos2(clips.right(), main.top()), pos2(side.left(), main.bottom()));

        self.stage_ui(ctx, ui, stage);
        self.clips_ui(ctx, ui, clips);
        self.inspector_ui(ctx, ui, side);
        self.timeline_ui(ctx, ui, tl);
        self.top_ui(ctx, ui, top);
    }

    // ───────── top bar
    fn top_ui(&mut self, ctx: &egui::Context, ui: &mut Ui, rect: Rect) {
        fill(ui, rect, 0, PANEL);
        ui.painter().hline(rect.x_range(), rect.bottom() - 0.5, Stroke::new(1.0, LINE));
        let drag = ui.interact(rect, Id::new("top_drag"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        if drag.double_clicked() {
            let max = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!max));
        }
        let inner = Rect::from_min_max(pos2(rect.left() + 82.0, rect.top()), pos2(rect.right() - 14.0, rect.bottom()));
        let mut row = Row::new(inner, 8.0);

        let r = row.left(32.0, 32.0);
        if icon_button(ui, &self.icons, Id::new("x_link"), r, "xlogo", false, "OpenClip on X (@mdnlabs)").clicked() {
            open_url("https://x.com/mdnlabs");
        }
        let r = row.left(32.0, 32.0);
        row.x -= 4.0; // the GitHub button sits closer to Projects
        if icon_button(ui, &self.icons, Id::new("gh_link"), r, "github", false, "OpenClip on GitHub").clicked() {
            open_url("https://github.com/MarshallNickolauson/OpenClip");
        }
        // Projects
        let pw = 6.0 + 16.0 + 4.0 + text_w(ui, "Projects", font(13.0)) + 10.0;
        let pr = row.left(pw, 32.0);
        let resp = ui.interact(pr, Id::new("btn_home"), Sense::click());
        if resp.hovered() {
            fill(ui, pr, 7, RAISE);
        }
        self.icons.paint(ui, "back", Rect::from_min_size(pos2(pr.left() + 6.0, pr.center().y - 8.0), vec2(16.0, 16.0)), if resp.hovered() { Color32::WHITE } else { hex(0xcdd2ea) });
        label(ui, pos2(pr.left() + 26.0, pr.center().y), Align2::LEFT_CENTER, "Projects", font(13.0), if resp.hovered() { Color32::WHITE } else { hex(0xcdd2ea) });
        if resp.clicked() {
            self.request_leave(ctx, After::Home);
        }
        // title
        let (title, dirty, editing) = {
            let ed = self.ed.as_ref().unwrap();
            (ed.project.title.clone(), ed.dirty, ed.editing_title.clone())
        };
        row.x -= 4.0;
        if let Some(mut buf) = editing {
            let tr = row.left(320.0, 28.0);
            let te = egui::TextEdit::singleline(&mut buf).font(font(13.5)).frame(egui::Frame::NONE).margin(vec2(6.0, 4.0));
            fill(ui, tr, 6, BG);
            border(ui, tr, 6, ACCENT2);
            let out = ui.put(tr, te);
            let ed = self.ed.as_mut().unwrap();
            if !out.has_focus() && !out.gained_focus() && out.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                let v = buf.trim().to_string();
                if !v.is_empty() && v != ed.project.title {
                    ed.mutate("", |p| p.title = v);
                }
                ed.editing_title = None;
            } else if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                ed.editing_title = None;
            } else {
                ed.editing_title = Some(buf);
                out.request_focus();
            }
        } else {
            let shown = format!("{}{}", title, if dirty { " *" } else { "" });
            let tw = (text_w(ui, &shown, bold(13.5)) + 18.0).min(inner.width() * 0.38);
            let tr = row.left(tw, 30.0);
            ui.painter().vline(tr.left(), tr.y_range().shrink(0.0), Stroke::new(1.0, LINE2));
            let resp = ui.interact(tr, Id::new("title"), Sense::click());
            if resp.hovered() {
                fill(ui, tr, 6, alpha(Color32::WHITE, 0.06));
                ctx.set_cursor_icon(egui::CursorIcon::Text);
            }
            let shown = ellipsize(ui, &shown, bold(13.5), tw - 18.0);
            label(ui, pos2(tr.left() + 9.0, tr.center().y), Align2::LEFT_CENTER, &shown, bold(13.5), TEXT);
            if resp.clicked() {
                self.ed.as_mut().unwrap().editing_title = Some(title);
            }
        }
        // face chip
        if let Some(p) = self.ed.as_ref().unwrap().face_progress {
            let t = format!("Tracking faces {}%", p.round() as i32);
            let w = text_w(ui, &t, font(11.5)) + 18.0;
            row.x += 6.0;
            let cr = row.left(w, 22.0);
            border(ui, cr, 11, alpha(ACCENT2, 0.4));
            label(ui, cr.center(), Align2::CENTER_CENTER, &t, font(11.5), ACCENT2);
        }

        // right side, from the edge inwards: Export, redo, undo, Save, Claude
        let (picked, can_undo, can_redo) = {
            let ed = self.ed.as_ref().unwrap();
            (ed.picked().len(), ed.can_undo(), ed.can_redo())
        };
        let ex_label = if picked > 0 { format!("Export {picked}") } else { "Export".to_string() };
        let ew = text_w(ui, &ex_label, bold(13.0)) + 30.0 + 25.0;
        let er = row.right(ew, 34.0);
        if primary(ui, Id::new("btn_export"), er, &ex_label, Some(&self.icons), "upload").clicked() {
            if picked == 0 {
                self.ed.as_mut().unwrap().tab = crate::state::Tab::Export;
                self.toast("Tick the clips you want to export");
            } else {
                self.run_export(ctx);
            }
        }
        let rr = row.right(32.0, 32.0);
        if icon_button(ui, &self.icons, Id::new("btn_redo"), rr, "redo", false, "Redo (⇧⌘Z)").clicked() {
            self.ed.as_mut().unwrap().redo();
        }
        let ur = row.right(32.0, 32.0);
        if icon_button(ui, &self.icons, Id::new("btn_undo"), ur, "undo", false, "Undo (⌘Z)").clicked() {
            self.ed.as_mut().unwrap().undo();
        }
        let _ = (can_undo, can_redo);
        let sw = pill_size(ui, "Save", true);
        let sr = row.right(sw.x, sw.y);
        if pill(ui, Id::new("btn_save"), sr, "Save", dirty, true, dirty).clicked() {
            self.ed.as_mut().unwrap().save();
            self.toast("Saved");
        }
        let live = self.claude_seen.map_or(false, |t| now_ms().saturating_sub(t) < 10 * 60 * 1000);
        let lab = if live { "Claude connected" } else { "Claude" };
        let cw = text_w(ui, lab, font(13.0)) + 11.0 * 2.0 + 8.0 + 7.0 + 2.0;
        let cr = row.right(cw, pill_size(ui, lab, false).y);
        let resp = pill(ui, Id::new("btn_claude"), cr, "", false, false, true);
        let dot = pos2(cr.left() + 11.0 + 4.0, cr.center().y);
        if live {
            ui.painter().circle_filled(dot, 7.0, alpha(OK, 0.18));
            ui.painter().circle_filled(dot, 4.0, OK);
        } else {
            ui.painter().circle_filled(dot, 4.0, hex(0x5b6285));
        }
        label(ui, pos2(cr.left() + 11.0 + 8.0 + 7.0, cr.center().y), Align2::LEFT_CENTER, lab, font(13.0), SOFT);
        if resp.on_hover_text("Connect Claude (MCP)").clicked() {
            self.modal = Modal::Connect { result: None, mcp_path: App::mcp_path(), mcp_exists: std::path::Path::new(&App::mcp_path()).exists() };
        }
    }

    // ───────── clip list
    fn clips_ui(&mut self, ctx: &egui::Context, ui: &mut Ui, rect: Rect) {
        fill(ui, rect, 0, PANEL);
        ui.painter().vline(rect.right() - 0.5, rect.y_range(), Stroke::new(1.0, LINE));
        let head = Rect::from_min_size(rect.min, vec2(rect.width() - 1.0, 52.0));
        ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, LINE));
        let (count, sort_score) = {
            let ed = self.ed.as_ref().unwrap();
            (ed.live().len(), ed.sort_score)
        };
        let title = if count > 0 { format!("{count} clip{}", if count == 1 { "" } else { "s" }) } else { "Clips".into() };
        label(ui, pos2(head.left() + 16.0, head.center().y), Align2::LEFT_CENTER, &title, bold(13.5), TEXT);
        let plus = Rect::from_center_size(pos2(head.right() - 12.0 - 16.0, head.center().y), vec2(32.0, 32.0));
        if icon_button(ui, &self.icons, Id::new("add_clip"), plus, "plus", false, "New clip at the playhead (N)").clicked() {
            let ed = self.ed.as_mut().unwrap();
            let (a, b) = (ed.snap_start(ed.t), ed.snap_end((ed.t + 30000.0).min(ed.duration())));
            let id = ed.new_clip(a, b);
            self.select_clip(Some(id), false);
        }
        let opts = ["Time", "Best"];
        let sw = seg_size(ui, &opts, 9.0, 12.0);
        if let Some(i) = seg(ui, Id::new("sort_seg"), pos2(plus.left() - 8.0 - sw, head.center().y - 11.5), &opts, sort_score as usize, 9.0, 23.0, 12.0) {
            self.ed.as_mut().unwrap().sort_score = i == 1;
        }

        let list = Rect::from_min_max(pos2(rect.left(), head.bottom()), pos2(rect.right() - 1.0, rect.bottom()));
        let items: Vec<openclip_core::project::Clip> = self.ed.as_ref().unwrap().ordered().into_iter().cloned().collect();
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(list));
        child.set_clip_rect(list.intersect(ui.clip_rect()));
        if items.is_empty() {
            self.empty_card(ctx, &mut child, list);
            return;
        }
        let mut select: Option<String> = None;
        let mut pick: Option<String> = None;
        let mut dismiss: Option<String> = None;
        let sel = self.ed.as_ref().unwrap().sel.clone();
        let fresh = self.ed.as_ref().unwrap().fresh.clone();
        let mut want = Vec::new();
        egui::ScrollArea::vertical().auto_shrink([false, false]).id_salt("clip_scroll").show(&mut child, |ui| {
            let x0 = list.left() + 10.0;
            let w = list.width() - 20.0;
            let mut y = list.top() + 10.0;
            let card_h = 118.0;
            for c in &items {
                let card = Rect::from_min_size(pos2(x0, y), vec2(w, card_h));
                let active = sel.as_deref() == Some(c.id.as_str());
                let resp = ui.interact(card, Id::new(("card", &c.id)), Sense::click());
                let dim = if c.picked { 1.0 } else { 0.55 };
                let a = |col: Color32| alpha(col, dim);
                fill(ui, card, 10, if active { hex(0x16243a) } else { PANEL2 });
                let bc = if fresh.contains(&c.id) { ACCENT2 } else if active { ACCENT } else if resp.hovered() { LINE2 } else { Color32::TRANSPARENT };
                border(ui, card, 10, bc);
                // thumbnail
                let th = Rect::from_min_size(pos2(card.left() + 9.0, card.top() + 9.0), vec2(56.0, 56.0 * 16.0 / 9.0));
                fill(ui, th, 7, hex(0x0a0e18));
                let ed = self.ed.as_ref().unwrap();
                if let Some((_, tex)) = ed.thumbs.get(&c.id) {
                    ui.painter().add(epaint::RectShape::filled(th, r(7), Color32::WHITE).with_texture(tex.id(), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
                    if dim < 1.0 {
                        fill(ui, th, 7, alpha(PANEL2, 1.0 - dim));
                    }
                }
                want.push(c.clone());
                let dur = secs((c.end_ms - c.start_ms) as f64);
                let dw = text_w(ui, &dur, font(10.0)) + 8.0;
                let db = Rect::from_min_size(pos2(th.right() - 3.0 - dw, th.bottom() - 3.0 - 15.0), vec2(dw, 15.0));
                fill(ui, db, 4, alpha(Color32::BLACK, 0.65));
                label(ui, db.center(), Align2::CENTER_CENTER, &dur, font(10.0), Color32::WHITE);
                // text
                let tx = th.right() + 10.0;
                let tw = card.right() - 9.0 - 26.0 - 10.0 - tx;
                let lines = wrap_lines(ui, &c.title, bold(13.0), tw, 2);
                let mut ty = card.top() + 9.0;
                for l in &lines {
                    label(ui, pos2(tx, ty + 8.5), Align2::LEFT_CENTER, l, bold(13.0), a(TEXT));
                    ty += 17.0;
                }
                ty += 3.0;
                let range = format!("{}–{}", fmt(c.start_ms as f64, false), fmt(c.end_ms as f64, false));
                let mut sx = tx;
                let r1 = label(ui, pos2(sx, ty + 8.0), Align2::LEFT_CENTER, &range, font(11.5), a(MUTED));
                sx = r1.right() + 8.0;
                if c.score > 0.0 {
                    let s = format!("{}", c.score.round() as i32);
                    let sw = text_w(ui, &s, bold(10.5)) + 12.0;
                    let sb = Rect::from_min_size(pos2(sx, ty + 8.0 - 8.5), vec2(sw, 17.0));
                    let hi = c.score >= 80.0;
                    fill(ui, sb, 5, alpha(if hi { OK } else { CLIP }, 0.16 * dim));
                    label(ui, sb.center(), Align2::CENTER_CENTER, &s, bold(10.5), a(if hi { OK } else { CLIP }));
                    sx = sb.right() + 8.0;
                }
                if c.origin != openclip_core::project::Origin::Manual {
                    let o = if c.origin == openclip_core::project::Origin::Claude { "CLAUDE" } else { "AUTO" };
                    let r2 = label(ui, pos2(sx, ty + 8.0), Align2::LEFT_CENTER, o, font(10.0), a(ACCENT2));
                    sx = r2.right() + 8.0;
                }
                if c.exported.is_some() || ed.export.get(&c.id).map_or(false, |x| x.state == "done") {
                    label(ui, pos2(sx, ty + 8.0), Align2::LEFT_CENTER, "✓", font(11.0), a(OK));
                }
                ty += 17.0 + 4.0;
                if !c.why.is_empty() && c.why != "null" {
                    for l in wrap_lines(ui, &c.why, font(11.5), tw, 2) {
                        label(ui, pos2(tx, ty + 8.0), Align2::LEFT_CENTER, &l, font(11.5), a(hex(0xa2a8c4)));
                        ty += 16.0;
                    }
                }
                // checkbox
                let cb = Rect::from_min_size(pos2(card.right() - 9.0 - 22.0, card.top() + 9.0), vec2(22.0, 22.0));
                let cr = ui.interact(cb, Id::new(("pick", &c.id)), Sense::click());
                fill(ui, cb, 6, if c.picked { ACCENT } else { BG });
                border(ui, cb, 6, if c.picked { ACCENT } else { LINE2 });
                if c.picked {
                    self.icons.paint(ui, "check", Rect::from_center_size(cb.center(), vec2(14.0, 14.0)), Color32::WHITE);
                }
                cr.clone().on_hover_text("Include in export (X)");
                // dismiss
                let xb = Rect::from_min_size(pos2(card.right() - 6.0 - 22.0, card.bottom() - 6.0 - 22.0), vec2(22.0, 22.0));
                let xr = if resp.hovered() || cr.hovered() { Some(ui.interact(xb, Id::new(("dis", &c.id)), Sense::click())) } else { None };
                if let Some(xr) = &xr {
                    if xr.hovered() {
                        fill(ui, xb, 6, RAISE);
                    }
                    self.icons.paint(ui, "x", Rect::from_center_size(xb.center(), vec2(14.0, 14.0)), if xr.hovered() { hex(0xff8a97) } else { hex(0x9aa0bd) });
                }
                if cr.clicked() {
                    pick = Some(c.id.clone());
                } else if xr.as_ref().map_or(false, |x| x.clicked()) {
                    dismiss = Some(c.id.clone());
                } else if resp.clicked() {
                    select = Some(c.id.clone());
                }
                y += card_h + 8.0;
            }
            ui.allocate_rect(Rect::from_min_max(pos2(list.left(), list.top()), pos2(list.right(), y + 2.0)), Sense::hover());
        });
        for c in want {
            self.want_thumb(ctx, &c);
        }
        if let Some(ed) = &mut self.ed {
            if !ed.fresh.is_empty() && ctx.input(|i| i.time) > ed.fresh_until {
                ed.fresh.clear();
            }
        }
        if let Some(id) = pick {
            self.ed.as_mut().unwrap().toggle_pick(&id);
        }
        if let Some(id) = dismiss {
            let next = self.ed.as_mut().unwrap().dismiss(&id);
            self.select_clip(next, false);
        } else if let Some(id) = select {
            let auto = self.ed.as_ref().unwrap().auto_play;
            self.select_clip(Some(id), auto);
        }
    }

    fn empty_card(&mut self, _ctx: &egui::Context, ui: &mut Ui, list: Rect) {
        let ready = self.ed.as_ref().unwrap().project.transcribed;
        let card = Rect::from_min_size(pos2(list.left() + 16.0, list.top() + 16.0), vec2(list.width() - 32.0, 232.0));
        dashed_rounded(ui, card, 12.0, Stroke::new(1.0, LINE2));
        label(ui, pos2(card.left() + 16.0, card.top() + 28.0), Align2::LEFT_CENTER, if ready { "No clips yet" } else { "Transcribing…" }, bold(14.0), TEXT);
        let text = if ready {
            "Ask Claude to pick the best moments, or let OpenClip take a quick pass with its built-in scorer. You can also drag across the overview bar below to mark a clip yourself."
        } else {
            "Clips appear here once the talk has been transcribed."
        };
        let mut y = card.top() + 52.0;
        for l in wrap_lines(ui, text, font(13.0), card.width() - 32.0, 8) {
            label(ui, pos2(card.left() + 16.0, y + 8.0), Align2::LEFT_CENTER, &l, font(13.0), MUTED);
            y += 19.0;
        }
        if ready {
            let b1 = Rect::from_min_size(pos2(card.left() + 16.0, card.bottom() - 16.0 - 34.0 - 8.0 - 34.0), vec2(card.width() - 32.0, 34.0));
            if primary(ui, Id::new("empty_claude"), b1, "Find clips with Claude", None, "").clicked() {
                self.modal = Modal::Connect { result: None, mcp_path: App::mcp_path(), mcp_exists: std::path::Path::new(&App::mcp_path()).exists() };
            }
            let b2 = Rect::from_min_size(pos2(b1.left(), b1.bottom() + 8.0), vec2(b1.width(), 34.0));
            let resp = ui.interact(b2, Id::new("empty_quick"), Sense::click());
            fill(ui, b2, 7, if resp.hovered() { RAISE } else { PANEL2 });
            dashed_rounded(ui, b2, 7.0, Stroke::new(1.0, LINE2));
            label(ui, b2.center(), Align2::CENTER_CENTER, "Quick pick without Claude", font(13.0), hex(0xd0d4ea));
            if resp.clicked() {
                self.toast("Scoring the talk…");
                self.quick_pick();
            }
        }
    }

    // ───────── stage
    fn stage_ui(&mut self, ctx: &egui::Context, ui: &mut Ui, rect: Rect) {
        // soft glow
        let mut mesh = epaint::Mesh::default();
        let glow = hex(0x0e1624);
        let g = Rect::from_min_size(rect.min, vec2(rect.width(), rect.height() * 0.7));
        mesh.colored_vertex(g.left_top(), glow);
        mesh.colored_vertex(g.right_top(), glow);
        mesh.colored_vertex(g.right_bottom(), BG);
        mesh.colored_vertex(g.left_bottom(), BG);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        ui.painter().add(Shape::mesh(mesh));

        let wrap = Rect::from_min_max(pos2(rect.left() + 18.0, rect.top() + 14.0), pos2(rect.right() - 18.0, rect.bottom() - (46.0 + 8.0 + 10.0)));
        let (aw, ah) = (wrap.width() - 8.0, wrap.height() - 8.0);
        let bh = ah.min(aw * 16.0 / 9.0).max(120.0);
        let bw = bh * 9.0 / 16.0;
        let phone = Rect::from_center_size(wrap.center(), vec2(bw, bh));

        // size the preview texture in physical pixels, bounded
        let ppp = ctx.pixels_per_point();
        self.phone_px = ((bw * ppp).round().clamp(120.0, 720.0) as usize & !1, (bh * ppp).round().clamp(200.0, 1280.0) as usize & !1);
        let px = self.phone_px;
        {
            let ed = self.ed.as_mut().unwrap();
            self.player.tick(ctx, ed, px);
        }
        let ed = self.ed.as_ref().unwrap();

        // frame + ring + shadow
        ui.painter().add(epaint::Shadow { offset: [0, 24], blur: 60, spread: 0, color: alpha(Color32::BLACK, 0.55) }.as_shape(phone, r(14)));
        fill(ui, phone.expand(1.0), 15, LINE2);
        fill(ui, phone, 14, Color32::BLACK);
        if let Some(tex) = &self.player.tex {
            ui.painter().add(epaint::RectShape::filled(phone, r(14), Color32::WHITE).with_texture(tex.id(), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))));
        }
        let clip_ui = ui.painter().with_clip_rect(phone);
        if ed.caps_on && ed.sel_clip().map_or(false, |c| c.captions) {
            draw_captions(ui, &clip_ui, phone, ed);
        }
        if ed.guides {
            let g = Rect::from_min_max(pos2(phone.left() + phone.width() * 0.04, phone.top() + phone.height() * 0.12), pos2(phone.right() - phone.width() * 0.16, phone.bottom() - phone.height() * 0.20));
            dashed_rounded(ui, g, 6.0, Stroke::new(1.5, alpha(Color32::WHITE, 0.55)));
        }
        if self.player.failed {
            fill(ui, phone, 14, alpha(hex(0x080910), 0.9));
            let msg = "This file can't be previewed here (unsupported codec or the file moved).";
            for (i, l) in wrap_lines(ui, msg, font(13.0), phone.width() - 48.0, 5).iter().enumerate() {
                label(ui, pos2(phone.center().x, phone.center().y - 20.0 + i as f32 * 20.0), Align2::CENTER_CENTER, l, font(13.0), hex(0xd5d9ee));
            }
        }

        // dragging the picture pins a manual framing keyframe
        let resp = ui.interact(phone, Id::new("phone"), Sense::click_and_drag());
        if resp.hovered() {
            ctx.set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        if resp.drag_started() {
            let (cx, _, _, _) = self.ed.as_ref().unwrap().crop_at(self.ed.as_ref().unwrap().t);
            ctx.data_mut(|d| d.insert_temp(Id::new("drag_cx"), cx));
        }
        if resp.dragged() {
            let ed = self.ed.as_mut().unwrap();
            let cx0: f64 = ctx.data(|d| d.get_temp(Id::new("drag_cx"))).unwrap_or(0.5);
            let (_, _, cw, _) = ed.crop_at(ed.t);
            let dx = resp.drag_delta().x as f64;
            let total = ctx.input(|i| i.pointer.press_origin().map_or(0.0, |o| i.pointer.interact_pos().map_or(0.0, |p| (p.x - o.x) as f64)));
            let _ = dx;
            let nx = (cx0 - total / bw as f64 * cw).clamp(cw / 2.0, 1.0 - cw / 2.0) as f32;
            let t = ed.t.round() as u64;
            if let Some(id) = ed.sel.clone() {
                ed.edit_clip(&id, "key", |c| {
                    let keys = &mut c.framing.keys;
                    if let Some(k) = keys.iter_mut().find(|k| (k.t as i64 - t as i64).abs() < 250) {
                        k.x = nx;
                    } else {
                        keys.push(openclip_core::framing::Key { t, x: nx });
                    }
                    keys.sort_by_key(|k| k.t);
                });
            }
        }

        // transport
        let ed = self.ed.as_ref().unwrap();
        let c = ed.sel_clip().cloned();
        let cur = c.as_ref().map_or(0.0, |c| (ed.t - c.start_ms as f64).max(0.0));
        let dur = c.as_ref().map_or(0.0, |c| ed.eff_len(c));
        let playing = self.player.playing();
        let labels = [("Loop", ed.looping), ("Auto-play", ed.auto_play), ("Jump cuts", c.as_ref().map_or(false, |c| c.jump_cut)), ("Captions", ed.caps_on), ("Guides", ed.guides)];
        let pw: Vec<f32> = labels.iter().map(|(t, _)| pill_size(ui, t, true).x).collect();
        let total = 12.0 + 56.0 + 8.0 + 32.0 + 8.0 + 36.0 + 8.0 + 32.0 + 8.0 + 56.0 + 8.0 + 1.0 + 8.0 + pw.iter().sum::<f32>() + 8.0 * (pw.len() as f32 - 1.0) + 12.0;
        let bar = Rect::from_center_size(pos2(rect.center().x, rect.bottom() - 10.0 - 23.0), vec2(total.min(rect.width() - 36.0), 46.0));
        fill(ui, bar, 12, PANEL);
        border(ui, bar, 12, LINE);
        let mut x = bar.left() + 12.0;
        label(ui, pos2(x + 28.0, bar.center().y), Align2::CENTER_CENTER, &fmt(cur, true), font(12.0), MUTED);
        x += 56.0 + 8.0;
        let b1 = Rect::from_min_size(pos2(x, bar.center().y - 16.0), vec2(32.0, 32.0));
        x += 40.0;
        let pb = Rect::from_min_size(pos2(x, bar.center().y - 16.0), vec2(36.0, 32.0));
        x += 44.0;
        let b3 = Rect::from_min_size(pos2(x, bar.center().y - 16.0), vec2(32.0, 32.0));
        x += 40.0;
        if icon_button(ui, &self.icons, Id::new("t_start"), b1, "skip_back", false, "Restart clip (Q)").clicked() {
            if let Some(c) = &c {
                let ed = self.ed.as_mut().unwrap();
                self.player.seek(ed, c.start_ms as f64);
            }
        }
        let presp = ui.interact(pb, Id::new("t_play"), Sense::click());
        fill(ui, pb, 8, if presp.hovered() { ACCENT2 } else { ACCENT });
        self.icons.paint(ui, if playing { "pause" } else { "play" }, Rect::from_center_size(pb.center(), vec2(18.0, 18.0)), Color32::WHITE);
        if presp.on_hover_text("Play / pause (Space)").clicked() {
            let ed = self.ed.as_ref().unwrap();
            self.player.toggle(ed);
        }
        if icon_button(ui, &self.icons, Id::new("t_next"), b3, "skip_next", false, "Next clip (↓)").clicked() {
            self.step_clip(1);
        }
        label(ui, pos2(x + 28.0, bar.center().y), Align2::CENTER_CENTER, &fmt(dur, true), font(12.0), MUTED);
        x += 56.0 + 8.0;
        ui.painter().vline(x, Rangef_(bar.center().y - 10.0, bar.center().y + 10.0), Stroke::new(1.0, LINE2));
        x += 1.0 + 8.0 + 4.0;
        let tips = ["Loop (L)", "Play automatically when a clip is selected", "Cut silences in this clip (T)", "Captions on / off (C)", "Platform safe zones"];
        let mut clicked = None;
        for (i, ((t, on), w)) in labels.iter().zip(&pw).enumerate() {
            let pr = Rect::from_min_size(pos2(x, bar.center().y - 13.5), vec2(*w, 27.0));
            if pill(ui, Id::new(("tp", i)), pr, t, *on, true, true).on_hover_text(tips[i]).clicked() {
                clicked = Some(i);
            }
            x += w + 8.0;
        }
        let id = c.as_ref().map(|c| c.id.clone());
        let ed = self.ed.as_mut().unwrap();
        match clicked {
            Some(0) => ed.looping = !ed.looping,
            Some(1) => ed.auto_play = !ed.auto_play,
            Some(2) => {
                if let Some(id) = id {
                    ed.edit_clip(&id, "cuts", |c| c.jump_cut = !c.jump_cut);
                }
            }
            Some(3) => ed.caps_on = !ed.caps_on,
            Some(4) => ed.guides = !ed.guides,
            _ => {}
        }
    }
}

#[allow(non_snake_case)]
fn Rangef_(a: f32, b: f32) -> egui::Rangef {
    egui::Rangef::new(a, b)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

pub fn open_url(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

/// Greedy word wrap to at most `max_lines` lines (the last one ellipsised).
pub fn wrap_lines(ui: &Ui, text: &str, f: egui::FontId, max_w: f32, max_lines: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut words = text.split_whitespace().peekable();
    while let Some(w) = words.next() {
        let try_line = if cur.is_empty() { w.to_string() } else { format!("{cur} {w}") };
        if text_w(ui, &try_line, f.clone()) <= max_w || cur.is_empty() {
            cur = try_line;
        } else {
            lines.push(std::mem::take(&mut cur));
            cur = w.to_string();
            if lines.len() == max_lines {
                break;
            }
        }
        if words.peek().is_none() {
            lines.push(std::mem::take(&mut cur));
        }
    }
    if lines.len() > max_lines {
        lines.truncate(max_lines);
    }
    if lines.len() == max_lines {
        let used: usize = lines.iter().map(|l| l.split_whitespace().count()).sum();
        if used < text.split_whitespace().count() {
            let last = lines.pop().unwrap();
            lines.push(ellipsize(ui, &format!("{last} …"), f.clone(), max_w + 0.0));
        }
    }
    lines
}

/// Draw the burned-in captions the way the exporter does.
pub fn draw_captions(ui: &Ui, painter: &egui::Painter, phone: Rect, ed: &Editor) {
    let Some(caps) = &ed.caps else { return };
    let t = ed.t as u32;
    let Some((ci, chunk)) = caps.chunks.iter().enumerate().find(|(_, c)| t >= c.t0 && t < c.t1) else { return };
    let _ = ci;
    let st = &caps.style;
    let bw = phone.width();
    let size = st.size * bw;
    let f = cap_font(&st.font, st.bold, size);
    let words: Vec<String> = chunk.words.iter().map(|w| if st.upper { w.0.to_uppercase() } else { w.0.clone() }).collect();
    let active = chunk.words.iter().rposition(|w| w.1 <= t).unwrap_or(0);
    let max_w = st.max_w * bw;
    let space = size * 0.28;
    let widths: Vec<f32> = words.iter().map(|w| text_w(ui, w, f.clone())).collect();
    // wrap
    let mut rows: Vec<Vec<usize>> = vec![vec![]];
    let mut rw = 0.0;
    for (i, w) in widths.iter().enumerate() {
        if rw > 0.0 && rw + space + w > max_w {
            rows.push(vec![]);
            rw = 0.0;
        }
        rows.last_mut().unwrap().push(i);
        rw += if rw > 0.0 { space } else { 0.0 } + w;
    }
    let line_h = size * 1.12;
    let block_h = rows.len() as f32 * line_h;
    let top = phone.top() + st.y * phone.height() - block_h / 2.0;
    let fill_c = parse_hex(&st.fill);
    let accent = parse_hex(&st.accent);
    let stroke_c = parse_hex(&st.stroke);
    let bg = st.bg.as_deref().map(parse_hex);
    let sw = st.stroke_w * bw;
    for (ri, row) in rows.iter().enumerate() {
        let total: f32 = row.iter().map(|&i| widths[i]).sum::<f32>() + space * (row.len() as f32 - 1.0);
        let mut x = phone.center().x - total / 2.0;
        let cy = top + ri as f32 * line_h + line_h / 2.0;
        if let Some(bg) = bg {
            let b = Rect::from_center_size(pos2(phone.center().x, cy), vec2(total + size * 0.9, line_h * 0.92));
            painter.rect_filled(b, r((size * 0.3) as u8), bg);
        }
        for &i in row {
            let pos = pos2(x, cy);
            if st.shadow {
                painter.text(pos + vec2(0.0, size * 0.05), Align2::LEFT_CENTER, &words[i], f.clone(), alpha(Color32::BLACK, 0.65));
            }
            if sw > 0.0 {
                for k in 0..16 {
                    let a = k as f32 / 16.0 * std::f32::consts::TAU;
                    painter.text(pos + vec2(a.cos() * sw, a.sin() * sw), Align2::LEFT_CENTER, &words[i], f.clone(), stroke_c);
                }
            }
            painter.text(pos, Align2::LEFT_CENTER, &words[i], f.clone(), if i == active { accent } else { fill_c });
            x += widths[i] + space;
        }
    }
}

pub fn parse_hex(s: &str) -> Color32 {
    let h = s.trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).unwrap_or(0xffffff);
    if h.len() >= 8 {
        Color32::from_rgba_unmultiplied((v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8)
    } else {
        Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
    }
}

#[allow(dead_code)]
fn _s(_: Pos2, _: StrokeKind) {}
