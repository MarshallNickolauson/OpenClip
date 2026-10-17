//! The open project and everything derived from it: edits with undo, camera path, captions,
//! snapping helpers. UI-free so it stays easy to reason about.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::time::Instant;

use eframe::egui::TextureHandle;
use openclip_core::captions::{self, Chunk, ResolvedStyle};
use openclip_core::framing::{camera_path, CamPath, FaceTrack, Framing};
use openclip_core::project::{new_id, Clip, Origin, Project};
use openclip_core::store::{self, Energy};
use openclip_core::transcript::{keep_segments, Sentence, Word};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Clip,
    Captions,
    Framing,
    Export,
}

pub struct Caps {
    pub style: ResolvedStyle,
    pub chunks: Vec<Chunk>,
    pub segments: Vec<(u64, u64)>,
}

#[derive(Clone, Default)]
pub struct ExportItem {
    pub state: String, // queued | running | done | error
    pub pct: f32,
    pub path: Option<String>,
    pub error: Option<String>,
}

pub struct Editor {
    pub dir: PathBuf,
    pub project: Project,
    pub words: Vec<Word>,
    pub sents: Vec<Sentence>,
    pub energy: Option<Energy>,
    pub faces: Option<FaceTrack>,
    pub sel: Option<String>,
    pub tab: Tab,
    pub dirty: bool,
    undo: Vec<String>,
    redo: Vec<String>,
    last_key: String,
    last_at: Instant,
    pub path: Option<CamPath>,
    pub caps: Option<Caps>,
    path_key: u64,
    caps_key: u64,
    pub view: (f64, f64),
    pub thumbs: HashMap<String, (String, TextureHandle)>,
    pub thumb_pending: HashSet<String>,
    pub sort_score: bool,
    pub looping: bool,
    pub auto_play: bool,
    pub caps_on: bool,
    pub guides: bool,
    pub export: HashMap<String, ExportItem>,
    pub exporting: bool,
    pub fresh: HashSet<String>,
    pub fresh_until: f64,
    pub face_progress: Option<f32>,
    pub tl_h: f32,
    pub clip_scroll: f32,
    pub panel_scroll: f32,
    pub edit_word: Option<(usize, String)>,
    pub editing_title: Option<String>,
    pub t: f64,
}

type Snap = (Vec<Clip>, openclip_core::captions::CaptionStyle, openclip_core::framing::Subject, f32, openclip_core::project::ExportSettings);

impl Editor {
    pub fn new(dir: PathBuf, project: Project, words: Vec<Word>, sents: Vec<Sentence>, energy: Option<Energy>, faces: Option<FaceTrack>) -> Self {
        Self {
            dir,
            project,
            words,
            sents,
            energy,
            faces,
            sel: None,
            tab: Tab::Clip,
            dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            last_key: String::new(),
            last_at: Instant::now(),
            path: None,
            caps: None,
            path_key: 0,
            caps_key: 0,
            view: (0.0, 60000.0),
            thumbs: HashMap::new(),
            thumb_pending: HashSet::new(),
            sort_score: false,
            looping: true,
            auto_play: true,
            caps_on: true,
            guides: false,
            export: HashMap::new(),
            exporting: false,
            fresh: HashSet::new(),
            fresh_until: 0.0,
            face_progress: None,
            tl_h: 300.0,
            clip_scroll: 0.0,
            panel_scroll: 0.0,
            edit_word: None,
            editing_title: None,
            t: 0.0,
        }
    }

    pub fn duration(&self) -> f64 {
        self.project.source.duration_ms as f64
    }
    pub fn clip(&self, id: &str) -> Option<&Clip> {
        self.project.clips.iter().find(|c| c.id == id)
    }
    pub fn sel_clip(&self) -> Option<&Clip> {
        self.sel.as_deref().and_then(|id| self.clip(id))
    }
    pub fn live(&self) -> Vec<&Clip> {
        self.project.clips.iter().filter(|c| !c.dismissed).collect()
    }
    pub fn ordered(&self) -> Vec<&Clip> {
        let mut l = self.live();
        if self.sort_score {
            l.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.start_ms.cmp(&b.start_ms)));
        } else {
            l.sort_by_key(|c| c.start_ms);
        }
        l
    }
    pub fn picked(&self) -> Vec<&Clip> {
        self.live().into_iter().filter(|c| c.picked).collect()
    }

    // ───────── words & sentences
    pub fn word_at(&self, t: f64) -> usize {
        self.words.partition_point(|w| (w.2 as f64) <= t)
    }
    pub fn sentence_at(&self, t: f64) -> usize {
        if self.sents.is_empty() {
            return 0;
        }
        self.sents.partition_point(|s| (s.t1 as f64) <= t).min(self.sents.len() - 1)
    }
    pub fn snap_start(&self, t: f64) -> f64 {
        if self.words.is_empty() {
            return t;
        }
        let i = self.word_at(t).min(self.words.len() - 1);
        let lo = i.saturating_sub(2);
        let hi = (i + 2).min(self.words.len() - 1);
        let best = (lo..=hi).min_by(|&a, &b| (self.words[a].1 as f64 - t).abs().total_cmp(&(self.words[b].1 as f64 - t).abs())).unwrap_or(i);
        let prev_end = if best > 0 { self.words[best - 1].2 as f64 } else { 0.0 };
        prev_end.max(self.words[best].1 as f64 - 120.0)
    }
    pub fn snap_end(&self, t: f64) -> f64 {
        if self.words.is_empty() {
            return t;
        }
        let i = self.word_at(t).min(self.words.len() - 1);
        let lo = i.saturating_sub(2);
        let hi = (i + 2).min(self.words.len() - 1);
        let best = (lo..=hi).min_by(|&a, &b| (self.words[a].2 as f64 - t).abs().total_cmp(&(self.words[b].2 as f64 - t).abs())).unwrap_or(i);
        let next = if best + 1 < self.words.len() { self.words[best + 1].1 as f64 } else { self.duration() };
        (next - 10.0).min(self.words[best].2 as f64 + 260.0)
    }

    // ───────── editing with undo
    fn snapshot(&self) -> String {
        let p = &self.project;
        let s: (&Vec<Clip>, _, _, f32, _) = (&p.clips, &p.style, &p.subject, p.smooth, &p.export);
        serde_json::to_string(&s).unwrap_or_default()
    }
    fn restore(&mut self, j: &str) {
        if let Ok(s) = serde_json::from_str::<Snap>(j) {
            let p = &mut self.project;
            p.clips = s.0;
            p.style = s.1;
            p.subject = s.2;
            p.smooth = s.3;
            p.export = s.4;
        }
    }
    /// Apply a change. Changes sharing `key` within 600 ms share one undo step (sliders, drags).
    pub fn mutate(&mut self, key: &str, f: impl FnOnce(&mut Project)) {
        let now = Instant::now();
        if !(!key.is_empty() && key == self.last_key && now.duration_since(self.last_at).as_millis() < 600) {
            let s = self.snapshot();
            self.undo.push(s);
            if self.undo.len() > 80 {
                self.undo.remove(0);
            }
            self.redo.clear();
        }
        self.last_key = key.to_string();
        self.last_at = now;
        f(&mut self.project);
        self.dirty = true;
    }
    pub fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            let cur = self.snapshot();
            self.redo.push(cur);
            self.restore(&s);
            self.dirty = true;
            self.force_refresh();
        }
    }
    pub fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            let cur = self.snapshot();
            self.undo.push(cur);
            self.restore(&s);
            self.dirty = true;
            self.force_refresh();
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    /// Edit a clip and flag it as user-touched so later Claude revisions don't overwrite it.
    pub fn edit_clip(&mut self, id: &str, key: &str, f: impl FnOnce(&mut Clip)) {
        let id = id.to_string();
        self.mutate(&format!("{key}-{id}"), |p| {
            if let Some(c) = p.clips.iter_mut().find(|c| c.id == id) {
                f(c);
                c.touched = true;
            }
        });
    }

    pub fn force_refresh(&mut self) {
        self.path_key = 0;
        self.caps_key = 0;
    }

    /// Recompute the camera path and captions when something they depend on changed.
    pub fn refresh(&mut self) {
        let Some(c) = self.sel_clip().cloned() else {
            self.path = None;
            self.caps = None;
            return;
        };
        let p = &self.project;
        let mut h = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&(c.start_ms, c.end_ms, &c.framing, p.subject, p.smooth.to_bits(), self.faces.is_some())).unwrap_or_default().hash(&mut h);
        let pk = h.finish() | 1;
        if pk != self.path_key {
            self.path_key = pk;
            self.path = Some(camera_path(self.faces.as_ref(), p.source.width, p.source.height, c.start_ms, c.end_ms, &c.framing, p.subject, p.smooth));
        }
        let mut h = std::collections::hash_map::DefaultHasher::new();
        let style = c.style.clone().unwrap_or_else(|| p.style.clone());
        serde_json::to_string(&(c.start_ms, c.end_ms, &c.edits, c.jump_cut, &style)).unwrap_or_default().hash(&mut h);
        let ck = h.finish() | 1;
        if ck != self.caps_key {
            self.caps_key = ck;
            let st = captions::resolve(&style);
            let chunks = captions::chunks(&self.words, c.start_ms, c.end_ms, st.words, &c.edits);
            let segments = if c.jump_cut { keep_segments(&self.words, self.energy.as_ref(), c.start_ms, c.end_ms) } else { vec![(c.start_ms, c.end_ms)] };
            self.caps = Some(Caps { style: st, chunks, segments });
        }
    }

    /// Length of the selected clip after jump cuts.
    pub fn eff_len(&self, c: &Clip) -> f64 {
        if c.jump_cut && Some(&c.id) == self.sel.as_ref() {
            if let Some(cp) = &self.caps {
                return cp.segments.iter().map(|g| (g.1 - g.0) as f64).sum();
            }
        }
        (c.end_ms - c.start_ms) as f64
    }

    /// Crop window (cx, cy, cw, ch) in source fractions at time `t` (ms).
    pub fn crop_at(&self, t: f64) -> (f64, f64, f64, f64) {
        match &self.path {
            Some(p) if !p.cx.is_empty() => {
                let f = ((t - p.t0 as f64) / p.step as f64).clamp(0.0, (p.cx.len() - 1) as f64);
                let i = f.floor() as usize;
                let j = (i + 1).min(p.cx.len() - 1);
                let u = f - i as f64;
                let lerp = |a: f32, b: f32| a as f64 * (1.0 - u) + b as f64 * u;
                (lerp(p.cx[i], p.cx[j]), lerp(p.cy[i], p.cy[j]), p.crop_w as f64, p.crop_h as f64)
            }
            _ => {
                let (w, h) = openclip_core::framing::crop_size(self.project.source.width, self.project.source.height, 1.0);
                (0.5, 0.5, w as f64, h as f64)
            }
        }
    }

    // ───────── clip operations
    pub fn trim_to(&mut self, id: &str, start: bool, ms: f64) {
        let dur = self.duration();
        self.edit_clip(id, "trim", |c| {
            if start {
                c.start_ms = ms.clamp(0.0, (c.end_ms as f64 - 2000.0).max(0.0)).round() as u64;
            } else {
                c.end_ms = ms.clamp(c.start_ms as f64 + 2000.0, dur).round() as u64;
            }
        });
    }
    pub fn step_sentence(&mut self, id: &str, start: bool, dir: i32) {
        if self.sents.is_empty() {
            return;
        }
        let Some(c) = self.clip(id).cloned() else { return };
        let n = self.sents.len() as i64;
        if start {
            let mut i = self.sentence_at(c.start_ms as f64 + 200.0) as i64;
            if self.sents[i as usize].t0 as f64 >= c.start_ms as f64 - 250.0 && dir < 0 {
                i -= 1;
            } else if dir > 0 {
                i += 1;
            }
            let i = i.clamp(0, n - 1) as usize;
            let prev = if self.sents[i].w0 > 0 { self.words[self.sents[i].w0 as usize - 1].2 as f64 } else { 0.0 };
            let t = prev.max(self.sents[i].t0 as f64 - 180.0);
            self.trim_to(id, true, t);
        } else {
            let mut i = self.sentence_at(c.end_ms as f64 - 200.0) as i64;
            if dir > 0 {
                i += 1;
            } else if self.sents[i as usize].t1 as f64 >= c.end_ms as f64 - 450.0 {
                i -= 1;
            }
            let i = i.clamp(0, n - 1) as usize;
            self.trim_to(id, false, self.sents[i].t1 as f64 + 380.0);
        }
    }
    pub fn toggle_pick(&mut self, id: &str) {
        self.edit_clip(id, "pick", |c| c.picked = !c.picked);
    }
    pub fn dismiss(&mut self, id: &str) -> Option<String> {
        let order: Vec<String> = self.ordered().iter().map(|c| c.id.clone()).collect();
        let idx = order.iter().position(|x| x == id).unwrap_or(0);
        let idc = id.to_string();
        self.mutate("", |p| {
            if let Some(c) = p.clips.iter_mut().find(|c| c.id == idc) {
                c.dismissed = true;
                c.picked = false;
            }
        });
        if self.sel.as_deref() == Some(id) {
            let rest: Vec<String> = self.ordered().iter().map(|c| c.id.clone()).collect();
            return rest.get(idx.min(rest.len().saturating_sub(1))).cloned();
        }
        self.sel.clone()
    }
    pub fn new_clip(&mut self, start: f64, end: f64) -> String {
        let c = blank_clip(start as u64, end as u64);
        let id = c.id.clone();
        self.mutate("", |p| p.clips.push(c));
        id
    }
    pub fn duplicate(&mut self, id: &str) -> Option<String> {
        let mut n = self.clip(id)?.clone();
        n.id = new_id("c");
        n.title = format!("{} (copy)", n.title);
        n.origin = Origin::Manual;
        n.exported = None;
        n.touched = true;
        let nid = n.id.clone();
        self.mutate("", |p| p.clips.push(n));
        Some(nid)
    }

    /// Merge suggested or locally picked clips in. Returns how many were new.
    pub fn merge_clips(&mut self, incoming: Vec<Clip>, clean: bool) -> usize {
        let was_clean = !self.dirty;
        let mut added = 0;
        let mut adds = Vec::new();
        let mut updates: Vec<Clip> = Vec::new();
        for n in incoming {
            match self.clip(&n.id) {
                None => adds.push(n),
                Some(cur) if cur.origin == Origin::Claude && !cur.touched && n.srev > cur.srev => updates.push(n),
                _ => {}
            }
        }
        if adds.is_empty() && updates.is_empty() {
            return 0;
        }
        for u in &updates {
            self.fresh.insert(u.id.clone());
        }
        for a in &adds {
            self.fresh.insert(a.id.clone());
        }
        self.mutate("merge", |p| {
            for u in updates {
                if let Some(c) = p.clips.iter_mut().find(|c| c.id == u.id) {
                    c.title = u.title;
                    c.why = u.why;
                    c.score = u.score;
                    c.start_ms = u.start_ms;
                    c.end_ms = u.end_ms;
                    c.srev = u.srev;
                }
            }
            for a in adds {
                p.clips.push(a);
                added += 1;
            }
        });
        // Claude's suggestions already live in suggestions.json, so merging them is not an unsaved change
        if clean && was_clean {
            self.dirty = false;
        }
        added
    }

    pub fn save(&mut self) {
        if store::save_project(&self.dir, &self.project).is_ok() {
            self.dirty = false;
        }
    }
}

pub fn blank_clip(start: u64, end: u64) -> Clip {
    Clip { id: new_id("c"), title: "New clip".into(), start_ms: start, end_ms: end, picked: true, touched: true, framing: Framing::default(), ..Clip::default() }
}
