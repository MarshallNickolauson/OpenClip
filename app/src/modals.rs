//! Dialogs: import progress, connect Claude, unsaved changes, delete project.

use eframe::egui::{self, pos2, vec2, Align2, Color32, Id, Rect, Sense};

use crate::app::{After, App, Modal};
use crate::editor::wrap_lines;
use crate::theme::*;

impl App {
    pub fn modal_ui(&mut self, ctx: &egui::Context) {
        if matches!(self.modal, Modal::None) {
            return;
        }
        let screen = ctx.content_rect();
        // scrim
        egui::Area::new(Id::new("scrim")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(r, 0.0, alpha(hex(0x04070c), 0.68));
            let _ = resp;
        });
        let mut next: Option<Modal> = None;
        let mut action: Option<ModalAction> = None;
        let w = match &self.modal {
            Modal::ConfirmLeave(_) | Modal::ConfirmDelete { .. } => 420.0,
            _ => 540.0,
        };
        egui::Area::new(Id::new("dialog")).order(egui::Order::Tooltip).anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0)).show(ctx, |ui| {
            let h = match &self.modal {
                Modal::Import { stages, .. } => 150.0 + stages.len() as f32 * 44.0 + 70.0,
                Modal::Connect { result, .. } => 610.0 + if result.is_some() { 36.0 } else { 0.0 },
                Modal::ConfirmLeave(_) => 168.0,
                Modal::ConfirmDelete { .. } => 168.0,
                Modal::None => 0.0,
            };
            let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
            ui.painter().add(egui::epaint::Shadow { offset: [0, 24], blur: 70, spread: 0, color: alpha(Color32::BLACK, 0.55) }.as_shape(rect, r(14)));
            fill(ui, rect, 14, PANEL);
            border(ui, rect, 14, LINE2);
            let x = rect.left() + 24.0;
            let iw = rect.width() - 48.0;
            let mut y = rect.top() + 24.0;
            match &self.modal {
                Modal::Import { stages, .. } => {
                    label(ui, pos2(x, y + 9.0), Align2::LEFT_CENTER, "Getting your video ready", bold(17.0), TEXT);
                    y += 30.0;
                    for l in wrap_lines(ui, "Transcribing on this Mac. A 30-minute talk takes a few minutes; the face tracking runs at the same time.", font(13.0), iw, 3) {
                        label(ui, pos2(x, y + 9.0), Align2::LEFT_CENTER, &l, font(13.0), hex(0xb3b9d6));
                        y += 20.0;
                    }
                    y += 14.0;
                    for (_, name, pct, started) in stages {
                        let a = if *started { 1.0 } else { 0.45 };
                        label(ui, pos2(x, y + 8.0), Align2::LEFT_CENTER, name, font(13.0), alpha(LABEL, a));
                        if *started {
                            label(ui, pos2(x + iw, y + 8.0), Align2::RIGHT_CENTER, &format!("{}%", pct.round() as i32), font(13.0), MUTED);
                        }
                        let bar = Rect::from_min_size(pos2(x, y + 22.0), vec2(iw, 6.0));
                        fill(ui, bar, 3, alpha(LINE2, a));
                        fill(ui, Rect::from_min_size(bar.min, vec2(bar.width() * (pct / 100.0).clamp(0.0, 1.0), 6.0)), 3, ACCENT2);
                        y += 44.0;
                    }
                    let b = Rect::from_min_size(pos2(rect.right() - 24.0 - 84.0, rect.bottom() - 24.0 - 34.0), vec2(84.0, 34.0));
                    if ghost_button(ui, "import_cancel", b, "Cancel") {
                        action = Some(ModalAction::CancelImport);
                    }
                }
                Modal::Connect { result, mcp_path, mcp_exists } => {
                    label(ui, pos2(x, y + 9.0), Align2::LEFT_CENTER, "Let Claude pick the clips", bold(17.0), TEXT);
                    y += 30.0;
                    for l in wrap_lines(ui, "OpenClip ships a local MCP server. Claude starts it on your Mac when it needs it, so there is nothing to deploy or sign in to. It only sees the transcript and writes clip suggestions back here.", font(13.0), iw, 5) {
                        label(ui, pos2(x, y + 9.0), Align2::LEFT_CENTER, &l, font(13.0), hex(0xb3b9d6));
                        y += 20.0;
                    }
                    if !*mcp_exists {
                        label(ui, pos2(x, y + 9.0), Align2::LEFT_CENTER, "The MCP helper was not found next to the app. Build with ./scripts/bundle.sh.", font(13.0), DANGER);
                        y += 20.0;
                    }
                    y += 14.0;
                    label(ui, pos2(x, y + 7.0), Align2::LEFT_CENTER, "Claude Desktop", bold(13.0), TEXT);
                    y += 22.0;
                    let bw = text_w(ui, "Add to Claude Desktop", bold(13.0)) + 30.0;
                    let b = Rect::from_min_size(pos2(x, y), vec2(bw, 34.0));
                    if primary(ui, Id::new("add_desktop"), b, "Add to Claude Desktop", None, "").clicked() {
                        action = Some(ModalAction::AddDesktop);
                    }
                    y += 34.0 + 6.0;
                    if let Some((ok, msg)) = result {
                        for l in wrap_lines(ui, msg, font(12.0), iw, 3) {
                            label(ui, pos2(x, y + 9.0), Align2::LEFT_CENTER, &l, font(12.0), if *ok { MUTED } else { DANGER });
                            y += 18.0;
                        }
                    }
                    y += 22.0;
                    label(ui, pos2(x, y + 7.0), Align2::LEFT_CENTER, "Claude Code", bold(13.0), TEXT);
                    y += 22.0;
                    let cmd = format!("claude mcp add openclip -- \"{mcp_path}\"");
                    y = code_row(ui, x, y, iw, &cmd, "copy1", &mut action);
                    y += 14.0;
                    label(ui, pos2(x, y + 7.0), Align2::LEFT_CENTER, "Then ask", bold(13.0), TEXT);
                    y += 26.0;
                    for (i, t) in ["Keep this project open in OpenClip.", "In Claude, say: “Find the best clips in my open OpenClip project.”", "Suggestions appear in the list as Claude saves them. Preview, caption, tick, export."].iter().enumerate() {
                        let c = pos2(x + 11.0, y + 11.0);
                        ui.painter().circle_filled(c, 11.0, ACCENT);
                        label(ui, c, Align2::CENTER_CENTER, &format!("{}", i + 1), bold(12.0), Color32::WHITE);
                        let lines = wrap_lines(ui, t, font(13.0), iw - 34.0, 2);
                        for (k, l) in lines.iter().enumerate() {
                            label(ui, pos2(x + 34.0, y + 11.0 + k as f32 * 19.0), Align2::LEFT_CENTER, l, font(13.0), LABEL);
                        }
                        y += 22.0 + (lines.len() as f32 - 1.0) * 19.0 + 10.0;
                    }
                    y = code_row(ui, x, y + 2.0, iw, "Find the best 5–10 vertical clips in my open OpenClip project.", "copy2", &mut action);
                    let _ = y;
                    let b = Rect::from_min_size(pos2(rect.right() - 24.0 - 72.0, rect.bottom() - 24.0 - 34.0), vec2(72.0, 34.0));
                    if ghost_button(ui, "connect_done", b, "Done") {
                        next = Some(Modal::None);
                    }
                }
                Modal::ConfirmLeave(after) => {
                    let title = self.ed.as_ref().map(|e| e.project.title.clone()).unwrap_or_default();
                    label(ui, pos2(x, y + 9.0), Align2::LEFT_CENTER, &format!("Save changes to “{title}”?"), bold(17.0), TEXT);
                    y += 34.0;
                    label(ui, pos2(x, y + 9.0), Align2::LEFT_CENTER, "Your edits will be lost if you don't save them.", font(13.0), hex(0xb3b9d6));
                    let by = rect.bottom() - 24.0 - 34.0;
                    let dont = Rect::from_min_size(pos2(x, by), vec2(text_w(ui, "Don't Save", font(13.0)) + 28.0, 34.0));
                    if ghost_button(ui, "leave_dont", dont, "Don't Save") {
                        action = Some(ModalAction::Leave(after.clone(), false));
                    }
                    let save = Rect::from_min_size(pos2(rect.right() - 24.0 - 70.0, by), vec2(70.0, 34.0));
                    if primary(ui, Id::new("leave_save"), save, "Save", None, "").clicked() {
                        action = Some(ModalAction::Leave(after.clone(), true));
                    }
                    let cancel = Rect::from_min_size(pos2(save.left() - 10.0 - 76.0, by), vec2(76.0, 34.0));
                    if ghost_button(ui, "leave_cancel", cancel, "Cancel") {
                        next = Some(Modal::None);
                    }
                }
                Modal::ConfirmDelete { dir, title } => {
                    label(ui, pos2(x, y + 9.0), Align2::LEFT_CENTER, &format!("Delete “{title}”?"), bold(17.0), TEXT);
                    y += 34.0;
                    label(ui, pos2(x, y + 9.0), Align2::LEFT_CENTER, "The original video is not touched.", font(13.0), hex(0xb3b9d6));
                    let by = rect.bottom() - 24.0 - 34.0;
                    let del = Rect::from_min_size(pos2(rect.right() - 24.0 - 80.0, by), vec2(80.0, 34.0));
                    let resp = ui.interact(del, Id::new("del_ok"), Sense::click());
                    fill(ui, del, 7, if resp.hovered() { hex(0xff7a88) } else { DANGER });
                    label(ui, del.center(), Align2::CENTER_CENTER, "Delete", bold(13.0), Color32::WHITE);
                    if resp.clicked() {
                        action = Some(ModalAction::Delete(dir.clone()));
                    }
                    let cancel = Rect::from_min_size(pos2(del.left() - 10.0 - 76.0, by), vec2(76.0, 34.0));
                    if ghost_button(ui, "del_cancel", cancel, "Cancel") {
                        next = Some(Modal::None);
                    }
                }
                Modal::None => {}
            }
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !matches!(self.modal, Modal::Import { .. }) {
            next = Some(Modal::None);
        }
        if let Some(m) = next {
            self.modal = m;
        }
        if let Some(a) = action {
            match a {
                ModalAction::CancelImport => {
                    self.cancel_import.store(true, std::sync::atomic::Ordering::Relaxed);
                    self.modal = Modal::None;
                    self.refresh_home();
                }
                ModalAction::AddDesktop => {
                    let res = connect_claude_desktop();
                    if let Modal::Connect { result, .. } = &mut self.modal {
                        *result = Some(match res {
                            Ok(p) => (true, format!("Added to {p}. Quit and reopen Claude Desktop to load it.")),
                            Err(e) => (false, format!("Could not update the config: {e}")),
                        });
                    }
                }
                ModalAction::Copy(t) => {
                    ctx.copy_text(t);
                    self.toast("Copied");
                }
                ModalAction::Leave(after, save) => {
                    if save {
                        if let Some(ed) = &mut self.ed {
                            ed.save();
                        }
                    } else if let Some(ed) = &mut self.ed {
                        ed.dirty = false;
                    }
                    self.modal = Modal::None;
                    self.finish_leave(ctx, after);
                }
                ModalAction::Delete(dir) => {
                    let p = std::path::Path::new(&dir);
                    if matches!(p.extension().and_then(|s| s.to_str()), Some("openclip") | Some("clipcraft")) && openclip_core::store::project_file(p).exists() {
                        let _ = std::fs::remove_dir_all(p);
                    }
                    self.modal = Modal::None;
                    self.refresh_home();
                }
            }
        }
        let _ = After::Home;
    }
}

enum ModalAction {
    CancelImport,
    AddDesktop,
    Copy(String),
    Leave(After, bool),
    Delete(String),
}

fn ghost_button(ui: &mut egui::Ui, id: &str, rect: Rect, text: &str) -> bool {
    let resp = ui.interact(rect, Id::new(id), Sense::click());
    fill(ui, rect, 7, if resp.hovered() { RAISE } else { PANEL2 });
    if resp.hovered() {
        border(ui, rect, 7, LINE2);
    } else {
        crate::home::dashed_rounded(ui, rect, 7.0, egui::Stroke::new(1.0, LINE2));
    }
    label(ui, rect.center(), Align2::CENTER_CENTER, text, font(13.0), hex(0xd0d4ea));
    resp.clicked()
}

fn code_row(ui: &mut egui::Ui, x: f32, y: f32, w: f32, text: &str, id: &str, action: &mut Option<ModalAction>) -> f32 {
    let cw = w - 8.0 - 62.0;
    let lines = wrap_chars(ui, text, mono(12.0), cw - 22.0);
    let h = (lines.len() as f32 * 17.0 + 20.0).max(40.0);
    let r = Rect::from_min_size(pos2(x, y), vec2(cw, h));
    fill(ui, r, 8, BG);
    border(ui, r, 8, LINE2);
    for (i, l) in lines.iter().enumerate() {
        label(ui, pos2(r.left() + 11.0, r.top() + 10.0 + i as f32 * 17.0 + 8.0), Align2::LEFT_CENTER, l, mono(12.0), hex(0xcfd4f0));
    }
    let b = Rect::from_min_size(pos2(r.right() + 8.0, y), vec2(62.0, h));
    let resp = ui.interact(b, Id::new(id), Sense::click());
    fill(ui, b, 7, if resp.hovered() { RAISE } else { PANEL2 });
    border(ui, b, 7, LINE);
    label(ui, b.center(), Align2::CENTER_CENTER, "Copy", font(12.0), SOFT);
    if resp.clicked() {
        *action = Some(ModalAction::Copy(text.to_string()));
    }
    y + h
}

/// Wrap on characters (commands have no spaces to break at).
fn wrap_chars(ui: &egui::Ui, text: &str, f: egui::FontId, max_w: f32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut cur = String::new();
    for ch in text.chars() {
        let mut t = cur.clone();
        t.push(ch);
        if text_w(ui, &t, f.clone()) > max_w && !cur.is_empty() {
            lines.push(std::mem::take(&mut cur));
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Add OpenClip to Claude Desktop's MCP config (merging, never replacing other servers).
fn connect_claude_desktop() -> Result<String, String> {
    use serde_json::{json, Value};
    let h = std::env::var("HOME").map_err(|e| e.to_string())?;
    let dir = std::path::PathBuf::from(&h).join("Library/Application Support/Claude");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("claude_desktop_config.json");
    let mut cfg: Value = match std::fs::read(&file) {
        Ok(b) if !b.is_empty() => serde_json::from_slice(&b).map_err(|e| format!("existing config is not valid JSON: {e}"))?,
        _ => json!({}),
    };
    let obj = cfg.as_object_mut().ok_or("existing config is not a JSON object")?;
    let servers = obj.entry("mcpServers").or_insert_with(|| json!({}));
    let map = servers.as_object_mut().ok_or("mcpServers is not an object")?;
    map.remove("clipcraft");
    map.insert("openclip".into(), json!({"command": App::mcp_path(), "args": []}));
    if file.exists() {
        let _ = std::fs::copy(&file, dir.join("claude_desktop_config.json.bak"));
    }
    std::fs::write(&file, serde_json::to_vec_pretty(&cfg).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(file.to_string_lossy().into_owned())
}
