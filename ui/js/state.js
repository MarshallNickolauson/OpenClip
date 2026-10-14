// Single source of truth for the open project, with undo/redo, autosave and Claude-suggestion merging.
import { invoke } from './api.js';
import { clamp } from './util.js';

export const S = {
  dir: null,
  project: null,
  words: [], // [text, t0, t1]
  sents: [], // {w0,w1,t0,t1}
  energy: null, // {stepMs, v}
  sel: null, // selected clip id
  t: 0, // playhead, source ms
  playing: false,
  loop: true,
  auto: true,
  capsOn: true,
  guides: false,
  sort: 'time',
  tab: 'clip',
  path: null, // camera path of the selected clip
  caps: null, // {style, chunks} for the selected clip
  thumbs: {}, // clip id -> {key, url}
  fresh: new Set(),
  faceProgress: null,
  export: {}, // clip id -> {state, pct, path, error}
  exporting: false,
  dirty: false,
};

const bus = new EventTarget();
export const on = (n, f) => bus.addEventListener(n, (e) => f(e.detail));
export const emit = (n, d) => bus.dispatchEvent(new CustomEvent(n, { detail: d }));

export const clip = (id = S.sel) => S.project?.clips.find((c) => c.id === id) || null;
export const live = () => (S.project ? S.project.clips.filter((c) => !c.dismissed) : []);
export function ordered() {
  const l = live().slice();
  l.sort(S.sort === 'score' ? (a, b) => b.score - a.score || a.startMs - b.startMs : (a, b) => a.startMs - b.startMs);
  return l;
}
export const picked = () => live().filter((c) => c.picked);
export const duration = () => S.project?.source.durationMs || 0;

// ───────── words and sentences
export function wordAt(t) {
  const w = S.words;
  let lo = 0, hi = w.length;
  while (lo < hi) {
    const m = (lo + hi) >> 1;
    if (w[m][2] <= t) lo = m + 1; else hi = m;
  }
  return lo;
}
export function sentenceAt(t) {
  const s = S.sents;
  let lo = 0, hi = s.length - 1;
  while (lo < hi) {
    const m = (lo + hi) >> 1;
    if (s[m].t1 <= t) lo = m + 1; else hi = m;
  }
  return lo;
}
export function snapStart(t) {
  const i = clamp(wordAt(t), 0, S.words.length - 1);
  let best = i, d = Infinity;
  for (let k = Math.max(0, i - 2); k <= Math.min(S.words.length - 1, i + 2); k++) {
    const dk = Math.abs(S.words[k][1] - t);
    if (dk < d) { d = dk; best = k; }
  }
  const prevEnd = best > 0 ? S.words[best - 1][2] : 0;
  return Math.max(prevEnd, S.words[best][1] - 120);
}
export function snapEnd(t) {
  const i = clamp(wordAt(t), 0, S.words.length - 1);
  let best = i, d = Infinity;
  for (let k = Math.max(0, i - 2); k <= Math.min(S.words.length - 1, i + 2); k++) {
    const dk = Math.abs(S.words[k][2] - t);
    if (dk < d) { d = dk; best = k; }
  }
  const nextStart = best + 1 < S.words.length ? S.words[best + 1][1] : duration();
  return Math.min(nextStart - 10, S.words[best][2] + 260);
}

// ───────── saving (explicit, ⌘S) + undo
export function markDirty(v = true) {
  if (S.dirty !== v) { S.dirty = v; emit('dirty'); }
}
export async function saveNow() {
  if (!S.dir || !S.project) return;
  await invoke('save_project', { dir: S.dir, project: S.project });
  markDirty(false);
}

const undoStack = [];
const redoStack = [];
let lastKey = '', lastAt = 0;
const snapshot = () => JSON.stringify({ c: S.project.clips, s: S.project.style, su: S.project.subject, sm: S.project.smooth, e: S.project.export });
function restore(j) {
  const o = JSON.parse(j);
  S.project.clips = o.c; S.project.style = o.s; S.project.subject = o.su; S.project.smooth = o.sm; S.project.export = o.e;
}

/** Apply a change. Changes with the same `key` within 600 ms share one undo step (sliders, drags). */
export function mutate(fn, key = '') {
  if (!S.project) return;
  const now = Date.now();
  if (!(key && key === lastKey && now - lastAt < 600)) {
    undoStack.push(snapshot());
    if (undoStack.length > 80) undoStack.shift();
    redoStack.length = 0;
  }
  lastKey = key; lastAt = now;
  fn(S.project);
  markDirty();
  emit('project');
}
export function undo() {
  if (!undoStack.length) return;
  redoStack.push(snapshot());
  restore(undoStack.pop());
  markDirty(); emit('project'); emit('select');
}
export function redo() {
  if (!redoStack.length) return;
  undoStack.push(snapshot());
  restore(redoStack.pop());
  markDirty(); emit('project'); emit('select');
}
export function resetHistory() { undoStack.length = 0; redoStack.length = 0; }

/** Edit a clip and flag it as user-touched so later Claude revisions don't overwrite it. */
export function editClip(id, fn, key) {
  mutate((p) => {
    const c = p.clips.find((x) => x.id === id);
    if (c) { fn(c); c.touched = true; }
  }, key || `edit-${id}`);
  emit('clipchange', id);
}

// ───────── Claude / local suggestions → project clips
export function mergeClips(incoming, { fresh = true, clean = false } = {}) {
  if (!S.project) return 0;
  const wasClean = !S.dirty;
  let added = 0;
  const have = new Map(S.project.clips.map((c) => [c.id, c]));
  const adds = [];
  for (const n of incoming) {
    const cur = have.get(n.id);
    if (!cur) {
      adds.push(n);
    } else if (cur.origin === 'claude' && !cur.touched && (n.srev || 0) > (cur.srev || 0)) {
      Object.assign(cur, { title: n.title, why: n.why, score: n.score, startMs: n.startMs, endMs: n.endMs, srev: n.srev });
      S.fresh.add(cur.id);
    }
  }
  if (adds.length || incoming.length) {
    mutate((p) => {
      for (const n of adds) { p.clips.push(n); added++; if (fresh) S.fresh.add(n.id); }
    }, 'merge');
    // Claude's suggestions already live in suggestions.json, so merging them is not an unsaved change
    if (clean && wasClean) markDirty(false);
  }
  return added;
}

export function newClipId() {
  return 'c' + Math.random().toString(16).slice(2, 12);
}
export function blankClip(startMs, endMs, extra = {}) {
  return {
    id: newClipId(), title: 'New clip', why: '', score: 0, startMs, endMs, origin: 'manual', picked: true, dismissed: false,
    captions: true, jumpCut: false, style: null, framing: { follow: true, x: 0.5, zoom: 1, shift: 0, keys: [] }, edits: {}, srev: 0, touched: true, exported: null,
    ...extra,
  };
}
