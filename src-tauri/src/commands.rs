//! Tauri commands: the thin layer between the UI and the engine.

use crate::engine::{self, Emit};
use crate::{native, whisper};
use clipcraft_core::captions::{self, CaptionStyle};
use clipcraft_core::framing::{CamPath, FaceTrack, Subject};
use clipcraft_core::moments::{candidates, Ctx};
use clipcraft_core::project::{new_id, Clip, Origin, Project};
use clipcraft_core::store::{self, Energy, Suggestions, Summary};
use clipcraft_core::transcript::{join, sentences, Sentence, Word};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager, State};

pub struct Loaded {
    words: Vec<Word>,
    sents: Vec<Sentence>,
    faces: Option<FaceTrack>,
    energy: Option<Energy>,
    src_w: u32,
    src_h: u32,
    src_path: String,
}

#[derive(Default)]
pub struct AppState {
    loaded: Mutex<HashMap<String, Arc<Loaded>>>,
    cancel_import: Arc<AtomicBool>,
    cancel_export: Arc<AtomicBool>,
    watch: Mutex<Option<PathBuf>>,
}

type R<T> = Result<T, String>;

fn emitter(app: &AppHandle) -> Emit {
    let app = app.clone();
    Arc::new(move |name: &str, v: Value| {
        let _ = app.emit(name, v);
    })
}

fn mcp_path() -> String {
    let exe = std::env::current_exe().unwrap_or_default();
    let dir = exe.parent().map(Path::to_path_buf).unwrap_or_default();
    let p = dir.join("clipcraft-mcp");
    if p.exists() {
        return p.to_string_lossy().into_owned();
    }
    // dev builds: both binaries sit side by side in target/<profile>/
    p.to_string_lossy().into_owned()
}

#[tauri::command]
pub fn app_info() -> Value {
    let models: Vec<Value> = whisper::MODELS
        .iter()
        .map(|(id, label, _, size)| json!({"id": id, "label": label, "present": whisper::model_present(id), "mb": size / 1_000_000}))
        .collect();
    let mcp = mcp_path();
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "home": store::home(),
        "models": models,
        "mcpPath": mcp,
        "mcpExists": Path::new(&mcp).exists(),
        "exportDir": engine::default_export_dir(),
    })
}

#[tauri::command]
pub fn list_projects() -> Vec<Summary> {
    store::list()
}

#[tauri::command]
pub async fn import_video(app: AppHandle, state: State<'_, AppState>, path: String, model: String, language: Option<String>) -> R<String> {
    let (dir, _) = {
        let p = path.clone();
        tauri::async_runtime::spawn_blocking(move || engine::create_project(&p)).await.map_err(|e| e.to_string())??
    };
    state.cancel_import.store(false, Ordering::Relaxed);
    let cancel = state.cancel_import.clone();
    let emit = emitter(&app);
    let d2 = dir.clone();
    std::thread::spawn(move || {
        let opts = engine::ImportOpts { model, language: language.unwrap_or_else(|| "en".into()) };
        if let Err(e) = engine::run_pipeline(&d2, &opts, &emit, &cancel) {
            emit("import-progress", json!({"dir": d2.to_string_lossy(), "stage": "error", "pct": 0, "error": e}));
        }
    });
    Ok(dir.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn cancel_import(state: State<'_, AppState>) {
    state.cancel_import.store(true, Ordering::Relaxed);
}

fn load_for(state: &AppState, dir: &str) -> R<Arc<Loaded>> {
    if let Some(l) = state.loaded.lock().unwrap().get(dir) {
        return Ok(l.clone());
    }
    let p = Path::new(dir);
    let proj = store::load_project(p)?;
    let t = store::load_transcript(p)?;
    let sents = sentences(&t.words);
    let l = Arc::new(Loaded {
        words: t.words,
        sents,
        faces: store::load_faces(p),
        energy: store::load_energy(p),
        src_w: proj.source.width,
        src_h: proj.source.height,
        src_path: proj.source.path,
    });
    state.loaded.lock().unwrap().insert(dir.to_string(), l.clone());
    Ok(l)
}

#[tauri::command]
pub fn open_project(state: State<'_, AppState>, dir: String) -> R<Value> {
    let p = Path::new(&dir);
    let mut proj = store::load_project(p)?;
    // forget cached data: the pipeline may have finished since the last open
    state.loaded.lock().unwrap().remove(&dir);
    let mut words: Vec<Word> = Vec::new();
    let mut sents: Vec<Sentence> = Vec::new();
    let mut energy: Option<Energy> = None;
    if proj.transcribed {
        if let Ok(l) = load_for(&state, &dir) {
            words = l.words.clone();
            sents = l.sents.clone();
            energy = l.energy.clone();
        }
        if !proj.faces && p.join("faces.json").exists() {
            proj.faces = true;
        }
    }
    store::mark_current(p);
    *state.watch.lock().unwrap() = Some(p.to_path_buf());
    Ok(json!({
        "dir": dir,
        "project": proj,
        "words": words,
        "sentences": sents,
        "energy": energy,
        "sourceExists": Path::new(&proj.source.path).exists(),
    }))
}

#[tauri::command]
pub fn save_project(dir: String, project: Project) -> R<()> {
    store::save_project(Path::new(&dir), &project)
}

#[tauri::command]
pub fn clip_path(state: State<'_, AppState>, dir: String, clip: Clip, subject: Subject, smooth: f32) -> R<CamPath> {
    let l = load_for(&state, &dir)?;
    // faces may have been scanned after the project was first opened
    let faces = match &l.faces {
        Some(f) => Some(f.clone()),
        None => store::load_faces(Path::new(&dir)),
    };
    Ok(clipcraft_core::framing::camera_path(faces.as_ref(), l.src_w, l.src_h, clip.start_ms, clip.end_ms, &clip.framing, subject, smooth))
}

#[tauri::command]
pub fn clip_captions(state: State<'_, AppState>, dir: String, start_ms: u64, end_ms: u64, style: CaptionStyle, edits: BTreeMap<String, String>, jump_cut: Option<bool>) -> R<Value> {
    let l = load_for(&state, &dir)?;
    let st = captions::resolve(&style);
    let chunks = captions::chunks(&l.words, start_ms, end_ms, st.words, &edits);
    let segments = if jump_cut.unwrap_or(false) { clipcraft_core::transcript::keep_segments(&l.words, l.energy.as_ref(), start_ms, end_ms) } else { vec![(start_ms, end_ms)] };
    Ok(json!({"style": st, "chunks": chunks, "segments": segments}))
}

#[tauri::command]
pub async fn thumbnail(state: State<'_, AppState>, dir: String, clip: Clip, subject: Subject, smooth: f32) -> R<String> {
    let l = load_for(&state, &dir)?;
    let faces = l.faces.clone().or_else(|| store::load_faces(Path::new(&dir)));
    let t = clip.start_ms + 900;
    let out_dir = Path::new(&dir).join("thumbs");
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let file = out_dir.join(format!("{}-{}.jpg", clip.id, t / 250));
    if file.exists() {
        return Ok(file.to_string_lossy().into_owned());
    }
    let mut c = clip.clone();
    c.end_ms = c.start_ms + 2000;
    let path = clipcraft_core::framing::camera_path(faces.as_ref(), l.src_w, l.src_h, c.start_ms, c.end_ms, &c.framing, subject, smooth);
    let cx = path.cx.get(9).copied().unwrap_or(0.5) as f64;
    let (src, f2) = (l.src_path.clone(), file.clone());
    tauri::async_runtime::spawn_blocking(move || native::thumbnail(&src, t as f64, 360, cx, &f2.to_string_lossy()))
        .await
        .map_err(|e| e.to_string())??;
    Ok(file.to_string_lossy().into_owned())
}

/// Quick local pick without Claude: the best non-overlapping moments by the built-in scoring.
#[tauri::command]
pub fn local_pick(state: State<'_, AppState>, dir: String, n: usize) -> R<Vec<Clip>> {
    let l = load_for(&state, &dir)?;
    let faces = l.faces.clone().or_else(|| store::load_faces(Path::new(&dir)));
    let c = candidates(&Ctx { words: &l.words, sents: &l.sents, energy: l.energy.as_ref(), faces: faces.as_ref() }, n, 20_000, 70_000);
    let mut out = Vec::new();
    for m in c {
        let w0 = l.sents[m.s0 as usize].w0 as usize;
        let w1 = l.sents[m.s1 as usize].w1 as usize;
        let mut start = l.sents[m.s0 as usize].t0.saturating_sub(180);
        if w0 > 0 {
            start = start.max(l.words[w0 - 1].2);
        }
        let mut end = l.sents[m.s1 as usize].t1 + 380;
        if w1 + 1 < l.words.len() {
            end = end.min(l.words[w1 + 1].1.saturating_sub(20).max(l.sents[m.s1 as usize].t1));
        }
        let text = join(&l.words[w0..(w0 + 7).min(w1 + 1)]);
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
    Ok(out)
}

#[tauri::command]
pub fn read_suggestions(dir: String) -> Suggestions {
    store::load_suggestions(Path::new(&dir))
}

#[tauri::command]
pub fn mcp_seen() -> Option<Value> {
    store::read_mcp_seen().map(|(t, tool)| json!({"ts": t, "tool": tool}))
}

#[tauri::command]
pub async fn export_clips(app: AppHandle, state: State<'_, AppState>, dir: String, project: Project, ids: Vec<String>) -> R<()> {
    let l = load_for(&state, &dir)?;
    state.cancel_export.store(false, Ordering::Relaxed);
    let cancel = state.cancel_export.clone();
    let emit = emitter(&app);
    std::thread::spawn(move || {
        let faces = l.faces.clone().or_else(|| store::load_faces(Path::new(&dir)));
        let total = ids.len();
        for (n, id) in ids.iter().enumerate() {
            let Some(clip) = project.clips.iter().find(|c| &c.id == id) else { continue };
            emit("export-progress", json!({"id": id, "state": "running", "pct": 0.0, "n": n, "total": total}));
            let last = std::sync::atomic::AtomicI64::new(-1);
            let r = engine::export_clip(Path::new(&dir), &project, &l.words, faces.as_ref(), clip, &|p| {
                let q = p as i64;
                if last.swap(q, Ordering::Relaxed) != q {
                    emit("export-progress", json!({"id": id, "state": "running", "pct": p, "n": n, "total": total}));
                }
                cancel.load(Ordering::Relaxed)
            });
            match r {
                Ok(path) => emit("export-progress", json!({"id": id, "state": "done", "pct": 100.0, "path": path, "n": n, "total": total})),
                Err(e) => {
                    let cancelled = e == "cancelled";
                    emit("export-progress", json!({"id": id, "state": if cancelled { "cancelled" } else { "error" }, "error": e, "n": n, "total": total}));
                    if cancelled {
                        break;
                    }
                }
            }
        }
        emit("export-finished", json!({}));
    });
    Ok(())
}

#[tauri::command]
pub fn cancel_export(state: State<'_, AppState>) {
    state.cancel_export.store(true, Ordering::Relaxed);
}

#[tauri::command]
pub fn reveal(path: String) {
    let _ = std::process::Command::new("open").arg("-R").arg(path).spawn();
}

#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, dir: String) -> R<()> {
    let p = Path::new(&dir);
    // only ever remove ClipCraft project folders
    if p.extension().and_then(|s| s.to_str()) != Some(store::EXT) || !store::project_file(p).exists() {
        return Err("not a ClipCraft project".into());
    }
    state.loaded.lock().unwrap().remove(&dir);
    std::fs::remove_dir_all(p).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn relink_source(state: State<'_, AppState>, dir: String, path: String) -> R<Value> {
    let pr = native::probe(&path)?;
    let mut p = store::load_project(Path::new(&dir))?;
    p.source.path = path.clone();
    p.source.name = Path::new(&path).file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    p.source.width = pr.width;
    p.source.height = pr.height;
    store::save_project(Path::new(&dir), &p)?;
    state.loaded.lock().unwrap().remove(&dir);
    Ok(json!(p))
}

/// Add ClipCraft to Claude Desktop's MCP config (merging, never replacing other servers).
#[tauri::command]
pub fn connect_claude_desktop() -> R<String> {
    let h = std::env::var("HOME").map_err(|e| e.to_string())?;
    let dir = PathBuf::from(&h).join("Library/Application Support/Claude");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let file = dir.join("claude_desktop_config.json");
    let mut cfg: Value = match std::fs::read(&file) {
        Ok(b) if !b.is_empty() => serde_json::from_slice(&b).map_err(|e| format!("existing config is not valid JSON: {e}"))?,
        _ => json!({}),
    };
    if !cfg.is_object() {
        return Err("existing config is not a JSON object".into());
    }
    let servers = cfg.as_object_mut().unwrap().entry("mcpServers").or_insert_with(|| json!({}));
    let Some(map) = servers.as_object_mut() else { return Err("mcpServers is not an object".into()) };
    map.insert("clipcraft".into(), json!({"command": mcp_path(), "args": []}));
    if file.exists() {
        let _ = std::fs::copy(&file, dir.join("claude_desktop_config.json.bak"));
    }
    let out = serde_json::to_vec_pretty(&cfg).map_err(|e| e.to_string())?;
    std::fs::write(&file, out).map_err(|e| e.to_string())?;
    Ok(file.to_string_lossy().into_owned())
}

#[tauri::command]
pub async fn download_model(app: AppHandle, id: String) -> R<()> {
    tauri::async_runtime::spawn_blocking(move || {
        whisper::ensure_model(&id, &|p| {
            let _ = app.emit("model-progress", json!({"id": id, "pct": p}));
        })
        .map(|_| ())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Poll the open project's suggestions file and the MCP heartbeat; cheap and dependency-free.
pub fn start_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
        let mut last_sug = None;
        let mut last_seen = None;
        let mut last_dir: Option<PathBuf> = None;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(400));
            let state = app.state::<AppState>();
            let dir = state.watch.lock().unwrap().clone();
            if dir != last_dir {
                last_sug = None;
                last_dir = dir.clone();
            }
            if let Some(d) = dir {
                let m = mtime(&d.join("suggestions.json"));
                if m != last_sug {
                    if last_sug.is_some() || m.is_some() {
                        let _ = app.emit("suggestions-changed", json!({"dir": d.to_string_lossy()}));
                    }
                    last_sug = m;
                }
            }
            let seen = mtime(&store::home().join("mcp-seen"));
            if seen != last_seen {
                last_seen = seen;
                if let Some((t, tool)) = store::read_mcp_seen() {
                    let _ = app.emit("mcp-seen", json!({"ts": t, "tool": tool}));
                }
            }
        }
    });
}

#[tauri::command]
pub fn media_url(path: String) -> String {
    crate::media::url_for(&path)
}

pub static ALLOW_EXIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Called by the UI once any unsaved-changes question has been answered.
#[tauri::command]
pub fn exit_app(app: AppHandle) {
    ALLOW_EXIT.store(true, Ordering::SeqCst);
    app.exit(0);
}
