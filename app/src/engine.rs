//! The import pipeline (probe -> audio -> whisper + faces in parallel) and clip export.
//! Used by the app and by the headless `openclip import` / `openclip export` commands.

use crate::native;
use crate::whisper;
use openclip_core::captions::{self, CaptionStyle};
use openclip_core::framing::{camera_path, FaceTrack};
use openclip_core::project::{new_id, now_ms, slug, Clip, Project, Source};
use openclip_core::store::{self, Energy};
use openclip_core::transcript::{keep_segments, Transcript, Word};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub type Emit = Arc<dyn Fn(&str, Value) + Send + Sync>;

pub fn unique_dir(title: &str) -> PathBuf {
    let base = store::home();
    let s = slug(title);
    let mut i = 1;
    loop {
        let name = if i == 1 { format!("{s}.{}", store::EXT) } else { format!("{s}-{i}.{}", store::EXT) };
        let p = base.join(name);
        if !p.exists() {
            return p;
        }
        i += 1;
    }
}

/// 100 ms loudness envelope: dBFS -60..0 mapped to 0..255.
fn energy_of(pcm: &[f32]) -> Energy {
    let bin = 1600;
    let v = pcm
        .chunks(bin)
        .map(|c| {
            let ms = c.iter().map(|x| x * x).sum::<f32>() / c.len().max(1) as f32;
            let db = 10.0 * ms.max(1e-9).log10();
            (((db + 60.0) / 60.0).clamp(0.0, 1.0) * 255.0) as u8
        })
        .collect();
    Energy { step_ms: 100, v }
}

pub struct ImportOpts {
    pub model: String,
    pub language: String,
}

/// Create the project folder and return immediately with its path; call [`run_pipeline`] to fill it.
pub fn create_project(path: &str) -> Result<(PathBuf, Project), String> {
    let pr = native::probe(path)?;
    let p = Path::new(path);
    let name = p.file_name().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let title = p.file_stem().map(|s| s.to_string_lossy().replace(['_', '-'], " ")).unwrap_or_else(|| "Untitled".into());
    let dir = unique_dir(&title);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let mut proj = Project {
        id: new_id("p"),
        title,
        created_ms: now_ms(),
        source: Source { path: path.to_string(), name, duration_ms: pr.duration_ms, width: pr.width, height: pr.height, fps: pr.fps },
        ..Project::default()
    };
    if !pr.has_audio {
        return Err("this video has no audio track to transcribe".into());
    }
    proj.export.dir = default_export_dir();
    store::save_project(&dir, &proj)?;
    Ok((dir, proj))
}

pub fn default_export_dir() -> String {
    let h = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    format!("{h}/Movies/OpenClip")
}

/// Transcribe, find faces, build the loudness envelope. Emits `import-progress` events.
pub fn run_pipeline(dir: &Path, opts: &ImportOpts, emit: &Emit, cancel: &Arc<AtomicBool>) -> Result<(), String> {
    let mut proj = store::load_project(dir)?;
    let src = proj.source.path.clone();
    let dstr = dir.to_string_lossy().to_string();
    let ev = |stage: &str, pct: f64| emit("import-progress", json!({"dir": dstr, "stage": stage, "pct": pct}));

    // faces run alongside speech recognition (Vision on the ANE/GPU, whisper on the GPU)
    let faces_handle = {
        let (src, dir, emit, cancel, dstr) = (src.clone(), dir.to_path_buf(), emit.clone(), cancel.clone(), dstr.clone());
        std::thread::spawn(move || -> Result<(), String> {
            let last = std::sync::atomic::AtomicI64::new(-1);
            let prog = |pct: f64| {
                let p = pct as i64;
                if last.swap(p, Ordering::Relaxed) != p {
                    emit("import-progress", json!({"dir": dstr, "stage": "faces", "pct": pct}));
                }
                cancel.load(Ordering::Relaxed)
            };
            let track = native::scan_faces(&src, 250, &prog)?;
            store::save_faces(&dir, &track)?;
            emit("import-progress", json!({"dir": dstr, "stage": "faces", "pct": 100.0}));
            Ok(())
        })
    };

    ev("audio", 0.0);
    let last = std::sync::atomic::AtomicI64::new(-1);
    let pcm = native::decode_audio(&src, &|p| {
        let q = p as i64;
        if last.swap(q, Ordering::Relaxed) != q {
            ev("audio", p);
        }
        cancel.load(Ordering::Relaxed)
    })?;
    if cancel.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    store::save_energy(dir, &energy_of(&pcm))?;

    ev("model", 0.0);
    let model_file = whisper::ensure_model(&opts.model, &|p| ev("model", p))?;
    ev("transcribe", 0.0);
    let (emit2, dstr2) = (emit.clone(), dstr.clone());
    let words: Vec<Word> = whisper::transcribe(&model_file, &opts.model, &opts.language, &pcm, move |p| {
        emit2("import-progress", json!({"dir": dstr2, "stage": "transcribe", "pct": p as f64}));
    })?;
    drop(pcm);
    store::save_transcript(dir, &Transcript { model: opts.model.clone(), words })?;
    proj = store::load_project(dir).unwrap_or(proj);
    proj.transcribed = true;
    proj.model = opts.model.clone();
    store::save_project(dir, &proj)?;
    emit("import-progress", json!({"dir": dstr, "stage": "transcribed", "pct": 100.0}));

    match faces_handle.join() {
        Ok(Ok(())) => {
            let mut p = store::load_project(dir).unwrap_or(proj);
            p.faces = true;
            store::save_project(dir, &p)?;
        }
        Ok(Err(e)) => emit("import-progress", json!({"dir": dstr, "stage": "faces-error", "pct": 0, "error": e})),
        Err(_) => {}
    }
    emit("import-progress", json!({"dir": dstr, "stage": "done", "pct": 100.0}));
    Ok(())
}

/// Camera path for a clip given the project's face track.
pub fn path_for(proj: &Project, faces: Option<&FaceTrack>, clip: &Clip) -> openclip_core::framing::CamPath {
    camera_path(faces, proj.source.width, proj.source.height, clip.start_ms, clip.end_ms, &clip.framing, proj.subject, proj.smooth)
}

fn unique_file(dir: &Path, stem: &str) -> PathBuf {
    let mut i = 1;
    loop {
        let name = if i == 1 { format!("{stem}.mp4") } else { format!("{stem} {i}.mp4") };
        let p = dir.join(name);
        if !p.exists() {
            return p;
        }
        i += 1;
    }
}

/// Render one clip to an MP4. Returns the output path.
pub fn export_clip(
    dir: &Path,
    proj: &Project,
    words: &[Word],
    faces: Option<&FaceTrack>,
    clip: &Clip,
    progress: &(dyn Fn(f64) -> bool + Sync),
) -> Result<String, String> {
    let out_dir = if proj.export.dir.is_empty() { default_export_dir() } else { proj.export.dir.clone() };
    std::fs::create_dir_all(&out_dir).map_err(|e| format!("cannot create {out_dir}: {e}"))?;
    let width = proj.export.width.clamp(360, 2160) & !1;
    let height = (width * 16 / 9) & !1;
    let path = path_for(proj, faces, clip);
    let style_src: &CaptionStyle = clip.style.as_ref().unwrap_or(&proj.style);
    let caps = if clip.captions {
        let st = captions::resolve(style_src);
        let chunks = captions::chunks(words, clip.start_ms, clip.end_ms, st.words, &clip.edits);
        Some(json!({"style": st, "chunks": chunks}))
    } else {
        None
    };
    let file = unique_file(Path::new(&out_dir), &slug(&clip.title));
    let job = json!({
        "src": proj.source.path,
        "out": file.to_string_lossy(),
        "startMs": clip.start_ms,
        "endMs": clip.end_ms,
        "width": width,
        "height": height,
        "codec": proj.export.codec,
        "path": {"t0": path.t0, "step": path.step, "cx": path.cx, "cy": path.cy, "cropW": path.crop_w, "cropH": path.crop_h},
        "captions": caps,
        "segments": if clip.jump_cut { json!(keep_segments(words, store::load_energy(dir).as_ref(), clip.start_ms, clip.end_ms)) } else { Value::Null },
    });
    native::export(&job.to_string(), progress)?;
    Ok(file.to_string_lossy().into_owned())
}
