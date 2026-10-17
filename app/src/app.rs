//! Application state, message handling and the per-frame update loop.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Key, ViewportCommand};
use openclip_core::framing::{camera_path, FaceTrack};
use openclip_core::moments::{candidates, Ctx};
use openclip_core::project::{new_id, Clip, Origin};
use openclip_core::store::{self, Summary};
use openclip_core::transcript::{join, sentences, Word};
use serde_json::{json, Value};

use crate::engine::{self, Emit};
use crate::icons::Icons;
use crate::player::PlayerView;
use crate::state::{Editor, ExportItem, Tab};
use crate::{menu, native, theme, whisper};

pub enum Msg {
    Import(Value),
    Export(Value),
    ExportDone,
    Thumb { id: String, key: String, res: Result<String, String> },
    ModelProgress(f32),
    ModelDone(Result<(), String>),
}

#[derive(Clone)]
pub enum After {
    Home,
    Quit,
}

pub enum Modal {
    None,
    Import { dir: String, stages: Vec<(String, String, f32, bool)> },
    Connect { result: Option<(bool, String)>, mcp_path: String, mcp_exists: bool },
    ConfirmLeave(After),
    ConfirmDelete { dir: String, title: String },
}

pub struct ModelInfo {
    pub id: &'static str,
    pub label: &'static str,
    pub mb: u64,
    pub present: bool,
}

pub struct HomeState {
    pub models: Vec<ModelInfo>,
    pub model_idx: usize,
    pub projects: Vec<Summary>,
    pub drag_over: bool,
    pub model_open: bool,
    pub model_dl: Option<f32>,
}

pub struct App {
    pub icons: Icons,
    pub tx: Sender<Msg>,
    rx: Receiver<Msg>,
    pub ed: Option<Editor>,
    pub player: PlayerView,
    pub home: HomeState,
    pub modal: Modal,
    pub toast: Option<(String, Instant)>,
    pub claude_seen: Option<u64>,
    pub cancel_import: Arc<AtomicBool>,
    pub cancel_export: Arc<AtomicBool>,
    last_poll: Instant,
    sug_mtime: Option<std::time::SystemTime>,
    seen_mtime: Option<std::time::SystemTime>,
    _menu: Option<menu::AppMenu>,
    pub import_started: Option<Instant>,
    pub phone_px: (usize, usize),
    pub app_icon: Option<egui::TextureHandle>,
    pub pending_export: bool,
    /// Leave requests raised mid-draw; handled once the frame is done so the editor is not torn down under itself.
    pub pending_leave: Option<After>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        theme::install(&cc.egui_ctx);
        let (tx, rx) = channel();
        let mut app = Self {
            icons: Icons::new(&cc.egui_ctx),
            tx,
            rx,
            ed: None,
            player: PlayerView::default(),
            home: HomeState { models: Vec::new(), model_idx: 1, projects: Vec::new(), drag_over: false, model_open: false, model_dl: None },
            modal: Modal::None,
            toast: None,
            claude_seen: None,
            cancel_import: Arc::new(AtomicBool::new(false)),
            cancel_export: Arc::new(AtomicBool::new(false)),
            last_poll: Instant::now(),
            sug_mtime: None,
            seen_mtime: None,
            _menu: menu::install(&cc.egui_ctx),
            import_started: None,
            phone_px: (540, 960),
            app_icon: None,
            pending_export: false,
            pending_leave: None,
        };
        if let Ok(img) = image::load_from_memory(include_bytes!("../icons/icon.png")) {
            let rgba = img.to_rgba8();
            let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
            app.app_icon = Some(cc.egui_ctx.load_texture("app-icon", ci, egui::TextureOptions::LINEAR));
        }
        app.refresh_home();
        app
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn refresh_home(&mut self) {
        self.home.models = whisper::MODELS.iter().map(|(id, label, _, size)| ModelInfo { id, label, mb: size / 1_000_000, present: whisper::model_present(id) }).collect();
        self.home.projects = store::list();
    }

    pub fn mcp_path() -> String {
        let exe = std::env::current_exe().unwrap_or_default();
        exe.parent().map(|d| d.join("openclip-mcp")).unwrap_or_default().to_string_lossy().into_owned()
    }

    // ───────── projects
    pub fn open_project(&mut self, ctx: &egui::Context, dir: PathBuf) {
        let mut project = match store::load_project(&dir) {
            Ok(p) => p,
            Err(e) => return self.toast(e),
        };
        if !Path::new(&project.source.path).exists() {
            self.toast("The source video moved. Choose it again.");
            let Some(p) = rfd::FileDialog::new().add_filter("Video", &["mp4", "mov", "m4v"]).pick_file() else { return };
            let p = p.to_string_lossy().into_owned();
            if let Ok(pr) = native::probe(&p) {
                project.source.path = p.clone();
                project.source.name = Path::new(&p).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                project.source.width = pr.width;
                project.source.height = pr.height;
                let _ = store::save_project(&dir, &project);
            }
        }
        let (words, energy, faces) = if project.transcribed {
            match store::load_transcript(&dir) {
                Ok(t) => (t.words, store::load_energy(&dir), store::load_faces(&dir)),
                Err(e) => return self.toast(e),
            }
        } else {
            (Vec::new(), None, None)
        };
        let sents = sentences(&words);
        if faces.is_some() {
            project.faces = true;
        }
        store::mark_current(&dir);
        let mut ed = Editor::new(dir.clone(), project, words, sents, energy, faces);
        ed.face_progress = if ed.project.faces || !ed.project.transcribed { None } else { Some(0.0) };
        self.player.open(&ed.project.source.path.clone());
        self.ed = Some(ed);
        self.sug_mtime = std::fs::metadata(dir.join("suggestions.json")).and_then(|m| m.modified()).ok();
        self.pull_suggestions(true);
        let first = self.ed.as_ref().and_then(|e| e.ordered().first().map(|c| c.id.clone()));
        self.select_clip(first, false);
        if let Some((t, _)) = store::read_mcp_seen() {
            self.claude_seen = Some(t);
        }
        ctx.request_repaint();
    }

    pub fn close_project(&mut self) {
        self.player.pause();
        self.player.close();
        self.ed = None;
        self.refresh_home();
    }

    pub fn select_clip(&mut self, id: Option<String>, play: bool) {
        let Some(ed) = &mut self.ed else { return };
        ed.sel = id;
        ed.force_refresh();
        ed.refresh();
        self.player.pause();
        if let Some(c) = ed.sel_clip().cloned() {
            self.player.seek_raw(c.start_ms as f64);
            ed.t = c.start_ms as f64;
            ed.refresh();
            crate::timeline::fit_view(ed);
            if play || ed.auto_play && play {
                self.player.play(ed);
            }
        }
    }

    pub fn step_clip(&mut self, dir: i32) {
        let Some(ed) = &self.ed else { return };
        let items: Vec<String> = ed.ordered().iter().map(|c| c.id.clone()).collect();
        if items.is_empty() {
            return;
        }
        let i = items.iter().position(|id| Some(id) == ed.sel.as_ref());
        let n = (i.map_or(0, |i| i as i32 + dir)).clamp(0, items.len() as i32 - 1) as usize;
        let auto = ed.auto_play;
        self.select_clip(Some(items[n].clone()), auto);
    }

    pub fn pull_suggestions(&mut self, silent: bool) {
        let Some(ed) = &mut self.ed else { return };
        let sug = store::load_suggestions(&ed.dir);
        if sug.clips.is_empty() {
            return;
        }
        let incoming: Vec<Clip> = sug.clips.into_iter().filter(|c| !c.dismissed).collect();
        let n = ed.merge_clips(incoming, true);
        if n > 0 && !silent {
            let first = if ed.sel.is_none() { ed.ordered().first().map(|c| c.id.clone()) } else { None };
            self.toast(format!("Claude added {n} clip{}", if n == 1 { "" } else { "s" }));
            if first.is_some() {
                self.select_clip(first, false);
            }
        }
    }

    pub fn quick_pick(&mut self) {
        let Some(ed) = &mut self.ed else { return };
        let c = candidates(&Ctx { words: &ed.words, sents: &ed.sents, energy: ed.energy.as_ref(), faces: ed.faces.as_ref() }, 8, 20_000, 70_000);
        let mut out = Vec::new();
        for m in c {
            let w0 = ed.sents[m.s0 as usize].w0 as usize;
            let w1 = ed.sents[m.s1 as usize].w1 as usize;
            let mut start = ed.sents[m.s0 as usize].t0.saturating_sub(180);
            if w0 > 0 {
                start = start.max(ed.words[w0 - 1].2);
            }
            let mut end = ed.sents[m.s1 as usize].t1 + 380;
            if w1 + 1 < ed.words.len() {
                end = end.min(ed.words[w1 + 1].1.saturating_sub(20).max(ed.sents[m.s1 as usize].t1));
            }
            let text = join(&ed.words[w0..(w0 + 7).min(w1 + 1)]);
            out.push(Clip {
                id: new_id("c"),
                title: text.trim_end_matches(['.', ',', ';', ':']).to_string() + "…",
                why: "Picked by the built-in scorer: clean opening, steady pace, direct address.".into(),
                score: m.score,
                start_ms: start as u64,
                end_ms: end as u64,
                origin: Origin::Auto,
                picked: false,
                ..Clip::default()
            });
        }
        out.sort_by_key(|c| c.start_ms);
        ed.merge_clips(out, false);
        let first = if ed.sel.is_none() { ed.ordered().first().map(|c| c.id.clone()) } else { None };
        if first.is_some() {
            self.select_clip(first, false);
        }
    }

    // ───────── import
    pub fn start_import(&mut self, ctx: &egui::Context, path: String) {
        let model = self.home.models[self.home.model_idx].id.to_string();
        let (dir, _) = match engine::create_project(&path) {
            Ok(r) => r,
            Err(e) => return self.toast(e),
        };
        self.cancel_import.store(false, Ordering::Relaxed);
        let cancel = self.cancel_import.clone();
        let need_dl = !self.home.models[self.home.model_idx].present;
        let tx = self.tx.clone();
        let c2 = ctx.clone();
        let emit: Emit = Arc::new(move |_name: &str, v: Value| {
            let _ = tx.send(Msg::Import(v));
            c2.request_repaint();
        });
        let d2 = dir.clone();
        std::thread::spawn(move || {
            if let Err(e) = engine::run_pipeline(&d2, &engine::ImportOpts { model, language: "en".into() }, &emit, &cancel) {
                emit("import-progress", json!({"dir": d2.to_string_lossy(), "stage": "error", "pct": 0, "error": e}));
            }
        });
        let mut stages = vec![("audio".to_string(), "Reading audio".to_string(), 0.0, false)];
        if need_dl {
            stages.push(("model".into(), "Downloading transcription model".into(), 0.0, false));
        }
        stages.push(("transcribe".into(), "Transcribing speech".into(), 0.0, false));
        stages.push(("faces".into(), "Finding faces for framing".into(), 0.0, false));
        self.modal = Modal::Import { dir: dir.to_string_lossy().into_owned(), stages };
        self.import_started = Some(Instant::now());
    }

    fn on_import(&mut self, ctx: &egui::Context, v: Value) {
        let dir = v["dir"].as_str().unwrap_or("").to_string();
        let stage = v["stage"].as_str().unwrap_or("").to_string();
        let pct = v["pct"].as_f64().unwrap_or(0.0) as f32;
        if let Modal::Import { dir: d, stages } = &mut self.modal {
            if *d == dir {
                if let Some(s) = stages.iter_mut().find(|s| s.0 == stage) {
                    s.2 = pct;
                    s.3 = true;
                }
                match stage.as_str() {
                    "transcribed" => {
                        let d = PathBuf::from(&dir);
                        self.modal = Modal::None;
                        self.open_project(ctx, d);
                        return;
                    }
                    "error" => {
                        self.modal = Modal::None;
                        self.toast(format!("Import failed: {}", v["error"].as_str().unwrap_or("unknown error")));
                        self.refresh_home();
                        return;
                    }
                    _ => {}
                }
            }
        }
        if let Some(ed) = &mut self.ed {
            if ed.dir.to_string_lossy() == dir {
                match stage.as_str() {
                    "faces" => {
                        ed.face_progress = if pct >= 100.0 { None } else { Some(pct) };
                        if pct >= 100.0 {
                            ed.faces = store::load_faces(&ed.dir);
                            ed.project.faces = ed.faces.is_some();
                            ed.force_refresh();
                        }
                    }
                    "faces-error" => {
                        ed.face_progress = None;
                        self.toast("Face tracking failed");
                    }
                    _ => {}
                }
            }
        }
    }

    // ───────── export
    pub fn run_export(&mut self, ctx: &egui::Context) {
        let Some(ed) = &mut self.ed else { return };
        let ids: Vec<String> = ed.picked().iter().map(|c| c.id.clone()).collect();
        if ids.is_empty() {
            ed.tab = Tab::Export;
            return self.toast("Tick at least one clip to export");
        }
        if ed.exporting {
            return;
        }
        ed.exporting = true;
        ed.export.clear();
        for id in &ids {
            ed.export.insert(id.clone(), ExportItem { state: "queued".into(), ..Default::default() });
        }
        ed.tab = Tab::Export;
        self.cancel_export.store(false, Ordering::Relaxed);
        let cancel = self.cancel_export.clone();
        let (project, dir, words, faces) = (ed.project.clone(), ed.dir.clone(), ed.words.clone(), ed.faces.clone());
        let tx = self.tx.clone();
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let send = |v: Value| {
                let _ = tx.send(Msg::Export(v));
                c2.request_repaint();
            };
            let total = ids.len();
            for (n, id) in ids.iter().enumerate() {
                let Some(clip) = project.clips.iter().find(|c| &c.id == id) else { continue };
                send(json!({"id": id, "state": "running", "pct": 0.0, "n": n, "total": total}));
                let last = std::sync::atomic::AtomicI64::new(-1);
                let r = engine::export_clip(&dir, &project, &words, faces.as_ref(), clip, &|p| {
                    let q = p as i64;
                    if last.swap(q, Ordering::Relaxed) != q {
                        send(json!({"id": id, "state": "running", "pct": p, "n": n, "total": total}));
                    }
                    cancel.load(Ordering::Relaxed)
                });
                match r {
                    Ok(path) => send(json!({"id": id, "state": "done", "pct": 100.0, "path": path})),
                    Err(e) => {
                        let cancelled = e == "cancelled";
                        send(json!({"id": id, "state": if cancelled { "cancelled" } else { "error" }, "error": e}));
                        if cancelled {
                            break;
                        }
                    }
                }
            }
            let _ = tx.send(Msg::ExportDone);
            c2.request_repaint();
        });
    }

    // ───────── thumbnails
    pub fn want_thumb(&mut self, ctx: &egui::Context, c: &Clip) {
        let Some(ed) = &mut self.ed else { return };
        let key = format!("{}|{}|{}|{}", c.start_ms >> 8, c.framing.zoom, c.framing.follow, c.framing.keys.len());
        if ed.thumbs.get(&c.id).map_or(false, |t| t.0 == key) || ed.thumb_pending.contains(&format!("{}{}", c.id, key)) {
            return;
        }
        ed.thumb_pending.insert(format!("{}{}", c.id, key));
        let (src, w, h) = (ed.project.source.path.clone(), ed.project.source.width, ed.project.source.height);
        let faces: Option<FaceTrack> = ed.faces.clone();
        let (subject, smooth) = (ed.project.subject, ed.project.smooth);
        let out_dir = ed.dir.join("thumbs");
        let (id, clip) = (c.id.clone(), c.clone());
        let tx = self.tx.clone();
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let _ = std::fs::create_dir_all(&out_dir);
            let t = clip.start_ms + 900;
            let file = out_dir.join(format!("{}-{}.jpg", clip.id, t / 250));
            let res = (|| -> Result<String, String> {
                if !file.exists() {
                    let path = camera_path(faces.as_ref(), w, h, clip.start_ms, clip.start_ms + 2000, &clip.framing, subject, smooth);
                    let cx = path.cx.get(9).copied().unwrap_or(0.5) as f64;
                    native::thumbnail(&src, t as f64, 360, cx, &file.to_string_lossy())?;
                }
                Ok(file.to_string_lossy().into_owned())
            })();
            let _ = tx.send(Msg::Thumb { id, key, res });
            c2.request_repaint();
        });
    }

    fn on_thumb(&mut self, ctx: &egui::Context, id: String, key: String, res: Result<String, String>) {
        let Some(ed) = &mut self.ed else { return };
        ed.thumb_pending.retain(|k| !k.starts_with(&id));
        if let Ok(path) = res {
            if let Ok(img) = image::open(&path) {
                let rgba = img.to_rgba8();
                let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
                let tex = ctx.load_texture(format!("thumb-{id}-{key}"), ci, egui::TextureOptions::LINEAR);
                ed.thumbs.insert(id, (key, tex));
            }
        }
    }

    // ───────── leave / quit flow
    pub fn request_leave(&mut self, ctx: &egui::Context, after: After) {
        let dirty = self.ed.as_ref().map_or(false, |e| e.dirty);
        if dirty {
            self.modal = Modal::ConfirmLeave(after);
        } else {
            self.finish_leave(ctx, after);
        }
    }
    pub fn finish_leave(&mut self, ctx: &egui::Context, after: After) {
        match after {
            After::Home => self.close_project(),
            After::Quit => {
                self.player.pause();
                ctx.send_viewport_cmd(ViewportCommand::Close);
                ALLOW_CLOSE.store(true, Ordering::SeqCst);
                ctx.send_viewport_cmd(ViewportCommand::Close);
            }
        }
    }
}

pub static ALLOW_CLOSE: AtomicBool = AtomicBool::new(false);

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        theme::BG.to_normalized_gamma_f32()
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = &root.ctx().clone();
        // messages from worker threads
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Import(v) => self.on_import(ctx, v),
                Msg::Export(v) => {
                    if let Some(ed) = &mut self.ed {
                        let id = v["id"].as_str().unwrap_or("").to_string();
                        let it = ed.export.entry(id.clone()).or_default();
                        it.state = v["state"].as_str().unwrap_or("").to_string();
                        if let Some(p) = v["pct"].as_f64() {
                            it.pct = p as f32;
                        }
                        it.path = v["path"].as_str().map(String::from).or(it.path.clone());
                        it.error = v["error"].as_str().map(String::from);
                        if it.state == "done" {
                            let path = it.path.clone();
                            ed.edit_clip(&id, "exp", |c| c.exported = path);
                        }
                    }
                }
                Msg::ExportDone => {
                    if let Some(ed) = &mut self.ed {
                        ed.exporting = false;
                        let n = ed.export.values().filter(|x| x.state == "done").count();
                        if n > 0 {
                            self.toast(format!("Exported {n} clip{}", if n == 1 { "" } else { "s" }));
                        }
                    }
                }
                Msg::Thumb { id, key, res } => self.on_thumb(ctx, id, key, res),
                Msg::ModelProgress(p) => self.home.model_dl = Some(p),
                Msg::ModelDone(r) => {
                    self.home.model_dl = None;
                    if let Err(e) = r {
                        self.toast(format!("Download failed: {e}"));
                    }
                    self.refresh_home();
                }
            }
        }
        for id in menu::take() {
            if id == "app.quit" {
                self.request_leave(ctx, After::Quit);
            }
        }
        // window close button
        if ctx.input(|i| i.viewport().close_requested()) && !ALLOW_CLOSE.load(Ordering::SeqCst) {
            ctx.send_viewport_cmd(ViewportCommand::CancelClose);
            self.request_leave(ctx, After::Quit);
        }
        // dropped files
        let hovering = ctx.input(|i| !i.raw.hovered_files.is_empty());
        self.home.drag_over = hovering;
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if self.ed.is_none() && matches!(self.modal, Modal::None) {
            if let Some(p) = dropped.iter().find(|p| matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(), Some("mp4" | "mov" | "m4v"))) {
                self.start_import(ctx, p.to_string_lossy().into_owned());
            } else if !dropped.is_empty() {
                self.toast("Drop an MP4 or MOV file");
            }
        }
        // file watcher for Claude's suggestions + heartbeat
        if self.last_poll.elapsed() > Duration::from_millis(400) {
            self.last_poll = Instant::now();
            if let Some(ed) = &self.ed {
                let m = std::fs::metadata(ed.dir.join("suggestions.json")).and_then(|m| m.modified()).ok();
                if m != self.sug_mtime {
                    self.sug_mtime = m;
                    self.pull_suggestions(false);
                }
            }
            let m = std::fs::metadata(store::home().join("mcp-seen")).and_then(|m| m.modified()).ok();
            if m != self.seen_mtime {
                self.seen_mtime = m;
                if let Some((t, _)) = store::read_mcp_seen() {
                    self.claude_seen = Some(t);
                }
            }
        }
        ctx.request_repaint_after(Duration::from_millis(400));

        {
            let full = root.max_rect();
            root.painter().rect_filled(full, 0.0, theme::BG);
            if self.ed.is_some() {
                self.editor_ui(ctx, root, full);
            } else {
                self.home_ui(ctx, root, full);
            }
        }
        self.modal_ui(ctx);
        self.toast_ui(ctx);
        if let Some(after) = self.pending_leave.take() {
            self.request_leave(ctx, after);
        }
        if std::mem::take(&mut self.pending_export) {
            self.run_export(ctx);
        }
        if self.ed.is_some() {
            self.keyboard(ctx);
        }
    }
}

impl App {
    fn toast_ui(&mut self, ctx: &egui::Context) {
        let Some((msg, at)) = &self.toast else { return };
        if at.elapsed() > Duration::from_millis(2800) {
            self.toast = None;
            return;
        }
        let screen = ctx.content_rect();
        let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("toast")));
        let w = p.layout_no_wrap(msg.clone(), theme::font(13.0), theme::TEXT).size().x + 32.0;
        let rect = egui::Rect::from_center_size(egui::pos2(screen.center().x, screen.bottom() - 48.0), egui::vec2(w.min(screen.width() * 0.7), 40.0));
        p.rect_filled(rect, theme::r(10), theme::RAISE);
        p.rect_stroke(rect, theme::r(10), egui::Stroke::new(1.0, theme::LINE2), egui::StrokeKind::Inside);
        p.text(rect.center(), egui::Align2::CENTER_CENTER, msg, theme::font(13.0), theme::TEXT);
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn keyboard(&mut self, ctx: &egui::Context) {
        if !matches!(self.modal, Modal::None) || ctx.egui_wants_keyboard_input() {
            return;
        }
        let (cmd, shift) = ctx.input(|i| (i.modifiers.command, i.modifiers.shift));
        let pressed = |k: Key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, k));
        let pressed_shift = |k: Key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::SHIFT, k));
        if cmd {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, Key::Z)) {
                if let Some(ed) = &mut self.ed {
                    ed.redo();
                }
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::Z)) {
                if let Some(ed) = &mut self.ed {
                    ed.undo();
                }
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::S)) {
                if let Some(ed) = &mut self.ed {
                    ed.save();
                }
                self.toast("Saved");
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::E)) {
                self.run_export(ctx);
            }
            return;
        }
        let sel = self.ed.as_ref().and_then(|e| e.sel.clone());
        if pressed(Key::Space) {
            if let Some(ed) = &self.ed {
                self.player.toggle(ed);
            }
        }
        if pressed(Key::ArrowDown) || pressed(Key::J) {
            self.step_clip(1);
        }
        if pressed(Key::ArrowUp) || pressed(Key::K) {
            self.step_clip(-1);
        }
        let mut seek_by = 0.0;
        if pressed_shift(Key::ArrowRight) {
            seek_by = 5000.0;
        } else if pressed(Key::ArrowRight) {
            seek_by = 500.0;
        }
        if pressed_shift(Key::ArrowLeft) {
            seek_by = -5000.0;
        } else if pressed(Key::ArrowLeft) {
            seek_by = -500.0;
        }
        if seek_by != 0.0 {
            if let Some(ed) = &mut self.ed {
                let t = ed.t + seek_by;
                self.player.seek(ed, t);
            }
        }
        if let Some(id) = &sel {
            if pressed(Key::Backspace) || pressed(Key::Delete) {
                let next = self.ed.as_mut().and_then(|e| e.dismiss(id));
                self.select_clip(next, false);
                return;
            }
            if pressed(Key::X) || pressed(Key::Enter) {
                if let Some(ed) = &mut self.ed {
                    ed.toggle_pick(id);
                }
            }
            if pressed(Key::OpenBracket) || pressed_shift(Key::OpenBracket) {
                if let Some(ed) = &mut self.ed {
                    let t = if shift { ed.t } else { ed.snap_start(ed.t) };
                    ed.trim_to(id, true, t);
                }
            }
            if pressed(Key::CloseBracket) || pressed_shift(Key::CloseBracket) {
                if let Some(ed) = &mut self.ed {
                    let t = if shift { ed.t } else { ed.snap_end(ed.t) };
                    ed.trim_to(id, false, t);
                }
            }
            if pressed(Key::T) {
                if let Some(ed) = &mut self.ed {
                    ed.edit_clip(id, "cuts", |c| c.jump_cut = !c.jump_cut);
                }
            }
            if pressed(Key::Q) {
                if let Some(ed) = &mut self.ed {
                    let s = ed.sel_clip().map_or(0.0, |c| c.start_ms as f64);
                    self.player.seek(ed, s);
                }
            }
        }
        if let Some(ed) = &mut self.ed {
            if pressed(Key::L) {
                ed.looping = !ed.looping;
            }
            if pressed(Key::C) {
                ed.caps_on = !ed.caps_on;
            }
            if pressed(Key::Num1) {
                ed.tab = Tab::Clip;
            }
            if pressed(Key::Num2) {
                ed.tab = Tab::Captions;
            }
            if pressed(Key::Num3) {
                ed.tab = Tab::Framing;
            }
            if pressed(Key::Num4) {
                ed.tab = Tab::Export;
            }
            if pressed(Key::N) {
                let (a, b) = (ed.snap_start(ed.t), ed.snap_end((ed.t + 30000.0).min(ed.duration())));
                let id = ed.new_clip(a, b);
                self.select_clip(Some(id), false);
            }
        }
    }
}

#[allow(dead_code)]
fn _unused(_: Color32, _: &Word) {}
