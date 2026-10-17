//! On-device speech recognition with whisper.cpp (via `whisper-rs`, Metal on Apple silicon).
//!
//! Nothing is uploaded. The only network request is the optional, user-started download of a
//! ggml model file; after that everything runs on this machine.

use openclip_core::transcript::{clean_words, Word};
use std::io::{Read, Write};
use std::path::PathBuf;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// (id, label, file, approximate bytes)
pub const MODELS: &[(&str, &str, &str, u64)] = &[
    ("base.en", "Fast", "ggml-base.en.bin", 147_964_211),
    ("small.en", "Balanced", "ggml-small.en.bin", 487_614_201),
    ("large-v3-turbo-q5_0", "Best", "ggml-large-v3-turbo-q5_0.bin", 574_041_195),
];
const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

pub fn data_dir() -> PathBuf {
    let h = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    PathBuf::from(h).join("Library/Application Support/dev.openclip.app")
}

fn models_dir() -> PathBuf {
    data_dir().join("models")
}

fn info(id: &str) -> Result<(&'static str, u64), String> {
    MODELS.iter().find(|m| m.0 == id).map(|m| (m.2, m.3)).ok_or_else(|| format!("unknown model {id}"))
}

pub fn model_present(id: &str) -> bool {
    info(id).map(|(f, _)| models_dir().join(f).exists() || sibling_copy(f).is_some()).unwrap_or(false)
}

/// ScreenCraft keeps the same ggml files; reuse them instead of downloading again.
fn sibling_copy(file: &str) -> Option<PathBuf> {
    let h = std::env::var("HOME").ok()?;
    ["dev.clipcraft.app", "dev.screencraft.app"]
        .iter()
        .map(|d| PathBuf::from(&h).join("Library/Application Support").join(d).join("models").join(file))
        .find(|p| p.exists())
}

/// Make sure the model file exists locally, downloading it if needed.
pub fn ensure_model(id: &str, on_progress: &dyn Fn(f64)) -> Result<PathBuf, String> {
    let (file, approx) = info(id)?;
    let dir = models_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let dest = dir.join(file);
    if dest.exists() {
        return Ok(dest);
    }
    if let Some(src) = sibling_copy(file) {
        if std::fs::hard_link(&src, &dest).is_ok() || std::fs::copy(&src, &dest).is_ok() {
            return Ok(dest);
        }
    }
    let part = dest.with_extension("part");
    let resp = ureq::get(&format!("{BASE_URL}{file}")).call().map_err(|e| format!("download failed: {e}"))?;
    let total = resp.header("content-length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(approx);
    let mut reader = resp.into_reader();
    let mut out = std::fs::File::create(&part).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 256 * 1024];
    let (mut done, mut last) = (0u64, -1i64);
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("download interrupted: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        let pct = (done as f64 / total as f64 * 100.0).min(100.0);
        if pct as i64 != last {
            last = pct as i64;
            on_progress(pct);
        }
    }
    out.flush().map_err(|e| e.to_string())?;
    drop(out);
    if done < approx / 2 {
        let _ = std::fs::remove_file(&part);
        return Err("the model download was incomplete".into());
    }
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}

/// Transcribe 16 kHz mono samples into word-level timestamps.
pub fn transcribe(model_file: &std::path::Path, model_id: &str, language: &str, pcm: &[f32], on_progress: impl FnMut(i32) + 'static) -> Result<Vec<Word>, String> {
    let ctx = WhisperContext::new_with_params(model_file.to_str().ok_or("bad model path")?, WhisperContextParameters::default())
        .map_err(|e| format!("could not load the model: {e}"))?;
    let mut state = ctx.create_state().map_err(|e| e.to_string())?;

    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    let threads = std::thread::available_parallelism().map(|n| n.get() as i32).unwrap_or(4).min(8);
    params.set_n_threads(threads);
    let lang = if model_id.ends_with(".en") { "en" } else { language };
    if lang == "auto" {
        params.set_language(None);
    } else {
        params.set_language(Some(lang));
    }
    params.set_translate(false);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    params.set_suppress_blank(true);
    params.set_suppress_nst(true);
    // an earlier segment's text must not drag later ones into repetition loops on long talks
    params.set_no_context(true);
    // one segment per word, with token-level timestamps
    params.set_token_timestamps(true);
    params.set_max_len(1);
    params.set_split_on_word(true);
    params.set_initial_prompt("Hello, and welcome. Today, we're going to talk about something important.");
    params.set_progress_callback_safe(on_progress);

    state.full(params, pcm).map_err(|e| format!("transcription failed: {e}"))?;

    let mut raw = Vec::new();
    for seg in state.as_iter() {
        let text = seg.to_str_lossy().map_err(|e| e.to_string())?.trim().to_string();
        if text.is_empty() || text.starts_with("[_") || (text.starts_with('[') && text.ends_with(']')) || (text.starts_with('(') && text.ends_with(')')) || text.starts_with('♪') {
            continue;
        }
        let s = (seg.start_timestamp().max(0) as u32).saturating_mul(10);
        let e = (seg.end_timestamp().max(0) as u32).saturating_mul(10);
        raw.push(Word(text, s, e));
    }
    Ok(clean_words(raw))
}
