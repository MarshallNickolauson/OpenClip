import { S, markDirty, saveNow, on, emit, clip, ordered, live, picked, mutate, editClip, undo, redo, resetHistory, mergeClips, blankClip, snapStart, snapEnd, duration } from './state.js';
import { invoke, listen, pickVideo, onDrop, onDragState } from './api.js';
import { $, $$, h, fmt, fmtLong, secs, toast, clamp, ICON } from './util.js';
import * as player from './player.js';
import * as timeline from './timeline.js';
import { renderList, renderPanel, togglePick, dismiss, trimTo, effLen } from './panels.js';

let info = null;
let importing = null; // {dir, stages}

// ═════════════════════════ home
async function showHome() {
  if (S.project && S.dirty && !(await confirmLeave())) return;
  player.pause();
  markDirty(false);
  S.project = null; S.dir = null; S.sel = null;
  $('#editor').hidden = true;
  $('#home').hidden = false;
  info = await invoke('app_info');
  const sel = $('#selModel');
  const keep = localStorage.getItem('cc-model') || 'small.en';
  sel.innerHTML = '';
  for (const m of info.models) {
    sel.append(h('option', { value: m.id, selected: m.id === keep }, `${m.label} · ${m.id} · ${m.present ? 'ready' : m.mb + ' MB download'}`));
  }
  const upd = () => {
    localStorage.setItem('cc-model', sel.value);
    const m = info.models.find((x) => x.id === sel.value);
    $('#modelHint').textContent = m?.present ? 'Runs on this Mac. Nothing is uploaded.' : `Downloads once (${m?.mb} MB), then runs on this Mac.`;
  };
  sel.onchange = upd; upd();
  await renderRecent();
}
async function renderRecent() {
  const box = $('#recent');
  const list = await invoke('list_projects');
  box.innerHTML = '';
  if (!list.length) { box.append(h('div', { class: 'empty' }, 'Nothing yet. Drop a video above to start.')); return; }
  for (const p of list) {
    const card = h('button', { class: 'proj', onclick: () => openProject(p.dir) },
      h('b', {}, p.title),
      h('span', {}, `${fmtLong(p.durationMs)} · ${p.clips} clip${p.clips === 1 ? '' : 's'}${p.picked ? ` (${p.picked} picked)` : ''}`),
      !p.transcribed ? h('div', { class: 'warn' }, 'Not transcribed') : null,
      p.sourceMissing ? h('div', { class: 'warn' }, 'Source video moved') : null,
      h('span', { class: 'x', title: 'Delete project (the video is not touched)', onclick: async (e) => {
        e.stopPropagation();
        if (confirm(`Delete the project "${p.title}"? The original video is not touched.`)) { await invoke('delete_project', { dir: p.dir }); renderRecent(); }
      } }, h('span', { html: ICON.x })));
    box.append(card);
  }
}

async function startImport(path) {
  if (!path) return;
  const model = $('#selModel').value;
  try {
    const dir = await invoke('import_video', { path, model, language: 'en' });
    showImport(dir, model);
  } catch (e) {
    toast(String(e), 5000);
  }
}
$('#btnChoose').onclick = async () => startImport(await pickVideo());
onDrop((paths) => {
  const p = paths.find((x) => /\.(mp4|mov|m4v)$/i.test(x));
  if (!p) { if (paths.length) toast('Drop an MP4 or MOV file'); return; }
  if (!$('#home').hidden) startImport(p);
});
onDragState(() => $('#drop').classList.add('over'), () => $('#drop').classList.remove('over'));

// ───────── import progress
function showImport(dir, model) {
  const m = $('#modal');
  const needDl = !info.models.find((x) => x.id === model)?.present;
  const stage = (id, label) => h('div', { class: 'st idle', 'data-s': id }, h('div', { class: 'l' }, h('span', {}, label), h('span', { class: 'p muted' }, '')), h('div', { class: 'pbar' }, h('i')));
  m.innerHTML = '';
  m.append(h('div', { class: 'dialog' },
    h('h2', {}, 'Getting your video ready'),
    h('p', {}, 'Transcribing on this Mac. A 30-minute talk takes a few minutes; the face tracking runs at the same time.'),
    h('div', { class: 'prog' },
      stage('audio', 'Reading audio'),
      needDl ? stage('model', 'Downloading transcription model') : null,
      stage('transcribe', 'Transcribing speech'),
      stage('faces', 'Finding faces for framing')),
    h('div', { class: 'actions' }, h('button', { class: 'btn-line', style: 'width:auto;padding:8px 16px', onclick: () => { invoke('cancel_import'); m.hidden = true; renderRecent(); } }, 'Cancel'))));
  m.hidden = false;
  importing = { dir };
}
listen('import-progress', (e) => {
  if (importing && e.dir === importing.dir) {
    const row = $(`.st[data-s="${e.stage}"]`);
    if (row) {
      row.classList.remove('idle');
      $('i', row).style.width = e.pct + '%';
      $('.p', row).textContent = Math.round(e.pct) + '%';
    }
    // once speech is done the project is usable; faces keep going in the background
    if (e.stage === 'transcribed') {
      $$('.st').forEach((r) => r.classList.remove('idle'));
      const d = importing.dir;
      importing = null;
      $('#modal').hidden = true;
      openProject(d);
    }
    if (e.stage === 'error') {
      $('#modal').hidden = true;
      importing = null;
      toast('Import failed: ' + (e.error || 'unknown error'), 7000);
      renderRecent();
    }
  }
  if (S.dir && e.dir === S.dir) {
    if (e.stage === 'faces') {
      S.faceProgress = e.pct;
      const chip = $('#faceChip');
      chip.hidden = e.pct >= 100;
      chip.textContent = `Tracking faces ${Math.round(e.pct)}%`;
    }
    if (e.stage === 'faces' && e.pct >= 100) { S.project.faces = true; player.refreshPath(); }
    if (e.stage === 'faces-error') toast('Face tracking failed: ' + (e.error || ''), 5000);
  }
});

// ═════════════════════════ editor
async function openProject(dir) {
  let r;
  try { r = await invoke('open_project', { dir }); } catch (e) { toast(String(e), 5000); return; }
  S.dir = dir;
  S.project = r.project;
  S.words = r.words; S.sents = r.sentences; S.energy = r.energy;
  S.sel = null; S.thumbs = {}; S.export = {};
  resetHistory();
  $('#home').hidden = true;
  $('#editor').hidden = false;
  markDirty(false);
  updateTitle();
  $('#faceChip').hidden = r.project.faces || !r.project.transcribed;
  if (!r.project.faces && r.project.transcribed) $('#faceChip').textContent = 'Tracking faces…';
  if (!r.sourceExists) { await relink(); }
  player.setSource(r.project.source.path);
  player.layout();
  await pullSuggestions(true);
  const first = ordered()[0];
  selectClip(first ? first.id : null, { play: false });
  if (!live().length) setTab('clip');
  renderList(); renderPanel();
  syncButtons();
  const seen = await invoke('mcp_seen');
  if (seen) setClaude(seen);
}

async function relink() {
  toast('The source video moved. Choose it again.', 4000);
  const p = await pickVideo();
  if (!p) return;
  const proj = await invoke('relink_source', { dir: S.dir, path: p });
  S.project = proj;
}

function setTab(t) { S.tab = t; renderPanel(); }

function selectClip(id, opts = {}) {
  S.sel = id;
  const c = clip();
  $('#tDur').textContent = c ? fmt(c.endMs - c.startMs, true) : '0:00.0';
  S.path = null; S.caps = null;
  if (c) {
    player.video.pause();
    player.video.currentTime = c.startMs / 1000;
    S.t = c.startMs;
    player.refreshPath();
    player.refreshCaps();
    if ((opts.play ?? S.auto)) setTimeout(() => player.play(), 60);
    document.querySelector(`.card[data-id="${id}"]`)?.scrollIntoView({ block: 'nearest' });
  }
  emit('select');
  timeline.ensureVisible();
}
on('selectclip', (id) => selectClip(id));

function step(dir) {
  const items = ordered();
  if (!items.length) return;
  const i = items.findIndex((c) => c.id === S.sel);
  selectClip(items[clamp((i < 0 ? 0 : i + dir), 0, items.length - 1)].id);
}

// react to edits
on('clipchange', () => { player.refreshPath(); player.refreshCaps(); });
on('trim', () => { const c = clip(); if (c) $('#tDur').textContent = fmt(c.endMs - c.startMs, true); player.refreshPath(); player.refreshCaps(); });
on('framechange', () => player.refreshPath());
on('captionschange', () => { player.refreshCaps(); });
on('select', () => syncButtons());
on('caps', () => {
  const c = clip();
  if (c) $('#tDur').textContent = fmt(effLen(c), true);
  if (S.tab === 'clip') renderPanel();
});
on('project', () => { syncButtons(); player.refreshPath(); player.refreshCaps(); const c = clip(); if (c) $('#tDur').textContent = fmt(c.endMs - c.startMs, true); });

// manual framing from dragging the preview
on('framekey', ({ t, x }) => {
  const c = clip();
  if (!c) return;
  editClip(c.id, (cl) => {
    const keys = cl.framing.keys || (cl.framing.keys = []);
    const near = keys.find((k) => Math.abs(k.t - t) < 250);
    if (near) near.x = x; else keys.push({ t, x });
    keys.sort((a, b) => a.t - b.t);
  }, 'key-' + c.id);
  emit('framechange');
  if (S.tab === 'framing') renderPanel();
});

// ───────── Claude suggestions
async function pullSuggestions(silent = false) {
  if (!S.dir) return;
  const sug = await invoke('read_suggestions', { dir: S.dir });
  if (!sug.clips.length) return;
  const before = live().length;
  mergeClips(sug.clips.filter((c) => !c.dismissed), { clean: true });
  const added = live().length - before;
  if (added > 0 && !silent) {
    toast(`Claude added ${added} clip${added === 1 ? '' : 's'}`);
    if (!clip()) { const f = ordered()[0]; if (f) selectClip(f.id, { play: false }); }
  }
  renderList();
}
listen('suggestions-changed', (e) => { if (e.dir === S.dir) pullSuggestions(); });
function setClaude(seen) {
  const age = Date.now() - seen.ts;
  $('#claudeDot').classList.toggle('live', age < 10 * 60 * 1000);
  $('#claudeLabel').textContent = age < 10 * 60 * 1000 ? 'Claude connected' : 'Claude';
}
listen('mcp-seen', setClaude);

emitLocal();
function emitLocal() {
  on('localpick', async () => {
    toast('Scoring the talk…');
    const clips = await invoke('local_pick', { dir: S.dir, n: 8 });
    mergeClips(clips);
    const f = ordered()[0];
    if (f && !clip()) selectClip(f.id, { play: false });
    renderList();
  });
  on('connect', () => showConnect());
  on('export', () => runExport());
}

// ───────── connect Claude dialog
async function showConnect() {
  info = await invoke('app_info');
  const cmd = `claude mcp add clipcraft -- "${info.mcpPath}"`;
  const copy = (text) => { navigator.clipboard?.writeText(text); toast('Copied'); };
  const row = (text) => h('div', { class: 'code' }, h('code', {}, text), h('button', { class: 'pill small', onclick: () => copy(text) }, 'Copy'));
  const m = $('#modal');
  m.innerHTML = '';
  const result = h('p', { class: 'hint' }, '');
  m.append(h('div', { class: 'dialog' },
    h('h2', {}, 'Let Claude pick the clips'),
    h('p', {}, 'ClipCraft ships a local MCP server. Claude starts it on your Mac when it needs it, so there is nothing to deploy or sign in to. It only sees the transcript and writes clip suggestions back here.'),
    info.mcpExists ? null : h('p', { class: 'err' }, 'The MCP helper was not found next to the app. Build with ./scripts/bundle.sh.'),
    h('h4', {}, 'Claude Desktop'),
    h('button', { class: 'primary', onclick: async () => {
      try {
        const p = await invoke('connect_claude_desktop');
        result.textContent = `Added to ${p}. Quit and reopen Claude Desktop to load it.`;
      } catch (e) { result.textContent = 'Could not update the config: ' + e; result.className = 'err'; }
    } }, 'Add to Claude Desktop'),
    result,
    h('h4', {}, 'Claude Code'),
    row(cmd),
    h('h4', {}, 'Then ask'),
    h('ol', { class: 'steps' },
      h('li', {}, 'Keep this project open in ClipCraft.'),
      h('li', {}, 'In Claude, say: ', h('b', {}, '“Find the best clips in my open ClipCraft project.”')),
      h('li', {}, 'Suggestions appear in the list as Claude saves them. Preview, caption, tick, export.')),
    row('Find the best 5–10 vertical clips in my open ClipCraft project.'),
    h('div', { class: 'actions' }, h('button', { class: 'btn-line', style: 'width:auto;padding:8px 18px', onclick: () => (m.hidden = true) }, 'Done'))));
  m.hidden = false;
}
$('#btnClaude').onclick = showConnect;

// ───────── export
async function runExport() {
  const items = picked();
  if (!items.length) { toast('Tick at least one clip to export'); return; }
  if (S.exporting) return;
  S.exporting = true;
  S.export = {};
  items.forEach((c) => (S.export[c.id] = { state: 'queued', pct: 0 }));
  setTab('export');
  emit('exportprogress');
  try {
    await invoke('export_clips', { dir: S.dir, project: S.project, ids: items.map((c) => c.id) });
  } catch (e) { S.exporting = false; toast(String(e), 6000); }
}
listen('export-progress', (e) => {
  const ex = (S.export[e.id] = { ...(S.export[e.id] || {}), ...e });
  if (e.state === 'done') {
    editClip(e.id, (c) => { c.exported = e.path; }, 'exp');
    const cl = clip(e.id); if (cl) cl.touched = cl.touched; // keep flags
  }
  emit('exportprogress');
  void ex;
});
listen('export-finished', () => {
  S.exporting = false;
  const done = Object.values(S.export).filter((x) => x.state === 'done');
  emit('exportprogress');
  if (done.length) toast(`Exported ${done.length} clip${done.length === 1 ? '' : 's'}`);
});
$('#btnExport').onclick = () => { if (!picked().length) { setTab('export'); toast('Tick the clips you want to export'); } else { runExport(); } };

// ═════════════════════════ unsaved changes
function updateTitle() {
  const t = $('#title');
  if (t.querySelector('input')) return;
  t.textContent = (S.project ? S.project.title : '') + (S.dirty ? ' *' : '');
  $('#btnSave').disabled = !S.dirty;
  $('#btnSave').classList.toggle('on', S.dirty);
}
on('dirty', updateTitle);
$('#btnSave').onclick = () => saveNow().then(() => toast('Saved'));

/** Ask what to do with unsaved edits. Resolves true when it is fine to leave. */
function confirmLeave() {
  return new Promise((resolve) => {
    const m = $('#modal');
    const done = (ok) => { m.hidden = true; resolve(ok); };
    m.innerHTML = '';
    m.append(h('div', { class: 'dialog', style: 'width:420px' },
      h('h2', {}, `Save changes to “${S.project.title}”?`),
      h('p', {}, 'Your edits will be lost if you don\'t save them.'),
      h('div', { class: 'actions' },
        h('button', { class: 'btn-line', style: 'width:auto;padding:8px 14px', onclick: () => { markDirty(false); done(true); } }, 'Don\'t Save'),
        h('span', { class: 'spacer' }),
        h('button', { class: 'btn-line', style: 'width:auto;padding:8px 14px', onclick: () => done(false) }, 'Cancel'),
        h('button', { class: 'primary', onclick: async () => { await saveNow(); done(true); } }, 'Save'))));
    m.hidden = false;
  });
}
let closing = false;
listen('close-requested', async () => {
  if (closing) return;
  closing = true;
  if (!S.project || !S.dirty || (await confirmLeave())) await invoke('exit_app');
  closing = false;
});

// ═════════════════════════ chrome
$$('.ext-link').forEach((b) => (b.onclick = () => window.__TAURI__.opener.openUrl(b.dataset.url)));
$('#btnHome').onclick = () => { showHome(); };
$('#btnUndo').onclick = undo;
$('#btnRedo').onclick = redo;
$('#btnPlay').onclick = () => player.toggle();
$('#btnStart').onclick = () => { const c = clip(); if (c) player.seek(c.startMs); };
$('#btnNext').onclick = () => step(1);
$('#btnAddClip').onclick = () => {
  if (!S.project) return;
  const a = snapStart(S.t), b = snapEnd(Math.min(duration(), S.t + 30000));
  const c = blankClip(a, b);
  mutate((p) => p.clips.push(c));
  selectClip(c.id, { play: false });
};
$('#btnLoop').onclick = () => { S.loop = !S.loop; syncButtons(); };
$('#btnAuto').onclick = () => { S.auto = !S.auto; syncButtons(); };
$('#btnCuts').onclick = toggleCuts;
function toggleCuts() {
  const c = clip();
  if (!c) return;
  editClip(c.id, (x) => { x.jumpCut = !x.jumpCut; });
  emit('captionschange');
  renderPanel();
}
$('#btnCaps').onclick = () => { S.capsOn = !S.capsOn; syncButtons(); player.updateCaps(S.t); };
$('#btnGuides').onclick = () => { S.guides = !S.guides; syncButtons(); };
$('#btnSetIn').onclick = () => { const c = clip(); if (c) trimTo(c, 'start', S.t); };
$('#btnSetOut').onclick = () => { const c = clip(); if (c) trimTo(c, 'end', S.t); };
$$('#sortSeg button').forEach((b) => (b.onclick = () => { S.sort = b.dataset.s; $$('#sortSeg button').forEach((x) => x.classList.toggle('on', x === b)); renderList(); }));
function syncButtons() {
  $('#btnLoop').classList.toggle('on', S.loop);
  $('#btnAuto').classList.toggle('on', S.auto);
  $('#btnCaps').classList.toggle('on', S.capsOn);
  $('#btnCuts').classList.toggle('on', !!clip()?.jumpCut);
  $('#btnGuides').classList.toggle('on', S.guides);
  $('#guides').hidden = !S.guides;
}
on('play', () => { $('#icoPlay').hidden = S.playing; $('#icoPause').hidden = !S.playing; });
on('tick', () => {
  const c = clip();
  $('#tCur').textContent = c ? fmt(Math.max(0, S.t - c.startMs), true) : '0:00.0';
});
$('#title').onclick = () => {
  const t = $('#title');
  if (t.querySelector('input')) return;
  const inp = h('input', { type: 'text', value: S.project.title });
  t.textContent = ''; t.append(inp); inp.focus(); inp.select();
  const done = (ok) => {
    const v = inp.value.trim();
    if (ok && v) mutate((p) => { p.title = v; });
    updateTitle();
  };
  inp.onkeydown = (e) => { e.stopPropagation(); if (e.key === 'Enter') done(true); if (e.key === 'Escape') done(false); };
  inp.onblur = () => done(true);
};

// vertical splitter
(() => {
  const sp = $('#splitV');
  let y0 = 0, h0 = 0;
  sp.addEventListener('pointerdown', (e) => { sp.setPointerCapture(e.pointerId); y0 = e.clientY; h0 = $('#timeline').getBoundingClientRect().height; document.body.classList.add('resizing'); });
  sp.addEventListener('pointermove', (e) => {
    if (!document.body.classList.contains('resizing')) return;
    const hh = clamp(h0 - (e.clientY - y0), 200, window.innerHeight * 0.6);
    $('#editor').style.setProperty('--tl-h', hh + 'px');
  });
  sp.addEventListener('pointerup', () => document.body.classList.remove('resizing'));
  sp.addEventListener('dblclick', () => $('#editor').style.removeProperty('--tl-h'));
})();

// ═════════════════════════ keyboard
window.addEventListener('keydown', (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 's' && !$('#editor').hidden) { e.preventDefault(); saveNow().then(() => toast('Saved')); return; }
  if (e.target.matches('input, textarea, select')) return;
  if (!$('#modal').hidden) { if (e.key === 'Escape') $('#modal').hidden = true; return; }
  if ($('#editor').hidden) return;
  const mod = e.metaKey || e.ctrlKey;
  const c = clip();
  const k = e.key.toLowerCase();
  if (mod && k === 's') { e.preventDefault(); saveNow().then(() => toast('Saved')); return; }
  if (mod && k === 'z') { e.preventDefault(); e.shiftKey ? redo() : undo(); return; }
  if (mod && k === 'e') { e.preventDefault(); runExport(); return; }
  if (mod) return;
  switch (e.key) {
    case ' ': e.preventDefault(); player.toggle(); break;
    case 'ArrowDown': e.preventDefault(); step(1); break;
    case 'ArrowUp': e.preventDefault(); step(-1); break;
    case 'ArrowRight': e.preventDefault(); if (c) player.seek(S.t + (e.shiftKey ? 5000 : 500)); break;
    case 'ArrowLeft': e.preventDefault(); if (c) player.seek(S.t - (e.shiftKey ? 5000 : 500)); break;
    case 'Backspace': case 'Delete': if (c) { e.preventDefault(); dismiss(c.id); } break;
    case '[': if (c) trimTo(c, 'start', e.shiftKey ? S.t : snapStart(S.t)); break;
    case ']': if (c) trimTo(c, 'end', e.shiftKey ? S.t : snapEnd(S.t)); break;
    default:
      if (k === 'j') step(1);
      else if (k === 'k') step(-1);
      else if (k === 'x' || e.key === 'Enter') { if (c) togglePick(c.id); }
      else if (k === 'l') { S.loop = !S.loop; syncButtons(); }
      else if (k === 't') toggleCuts();
      else if (k === 'c') { S.capsOn = !S.capsOn; syncButtons(); player.updateCaps(S.t); }
      else if (k === 'q') { if (c) player.seek(c.startMs); }
      else if (k === 'n') $('#btnAddClip').click();
      else if (k === '1') setTab('clip');
      else if (k === '2') setTab('captions');
      else if (k === '3') setTab('framing');
      else if (k === '4') setTab('export');
  }
});

showHome();
