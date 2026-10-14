//! Word-level transcript, sentence segmentation and the compact text form handed to Claude.

use serde::{Deserialize, Serialize};

/// One spoken word: text (punctuation attached), start ms, end ms. Serialises as `["word",t0,t1]`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Word(pub String, pub u32, pub u32);

impl Word {
    pub fn text(&self) -> &str {
        &self.0
    }
    pub fn t0(&self) -> u32 {
        self.1
    }
    pub fn t1(&self) -> u32 {
        self.2
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Transcript {
    pub model: String,
    pub words: Vec<Word>,
}

/// A run of words ending in sentence punctuation or a long pause.
#[derive(Serialize, Clone, Copy, Debug, PartialEq)]
pub struct Sentence {
    /// first word index
    pub w0: u32,
    /// last word index (inclusive)
    pub w1: u32,
    pub t0: u32,
    pub t1: u32,
}

/// Make raw recogniser output safe to work with: trimmed text, no empty words, monotone times.
pub fn clean_words(raw: Vec<Word>) -> Vec<Word> {
    let mut out: Vec<Word> = Vec::with_capacity(raw.len());
    for Word(text, mut t0, mut t1) in raw {
        let text = text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        if let Some(prev) = out.last_mut() {
            if t0 < prev.1 {
                t0 = prev.1;
            }
            if t0 < prev.2 {
                // overlap: trim the previous word, unless that would erase it
                if t0 >= prev.1 + 40 {
                    prev.2 = t0;
                } else {
                    t0 = prev.2;
                }
            }
        }
        t1 = t1.max(t0 + 40);
        out.push(Word(text, t0, t1));
    }
    out
}

const ABBREV: &[&str] = &["mr.", "mrs.", "ms.", "dr.", "st.", "vs.", "jr.", "sr.", "e.g.", "i.e.", "etc.", "no.", "u.s."];

fn ends_sentence(w: &str) -> bool {
    let t = w.trim_end_matches(['"', '\'', ')', ']', '”', '’']);
    if !(t.ends_with('.') || t.ends_with('?') || t.ends_with('!') || t.ends_with('…')) {
        return false;
    }
    let low = t.to_lowercase();
    !ABBREV.contains(&low.as_str())
}

/// Split into sentences at terminal punctuation, long pauses, or (as a last resort) very long runs.
pub fn sentences(words: &[Word]) -> Vec<Sentence> {
    let mut out = Vec::new();
    if words.is_empty() {
        return out;
    }
    let mut start = 0usize;
    for i in 0..words.len() {
        let last = i + 1 == words.len();
        let gap = if last { 0 } else { words[i + 1].1.saturating_sub(words[i].2) };
        let len = i + 1 - start;
        let brk = last
            || ends_sentence(&words[i].0)
            || gap > 1100
            || (len >= 40 && (words[i].0.ends_with(',') || gap > 450))
            || len >= 60;
        if brk {
            out.push(Sentence { w0: start as u32, w1: i as u32, t0: words[start].1, t1: words[i].2 });
            start = i + 1;
        }
    }
    out
}

pub fn join(words: &[Word]) -> String {
    let mut s = String::new();
    for (i, w) in words.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&w.0);
    }
    s
}

/// `m:ss`, or `h:mm:ss` past an hour.
pub fn fmt_ts(ms: u64) -> String {
    let s = ms / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, (s / 60) % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// Silences longer than this are cut when jump cuts are on (ms).
pub const CUT_GAP_MS: u32 = 600;
/// Air kept after a phrase ends (ms).
pub const CUT_TAIL_MS: u32 = 140;
/// Air kept before speech resumes (ms). Larger: word onsets are soft and must never be clipped.
pub const CUT_LEAD_MS: u32 = 260;

/// The parts of `[start, end)` to keep when cutting silences. Pauses are found in the loudness
/// envelope (recogniser word times stretch across silence, so they cannot be trusted for this):
/// quiet runs longer than [`CUT_GAP_MS`] are removed, leaving [`CUT_TAIL_MS`] / [`CUT_LEAD_MS`] either side, and leading /
/// trailing pauses are trimmed. Without an envelope, word-to-word gaps are used instead.
pub fn keep_segments(words: &[Word], energy: Option<&crate::store::Energy>, start: u64, end: u64) -> Vec<(u64, u64)> {
    if end <= start {
        return vec![(start, end)];
    }
    let (tail, lead) = (CUT_TAIL_MS as u64, CUT_LEAD_MS as u64);
    let gap = CUT_GAP_MS as u64;
    // quiet runs as (from_ms, to_ms)
    let mut quiet: Vec<(u64, u64)> = Vec::new();
    match energy {
        Some(e) if !e.v.is_empty() => {
            let mut sorted = e.v.clone();
            sorted.sort_unstable();
            let thr = (sorted[sorted.len() * 9 / 10] as f64 * 0.4) as u8;
            let step = e.step_ms.max(1) as u64;
            let (a, b) = ((start / step) as usize, ((end / step) as usize).min(e.v.len()));
            let mut run: Option<usize> = None;
            for i in a..b {
                let q = e.v[i] < thr;
                match (q, run) {
                    (true, None) => run = Some(i),
                    (false, Some(r)) => {
                        quiet.push((r as u64 * step, i as u64 * step));
                        run = None;
                    }
                    _ => {}
                }
            }
            if let Some(r) = run {
                quiet.push((r as u64 * step, b as u64 * step));
            }
        }
        _ => {
            let inside: Vec<&Word> = words.iter().filter(|w| w.1 as u64 >= start && (w.1 as u64) < end).collect();
            if let Some(f) = inside.first() {
                quiet.push((start, f.1 as u64));
            }
            for p in inside.windows(2) {
                quiet.push((p[0].2 as u64, p[1].1 as u64));
            }
            if let Some(l) = inside.last() {
                quiet.push((l.2 as u64, end));
            }
        }
    }
    let mut out = Vec::new();
    let mut seg_start = start;
    for (qa, qb) in quiet {
        let qa = qa.max(start);
        let qb = qb.min(end);
        if qb <= qa || qb - qa < gap {
            continue;
        }
        // keep a little air next to speech; at the very ends trim right up to the pad
        let cut_a = if qa <= start { start } else { qa + tail };
        let cut_b = if qb >= end { end } else { qb.saturating_sub(lead) };
        if cut_b <= cut_a + 120 {
            continue;
        }
        if cut_a > seg_start {
            out.push((seg_start, cut_a));
        }
        seg_start = cut_b;
    }
    if end > seg_start {
        out.push((seg_start, end));
    }
    if out.is_empty() {
        out.push((start, end));
    }
    out
}

/// Index of the first word whose end is after `t`.
pub fn word_at(words: &[Word], t: u32) -> usize {
    words.partition_point(|w| w.2 <= t)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn synth(text: &str, start_ms: u32) -> Vec<Word> {
        let mut t = start_ms;
        text.split_whitespace()
            .map(|w| {
                let dur = 260 + (w.len() as u32 * 14);
                let word = Word(w.to_string(), t, t + dur);
                t += dur + 40;
                word
            })
            .collect()
    }

    #[test]
    fn splits_on_punctuation_and_keeps_abbreviations() {
        let w = synth("Hello there Dr. Smith. How are you? Fine thanks.", 0);
        let s = sentences(&w);
        assert_eq!(s.len(), 3);
        assert_eq!(s[0].w1, 3);
        assert_eq!(s[1].w0, 4);
    }

    #[test]
    fn cleans_overlaps() {
        let raw = vec![Word(" a".into(), 0, 500), Word("b".into(), 300, 400), Word("".into(), 0, 0)];
        let c = clean_words(raw);
        assert_eq!(c.len(), 2);
        assert!(c[1].1 >= c[0].2);
    }

    #[test]
    fn cuts_long_silences_only() {
        let mut w = synth("one two three", 0);
        let gap_start = w[2].2 + 1500;
        w.extend(synth("four five", gap_start));
        let seg = keep_segments(&w, None, 0, 20_000);
        assert_eq!(seg.len(), 2, "{seg:?}");
        assert!(seg[0].1 < seg[1].0);
        let short = keep_segments(&synth("a b c d", 0), None, 0, 20_000);
        assert_eq!(short.len(), 1);
    }

    #[test]
    fn cuts_from_the_loudness_envelope() {
        // 0-2 s speech, 2-3.5 s silence, 3.5-6 s speech (100 ms bins)
        let v: Vec<u8> = (0..60).map(|i| if (20..35).contains(&i) { 0 } else { 200 }).collect();
        let e = crate::store::Energy { step_ms: 100, v };
        let seg = keep_segments(&[], Some(&e), 0, 6000);
        assert_eq!(seg.len(), 2, "{seg:?}");
        assert!(seg[0].1 >= 2000 && seg[1].0 <= 3500);
        let cut: u64 = seg[1].0 - seg[0].1;
        assert!(cut > 1000, "{cut}");
    }

    #[test]
    fn timestamps() {
        assert_eq!(fmt_ts(65_000), "1:05");
        assert_eq!(fmt_ts(3_725_000), "1:02:05");
    }
}
