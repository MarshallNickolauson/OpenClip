// Canvas timeline: overview of the whole talk on top, detail of the selected clip below
// (waveform + words, caption blocks, framing path with keyframes).
import { S, clip, on, emit, editClip, snapStart, snapEnd, duration, live, wordAt, blankClip, mutate } from './state.js';
import { seek } from './player.js';
import { $, clamp, fmt } from './util.js';

const ov = $('#overview'), tl = $('#tl');
const LABEL_W = 62;
const css = getComputedStyle(document.documentElement);
const C = {
  line: '#1d2738', line2: '#2a374d', muted: '#8693aa', accent: '#3b82c4', accent2: '#6aaee8', clip: '#d9895e', text: '#e5eaf4',
  panel2: '#141c2a', bg: '#090d14', ok: '#3ddc97',
};
void css;
let view = { t0: 0, t1: 60000 };
let dirty = true;
const invalidate = () => { dirty = true; };
function loop() {
  if (dirty) { dirty = false; drawOverview(); drawDetail(); }
  requestAnimationFrame(loop);
}
requestAnimationFrame(loop);

function size(cv) {
  const r = cv.getBoundingClientRect();
  const d = window.devicePixelRatio || 1;
  const w = Math.max(1, Math.round(r.width * d)), hh = Math.max(1, Math.round(r.height * d));
  if (cv.width !== w || cv.height !== hh) { cv.width = w; cv.height = hh; }
  const ctx = cv.getContext('2d');
  ctx.setTransform(d, 0, 0, d, 0, 0);
  return { ctx, w: r.width, h: r.height };
}

// ───────── view helpers
export function fitView(pad = 0.12) {
  const c = clip();
  if (!c) return;
  const len = c.endMs - c.startMs;
  view = { t0: Math.max(0, c.startMs - len * pad - 1500), t1: Math.min(duration(), c.endMs + len * pad + 1500) };
  invalidate();
}
export function wideView() {
  const c = clip();
  if (!c) return;
  const mid = (c.startMs + c.endMs) / 2;
  view = { t0: Math.max(0, mid - 60000), t1: Math.min(duration(), mid + 60000) };
  invalidate();
}
export function zoomView(f, around) {
  const a = around ?? (view.t0 + view.t1) / 2;
  const span = clamp((view.t1 - view.t0) * f, 4000, Math.max(4000, duration()));
  const r = (a - view.t0) / (view.t1 - view.t0);
  view.t0 = clamp(a - span * r, 0, Math.max(0, duration() - span));
  view.t1 = view.t0 + span;
  invalidate();
}
const xOf = (t, w) => LABEL_W + ((t - view.t0) / (view.t1 - view.t0)) * (w - LABEL_W - 8);
const tOf = (x, w) => view.t0 + ((x - LABEL_W) / (w - LABEL_W - 8)) * (view.t1 - view.t0);

// ───────── overview
function drawOverview() {
  const { ctx, w, h } = size(ov);
  ctx.clearRect(0, 0, w, h);
  const dur = duration();
  if (!dur) return;
  ctx.fillStyle = C.panel2;
  ctx.fillRect(0, 4, w, h - 8);
  const e = S.energy;
  if (e && e.v.length) {
    ctx.fillStyle = '#33466a';
    const n = Math.floor(w);
    for (let x = 0; x < n; x++) {
      const a = Math.floor((x / n) * e.v.length), b = Math.max(a + 1, Math.floor(((x + 1) / n) * e.v.length));
      let m = 0;
      for (let i = a; i < b; i++) m = Math.max(m, e.v[i]);
      const hh = (m / 255) * (h - 12);
      ctx.fillRect(x, h / 2 - hh / 2, 1, Math.max(1, hh));
    }
  }
  // clips
  for (const c of live()) {
    const x0 = (c.startMs / dur) * w, x1 = Math.max(x0 + 3, (c.endMs / dur) * w);
    ctx.fillStyle = c.id === S.sel ? 'rgba(217,137,94,.95)' : c.picked ? 'rgba(106,174,232,.8)' : 'rgba(138,144,173,.55)';
    ctx.fillRect(x0, 2, x1 - x0, h - 4);
  }
  // detail window
  const vx0 = (view.t0 / dur) * w, vx1 = (view.t1 / dur) * w;
  ctx.strokeStyle = '#fff'; ctx.lineWidth = 1.5;
  ctx.strokeRect(vx0 + .5, 1.5, Math.max(4, vx1 - vx0) - 1, h - 3);
  // playhead
  const px = (S.t / dur) * w;
  ctx.fillStyle = C.accent2;
  ctx.fillRect(px - 1, 0, 2, h);
}

// ───────── detail
const TR = { ruler: 22, words: 66, caps: 26, frame: 50 };
function drawDetail() {
  const { ctx, w, h } = size(tl);
  ctx.clearRect(0, 0, w, h);
  const c = clip();
  if (!c) {
    ctx.fillStyle = C.muted; ctx.font = '12.5px system-ui'; ctx.textAlign = 'center';
    ctx.fillText('Select a clip to edit it on the timeline', w / 2, h / 2);
    return;
  }
  const trackW = w - LABEL_W - 8;
  const y0 = { ruler: 2, words: TR.ruler + 6, caps: TR.ruler + 6 + TR.words + 6, frame: TR.ruler + 6 + TR.words + 6 + TR.caps + 6 };
  // labels
  ctx.font = '10px system-ui'; ctx.textAlign = 'left'; ctx.fillStyle = C.muted;
  [['WORDS', y0.words + TR.words / 2], ['CAPTIONS', y0.caps + TR.caps / 2], ['FRAMING', y0.frame + TR.frame / 2]].forEach(([t, y]) => ctx.fillText(t, 2, y + 3));

  // ruler
  const span = view.t1 - view.t0;
  const steps = [250, 500, 1000, 2000, 5000, 10000, 15000, 30000, 60000, 120000, 300000];
  const step = steps.find((s) => (s / span) * trackW > 70) || 600000;
  ctx.strokeStyle = C.line2; ctx.fillStyle = C.muted; ctx.font = '10.5px system-ui'; ctx.textAlign = 'center';
  for (let t = Math.ceil(view.t0 / step) * step; t < view.t1; t += step) {
    const x = xOf(t, w);
    ctx.beginPath(); ctx.moveTo(x + .5, y0.ruler + 12); ctx.lineTo(x + .5, y0.ruler + 18); ctx.stroke();
    ctx.fillText(fmt(t, step < 1000), x, y0.ruler + 9);
  }

  // words track background + waveform
  const wy = y0.words, wh = TR.words;
  ctx.fillStyle = C.panel2;
  ctx.fillRect(LABEL_W, wy, trackW, wh);
  const e = S.energy;
  const xa = xOf(c.startMs, w), xb = xOf(c.endMs, w);
  if (e && e.v.length) {
    for (let x = LABEL_W; x < LABEL_W + trackW; x += 2) {
      const t = tOf(x, w);
      const i = clamp(Math.floor(t / e.stepMs), 0, e.v.length - 1);
      const hh = (e.v[i] / 255) * (wh - 14);
      ctx.fillStyle = x >= xa && x <= xb ? 'rgba(106,174,232,.55)' : 'rgba(110,116,150,.35)';
      ctx.fillRect(x, wy + wh / 2 - hh / 2, 1.5, Math.max(1, hh));
    }
  }
  // dim outside, highlight inside
  ctx.fillStyle = 'rgba(10,11,20,.55)';
  ctx.fillRect(LABEL_W, wy, Math.max(0, xa - LABEL_W), wh);
  ctx.fillRect(xb, wy, Math.max(0, LABEL_W + trackW - xb), wh);
  // words
  const pxPerMs = trackW / span;
  const w0 = Math.max(0, wordAt(view.t0) - 1);
  ctx.font = '11px system-ui'; ctx.textAlign = 'left';
  if (pxPerMs * 150 > 1.5) {
    for (let i = w0; i < S.words.length; i++) {
      const [txt, a, b] = S.words[i];
      if (a > view.t1) break;
      const x = xOf(a, w), ww = (b - a) * pxPerMs;
      const inside = a >= c.startMs - 1 && b <= c.endMs + 1;
      const hid = c.edits && c.edits[String(i)] === '';
      ctx.fillStyle = inside ? (hid ? 'rgba(217,137,94,.25)' : 'rgba(59,130,196,.35)') : 'rgba(60,66,100,.4)';
      ctx.fillRect(x + .5, wy + wh - 22, Math.max(1, ww - 1), 18);
      if (ww > 18) {
        ctx.save();
        ctx.beginPath(); ctx.rect(x + 2, wy, Math.max(0, ww - 3), wh); ctx.clip();
        ctx.fillStyle = inside ? '#f0f1ff' : '#7c82a5';
        ctx.fillText(txt, x + 3, wy + wh - 9);
        ctx.restore();
      }
    }
  }
  // silences that jump cuts will remove
  if (c.jumpCut && S.caps?.segments?.length > 1) {
    const sg = S.caps.segments;
    ctx.fillStyle = 'rgba(217,137,94,.35)';
    for (let k = 0; k + 1 < sg.length; k++) {
      const x0 = xOf(sg[k][1], w), x1 = xOf(sg[k + 1][0], w);
      ctx.fillRect(x0, wy, Math.max(1.5, x1 - x0), wh);
    }
  }
  // trim handles
  for (const [x, left] of [[xa, true], [xb, false]]) {
    ctx.fillStyle = C.clip;
    ctx.fillRect(x - (left ? 0 : 3), wy, 3, wh);
    ctx.beginPath();
    if (left) { ctx.moveTo(x, wy); ctx.lineTo(x + 9, wy); ctx.lineTo(x, wy + 11); } else { ctx.moveTo(x, wy); ctx.lineTo(x - 9, wy); ctx.lineTo(x, wy + 11); }
    ctx.fill();
  }

  // captions track
  ctx.fillStyle = C.panel2;
  ctx.fillRect(LABEL_W, y0.caps, trackW, TR.caps);
  if (S.caps && c.captions) {
    for (const ch of S.caps.chunks) {
      if (ch.t1 < view.t0 || ch.t0 > view.t1) continue;
      const x = xOf(ch.t0, w), ww = Math.max(2, xOf(ch.t1, w) - x - 1);
      ctx.fillStyle = 'rgba(61,220,151,.35)';
      ctx.fillRect(x, y0.caps + 3, ww, TR.caps - 6);
      if (ww > 30) {
        ctx.save(); ctx.beginPath(); ctx.rect(x, y0.caps, ww, TR.caps); ctx.clip();
        ctx.fillStyle = '#d8ffee'; ctx.font = '10.5px system-ui';
        ctx.fillText(ch.words.map((q) => q[0]).join(' '), x + 4, y0.caps + TR.caps / 2 + 4);
        ctx.restore();
      }
    }
  } else {
    ctx.fillStyle = C.muted; ctx.font = '11px system-ui'; ctx.fillText(c.captions ? '…' : 'Captions off for this clip', LABEL_W + 8, y0.caps + 17);
  }

  // framing track: crop centre across the source frame
  ctx.fillStyle = C.panel2;
  ctx.fillRect(LABEL_W, y0.frame, trackW, TR.frame);
  ctx.strokeStyle = C.line2; ctx.setLineDash([3, 4]);
  ctx.beginPath(); ctx.moveTo(LABEL_W, y0.frame + TR.frame / 2 + .5); ctx.lineTo(LABEL_W + trackW, y0.frame + TR.frame / 2 + .5); ctx.stroke();
  ctx.setLineDash([]);
  const p = S.path;
  const fy = (cx) => y0.frame + 5 + cx * (TR.frame - 10);
  if (p && p.cx.length) {
    ctx.strokeStyle = C.accent2; ctx.lineWidth = 2; ctx.beginPath();
    p.cx.forEach((cx, i) => {
      const x = xOf(p.t0 + i * p.step, w);
      i ? ctx.lineTo(x, fy(cx)) : ctx.moveTo(x, fy(cx));
    });
    ctx.stroke(); ctx.lineWidth = 1;
    // crop width band
    ctx.fillStyle = 'rgba(106,174,232,.12)';
    ctx.beginPath();
    p.cx.forEach((cx, i) => { const x = xOf(p.t0 + i * p.step, w); const yy = fy(cx - p.cropW / 2); i ? ctx.lineTo(x, yy) : ctx.moveTo(x, yy); });
    for (let i = p.cx.length - 1; i >= 0; i--) ctx.lineTo(xOf(p.t0 + i * p.step, w), fy(p.cx[i] + p.cropW / 2));
    ctx.fill();
  }
  for (const k of c.framing.keys || []) {
    const x = xOf(k.t, w), y = fy(k.x);
    ctx.fillStyle = C.clip; ctx.beginPath(); ctx.arc(x, y, 5, 0, 7); ctx.fill();
    ctx.strokeStyle = '#fff'; ctx.stroke();
  }

  // playhead
  const px = xOf(S.t, w);
  if (px >= LABEL_W && px <= w) {
    ctx.fillStyle = C.accent2;
    ctx.fillRect(px - 1, 0, 2, h);
    ctx.beginPath(); ctx.moveTo(px - 6, 0); ctx.lineTo(px + 6, 0); ctx.lineTo(px, 8); ctx.fill();
  }
  hit = { y0, w };
}
let hit = null;

// ───────── interaction: overview
let ovDrag = null;
ov.addEventListener('pointerdown', (e) => {
  const dur = duration();
  if (!dur) return;
  ov.setPointerCapture(e.pointerId);
  const w = ov.getBoundingClientRect().width;
  const t = (e.offsetX / w) * dur;
  // clicking a clip selects it; dragging empty space makes a new one
  const hitClip = live().find((c) => t >= c.startMs && t <= c.endMs);
  if (hitClip && !e.shiftKey) {
    ovDrag = { kind: 'select' };
    emit('selectclip', hitClip.id);
  } else {
    ovDrag = { kind: 'new', a: t, b: t };
  }
});
ov.addEventListener('pointermove', (e) => {
  if (!ovDrag || ovDrag.kind !== 'new') return;
  ovDrag.b = (e.offsetX / ov.getBoundingClientRect().width) * duration();
});
ov.addEventListener('pointerup', () => {
  const d = ovDrag; ovDrag = null;
  if (d?.kind === 'new') {
    const a = Math.min(d.a, d.b), b = Math.max(d.a, d.b);
    if (b - a >= 4000) {
      const c = blankClip(snapStart(a), snapEnd(b), { title: 'New clip' });
      mutate((p) => p.clips.push(c));
      emit('selectclip', c.id);
    } else {
      // a plain click pans the detail view there
      const mid = d.a;
      const span = view.t1 - view.t0;
      view.t0 = clamp(mid - span / 2, 0, Math.max(0, duration() - span));
      view.t1 = view.t0 + span;
      invalidate();
    }
  }
});

// ───────── interaction: detail
let drag = null;
function nearHandle(x, c, w) {
  const xa = xOf(c.startMs, w), xb = xOf(c.endMs, w);
  if (Math.abs(x - xa) < 8) return 'start';
  if (Math.abs(x - xb) < 8) return 'end';
  return null;
}
tl.addEventListener('pointerdown', (e) => {
  const c = clip();
  if (!c || !hit) return;
  tl.setPointerCapture(e.pointerId);
  const r = tl.getBoundingClientRect();
  const x = e.clientX - r.left, y = e.clientY - r.top, w = r.width;
  const { y0 } = hit;
  const hdl = y >= y0.words && y < y0.caps ? nearHandle(x, c, w) : null;
  if (hdl) { drag = { kind: hdl }; return; }
  if (y >= y0.frame) {
    // existing keyframe? drag it (alt = delete); otherwise add one on the current path
    const keys = c.framing.keys || [];
    const k = keys.findIndex((k) => Math.abs(xOf(k.t, w) - x) < 8);
    if (k >= 0) {
      if (e.altKey || e.detail === 2) { editClip(c.id, (cl) => cl.framing.keys.splice(k, 1)); emit('framechange'); return; }
      drag = { kind: 'key', i: k };
      return;
    }
    const t = clamp(tOf(x, w), c.startMs, c.endMs);
    const cx = clamp(((y - y0.frame - 5) / (TR.frame - 10)), 0.05, 0.95);
    editClip(c.id, (cl) => { cl.framing.keys = [...(cl.framing.keys || []), { t: Math.round(t), x: cx }].sort((a, b) => a.t - b.t); });
    drag = { kind: 'key', i: (clip().framing.keys || []).findIndex((k) => k.t === Math.round(t)) };
    emit('framechange');
    return;
  }
  drag = { kind: 'seek' };
  seek(tOf(x, w));
});
tl.addEventListener('pointermove', (e) => {
  const c = clip();
  if (!c || !hit) return;
  const r = tl.getBoundingClientRect();
  const x = e.clientX - r.left, y = e.clientY - r.top, w = r.width;
  if (!drag) {
    const hdl = y >= hit.y0.words && y < hit.y0.caps ? nearHandle(x, c, w) : null;
    tl.style.cursor = hdl ? 'ew-resize' : y >= hit.y0.frame ? 'crosshair' : 'default';
    return;
  }
  const t = tOf(x, w);
  if (drag.kind === 'seek') seek(t);
  else if (drag.kind === 'start') {
    const v = e.shiftKey ? t : snapStart(t);
    editClip(c.id, (cl) => { cl.startMs = Math.round(clamp(v, 0, cl.endMs - 2000)); }, 'trim-' + c.id);
    emit('trim');
  } else if (drag.kind === 'end') {
    const v = e.shiftKey ? t : snapEnd(t);
    editClip(c.id, (cl) => { cl.endMs = Math.round(clamp(v, cl.startMs + 2000, duration())); }, 'trim-' + c.id);
    emit('trim');
  } else if (drag.kind === 'key') {
    const cx = clamp((y - hit.y0.frame - 5) / (TR.frame - 10), 0.05, 0.95);
    const tt = clamp(Math.round(t), c.startMs, c.endMs);
    editClip(c.id, (cl) => {
      const k = cl.framing.keys[drag.i];
      if (k) { k.x = cx; k.t = tt; }
    }, 'key-' + c.id);
    emit('framechange');
  }
});
tl.addEventListener('pointerup', () => {
  if (drag && drag.kind === 'key') { const c = clip(); if (c) editClip(c.id, (cl) => cl.framing.keys.sort((a, b) => a.t - b.t)); }
  drag = null;
});
tl.addEventListener('wheel', (e) => {
  e.preventDefault();
  const r = tl.getBoundingClientRect();
  if (e.metaKey || e.ctrlKey) {
    zoomView(e.deltaY > 0 ? 1.15 : 1 / 1.15, tOf(e.clientX - r.left, r.width));
  } else {
    const span = view.t1 - view.t0;
    const dt = ((Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY) / r.width) * span * 0.8;
    view.t0 = clamp(view.t0 + dt, 0, Math.max(0, duration() - span));
    view.t1 = view.t0 + span;
    invalidate();
  }
}, { passive: false });

export function ensureVisible() {
  const c = clip();
  if (!c) return;
  if (c.startMs < view.t0 + 500 || c.endMs > view.t1 - 500 || view.t1 - view.t0 > (c.endMs - c.startMs) * 6 + 40000) fitView();
}
export const redraw = invalidate;
for (const n of ['tick', 'project', 'select', 'path', 'caps', 'framechange', 'trim']) on(n, invalidate);
new ResizeObserver(invalidate).observe(tl);
new ResizeObserver(invalidate).observe(ov);
$('#tlZoomIn').onclick = () => zoomView(1 / 1.5);
$('#tlZoomOut').onclick = () => zoomView(1.5);
$('#tlFit').onclick = () => fitView();
$('#tlWide').onclick = () => wideView();
on('select', () => fitView());
