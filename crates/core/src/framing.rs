//! Face tracks and the virtual camera that turns a 16:9 frame into a 9:16 crop.
//!
//! The camera behaves like a patient operator: it holds still while the subject stays inside a
//! dead zone, glides when they leave it, and cuts (instead of panning across the frame) when the
//! source itself cuts between cameras.

use crate::spring::{ease, smooth_damp};
use serde::{Deserialize, Serialize};

/// A detected face, normalised to the frame (0..1, origin top-left): centre x, centre y, width, height.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Face(pub f32, pub f32, pub f32, pub f32);

impl Face {
    fn area(&self) -> f32 {
        self.2 * self.3
    }
}

/// All faces seen at one instant (ms).
#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct FaceSample(pub u32, pub Vec<Face>);

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct FaceTrack {
    pub step_ms: u32,
    pub src_w: u32,
    pub src_h: u32,
    pub samples: Vec<FaceSample>,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Subject {
    #[default]
    Largest,
    Left,
    Right,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct Key {
    pub t: u64,
    pub x: f32,
}

/// Per-clip framing choices.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Framing {
    /// follow the detected face; otherwise hold `x`
    pub follow: bool,
    /// fixed centre (0..1 of frame width) used when not following
    pub x: f32,
    /// 1 = full frame height; higher punches in
    pub zoom: f32,
    /// manual nudge added to the path, in crop widths (-1..1)
    pub shift: f32,
    /// manual keyframes (source ms, centre x); when present they replace the automatic path
    pub keys: Vec<Key>,
}

impl Default for Framing {
    fn default() -> Self {
        Self { follow: true, x: 0.5, zoom: 1.0, shift: 0.0, keys: Vec::new() }
    }
}

pub const PATH_STEP_MS: u32 = 100;

/// Crop centre sampled every [`PATH_STEP_MS`] from `t0`; consumers interpolate linearly.
#[derive(Serialize, Clone, Debug, Default)]
#[serde(rename_all = "camelCase")]
pub struct CamPath {
    pub t0: u32,
    pub step: u32,
    pub cx: Vec<f32>,
    pub cy: Vec<f32>,
    pub zoom: f32,
    /// crop size as a fraction of the source frame
    pub crop_w: f32,
    pub crop_h: f32,
    /// source ms at which the subject was lost for good, if any (informational)
    pub faces_found: bool,
}

/// Crop size (fractions of the source frame) for a 9:16 window at `zoom`.
pub fn crop_size(src_w: u32, src_h: u32, zoom: f32) -> (f32, f32) {
    let a = src_w.max(1) as f32 / src_h.max(1) as f32;
    let zoom = zoom.clamp(1.0, 3.0);
    let mut h = 1.0 / zoom;
    let mut w = h * (9.0 / 16.0) / a;
    if w > 1.0 {
        w = 1.0;
        h = (w * a * 16.0 / 9.0).min(1.0);
    }
    (w, h)
}

fn pick(faces: &[Face], subject: Subject, prev: Option<(f32, f32)>) -> Option<Face> {
    let max_area = faces.iter().map(Face::area).fold(0.0f32, f32::max);
    if max_area <= 0.0 {
        return None;
    }
    // ignore the small faces in the background
    let cand: Vec<&Face> = faces.iter().filter(|f| f.area() >= max_area * 0.3).collect();
    match subject {
        Subject::Left => cand.into_iter().min_by(|a, b| a.0.total_cmp(&b.0)).copied(),
        Subject::Right => cand.into_iter().max_by(|a, b| a.0.total_cmp(&b.0)).copied(),
        Subject::Largest => {
            let biggest = cand.iter().copied().max_by(|a, b| a.area().total_cmp(&b.area())).copied()?;
            if let Some((px, py)) = prev {
                // stick with the person we were already following unless they are clearly smaller
                let near = cand
                    .iter()
                    .copied()
                    .filter(|f| (f.0 - px).abs() < 0.12 && (f.1 - py).abs() < 0.2 && f.area() >= biggest.area() * 0.6)
                    .min_by(|a, b| ((a.0 - px).abs()).total_cmp(&(b.0 - px).abs()));
                if let Some(f) = near {
                    return Some(*f);
                }
            }
            Some(biggest)
        }
    }
}

fn median3(v: &mut [f32]) {
    if v.len() < 3 {
        return;
    }
    let src = v.to_vec();
    for i in 1..src.len() - 1 {
        let (a, b, c) = (src[i - 1], src[i], src[i + 1]);
        v[i] = a.max(b).min(a.min(b).max(c));
    }
}

/// Subject position over time as `(t_ms, cx, cy, size)`, one entry per face sample in range.
fn subject_series(track: &FaceTrack, from: u32, to: u32, subject: Subject) -> Vec<(u32, f32, f32, f32)> {
    let mut out: Vec<(u32, f32, f32, f32)> = Vec::new();
    let mut prev: Option<(f32, f32)> = None;
    let lo = track.samples.partition_point(|s| s.0 < from);
    for s in &track.samples[lo..] {
        if s.0 > to {
            break;
        }
        if let Some(f) = pick(&s.1, subject, prev) {
            prev = Some((f.0, f.1));
            out.push((s.0, f.0, f.1, f.3));
        }
    }
    out
}

/// Build the crop-centre path for `[start_ms, end_ms]`.
pub fn camera_path(track: Option<&FaceTrack>, src_w: u32, src_h: u32, start_ms: u64, end_ms: u64, fr: &Framing, subject: Subject, smooth: f32) -> CamPath {
    let (cw, ch) = crop_size(src_w, src_h, fr.zoom);
    let step = PATH_STEP_MS;
    let t0 = start_ms as u32;
    let n = (((end_ms.saturating_sub(start_ms)) as u32) / step + 2) as usize;
    let mut path = CamPath { t0, step, cx: vec![0.5; n], cy: vec![0.5; n], zoom: fr.zoom.clamp(1.0, 3.0), crop_w: cw, crop_h: ch, faces_found: false };

    let half_w = cw / 2.0;
    let half_h = ch / 2.0;
    let clamp_x = |x: f32| if cw >= 1.0 { 0.5 } else { x.clamp(half_w, 1.0 - half_w) };
    let clamp_y = |y: f32| if ch >= 1.0 { 0.5 } else { y.clamp(half_h, 1.0 - half_h) };

    // 1) subject series (hold the last known position through gaps)
    let mut series: Vec<(u32, f32, f32, f32)> = Vec::new();
    if fr.follow {
        if let Some(tr) = track {
            // a little look-behind so the camera starts where the subject already was
            let from = t0.saturating_sub(2000);
            series = subject_series(tr, from, (end_ms as u32).saturating_add(500), subject);
        }
    }
    path.faces_found = !series.is_empty();

    if !series.is_empty() {
        let mut xs: Vec<f32> = series.iter().map(|s| s.1).collect();
        let mut ys: Vec<f32> = series.iter().map(|s| s.2).collect();
        median3(&mut xs);
        median3(&mut ys);

        // 2) simulate the operator
        let dead = half_w * 0.45; // keep the face within the middle ~45% of the crop
        let st = smooth.clamp(0.1, 2.0) as f64;
        let cut_jump = cw * 0.55;
        let mut idx = series.partition_point(|s| s.0 < t0).saturating_sub(1);
        let sample_at = |idx: usize, t: u32| -> (f32, f32, bool) {
            // linear interpolation between neighbouring subject samples, flagging cuts
            let a = idx.min(series.len() - 1);
            let b = (a + 1).min(series.len() - 1);
            if a == b || series[b].0 <= series[a].0 {
                return (xs[a], ys[a], false);
            }
            let f = ((t.saturating_sub(series[a].0)) as f32 / (series[b].0 - series[a].0) as f32).clamp(0.0, 1.0);
            let cut = (xs[b] - xs[a]).abs() > cut_jump;
            if cut {
                // no interpolation across a cut: jump at the later sample
                if f < 0.5 { (xs[a], ys[a], false) } else { (xs[b], ys[b], true) }
            } else {
                (xs[a] + (xs[b] - xs[a]) * f, ys[a] + (ys[b] - ys[a]) * f, false)
            }
        };
        let (sx0, sy0, _) = sample_at(idx, t0);
        let (mut camx, mut camy) = (clamp_x(sx0), clamp_y(sy0 + ch * 0.06));
        let (mut vx, mut vy) = (0.0f64, 0.0f64);
        for i in 0..n {
            let t = t0 + i as u32 * step;
            while idx + 1 < series.len() && series[idx + 1].0 <= t {
                idx += 1;
            }
            let (sx, sy, cut) = sample_at(idx, t);
            if cut {
                camx = clamp_x(sx);
                camy = clamp_y(sy + ch * 0.06);
                vx = 0.0;
                vy = 0.0;
            }
            // dead zone: only move as far as needed to bring the subject back inside it
            let tx = if (sx - camx).abs() <= dead { camx } else { sx - dead * (sx - camx).signum() };
            let ty_goal = sy + ch * 0.06; // eyes slightly above centre when zoomed in
            let ty = if (ty_goal - camy).abs() <= ch * 0.08 { camy } else { ty_goal };
            let dt = step as f64 / 1000.0;
            camx = clamp_x(smooth_damp(camx as f64, clamp_x(tx) as f64, &mut vx, st, dt) as f32);
            camy = clamp_y(smooth_damp(camy as f64, clamp_y(ty) as f64, &mut vy, st * 1.4, dt) as f32);
            path.cx[i] = camx;
            path.cy[i] = camy;
        }
    } else {
        let x = clamp_x(fr.x);
        path.cx.iter_mut().for_each(|v| *v = x);
        path.cy.iter_mut().for_each(|v| *v = 0.5);
    }

    // 3) manual keyframes replace the automatic horizontal path
    if !fr.keys.is_empty() {
        let mut keys = fr.keys.clone();
        keys.sort_by_key(|k| k.t);
        for i in 0..n {
            let t = (t0 + i as u32 * step) as u64;
            path.cx[i] = clamp_x(key_value(&keys, t));
        }
    }
    // 4) manual nudge
    if fr.shift != 0.0 {
        for v in path.cx.iter_mut() {
            *v = clamp_x(*v + fr.shift * cw);
        }
    }
    path
}

fn key_value(keys: &[Key], t: u64) -> f32 {
    if t <= keys[0].t {
        return keys[0].x;
    }
    let last = keys[keys.len() - 1];
    if t >= last.t {
        return last.x;
    }
    let i = keys.partition_point(|k| k.t <= t) - 1;
    let (a, b) = (keys[i], keys[i + 1]);
    let f = ease((t - a.t) as f64 / (b.t - a.t).max(1) as f64) as f32;
    a.x + (b.x - a.x) * f
}

/// Fraction of face samples in `[t0, t1]` that contain a usable face.
pub fn coverage(track: &FaceTrack, t0: u32, t1: u32) -> f32 {
    let lo = track.samples.partition_point(|s| s.0 < t0);
    let (mut n, mut hit) = (0u32, 0u32);
    for s in &track.samples[lo..] {
        if s.0 > t1 {
            break;
        }
        n += 1;
        if s.1.iter().any(|f| f.area() > 0.0004) {
            hit += 1;
        }
    }
    if n == 0 { 0.0 } else { hit as f32 / n as f32 }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(xs: &[(u32, f32)]) -> FaceTrack {
        FaceTrack {
            step_ms: 250,
            src_w: 1920,
            src_h: 1080,
            samples: xs.iter().map(|&(t, x)| FaceSample(t, vec![Face(x, 0.4, 0.1, 0.18)])).collect(),
        }
    }

    #[test]
    fn crop_is_nine_sixteen() {
        let (w, h) = crop_size(1920, 1080, 1.0);
        assert!((w - 0.3164).abs() < 1e-3);
        assert_eq!(h, 1.0);
        let (w, h) = crop_size(1080, 1920, 1.0);
        assert_eq!((w, h), (1.0, 1.0));
    }

    #[test]
    fn holds_inside_dead_zone_then_follows() {
        let mut v = Vec::new();
        for i in 0..40 {
            v.push((i * 250, if i < 10 { 0.5 } else { 0.5 + 0.002 * (i - 10) as f32 }));
        }
        for i in 40..80 {
            v.push((i * 250, 0.70));
        }
        let tr = track(&v);
        let p = camera_path(Some(&tr), 1920, 1080, 0, 19_000, &Framing::default(), Subject::Largest, 0.4);
        assert!((p.cx[0] - 0.5).abs() < 0.01);
        // tiny drift is ignored
        assert!((p.cx[90] - 0.5).abs() < 0.02, "{}", p.cx[90]);
        // after the subject moves right the camera ends up near them
        let end = *p.cx.last().unwrap();
        assert!(end > 0.6 && end <= 0.84, "{end}");
    }

    #[test]
    fn cuts_instead_of_panning() {
        let mut v = Vec::new();
        for i in 0..20 {
            v.push((i * 250, 0.25));
        }
        for i in 20..40 {
            v.push((i * 250, 0.75));
        }
        let tr = track(&v);
        let p = camera_path(Some(&tr), 1920, 1080, 0, 9_000, &Framing::default(), Subject::Largest, 0.5);
        // 5.0s is the cut; by 5.2s the camera must already be on the new subject
        let i = 52;
        assert!((p.cx[i] - 0.75).abs() < 0.12, "{}", p.cx[i]);
    }

    #[test]
    fn keyframes_win() {
        let fr = Framing { keys: vec![Key { t: 0, x: 0.3 }, Key { t: 1000, x: 0.6 }], ..Default::default() };
        let p = camera_path(None, 1920, 1080, 0, 2000, &fr, Subject::Largest, 0.4);
        assert!((p.cx[0] - 0.3).abs() < 1e-4);
        assert!((p.cx[15] - 0.6).abs() < 1e-4);
    }
}
