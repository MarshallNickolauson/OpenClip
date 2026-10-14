//! Project and clip model (serialised to `project.json`).

use crate::captions::CaptionStyle;
use crate::framing::{Framing, Subject};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Source {
    pub path: String,
    pub name: String,
    pub duration_ms: u64,
    pub width: u32,
    pub height: u32,
    pub fps: f32,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    Claude,
    Auto,
    #[default]
    Manual,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Clip {
    pub id: String,
    pub title: String,
    /// why it was suggested (shown on the card)
    pub why: String,
    pub score: f32,
    pub start_ms: u64,
    pub end_ms: u64,
    pub origin: Origin,
    /// ticked for export
    pub picked: bool,
    /// removed from the list but kept so a re-sent suggestion does not come back
    pub dismissed: bool,
    pub captions: bool,
    /// cut out the silences between words
    pub jump_cut: bool,
    /// per-clip caption look; falls back to the project's
    pub style: Option<CaptionStyle>,
    pub framing: Framing,
    /// transcript word index -> replacement text ("" hides the word)
    pub edits: BTreeMap<String, String>,
    /// suggestion revision this clip last took from `suggestions.json`
    pub srev: u32,
    /// the user changed it, so later suggestions must not overwrite it
    pub touched: bool,
    pub exported: Option<String>,
}

impl Default for Clip {
    fn default() -> Self {
        Self {
            id: String::new(),
            title: String::new(),
            why: String::new(),
            score: 0.0,
            start_ms: 0,
            end_ms: 0,
            origin: Origin::Manual,
            picked: false,
            dismissed: false,
            captions: true,
            jump_cut: false,
            style: None,
            framing: Framing::default(),
            edits: BTreeMap::new(),
            srev: 0,
            touched: false,
            exported: None,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct ExportSettings {
    pub dir: String,
    /// output width in px (height is 16/9 of it)
    pub width: u32,
    /// "h264" | "hevc"
    pub codec: String,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self { dir: String::new(), width: 1080, codec: "h264".into() }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub version: u32,
    pub id: String,
    pub title: String,
    pub created_ms: u64,
    pub source: Source,
    pub transcribed: bool,
    pub faces: bool,
    pub model: String,
    pub style: CaptionStyle,
    pub subject: Subject,
    /// camera smoothing time in seconds
    pub smooth: f32,
    pub export: ExportSettings,
    pub clips: Vec<Clip>,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            version: VERSION,
            id: String::new(),
            title: String::new(),
            created_ms: 0,
            source: Source::default(),
            transcribed: false,
            faces: false,
            model: String::new(),
            style: CaptionStyle::default(),
            subject: Subject::Largest,
            smooth: 0.55,
            export: ExportSettings::default(),
            clips: Vec::new(),
        }
    }
}

/// Unique-enough id without a uuid dependency.
pub fn new_id(prefix: &str) -> String {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(now_ms());
    h.write_u64(N.fetch_add(1, Ordering::Relaxed));
    format!("{prefix}{:010x}", h.finish() & 0xff_ffff_ffff)
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// A file-system safe slug for folder and file names.
pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in s.chars() {
        if c.is_alphanumeric() {
            out.push(c);
            dash = false;
        } else if !dash && !out.is_empty() {
            out.push('-');
            dash = true;
        }
    }
    let t = out.trim_matches('-').to_string();
    let t: String = t.chars().take(60).collect();
    if t.is_empty() { "clip".into() } else { t }
}
