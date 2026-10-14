//! Caption chunking and the look of each preset. The UI draws the resolved style with CSS and the
//! exporter with CoreText, so both read the same numbers.

use crate::transcript::Word;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct CaptionStyle {
    /// "pop" | "boxed" | "clean"
    pub preset: String,
    /// multiplier on the preset's font size
    pub size: f32,
    /// vertical centre, 0 (top) .. 1 (bottom)
    pub y: f32,
    /// words per caption line
    pub words: u8,
    /// highlight colour for the spoken word, `#rrggbb`
    pub accent: String,
    pub upper: Option<bool>,
}

impl Default for CaptionStyle {
    fn default() -> Self {
        Self { preset: "pop".into(), size: 1.0, y: 0.68, words: 3, accent: "#ffd60a".into(), upper: None }
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedStyle {
    pub font: String,
    pub bold: bool,
    /// font size as a fraction of the output width
    pub size: f32,
    pub fill: String,
    pub accent: String,
    pub stroke: String,
    /// stroke width as a fraction of the output width (0 = none)
    pub stroke_w: f32,
    pub shadow: bool,
    /// background box colour (with alpha as `#rrggbbaa`), if any
    pub bg: Option<String>,
    pub upper: bool,
    pub y: f32,
    pub words: u8,
    /// widest a line may be, as a fraction of the output width
    pub max_w: f32,
}

pub fn resolve(s: &CaptionStyle) -> ResolvedStyle {
    let k = s.size.clamp(0.5, 2.0);
    let (font, bold, size, fill, stroke, stroke_w, shadow, bg, upper) = match s.preset.as_str() {
        "boxed" => ("Helvetica Neue", true, 0.056, "#ffffff", "#000000", 0.0, false, Some("#000000b8".to_string()), false),
        "clean" => ("Helvetica Neue", true, 0.052, "#ffffff", "#000000", 0.0, true, None, false),
        _ => ("Arial Black", false, 0.07, "#ffffff", "#000000", 0.0125, false, None, true),
    };
    ResolvedStyle {
        font: font.into(),
        bold,
        size: size * k,
        fill: fill.into(),
        accent: s.accent.clone(),
        stroke: stroke.into(),
        stroke_w: stroke_w * k,
        shadow,
        bg,
        upper: s.upper.unwrap_or(upper),
        y: s.y.clamp(0.1, 0.95),
        words: s.words.clamp(1, 7),
        max_w: 0.84,
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ChunkWord(pub String, pub u32, pub u32);

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct Chunk {
    pub t0: u32,
    pub t1: u32,
    pub words: Vec<ChunkWord>,
}

/// Group the words of `[start_ms, end_ms)` into on-screen caption lines.
/// `edits` maps a transcript word index (as text) to replacement text; an empty string hides it.
pub fn chunks(words: &[Word], start_ms: u64, end_ms: u64, max_words: u8, edits: &BTreeMap<String, String>) -> Vec<Chunk> {
    let max_words = max_words.clamp(1, 7) as usize;
    let mut out: Vec<Chunk> = Vec::new();
    let mut cur: Vec<ChunkWord> = Vec::new();
    let mut chars = 0usize;
    let flush = |cur: &mut Vec<ChunkWord>, out: &mut Vec<Chunk>, chars: &mut usize| {
        if !cur.is_empty() {
            let t0 = cur[0].1;
            let t1 = cur[cur.len() - 1].2;
            out.push(Chunk { t0, t1, words: std::mem::take(cur) });
            *chars = 0;
        }
    };
    let mut last_end: Option<u32> = None;
    for (i, w) in words.iter().enumerate() {
        if (w.1 as u64) < start_ms {
            continue;
        }
        if w.1 as u64 >= end_ms {
            break;
        }
        let text = match edits.get(&i.to_string()) {
            Some(t) if t.is_empty() => continue,
            Some(t) => t.clone(),
            None => w.0.clone(),
        };
        let gap = last_end.map(|e| w.1.saturating_sub(e)).unwrap_or(0);
        if !cur.is_empty() && (gap > 450 || cur.len() >= max_words || chars + text.chars().count() > 24) {
            flush(&mut cur, &mut out, &mut chars);
        }
        chars += text.chars().count() + 1;
        let t1 = w.2.min(end_ms as u32);
        cur.push(ChunkWord(text.clone(), w.1, t1));
        last_end = Some(t1);
        let t = text.trim_end_matches(['"', '\'', ')', '”', '’']);
        let hard = t.ends_with('.') || t.ends_with('?') || t.ends_with('!');
        let soft = t.ends_with(',') || t.ends_with(';') || t.ends_with(':') || t.ends_with('—');
        if hard || (soft && cur.len() >= 2) {
            flush(&mut cur, &mut out, &mut chars);
        }
    }
    flush(&mut cur, &mut out, &mut chars);
    // hold each caption until the next one starts when the gap is short, so lines do not flicker
    for i in 0..out.len() {
        let next = out.get(i + 1).map(|c| c.t0);
        let hold = out[i].t1 + 320;
        out[i].t1 = match next {
            Some(n) if n.saturating_sub(out[i].t1) < 320 => n,
            _ => hold,
        };
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::tests::synth;

    #[test]
    fn breaks_on_punctuation_and_size() {
        let w = synth("This is the first line. And here is another one that goes on, and on for a while", 0);
        let c = chunks(&w, 0, 60_000, 3, &BTreeMap::new());
        assert!(c.iter().all(|c| c.words.len() <= 3));
        assert_eq!(c[0].words.last().unwrap().0, "the");
        // the sentence end forces a break even though the line is short
        assert!(c.iter().any(|c| c.words.last().unwrap().0 == "line."));
        assert!(c.len() >= 4);
    }

    #[test]
    fn edits_apply_and_hide() {
        let w = synth("one two three", 0);
        let mut e = BTreeMap::new();
        e.insert("1".to_string(), String::new());
        e.insert("2".to_string(), "THREE".to_string());
        let c = chunks(&w, 0, 10_000, 5, &e);
        let txt: Vec<_> = c[0].words.iter().map(|w| w.0.as_str()).collect();
        assert_eq!(txt, vec!["one", "THREE"]);
    }

    #[test]
    fn range_is_respected() {
        let w = synth("a b c d e f g h", 0);
        let mid = w[3].1 as u64;
        let c = chunks(&w, mid, w[6].1 as u64, 7, &BTreeMap::new());
        let n: usize = c.iter().map(|c| c.words.len()).sum();
        assert_eq!(n, 3);
    }
}
