//! The home screen: drop a video, pick a transcription model, reopen a project.

use eframe::egui::{self, epaint, pos2, vec2, Align2, Color32, Id, Pos2, Rect, Sense, Shape, Stroke, Ui};

use crate::app::{App, Modal};
use crate::theme::*;

pub fn rounded_path(rect: Rect, rad: f32) -> Vec<Pos2> {
    let mut pts = Vec::new();
    let corners = [
        (pos2(rect.right() - rad, rect.top() + rad), -90.0f32),
        (pos2(rect.right() - rad, rect.bottom() - rad), 0.0),
        (pos2(rect.left() + rad, rect.bottom() - rad), 90.0),
        (pos2(rect.left() + rad, rect.top() + rad), 180.0),
    ];
    for (c, start) in corners {
        for i in 0..=8 {
            let a = (start + i as f32 * 90.0 / 8.0).to_radians();
            pts.push(pos2(c.x + a.cos() * rad, c.y + a.sin() * rad));
        }
    }
    pts.push(pts[0]);
    pts
}

pub fn dashed_rounded(ui: &Ui, rect: Rect, rad: f32, stroke: Stroke) {
    let pts = rounded_path(rect, rad);
    for s in Shape::dashed_line(&pts, stroke, 6.0, 5.0) {
        ui.painter().add(s);
    }
}

impl App {
    pub fn home_ui(&mut self, ctx: &egui::Context, ui: &mut Ui, full: Rect) {
        // soft glow at the top, as in the web UI's radial gradient
        let glow = hex(0x0e1624);
        let mut mesh = epaint::Mesh::default();
        let h = full.height() * 0.6;
        let r = Rect::from_min_size(full.min, vec2(full.width(), h));
        mesh.colored_vertex(r.left_top(), glow);
        mesh.colored_vertex(r.right_top(), glow);
        mesh.colored_vertex(r.right_bottom(), BG);
        mesh.colored_vertex(r.left_bottom(), BG);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        ui.painter().add(Shape::mesh(mesh));

        // the whole top strip drags the window
        let drag = ui.interact(Rect::from_min_size(full.min, vec2(full.width(), 40.0)), Id::new("home_drag"), Sense::click_and_drag());
        if drag.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }

        let wrap_w = (full.width() - 64.0).min(860.0);
        let left = full.center().x - wrap_w / 2.0;
        let scroll_rect = Rect::from_min_max(pos2(full.left(), full.top() + 40.0), full.max);
        let mut open: Option<std::path::PathBuf> = None;
        let mut delete: Option<(String, String)> = None;
        let mut chosen: Option<String> = None;

        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(scroll_rect));
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut child, |ui| {
            let top0 = ui.cursor().min.y;
            let origin = pos2(left, top0 + 24.0);
            let mut y = origin.y;

            // brand
            let icon = Rect::from_min_size(pos2(left, y), vec2(64.0, 64.0));
            self.icons_app_icon(ui, icon);
            label(ui, pos2(left + 80.0, y + 22.0), Align2::LEFT_CENTER, "OpenClip", bold(26.0), TEXT);
            label(ui, pos2(left + 80.0, y + 48.0), Align2::LEFT_CENTER, "Long talks in, vertical clips out.", font(14.0), MUTED);
            y += 64.0 + 30.0;

            // drop zone
            let zone = Rect::from_min_size(pos2(left, y), vec2(wrap_w, 196.0));
            let over = self.home.drag_over;
            fill(ui, zone, 16, if over { alpha(ACCENT, 0.12) } else { alpha(PANEL, 0.6) });
            dashed_rounded(ui, zone, 16.0, Stroke::new(1.5, if over { ACCENT2 } else { LINE2 }));
            let cx = zone.center().x;
            self.icons.paint(ui, "drop", Rect::from_center_size(pos2(cx, zone.top() + 44.0), vec2(34.0, 34.0)), TEXT);
            label(ui, pos2(cx, zone.top() + 84.0), Align2::CENTER_CENTER, "Drop a video here", bold(16.0), TEXT);
            label(ui, pos2(cx, zone.top() + 108.0), Align2::CENTER_CENTER, "16:9 MP4 or MOV: a service, a talk, a podcast", font(13.0), MUTED);
            let btn_w = text_w(ui, "Choose video…", bold(13.0)) + 30.0;
            let btn = Rect::from_center_size(pos2(cx, zone.top() + 148.0), vec2(btn_w, 34.0));
            if primary(ui, Id::new("choose"), btn, "Choose video…", None, "").clicked() {
                if let Some(p) = rfd::FileDialog::new().add_filter("Video", &["mp4", "mov", "m4v"]).pick_file() {
                    chosen = Some(p.to_string_lossy().into_owned());
                }
            }
            y += 196.0 + 16.0;

            // transcription model
            label(ui, pos2(left + 2.0, y + 14.0), Align2::LEFT_CENTER, "Transcription", font(13.0), MUTED);
            let lw = text_w(ui, "Transcription", font(13.0)) + 10.0;
            let m = &self.home.models[self.home.model_idx];
            let txt = format!("{} · {} · {}", m.label, m.id, if m.present { "ready".to_string() } else { format!("{} MB download", m.mb) });
            let sel = Rect::from_min_size(pos2(left + 2.0 + lw, y), vec2(308.0, 29.0));
            let resp = ui.interact(sel, Id::new("model_sel"), Sense::click());
            fill(ui, sel, 7, PANEL2);
            border(ui, sel, 7, if resp.hovered() { LINE2 } else { LINE });
            label(ui, pos2(sel.left() + 10.0, sel.center().y), Align2::LEFT_CENTER, &txt, font(13.0), TEXT);
            self.icons.paint(ui, "chev_down", Rect::from_center_size(pos2(sel.right() - 14.0, sel.center().y), vec2(11.0, 11.0)), MUTED);
            if resp.clicked() {
                self.home.model_open = !self.home.model_open;
            }
            let hint = if m.present { "Runs on this Mac. Nothing is uploaded.".to_string() } else { format!("Downloads once ({} MB), then runs on this Mac.", m.mb) };
            label(ui, pos2(sel.right() + 14.0, y + 14.0), Align2::LEFT_CENTER, &hint, font(13.0), MUTED);
            if self.home.model_open {
                let menu = Rect::from_min_size(pos2(sel.left(), sel.bottom() + 4.0), vec2(sel.width(), 6.0 + 30.0 * self.home.models.len() as f32));
                let area = egui::Area::new(Id::new("model_menu")).order(egui::Order::Foreground).fixed_pos(menu.min);
                area.show(ctx, |ui| {
                    let (rect, _) = ui.allocate_exact_size(menu.size(), Sense::hover());
                    fill(ui, rect, 10, RAISE);
                    border(ui, rect, 10, LINE2);
                    for (i, m) in self.home.models.iter().enumerate() {
                        let row = Rect::from_min_size(pos2(rect.left() + 3.0, rect.top() + 3.0 + i as f32 * 30.0), vec2(rect.width() - 6.0, 30.0));
                        let r = ui.interact(row, Id::new(("model_row", i)), Sense::click());
                        if r.hovered() || i == self.home.model_idx {
                            fill(ui, row, 6, if r.hovered() { RAISE.gamma_multiply(1.4) } else { PANEL2 });
                        }
                        let t = format!("{} · {} · {}", m.label, m.id, if m.present { "ready".to_string() } else { format!("{} MB download", m.mb) });
                        label(ui, pos2(row.left() + 8.0, row.center().y), Align2::LEFT_CENTER, &t, font(13.0), TEXT);
                        if r.clicked() {
                            self.home.model_idx = i;
                            self.home.model_open = false;
                        }
                    }
                });
                if ctx.input(|i| i.pointer.any_click()) && !ctx.input(|i| i.pointer.hover_pos().map_or(false, |p| menu.contains(p) || sel.contains(p))) {
                    self.home.model_open = false;
                }
            }
            y += 29.0 + 34.0;

            // projects
            section_title(ui, pos2(left, y), "Projects");
            y += 14.0 + 12.0;
            if self.home.projects.is_empty() {
                label(ui, pos2(left + 2.0, y + 8.0), Align2::LEFT_CENTER, "Nothing yet. Drop a video above to start.", font(13.0), MUTED);
                y += 24.0;
            } else {
                let min_w = 260.0;
                let cols = (((wrap_w + 12.0) / (min_w + 12.0)).floor() as usize).max(1);
                let cw = (wrap_w - 12.0 * (cols as f32 - 1.0)) / cols as f32;
                let mut row_h = 0.0;
                for (i, p) in self.home.projects.iter().enumerate() {
                    let (col, row) = (i % cols, i / cols);
                    let extra = (!p.transcribed) as i32 + p.source_missing as i32;
                    let h = 14.0 + 20.0 + 18.0 + extra as f32 * 18.0 + 14.0;
                    row_h = f32::max(row_h, h);
                    let rect = Rect::from_min_size(pos2(left + col as f32 * (cw + 12.0), y + row as f32 * (row_h + 12.0)), vec2(cw, h));
                    let resp = ui.interact(rect, Id::new(("proj", i)), Sense::click());
                    fill(ui, rect, 12, if resp.hovered() { PANEL2 } else { PANEL });
                    border(ui, rect, 12, if resp.hovered() { ACCENT } else { LINE });
                    let tw = rect.width() - 50.0;
                    let title = ellipsize(ui, &p.title, bold(14.0), tw);
                    label(ui, pos2(rect.left() + 16.0, rect.top() + 24.0), Align2::LEFT_CENTER, &title, bold(14.0), TEXT);
                    let sub = format!("{} · {} clip{}{}", crate::editor::fmt_long(p.duration_ms as f64), p.clips, if p.clips == 1 { "" } else { "s" }, if p.picked > 0 { format!(" ({} picked)", p.picked) } else { String::new() });
                    label(ui, pos2(rect.left() + 16.0, rect.top() + 46.0), Align2::LEFT_CENTER, &sub, font(12.0), MUTED);
                    let mut yy = rect.top() + 64.0;
                    if !p.transcribed {
                        label(ui, pos2(rect.left() + 16.0, yy), Align2::LEFT_CENTER, "Not transcribed", font(12.0), hex(0xffb86b));
                        yy += 18.0;
                    }
                    if p.source_missing {
                        label(ui, pos2(rect.left() + 16.0, yy), Align2::LEFT_CENTER, "Source video moved", font(12.0), hex(0xffb86b));
                    }
                    let over = ui.rect_contains_pointer(rect);
                    {
                        let x = Rect::from_min_size(pos2(rect.right() - 32.0, rect.top() + 8.0), vec2(24.0, 24.0));
                        let xr = ui.interact(x, Id::new(("proj_x", i)), Sense::click());
                        if over && xr.hovered() {
                            fill(ui, x, 6, RAISE);
                        }
                        if over {
                            self.icons.paint(ui, "x", Rect::from_center_size(x.center(), vec2(16.0, 16.0)), if xr.hovered() { hex(0xff8a97) } else { hex(0x9aa0bd) });
                        }
                        if xr.clicked() {
                            delete = Some((p.dir.clone(), p.title.clone()));
                        } else if resp.clicked() {
                            open = Some(p.dir.clone().into());
                        }
                    }
                }
                let rows = (self.home.projects.len() + cols - 1) / cols;
                y += rows as f32 * (row_h + 12.0);
            }
            ui.allocate_rect(Rect::from_min_max(pos2(left, top0), pos2(left + wrap_w, y + 60.0)), Sense::hover());
        });

        if let Some(p) = chosen {
            self.start_import(ctx, p);
        }
        if let Some(dir) = open {
            self.open_project(ctx, dir);
        }
        if let Some((dir, title)) = delete {
            self.modal = Modal::ConfirmDelete { dir, title };
        }
        if self.home.drag_over {
            let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, Id::new("drop_overlay")));
            p.rect_stroke(full.shrink(14.0), theme_r(18), Stroke::new(2.0, ACCENT2), egui::StrokeKind::Inside);
        }
    }

    pub fn icons_app_icon(&self, ui: &Ui, rect: Rect) {
        if let Some(t) = &self.app_icon {
            ui.painter().image(t.id(), rect, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
    }
}

fn theme_r(n: u8) -> egui::CornerRadius {
    egui::CornerRadius::same(n)
}

pub fn ellipsize(ui: &Ui, s: &str, f: egui::FontId, max_w: f32) -> String {
    if text_w(ui, s, f.clone()) <= max_w {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() && text_w(ui, &format!("{out}…"), f.clone()) > max_w {
        out.pop();
    }
    format!("{out}…")
}
