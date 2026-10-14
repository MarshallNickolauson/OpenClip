//! Safe wrappers over the Objective-C media layer in `native/clipnative.m`.

use clipcraft_core::framing::FaceTrack;
use std::ffi::{c_char, c_int, c_void, CStr, CString};

extern "C" {
    fn cn_probe(path: *const c_char) -> *mut c_char;
    fn cn_decode_audio(
        path: *const c_char,
        n: *mut usize,
        cb: Option<extern "C" fn(f64, *mut c_void) -> c_int>,
        ud: *mut c_void,
        err: *mut *mut c_char,
    ) -> *mut f32;
    fn cn_scan_faces(
        path: *const c_char,
        step_ms: c_int,
        cb: Option<extern "C" fn(f64, *mut c_void) -> c_int>,
        ud: *mut c_void,
        err: *mut *mut c_char,
    ) -> *mut c_char;
    fn cn_thumbnail(path: *const c_char, t_ms: f64, out_h: c_int, cx: f64, out: *const c_char) -> *mut c_char;
    fn cn_export(job: *const c_char, cb: Option<extern "C" fn(f64, *mut c_void) -> c_int>, ud: *mut c_void) -> *mut c_char;
    fn cn_free(p: *mut c_void);
    fn cn_trim_edit_menu();
}

/// This is an editor, not a form: drop the system's AutoFill / Dictation / Emoji items from Edit.
pub fn trim_edit_menu() {
    unsafe { cn_trim_edit_menu() }
}

/// Progress sink: gets 0..100, returns `true` to cancel.
pub type Progress<'a> = &'a (dyn Fn(f64) -> bool + Sync);

extern "C" fn tramp(pct: f64, ud: *mut c_void) -> c_int {
    // SAFETY: `ud` always points at a `Progress` that outlives the native call.
    let f = unsafe { &*(ud as *const Progress) };
    c_int::from(f(pct))
}

fn cstr(s: &str) -> Result<CString, String> {
    CString::new(s).map_err(|_| "path contains a NUL byte".to_string())
}

fn take(p: *mut c_char) -> Option<String> {
    if p.is_null() {
        return None;
    }
    // SAFETY: the native side returns a NUL-terminated strdup'd string.
    let s = unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned();
    unsafe { cn_free(p as *mut c_void) };
    Some(s)
}

#[derive(serde::Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Probe {
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
    pub has_audio: bool,
}

pub fn probe(path: &str) -> Result<Probe, String> {
    let p = cstr(path)?;
    let json = take(unsafe { cn_probe(p.as_ptr()) }).ok_or("probe failed")?;
    let v: serde_json::Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    if let Some(e) = v.get("error").and_then(|e| e.as_str()) {
        return Err(e.to_string());
    }
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// Mono 16 kHz PCM.
pub fn decode_audio(path: &str, progress: Progress) -> Result<Vec<f32>, String> {
    let p = cstr(path)?;
    let mut n = 0usize;
    let mut err: *mut c_char = std::ptr::null_mut();
    let cb: &Progress = &progress;
    let buf = unsafe { cn_decode_audio(p.as_ptr(), &mut n, Some(tramp), cb as *const Progress as *mut c_void, &mut err) };
    if buf.is_null() {
        return Err(take(err).unwrap_or_else(|| "audio decode failed".into()));
    }
    // SAFETY: `buf` holds `n` floats allocated with malloc.
    let v = unsafe { std::slice::from_raw_parts(buf, n) }.to_vec();
    unsafe { cn_free(buf as *mut c_void) };
    Ok(v)
}

pub fn scan_faces(path: &str, step_ms: i32, progress: Progress) -> Result<FaceTrack, String> {
    let p = cstr(path)?;
    let mut err: *mut c_char = std::ptr::null_mut();
    let cb: &Progress = &progress;
    let out = unsafe { cn_scan_faces(p.as_ptr(), step_ms, Some(tramp), cb as *const Progress as *mut c_void, &mut err) };
    let json = match take(out) {
        Some(j) => j,
        None => return Err(take(err).unwrap_or_else(|| "face scan failed".into())),
    };
    serde_json::from_str(&json).map_err(|e| e.to_string())
}

pub fn thumbnail(path: &str, t_ms: f64, out_h: i32, cx: f64, out: &str) -> Result<(), String> {
    let (p, o) = (cstr(path)?, cstr(out)?);
    match take(unsafe { cn_thumbnail(p.as_ptr(), t_ms, out_h, cx, o.as_ptr()) }) {
        None => Ok(()),
        Some(e) => Err(e),
    }
}

pub fn export(job: &str, progress: Progress) -> Result<(), String> {
    let j = cstr(job)?;
    let cb: &Progress = &progress;
    match take(unsafe { cn_export(j.as_ptr(), Some(tramp), cb as *const Progress as *mut c_void) }) {
        None => Ok(()),
        Some(e) => Err(e),
    }
}
