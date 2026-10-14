// Clip list (left) and inspector tabs (right).
import { S, clip, ordered, live, picked, mutate, editClip, emit, on, snapStart, snapEnd, sentenceAt, duration, blankClip, wordAt } from './state.js';
import { invoke, fileUrl, pickDir } from './api.js';
import { $, h, fmt, secs, clamp, ICON, toast } from './util.js';
import { seek } from './player.js';

// ═════════════════════════ clip list
const list = $('#clipList');
const thumbQueue = [];
let thumbBusy = false;

function thumbKey(c) { return `${c.startMs >> 8}|${c.framing.zoom}|${c.framing.follow}|${c.framing.keys.length}`; }
function wantThumb(c) {
  const t = S.thumbs[c.id];
  if (t && t.key === thumbKey(c)) return;
  if (thumbQueue.some((q) => q.id === c.id)) return;
  thumbQueue.push({ id: c.id, key: thumbKey(c) });
  pump();
}
async function pump() {
  if (thumbBusy) return;
  thumbBusy = true;
  while (thumbQueue.length) {
    const q = thumbQueue.shift();
    const c = clip(q.id);
    if (!c || !S.dir) continue;
    try {
      const f = await invoke('thumbnail', { dir: S.dir, clip: c, subject: S.project.subject, smooth: S.project.smooth });
      S.thumbs[q.id] = { key: q.key, url: fileUrl(f) + '?k=' + encodeURIComponent(q.key) };
      const el = list.querySelector(`[data-id="${q.id}"] .thumb`);
      if (el) el.style.backgroundImage = `url("${S.thumbs[q.id].url}")`;
    } catch (e) { /* source missing or frame unavailable: keep the placeholder */ }
  }
  thumbBusy = false;
}

export function renderList() {
  const items = ordered();
  $('#clipsCount').textContent = items.length ? `${items.length} clip${items.length === 1 ? '' : 's'}` : 'Clips';
  list.innerHTML = '';
  if (!items.length) {
    list.append(emptyCard());
    return;
  }
  for (const c of items) {
    const ex = S.export[c.id];
    const card = h('div', { class: `card${c.id === S.sel ? ' active' : ''}${c.picked ? '' : ' off'}${S.fresh.has(c.id) ? ' fresh' : ''}`, 'data-id': c.id },
      h('div', { class: 'thumb', style: S.thumbs[c.id] ? `background-image:url("${S.thumbs[c.id].url}")` : '' },
        h('span', { class: 'dur' }, secs(c.endMs - c.startMs))),
      h('div', { class: 'meta' },
        h('div', { class: 'ttl' }, c.title || 'Untitled'),
        h('div', { class: 'sub' },
          h('span', {}, `${fmt(c.startMs)}–${fmt(c.endMs)}`),
          c.score > 0 ? h('span', { class: `score${c.score >= 80 ? ' hi' : ''}` }, Math.round(c.score)) : null,
          c.origin !== 'manual' ? h('span', { class: 'origin' }, c.origin === 'claude' ? 'Claude' : 'Auto') : null,
          ex?.state === 'done' || c.exported ? h('span', { class: 'exp', title: 'Exported' }, '✓') : null),
        c.why ? h('div', { class: 'why' }, c.why) : null),
      h('button', {
        class: `check${c.picked ? ' on' : ''}`, title: 'Include in export (X)',
        onclick: (e) => { e.stopPropagation(); togglePick(c.id); },
      }, h('span', { html: ICON.check })),
      h('button', { class: 'x', title: 'Dismiss clip', onclick: (e) => { e.stopPropagation(); dismiss(c.id); } }, h('span', { html: ICON.x })));
    card.addEventListener('click', () => emit('selectclip', c.id));
    card.addEventListener('dblclick', () => emit('selectclip', c.id));
    list.append(card);
    wantThumb(c);
  }
  S.fresh.clear();
}

function emptyCard() {
  const ready = S.project?.transcribed;
  return h('div', { class: 'empty-card' },
    h('h4', {}, ready ? 'No clips yet' : 'Transcribing…'),
    h('p', {}, ready
      ? 'Ask Claude to pick the best moments, or let ClipCraft take a quick pass with its built-in scorer. You can also drag across the overview bar below to mark a clip yourself.'
      : 'Clips appear here once the talk has been transcribed.'),
    ready ? h('button', { class: 'primary', onclick: () => emit('connect') }, 'Find clips with Claude') : null,
    ready ? h('button', { class: 'btn-line', onclick: () => emit('localpick') }, 'Quick pick without Claude') : null);
}

export function togglePick(id) {
  editClip(id, (c) => { c.picked = !c.picked; }, 'pick-' + id);
}
export function dismiss(id) {
  const items = ordered();
  const i = items.findIndex((c) => c.id === id);
  mutate((p) => { const c = p.clips.find((x) => x.id === id); if (c) { c.dismissed = true; c.picked = false; } });
  if (S.sel === id) {
    const rest = ordered();
    emit('selectclip', (rest[Math.min(i, rest.length - 1)] || {}).id || null);
  }
}

// ═════════════════════════ inspector
const TABS = [['clip', 'Clip', ICON.clip], ['captions', 'Captions', ICON.caption], ['framing', 'Framing', ICON.frame], ['export', 'Export', ICON.export]];
const rail = $('#rail'), panel = $('#panel');
rail.append(...TABS.map(([id, label, svg]) => h('button', { 'data-t': id, title: label, onclick: () => { S.tab = id; renderPanel(); } }, h('span', { html: svg }))));

const field = (label, val, ...body) => h('div', { class: 'field' }, h('label', {}, h('span', {}, label), val != null ? h('span', { class: 'val' }, val) : null), ...body);
const range = (min, max, step, value, oninput) => h('input', { type: 'range', min, max, step, value, oninput: (e) => oninput(parseFloat(e.target.value), e) });
const toggle = (on, onclick) => h('div', { class: `toggle${on ? ' on' : ''}`, onclick });
const seg = (opts, cur, pick) => h('div', { class: 'seg' }, opts.map(([v, l]) => h('button', { class: cur === v ? 'on' : '', onclick: () => pick(v) }, l)));

export function renderPanel() {
  rail.querySelectorAll('button').forEach((b) => b.classList.toggle('on', b.dataset.t === S.tab));
  const y = panel.scrollTop;
  panel.innerHTML = '';
  const fn = { clip: tabClip, captions: tabCaptions, framing: tabFraming, export: tabExport }[S.tab];
  fn(panel);
  panel.scrollTop = y;
}

const put = (root, ...kids) => root.append(...kids.filter((k) => k != null && k !== false));

function noClip(root) {
  put(root, h('p', { class: 'hint' }, 'Select a clip in the list to edit it.'));
}

// ───────── clip tab
function tabClip(root) {
  const c = clip();
  if (!c) return noClip(root);
  put(root, 
    h('h3', {}, 'Title'),
    h('input', { type: 'text', value: c.title, onchange: (e) => editClip(c.id, (x) => { x.title = e.target.value.trim() || 'Untitled'; }) }),
    c.why ? h('p', { class: 'hint' }, c.why) : null,
    h('div', { class: 'row-t' }, h('div', {}, 'Jump cut silences', h('small', {}, cutLabel(c))),
      toggle(c.jumpCut, () => { editClip(c.id, (x) => { x.jumpCut = !x.jumpCut; }); emit('captionschange'); renderPanel(); })),
    c.jumpCut ? h('button', { class: 'btn-line', onclick: () => { mutate((p) => p.clips.forEach((x) => { if (!x.dismissed) x.jumpCut = true; })); emit('captionschange'); toast('Jump cuts on for every clip'); } }, 'Turn on for all clips') : null,
    h('h3', {}, 'Trim'),
    nudge('Starts', c.startMs, (d) => trimTo(c, 'start', c.startMs + d), () => trimTo(c, 'start', S.t)),
    nudge('Ends', c.endMs, (d) => trimTo(c, 'end', c.endMs + d), () => trimTo(c, 'end', S.t)),
    h('div', { class: 'btn-row' },
      h('button', { class: 'pill small', onclick: () => stepSentence(c, 'start', -1), title: 'Start one sentence earlier' }, '◂ Sentence'),
      h('button', { class: 'pill small', onclick: () => stepSentence(c, 'start', +1), title: 'Start one sentence later' }, 'Sentence ▸'),
      h('button', { class: 'pill small', onclick: () => stepSentence(c, 'end', -1), title: 'End one sentence earlier' }, '◂ Sentence'),
      h('button', { class: 'pill small', onclick: () => stepSentence(c, 'end', +1), title: 'End one sentence later' }, 'Sentence ▸')),
    h('p', { class: 'hint' }, 'Top pair moves the start, bottom pair the end. Length: ' + fmt(c.endMs - c.startMs, true)),
    h('h3', {}, 'Transcript'),
    transcript(c),
    h('p', { class: 'hint' }, 'Click a word to jump there · double-click to fix its spelling · ⌥-click to hide it from captions.'),
    h('h3', {}, 'Clip'),
    h('div', { class: 'btn-row' },
      h('button', { class: 'btn-line', onclick: () => duplicate(c) }, 'Duplicate'),
      h('button', { class: 'btn-line danger', onclick: () => dismissSel(c.id) }, 'Dismiss')));
}
export function effLen(c) {
  const sg = c.jumpCut && c.id === S.sel && S.caps?.segments;
  return sg ? sg.reduce((a, g) => a + (g[1] - g[0]), 0) : c.endMs - c.startMs;
}
function cutLabel(c) {
  const full = c.endMs - c.startMs;
  if (!c.jumpCut) return 'Remove pauses between words';
  const e = effLen(c);
  return e < full ? `${fmt(full, true)} → ${fmt(e, true)}` : 'No long pauses in this clip';
}
function dismissSel(id) { dismiss(id); }
function duplicate(c) {
  const n = JSON.parse(JSON.stringify(c));
  n.id = 'c' + Math.random().toString(16).slice(2, 12);
  n.title = c.title + ' (copy)'; n.origin = 'manual'; n.exported = null; n.touched = true;
  mutate((p) => p.clips.push(n));
  emit('selectclip', n.id);
}
function nudge(label, ms, by, set) {
  const mk = (d, t) => h('button', { onclick: () => by(d) }, t);
  return h('div', { class: 'nudge' },
    h('div', {}, h('div', { class: 'muted', style: 'font-size:11px' }, label), h('div', { class: 'tc' }, fmt(ms, true))),
    h('div', { class: 'btns' }, mk(-1000, '−1s'), mk(-100, '−.1'), mk(100, '+.1'), mk(1000, '+1s'),
      h('button', { onclick: set, title: 'Set to playhead' }, '⌖')));
}
export function trimTo(c, which, ms) {
  editClip(c.id, (x) => {
    if (which === 'start') x.startMs = Math.round(clamp(ms, 0, x.endMs - 2000));
    else x.endMs = Math.round(clamp(ms, x.startMs + 2000, duration()));
  }, `trim-${c.id}`);
  emit('trim');
}
function stepSentence(c, which, dir) {
  const s = S.sents;
  if (!s.length) return;
  if (which === 'start') {
    let i = sentenceAt(c.startMs + 200);
    if (s[i].t0 >= c.startMs - 250 && dir < 0) i -= 1; else if (dir > 0) i += 1;
    i = clamp(i, 0, s.length - 1);
    trimTo(c, 'start', Math.max(i > 0 ? S.words[s[i].w0 - 1][2] : 0, s[i].t0 - 180));
  } else {
    let i = sentenceAt(c.endMs - 200);
    if (dir > 0) i += 1; else if (s[i].t1 >= c.endMs - 450) i -= 1;
    i = clamp(i, 0, s.length - 1);
    trimTo(c, 'end', s[i].t1 + 380);
  }
}

function transcript(c) {
  const box = h('div', { class: 'tx', id: 'tx' });
  const i0 = wordAt(c.startMs), s0 = sentenceAt(c.startMs), s1 = sentenceAt(c.endMs);
  const from = S.sents[Math.max(0, s0 - 1)]?.w0 ?? i0;
  const to = S.sents[Math.min(S.sents.length - 1, s1 + 1)]?.w1 ?? i0;
  for (let i = from; i <= to && i < S.words.length; i++) {
    const [txt, a, b] = S.words[i];
    const inside = a >= c.startMs - 1 && a < c.endMs;
    const ov = c.edits?.[String(i)];
    const w = h('span', { class: `w${inside ? '' : ' ctx'}${ov === '' ? ' hid' : ''}${ov ? ' ed' : ''}`, 'data-i': i, 'data-a': a, 'data-b': b }, ov === '' ? txt : (ov || txt));
    w.addEventListener('click', (e) => {
      if (e.altKey) { toggleHide(c.id, i); return; }
      seek(clamp(a, c.startMs, c.endMs));
    });
    w.addEventListener('dblclick', () => editWord(c.id, i, w));
    box.append(w, ' ');
  }
  return box;
}
function toggleHide(id, i) {
  editClip(id, (c) => { if (c.edits[String(i)] === '') delete c.edits[String(i)]; else c.edits[String(i)] = ''; });
  emit('captionschange');
}
function editWord(id, i, el) {
  const cur = clip(id).edits?.[String(i)] || S.words[i][0];
  const inp = h('input', { type: 'text', value: cur, style: 'width:120px;padding:2px 6px;display:inline-block' });
  el.replaceWith(inp);
  inp.focus(); inp.select();
  const done = (ok) => {
    if (ok) {
      const v = inp.value.trim();
      editClip(id, (c) => { if (v === S.words[i][0]) delete c.edits[String(i)]; else c.edits[String(i)] = v; });
      emit('captionschange');
    } else renderPanel();
  };
  inp.addEventListener('keydown', (e) => { e.stopPropagation(); if (e.key === 'Enter') done(true); if (e.key === 'Escape') done(false); });
  inp.addEventListener('blur', () => done(true));
}
/** Highlight the word being spoken. */
export function markWord(t) {
  const tx = $('#tx');
  if (!tx) return;
  const cur = tx.querySelector('.cur');
  for (const el of tx.children) {
    if (!el.dataset || el.dataset.a === undefined) continue;
    if (+el.dataset.a <= t && t < +el.dataset.b + 40) {
      if (cur !== el) { cur?.classList.remove('cur'); el.classList.add('cur'); }
      return;
    }
  }
}

// ───────── captions tab
const PRESETS = [
  ['pop', 'Pop', 'Bold, outlined', 'font-family:"Arial Black";text-transform:uppercase;color:#fff;-webkit-text-stroke:1px #000;text-shadow:0 2px 0 #000'],
  ['boxed', 'Boxed', 'On a dark box', 'font-family:"Helvetica Neue";font-weight:800;color:#fff;background:rgba(0,0,0,.72);padding:1px 6px;border-radius:5px'],
  ['clean', 'Clean', 'Soft shadow', 'font-family:"Helvetica Neue";font-weight:800;color:#fff;text-shadow:0 2px 8px rgba(0,0,0,.7)'],
];
const ACCENTS = ['#ffd60a', '#ffffff', '#ff5d73', '#3ddc97', '#4cc9ff', '#c084fc'];
function tabCaptions(root) {
  const st = S.project.style;
  const c = clip();
  const setStyle = (patch, key) => { mutate((p) => Object.assign(p.style, patch), key || 'style'); emit('captionschange'); };
  put(root, 
    c ? h('div', { class: 'row-t' }, h('div', {}, 'Captions on this clip', h('small', {}, 'Burned in at export')),
      toggle(c.captions, () => { editClip(c.id, (x) => { x.captions = !x.captions; }); emit('captionschange'); renderPanel(); })) : null,
    h('h3', {}, 'Look (all clips)'),
    h('div', { class: 'presets' }, PRESETS.map(([id, name, sub, css]) =>
      h('button', { class: `preset${st.preset === id ? ' on' : ''}`, onclick: () => { setStyle({ preset: id, upper: null }); renderPanel(); } },
        h('div', {}, h('span', { style: css + ';font-size:14px' }, name), h('small', {}, sub))))),
    field('Size', st.size.toFixed(2) + '×', range(0.6, 1.7, 0.05, st.size, (v) => setStyle({ size: v }, 'cs'))),
    field('Position', Math.round(st.y * 100) + '%', range(0.15, 0.92, 0.01, st.y, (v) => setStyle({ y: v }, 'cy'))),
    field('Words per line', st.words, range(1, 6, 1, st.words, (v) => setStyle({ words: v }, 'cw'))),
    h('div', { class: 'field' }, h('label', {}, h('span', {}, 'Spoken-word colour')),
      h('div', { class: 'swatches' }, ACCENTS.map((a) => h('button', { class: st.accent === a ? 'on' : '', style: `background:${a}`, onclick: () => { setStyle({ accent: a }); renderPanel(); } })))),
    h('div', { class: 'row-t' }, h('div', {}, 'Uppercase'), toggle(st.upper ?? (st.preset === 'pop'), () => { setStyle({ upper: !(st.upper ?? (st.preset === 'pop')) }); renderPanel(); })),
    h('p', { class: 'hint' }, 'Captions come from the on-device transcript. Fix a word by double-clicking it in the Clip tab.'));
}

// ───────── framing tab
function tabFraming(root) {
  const c = clip();
  if (!c) return noClip(root);
  const f = c.framing;
  const set = (fn, key) => { editClip(c.id, (x) => fn(x.framing), key || 'fr-' + c.id); emit('framechange'); };
  const hasFaces = S.project.faces;
  put(root, 
    h('div', { class: 'row-t' }, h('div', {}, 'Follow the speaker', h('small', {}, hasFaces ? 'Camera glides to keep the face centred' : 'Face tracking is still running…')),
      toggle(f.follow, () => { set((x) => { x.follow = !x.follow; }); renderPanel(); })),
    f.follow ? field('Who to follow', null, seg([['largest', 'Closest'], ['left', 'Left'], ['right', 'Right']], S.project.subject,
      (v) => { mutate((p) => { p.subject = v; }); emit('framechange'); renderPanel(); })) : null,
    field('Camera smoothness', S.project.smooth.toFixed(2) + 's', range(0.2, 1.4, 0.05, S.project.smooth, (v) => { mutate((p) => { p.smooth = v; }, 'smooth'); emit('framechange'); })),
    h('h3', {}, 'This clip'),
    field('Zoom', f.zoom.toFixed(2) + '×', range(1, 2.2, 0.05, f.zoom, (v) => set((x) => { x.zoom = v; }))),
    field('Nudge', (f.shift >= 0 ? '+' : '') + f.shift.toFixed(2), range(-0.6, 0.6, 0.02, f.shift, (v) => set((x) => { x.shift = v; }))),
    h('h3', {}, 'Keyframes'),
    h('p', { class: 'hint' }, 'Drag the picture in the preview, or click the framing track, to pin the crop at a moment. With keyframes the crop follows them instead of the face.'),
    h('div', { class: 'keys' }, (f.keys || []).map((k, i) => h('div', { class: 'keyrow' },
      h('span', {}, fmt(k.t - c.startMs, true)),
      h('span', { class: 'muted' }, Math.round(k.x * 100) + '%'),
      h('button', { onclick: () => { set((x) => x.keys.splice(i, 1)); renderPanel(); } }, h('span', { html: ICON.x }))))),
    h('div', { class: 'btn-row' },
      h('button', { class: 'btn-line', onclick: () => { set((x) => { x.keys = []; x.shift = 0; x.zoom = 1; x.follow = true; }); renderPanel(); } }, 'Reset framing')));
}

// ───────── export tab
function tabExport(root) {
  const p = S.project;
  const items = picked();
  const total = items.reduce((a, c) => a + (c.endMs - c.startMs), 0);
  put(root, 
    h('h3', {}, 'Output'),
    h('div', { class: 'field' }, h('label', {}, h('span', {}, 'Folder')),
      h('div', { class: 'path' }, p.export.dir || '~/Movies/ClipCraft'),
      h('div', { class: 'btn-row' }, h('button', { class: 'btn-line', onclick: async () => {
        const d = await pickDir(p.export.dir || undefined);
        if (d) { mutate((q) => { q.export.dir = d; }); renderPanel(); }
      } }, 'Choose folder…'))),
    field('Size', null, seg([[720, '720 × 1280'], [1080, '1080 × 1920']], p.export.width, (v) => { mutate((q) => { q.export.width = v; }); renderPanel(); })),
    field('Codec', null, seg([['h264', 'H.264'], ['hevc', 'HEVC (smaller)']], p.export.codec, (v) => { mutate((q) => { q.export.codec = v; }); renderPanel(); })),
    h('h3', {}, `Picked · ${items.length} clip${items.length === 1 ? '' : 's'} · ${fmt(total)}`),
    h('div', { class: 'explist' }, items.length ? items.map((c) => {
      const ex = S.export[c.id];
      return h('div', { class: 'exrow' },
        h('div', { class: 'l' }, h('span', {}, c.title), h('span', { class: 'muted' }, secs(c.endMs - c.startMs))),
        ex ? h('div', { class: `pbar${ex.state === 'done' ? ' done' : ''}` }, h('i', { style: `width:${ex.pct || 0}%` })) : null,
        ex?.state === 'error' ? h('div', { class: 'err' }, ex.error) : null,
        ex?.state === 'done' ? h('button', { class: 'pill small', onclick: () => invoke('reveal', { path: ex.path }) }, 'Show in Finder') : null);
    }) : [h('p', { class: 'hint' }, 'Tick the clips you want in the list on the left.')]),
    S.exporting
      ? h('button', { class: 'btn-line danger', onclick: () => invoke('cancel_export') }, 'Cancel export')
      : h('button', { class: 'primary', style: 'width:100%;justify-content:center', disabled: !items.length, onclick: () => emit('export') }, `Export ${items.length || ''} clip${items.length === 1 ? '' : 's'}`),
    h('p', { class: 'hint' }, 'Rendered on this Mac with the hardware encoder. 30 fps max, AAC audio.'));
}

// ═════════════════════════ wiring
on('project', () => { renderList(); if (S.tab === 'export') renderPanel(); $('#exportLabel').textContent = picked().length ? `Export ${picked().length}` : 'Export'; });
on('select', () => { renderList(); renderPanel(); });
on('trim', () => { if (S.tab === 'clip') renderPanel(); });
on('exportprogress', () => { renderList(); if (S.tab === 'export') renderPanel(); });
let lastMark = 0;
on('tick', () => { const n = performance.now(); if (n - lastMark > 80) { lastMark = n; markWord(S.t); } });
