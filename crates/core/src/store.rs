//! Where projects live and how the app, the CLI and the MCP server share them.
//!
//! The app owns `project.json`. Claude (through the MCP server) never writes it; it drops
//! suggestions into `suggestions.json`, which the app merges in as soon as it changes.

use crate::framing::FaceTrack;
use crate::project::{Clip, Project};
use crate::transcript::Transcript;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const EXT: &str = "openclip";
/// Folder extension used before the app was renamed; still read, never written.
pub const OLD_EXT: &str = "clipcraft";

pub fn home() -> PathBuf {
    for var in ["OPENCLIP_HOME", "CLIPCRAFT_HOME"] {
        if let Ok(p) = std::env::var(var) {
            if !p.is_empty() {
                return PathBuf::from(p);
            }
        }
    }
    let h = std::env::var("HOME").unwrap_or_else(|_| ".".into());
    let new = PathBuf::from(&h).join("OpenClip Projects");
    // the app used to be called ClipCraft: carry its projects over once
    let old = PathBuf::from(&h).join("ClipCraft Projects");
    if !new.exists() && old.exists() {
        let _ = std::fs::rename(&old, &new);
    }
    new
}

pub fn project_file(dir: &Path) -> PathBuf {
    dir.join("project.json")
}

fn read_json<T: for<'a> Deserialize<'a>>(p: &Path) -> Result<T, String> {
    let b = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
    serde_json::from_slice(&b).map_err(|e| format!("{}: {e}", p.display()))
}

/// Write via a temp file and rename, so a reader never sees half a file.
pub fn write_json<T: Serialize>(p: &Path, v: &T) -> Result<(), String> {
    let tmp = p.with_extension("json.tmp");
    let bytes = serde_json::to_vec(v).map_err(|e| e.to_string())?;
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, p).map_err(|e| e.to_string())
}

pub fn load_project(dir: &Path) -> Result<Project, String> {
    read_json(&project_file(dir))
}
pub fn save_project(dir: &Path, p: &Project) -> Result<(), String> {
    write_json(&project_file(dir), p)
}
pub fn load_transcript(dir: &Path) -> Result<Transcript, String> {
    read_json(&dir.join("transcript.json"))
}
pub fn save_transcript(dir: &Path, t: &Transcript) -> Result<(), String> {
    write_json(&dir.join("transcript.json"), t)
}
pub fn load_faces(dir: &Path) -> Option<FaceTrack> {
    read_json(&dir.join("faces.json")).ok()
}
pub fn save_faces(dir: &Path, f: &FaceTrack) -> Result<(), String> {
    write_json(&dir.join("faces.json"), f)
}

/// Loudness per 100 ms, 0..255 mapping -60..0 dBFS.
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Energy {
    pub step_ms: u32,
    pub v: Vec<u8>,
}
pub fn load_energy(dir: &Path) -> Option<Energy> {
    read_json(&dir.join("energy.json")).ok()
}
pub fn save_energy(dir: &Path, e: &Energy) -> Result<(), String> {
    write_json(&dir.join("energy.json"), e)
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Suggestions {
    pub rev: u32,
    pub clips: Vec<Clip>,
}
pub fn load_suggestions(dir: &Path) -> Suggestions {
    read_json(&dir.join("suggestions.json")).unwrap_or_default()
}
pub fn save_suggestions(dir: &Path, s: &Suggestions) -> Result<(), String> {
    write_json(&dir.join("suggestions.json"), s)
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub dir: String,
    pub id: String,
    pub title: String,
    pub duration_ms: u64,
    pub clips: usize,
    pub picked: usize,
    pub transcribed: bool,
    pub modified_ms: u64,
    pub source_missing: bool,
}

pub fn list() -> Vec<Summary> {
    let mut out = Vec::new();
    let Ok(rd) = std::fs::read_dir(home()) else { return out };
    for e in rd.flatten() {
        let dir = e.path();
        if !matches!(dir.extension().and_then(|s| s.to_str()), Some(EXT) | Some(OLD_EXT)) {
            continue;
        }
        let Ok(p) = load_project(&dir) else { continue };
        let modified_ms = std::fs::metadata(project_file(&dir))
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let live: Vec<&Clip> = p.clips.iter().filter(|c| !c.dismissed).collect();
        out.push(Summary {
            dir: dir.to_string_lossy().into(),
            id: p.id.clone(),
            title: p.title.clone(),
            duration_ms: p.source.duration_ms,
            clips: live.len(),
            picked: live.iter().filter(|c| c.picked).count(),
            transcribed: p.transcribed,
            modified_ms,
            source_missing: !Path::new(&p.source.path).exists(),
        });
    }
    out.sort_by(|a, b| b.modified_ms.cmp(&a.modified_ms));
    out
}

/// Find a project by id, folder name or title fragment. `None` means the project open in the app,
/// else the most recently changed one.
pub fn resolve(which: Option<&str>) -> Result<PathBuf, String> {
    let all = list();
    if all.is_empty() {
        return Err("No OpenClip projects yet. Open OpenClip and drop a video in first.".into());
    }
    match which.map(str::trim).filter(|s| !s.is_empty()) {
        Some(q) => {
            let ql = q.to_lowercase();
            all.iter()
                .find(|s| s.id == q)
                .or_else(|| all.iter().find(|s| s.title.to_lowercase() == ql))
                .or_else(|| all.iter().find(|s| s.title.to_lowercase().contains(&ql)))
                .map(|s| PathBuf::from(&s.dir))
                .ok_or_else(|| format!("No project matches \"{q}\". Call list_projects."))
        }
        None => {
            if let Ok(cur) = std::fs::read_to_string(home().join("current")) {
                let p = PathBuf::from(cur.trim());
                if project_file(&p).exists() {
                    return Ok(p);
                }
            }
            Ok(PathBuf::from(&all[0].dir))
        }
    }
}

/// Remember which project the app has open so Claude can default to it.
pub fn mark_current(dir: &Path) {
    let _ = std::fs::create_dir_all(home());
    let _ = std::fs::write(home().join("current"), dir.to_string_lossy().as_bytes());
}

/// The MCP server stamps this on every call so the app can show "Claude connected".
pub fn mark_mcp_seen(tool: &str) {
    let _ = std::fs::create_dir_all(home());
    let line = format!("{} {}", crate::project::now_ms(), tool);
    let _ = std::fs::write(home().join("mcp-seen"), line);
}

pub fn read_mcp_seen() -> Option<(u64, String)> {
    let s = std::fs::read_to_string(home().join("mcp-seen")).ok()?;
    let (t, tool) = s.split_once(' ')?;
    Some((t.parse().ok()?, tool.trim().to_string()))
}
