//! Local pre-ranking of "moments": self-contained, sentence-aligned windows of a talk that are
//! likely to work as a short clip. This is what lets Claude pick clips from a few thousand tokens
//! instead of reading the whole transcript.

use crate::framing::{coverage, FaceTrack};
use crate::store::Energy;
use crate::transcript::{Sentence, Word};

#[derive(Clone, Debug)]
pub struct Candidate {
    pub s0: u32,
    pub s1: u32,
    pub t0: u32,
    pub t1: u32,
    pub score: f32,
    pub words: u32,
}

const FILLERS: &[&str] = &["um", "uh", "umm", "uhh", "erm", "hmm", "mm"];
const CONNECT: &[&str] = &["and", "but", "so", "because", "which", "then", "or", "also", "anyway", "though", "yet", "plus", "like", "well", "okay", "ok", "right", "yeah", "amen", "alright"];
const VAGUE: &[&str] = &["he", "she", "it", "they", "them", "him", "her", "this", "that", "those", "these", "there"];
const EMPHASIS: &[&str] = &[
    "never", "always", "every", "everything", "nothing", "nobody", "everyone", "only", "must", "truth", "real", "life", "love", "hope",
    "fear", "faith", "grace", "free", "choose", "change", "enough", "forever", "power", "purpose", "mistake", "secret", "most", "why",
];

fn norm(w: &str) -> String {
    w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase()
}

struct Prefix {
    words: Vec<u32>,
    filler: Vec<u32>,
    you: Vec<u32>,
    emph: Vec<u32>,
    anaphora: Vec<u32>,
    excl: Vec<u32>,
}

fn prefix(vals: impl Iterator<Item = u32>) -> Vec<u32> {
    let mut v = vec![0u32];
    for x in vals {
        v.push(v[v.len() - 1] + x);
    }
    v
}

pub struct Ctx<'a> {
    pub words: &'a [Word],
    pub sents: &'a [Sentence],
    pub energy: Option<&'a Energy>,
    pub faces: Option<&'a FaceTrack>,
}

/// Rank windows of whole sentences and return the best `n`, non-overlapping, best first.
pub fn candidates(c: &Ctx, n: usize, min_ms: u32, max_ms: u32) -> Vec<Candidate> {
    let s = c.sents;
    if s.is_empty() {
        return Vec::new();
    }
    // ---- per-sentence features
    let mut firsts: Vec<String> = Vec::with_capacity(s.len());
    let mut nw = Vec::new();
    let mut filler = Vec::new();
    let mut you = Vec::new();
    let mut emph = Vec::new();
    let mut excl = Vec::new();
    for sn in s {
        let ws = &c.words[sn.w0 as usize..=sn.w1 as usize];
        let toks: Vec<String> = ws.iter().map(|w| norm(&w.0)).collect();
        firsts.push(toks.first().cloned().unwrap_or_default());
        nw.push(ws.len() as u32);
        filler.push(toks.iter().filter(|t| FILLERS.contains(&t.as_str())).count() as u32);
        you.push(toks.iter().filter(|t| matches!(t.as_str(), "you" | "your" | "you're" | "yourself" | "we" | "our" | "let's")).count() as u32);
        emph.push(toks.iter().filter(|t| EMPHASIS.contains(&t.as_str())).count() as u32);
        let last = ws.last().map(|w| w.0.as_str()).unwrap_or("");
        excl.push(u32::from(last.ends_with('?') || last.ends_with('!')));
    }
    let mut ana = vec![0u32; s.len()];
    for i in 1..s.len() {
        if firsts[i].len() > 1 && firsts[i] == firsts[i - 1] {
            ana[i] = 1;
            ana[i - 1] = 1;
        }
    }
    let pf = Prefix {
        words: prefix(nw.iter().copied()),
        filler: prefix(filler.iter().copied()),
        you: prefix(you.iter().copied()),
        emph: prefix(emph.iter().copied()),
        anaphora: prefix(ana.iter().copied()),
        excl: prefix(excl.iter().copied()),
    };

    // ---- trigram hashes: repeated phrases are the fingerprint of sung lyrics and chanted refrains
    let toks: Vec<u64> = c
        .words
        .iter()
        .map(|w| {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            norm(&w.0).hash(&mut h);
            h.finish()
        })
        .collect();
    let tri: Vec<u64> = (0..toks.len().saturating_sub(2))
        .map(|i| toks[i].wrapping_mul(31).wrapping_add(toks[i + 1]).wrapping_mul(31).wrapping_add(toks[i + 2]))
        .collect();
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();

    // ---- loudness prefix sums (sum, sum of squares, count of "pause" frames)
    // Speech is full of tiny pauses; music and singing are continuous. That is the strongest cheap
    // cue for dropping worship songs and background-music stretches.
    let (en_step, en_pre, en_pre2, en_low) = match c.energy {
        Some(e) if !e.v.is_empty() => {
            let mut sorted: Vec<u8> = e.v.clone();
            sorted.sort_unstable();
            let p90 = sorted[sorted.len() * 9 / 10] as f64;
            let thr = (p90 * 0.62) as u8;
            let mut p = vec![0.0f64];
            let mut p2 = vec![0.0f64];
            let mut lo = vec![0u32];
            for &v in &e.v {
                let x = v as f64;
                p.push(p[p.len() - 1] + x);
                p2.push(p2[p2.len() - 1] + x * x);
                lo.push(lo[lo.len() - 1] + u32::from(v < thr));
            }
            (e.step_ms.max(1), p, p2, lo)
        }
        _ => (100, vec![0.0], vec![0.0], vec![0u32]),
    };
    let have_energy = en_pre.len() > 2;

    // ---- enumerate windows
    let mut cands: Vec<Candidate> = Vec::new();
    for i in 0..s.len() {
        for j in i..s.len() {
            let dur = s[j].t1.saturating_sub(s[i].t0);
            if dur > max_ms {
                break;
            }
            if dur < min_ms {
                continue;
            }
            let nsent = (j - i + 1) as f32;
            let words = (pf.words[j + 1] - pf.words[i]) as f32;
            let secs = dur as f32 / 1000.0;
            let mut sc = 48.0f32;

            // length: 30-60 s is the sweet spot
            let d = dur as f32 / 1000.0;
            sc += if (30.0..=60.0).contains(&d) { 12.0 } else if (22.0..30.0).contains(&d) || (60.0..=75.0).contains(&d) { 5.0 } else { 0.0 };

            // opening: should stand on its own
            let f = firsts[i].as_str();
            if CONNECT.contains(&f) {
                sc -= 14.0;
            } else if VAGUE.contains(&f) {
                sc -= 5.0;
            } else {
                sc += 6.0;
            }
            if nw[i] <= 3 {
                sc -= 10.0;
            }
            if excl[i] == 1 && nw[i] <= 16 {
                sc += 7.0; // opens on a question
            }
            if nw[i] <= 14 && nw[i] >= 4 {
                sc += 3.0; // punchy first line
            }
            // closing: not a stub
            if nw[j] <= 2 {
                sc -= 6.0;
            }

            // pace: dead air and rambling both hurt
            let wps = words / secs;
            sc += if (2.0..=3.4).contains(&wps) { 6.0 } else if wps < 1.5 { -12.0 } else { 0.0 };

            let fill = (pf.filler[j + 1] - pf.filler[i]) as f32 / words.max(1.0);
            sc -= (fill * 220.0).min(16.0);

            let yo = (pf.you[j + 1] - pf.you[i]) as f32 / words.max(1.0);
            sc += (yo * 90.0).min(10.0);
            let em = (pf.emph[j + 1] - pf.emph[i]) as f32 / words.max(1.0);
            sc += (em * 160.0).min(10.0);
            let an = (pf.anaphora[j + 1] - pf.anaphora[i]) as f32;
            sc += (an * 3.0).min(8.0);
            let ex = (pf.excl[j + 1] - pf.excl[i]) as f32 / nsent;
            sc += (ex * 10.0).min(5.0);

            // delivery: dynamic (varied) speech with pauses; continuous sound is music
            if have_energy {
                let a = (s[i].t0 / en_step) as usize;
                let b = ((s[j].t1 / en_step) as usize).min(en_pre.len() - 1);
                if b > a + 5 {
                    let n = (b - a) as f64;
                    let mean = (en_pre[b] - en_pre[a]) / n;
                    let var = ((en_pre2[b] - en_pre2[a]) / n - mean * mean).max(0.0);
                    sc += ((var.sqrt() / 70.0).clamp(0.0, 1.0) * 6.0) as f32;
                    let low = (en_low[b] - en_low[a]) as f32 / n as f32;
                    if low < 0.14 {
                        sc -= (0.14 - low) / 0.14 * 45.0;
                    }
                }
            }
            // sung or chanted lines repeat themselves; a talk rarely does
            if sc > 52.0 {
                let (a, b) = (s[i].w0 as usize, (s[j].w1 as usize + 1).min(tri.len()));
                if b > a + 6 {
                    seen.clear();
                    let mut rep = 0u32;
                    for h in &tri[a..b] {
                        if !seen.insert(*h) {
                            rep += 1;
                        }
                    }
                    let r = rep as f32 / (b - a) as f32;
                    if r > 0.1 {
                        sc -= ((r - 0.1) * 160.0).min(60.0);
                    }
                }
            }
            // a vertical clip needs someone on screen
            if let Some(ft) = c.faces {
                let cov = coverage(ft, s[i].t0, s[j].t1);
                if cov < 0.5 {
                    sc -= 30.0;
                } else if cov < 0.8 {
                    sc -= 8.0;
                }
            }
            cands.push(Candidate { s0: i as u32, s1: j as u32, t0: s[i].t0, t1: s[j].t1, score: sc.max(0.0), words: words as u32 });
        }
    }

    // scores are relative to this talk: the best moment lands near 100, never above
    let top = cands.iter().map(|c| c.score).fold(0.0f32, f32::max);
    if top > 100.0 {
        for c in cands.iter_mut() {
            c.score = c.score / top * 100.0;
        }
    }
    // ---- best first, drop anything overlapping a better pick
    cands.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.t0.cmp(&b.t0)));
    let mut out: Vec<Candidate> = Vec::new();
    for c in cands {
        if out.len() >= n {
            break;
        }
        let clash = out.iter().any(|o| {
            let lo = o.t0.max(c.t0);
            let hi = o.t1.min(c.t1);
            hi > lo && (hi - lo) as f32 > 0.15 * (c.t1 - c.t0).min(o.t1 - o.t0) as f32
        });
        if !clash {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcript::{sentences, tests::synth};

    #[test]
    fn prefers_standalone_punchy_windows() {
        let mut text = String::new();
        // filler-heavy, vague, slow ramble
        for _ in 0..8 {
            text.push_str("And um so yeah that is kind of the thing uh we were doing there. ");
        }
        // strong direct address
        text.push_str("Why do you keep waiting for permission? You were never meant to live small. ");
        text.push_str("You were made for more than comfort. Everything changes the moment you choose. ");
        text.push_str("Choose today. Choose truth. Choose love. Nothing else will ever be enough for you. ");
        for _ in 0..8 {
            text.push_str("And um so yeah that is kind of the thing uh we were doing there. ");
        }
        let w = synth(&text, 0);
        let s = sentences(&w);
        let c = candidates(&Ctx { words: &w, sents: &s, energy: None, faces: None }, 3, 10_000, 60_000);
        assert!(!c.is_empty());
        let best = &c[0];
        let t = crate::transcript::join(&w[s[best.s0 as usize].w0 as usize..=s[best.s1 as usize].w1 as usize]);
        assert!(t.contains("Why do you keep waiting"), "{t}");
    }
}
