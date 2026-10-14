// The 9:16 preview: one <video> of the source, panned/zoomed by the same camera path the exporter uses,
// plus a DOM caption layer. Playback is confined to the selected clip.
import { S, clip, emit, on } from './state.js';
import { invoke } from './api.js';
import { $, clamp } from './util.js';

export const video = $('#v');
const phone = $('#phone');
const capEl = $('#cap');
let boxW = 300, boxH = 533;
let srcUrl = '';
let capKey = '', capActive = -1;

export async function setSource(path) {
  const url = await invoke('media_url', { path });
  if (url === srcUrl) return;
  srcUrl = url;
  video.src = url;
  video.load();
  $('#vmsg').hidden = true;
}
function diag(why) {
  const m = $('#vmsg');
  m.hidden = false;
  m.innerHTML = `Preview unavailable (${why}).<br><span class="muted">readyState ${video.readyState} · network ${video.networkState} · error ${video.error ? video.error.code + ' ' + video.error.message : 'none'}</span>`;
}
video.addEventListener('loadeddata', () => { $('#vmsg').hidden = true; });
setInterval(() => { if (srcUrl && video.readyState < 2 && video.networkState !== 2 && S.project) diag('not loading'); }, 4000);
video.addEventListener('error', () => {
  const m = $('#vmsg');
  m.hidden = false;
  m.innerHTML = 'This file can\'t be previewed here (unsupported codec or the file moved).<br><span class="muted">Use an MP4 or MOV with H.264 / HEVC video.</span>';
});

// ───────── layout
export function layout() {
  const wrap = $('#stageWrap');
  const aw = wrap.clientWidth - 8, ah = wrap.clientHeight - 8;
  boxH = Math.max(120, Math.min(ah, aw * 16 / 9));
  boxW = boxH * 9 / 16;
  phone.style.width = boxW + 'px';
  phone.style.height = boxH + 'px';
  applyCam(S.t, true);
  capKey = ''; // restyle captions at the new size
  updateCaps(S.t);
}
new ResizeObserver(layout).observe($('#stageWrap'));

// ───────── camera
function camAt(t) {
  const p = S.path;
  if (!p || !p.cx.length) return { cx: 0.5, cy: 0.5, cw: 0.316, ch: 1 };
  const f = clamp((t - p.t0) / p.step, 0, p.cx.length - 1);
  const i = Math.floor(f), j = Math.min(p.cx.length - 1, i + 1), u = f - i;
  return { cx: p.cx[i] * (1 - u) + p.cx[j] * u, cy: p.cy[i] * (1 - u) + p.cy[j] * u, cw: p.cropW, ch: p.cropH };
}
let sizeKey = '';
export function applyCam(t) {
  const pr = S.project;
  if (!pr) return;
  const { width: sw, height: sh } = pr.source;
  if (!sw || !sh) return;
  const { cx, cy, ch } = camAt(t);
  const k = boxH / (sh * ch); // screen px per source px
  const vw = sw * k, vh = sh * k;
  const key = vw.toFixed(1) + 'x' + vh.toFixed(1);
  if (key !== sizeKey) {
    sizeKey = key;
    video.style.width = vw + 'px';
    video.style.height = vh + 'px';
  }
  const x = clamp(boxW / 2 - cx * vw, Math.min(0, boxW - vw), 0);
  const y = clamp(boxH / 2 - cy * vh, Math.min(0, boxH - vh), 0);
  video.style.transform = `translate3d(${x.toFixed(2)}px,${y.toFixed(2)}px,0)`;
}

// ───────── captions
function styleCaps(st) {
  const px = st.size * boxW;
  capEl.style.fontSize = px + 'px';
  capEl.style.top = st.y * boxH + 'px';
  capEl.style.fontFamily = `"${st.font}", system-ui, sans-serif`;
  capEl.style.fontWeight = st.bold ? '800' : '400';
  capEl.style.color = st.fill;
  capEl.style.padding = `0 ${((1 - st.maxW) / 2) * 100}%`;
  capEl.style.textTransform = st.upper ? 'uppercase' : 'none';
  const sw = st.strokeW * boxW;
  const shadows = [];
  if (sw > 0) {
    for (let a = 0; a < 16; a++) {
      const r = sw;
      shadows.push(`${(Math.cos((a / 16) * 6.2832) * r).toFixed(2)}px ${(Math.sin((a / 16) * 6.2832) * r).toFixed(2)}px 0 ${st.stroke}`);
    }
  }
  if (st.shadow) shadows.push(`0 ${px * 0.05}px ${px * 0.14}px rgba(0,0,0,.65)`);
  capEl.style.textShadow = shadows.join(',');
  capEl.dataset.bg = st.bg || '';
}
export function updateCaps(t) {
  const c = S.caps;
  if (!c || !S.capsOn || !clip()?.captions) { if (capKey !== 'off') { capEl.innerHTML = ''; capKey = 'off'; } return; }
  const chunks = c.chunks;
  let ci = -1;
  for (let i = 0; i < chunks.length; i++) if (t >= chunks[i].t0 && t < chunks[i].t1) { ci = i; break; }
  if (ci < 0) { if (capKey !== 'none') { capEl.innerHTML = ''; capKey = 'none'; } return; }
  const words = chunks[ci].words;
  let act = 0;
  for (let i = 0; i < words.length; i++) if (words[i][1] <= t) act = i;
  const key = `${ci}`;
  if (key !== capKey) {
    capKey = key; capActive = -1;
    styleCaps(c.style);
    capEl.innerHTML = '';
    const st = c.style;
    words.forEach((w, i) => {
      const s = document.createElement('span');
      s.className = 'w';
      s.textContent = w[0] + (i < words.length - 1 ? ' ' : '');
      if (st.bg) {
        s.style.background = st.bg;
        s.style.padding = `${st.size * boxW * 0.08}px ${st.size * boxW * 0.18}px`;
        s.style.borderRadius = st.size * boxW * 0.25 + 'px';
        s.style.webkitBoxDecorationBreak = 'clone';
      }
      capEl.append(s);
    });
  }
  if (act !== capActive) {
    capActive = act;
    const st = c.style;
    [...capEl.children].forEach((s, i) => (s.style.color = i === act ? st.accent : st.fill));
  }
}

// ───────── transport
export function seek(ms) {
  const c = clip();
  if (c) ms = clamp(ms, c.startMs, c.endMs);
  video.currentTime = ms / 1000;
  S.t = ms;
  applyCam(ms); updateCaps(ms);
  emit('tick');
}
export function play() {
  const c = clip();
  if (!c) return;
  if (video.currentTime * 1000 < c.startMs - 50 || video.currentTime * 1000 >= c.endMs - 60) video.currentTime = c.startMs / 1000;
  video.play().catch(() => {});
}
export function pause() { video.pause(); }
export function toggle() { video.paused ? play() : pause(); }

video.addEventListener('play', () => { S.playing = true; emit('play'); });
video.addEventListener('pause', () => { S.playing = false; emit('play'); });

let lastT = -1;
function tick() {
  const t = video.currentTime * 1000;
  const c = clip();
  if (S.playing && c) {
    // jump cuts: hop over the silences
    const segs = c.jumpCut && S.caps?.segments;
    let hop = null;
    if (segs && segs.length > 1 && t >= c.startMs && t < c.endMs) {
      if (!segs.some((g) => t >= g[0] - 15 && t < g[1])) {
        const nx = segs.find((g) => g[0] > t);
        hop = nx ? nx[0] : c.endMs;
      }
    }
    if (hop !== null && hop < c.endMs - 30) {
      video.currentTime = hop / 1000;
    } else if (t >= c.endMs - 30 || hop !== null) {
      if (S.loop) { video.currentTime = c.startMs / 1000; }
      else { video.pause(); video.currentTime = c.endMs / 1000; }
    } else if (t < c.startMs - 400) {
      video.currentTime = c.startMs / 1000;
    }
  }
  if (Math.abs(t - lastT) > 0.5 || S.playing) {
    lastT = t;
    S.t = t;
    applyCam(t);
    updateCaps(t);
    emit('tick');
  }
  requestAnimationFrame(tick);
}
requestAnimationFrame(tick);

// ───────── path / captions refresh
let pathSeq = 0;
export async function refreshPath() {
  const c = clip();
  if (!c || !S.dir) { S.path = null; return; }
  const seq = ++pathSeq;
  try {
    const p = await invoke('clip_path', { dir: S.dir, clip: c, subject: S.project.subject, smooth: S.project.smooth });
    if (seq !== pathSeq) return;
    S.path = p;
    applyCam(S.t);
    emit('path');
  } catch (e) { console.error(e); }
}
let capSeq = 0;
export async function refreshCaps() {
  const c = clip();
  if (!c || !S.dir) { S.caps = null; return; }
  const seq = ++capSeq;
  try {
    const r = await invoke('clip_captions', { dir: S.dir, startMs: c.startMs, endMs: c.endMs, style: c.style || S.project.style, edits: c.edits || {}, jumpCut: !!c.jumpCut });
    if (seq !== capSeq) return;
    S.caps = r;
    capKey = '';
    updateCaps(S.t);
    emit('caps');
  } catch (e) { console.error(e); }
}

// ───────── drag on the preview = manual framing keyframe
let drag = null;
phone.addEventListener('pointerdown', (e) => {
  const c = clip();
  if (!c) return;
  phone.setPointerCapture(e.pointerId);
  const { cx } = camAt(S.t);
  drag = { x: e.clientX, cx, moved: false };
});
phone.addEventListener('pointermove', (e) => {
  if (!drag) return;
  const c = clip();
  if (!c || !S.project) return;
  const dx = e.clientX - drag.x;
  if (Math.abs(dx) < 3 && !drag.moved) return;
  drag.moved = true;
  const { cw } = camAt(S.t);
  // dragging the picture right moves the crop left
  const nx = clamp(drag.cx - (dx / boxW) * cw, cw / 2, 1 - cw / 2);
  emit('framekey', { t: Math.round(S.t), x: nx });
});
phone.addEventListener('pointerup', () => { drag = null; });
window.addEventListener('resize', layout);
on('select', () => { sizeKey = ''; });
