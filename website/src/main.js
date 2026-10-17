import './style.css';

const nav = document.getElementById('nav');
const onScroll = () => nav?.classList.toggle('scrolled', scrollY > 8);
addEventListener('scroll', onScroll, { passive: true }); onScroll();

document.getElementById('burger')?.addEventListener('click', (e) => {
  const open = nav.classList.toggle('open');
  e.currentTarget.setAttribute('aria-expanded', String(open));
});

// highlight the current page in nav + docs sidebar
const path = location.pathname.replace(/\/index\.html$/, '/').replace(/\.html$/, '');
document.querySelectorAll('.nav-links a, .docs-nav a').forEach((a) => {
  const p = new URL(a.href, location.origin);
  if (p.hash === '' && p.pathname === path) a.classList.add('active');
});

const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
if (!reduceMotion) document.documentElement.classList.add('anim');

const store = {
  get(k) { try { return localStorage.getItem(k); } catch { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch { /* private mode: fine */ } },
};

// theme: dark unless the visitor chose otherwise (the choice is kept only in this browser)
const root = document.documentElement;
document.querySelectorAll('.theme-btn').forEach((b) => b.addEventListener('click', () => {
  const next = root.dataset.theme === 'light' ? 'dark' : 'light';
  root.dataset.theme = next;
  store.set('oc-theme', next);
}));

// split the headline into words so each can rise in turn
function wrapWords(h) {
  if (!h || reduceMotion) return;
  const text = h.textContent;
  const seg = typeof Intl.Segmenter === 'function' ? new Intl.Segmenter(root.lang || 'en', { granularity: 'word' }) : null;
  const parts = seg ? [...seg.segment(text)].map((x) => x.segment) : text.split(/(\s+)/);
  h.setAttribute('aria-label', text);
  h.textContent = '';
  let i = 0;
  // punctuation stays glued to the word before it
  const merged = [];
  for (const part of parts) {
    if (merged.length && /^[.,;:!?)\]"'’”…—-]+$/.test(part)) merged[merged.length - 1] += part; else merged.push(part);
  }
  for (const part of merged) {
    if (!part.trim()) { h.append(' '); continue; }
    const w = document.createElement('span');
    w.className = 'w'; w.setAttribute('aria-hidden', 'true');
    const inner = document.createElement('span');
    inner.style.setProperty('--i', i++);
    inner.textContent = part;
    w.append(inner);
    h.append(w);
  }
}

// the key list's description may hold inline <kbd>, so group it into one grid cell
function wrapKeys() {
  document.querySelectorAll('.keys li').forEach((li) => {
    const first = li.firstElementChild;
    if (!first || li.querySelector(':scope > .d')) return;
    const d = document.createElement('span');
    d.className = 'd';
    while (first.nextSibling) d.append(first.nextSibling);
    li.append(d);
  });
}

wrapWords(document.querySelector('.hero h1'));
wrapKeys();

// live GitHub star count, via our own cached endpoint (hidden while the repo is private)
fetch('/api/stars').then((r) => r.json()).then(({ stars }) => {
  if (typeof stars !== 'number') return;
  const text = stars >= 1000 ? (stars / 1000).toFixed(stars >= 10000 ? 0 : 1).replace(/\.0$/, '') + 'k' : String(stars);
  document.querySelectorAll('.star-count').forEach((el) => { el.textContent = text; el.hidden = false; });
}).catch(() => {});

// scroll reveals: tag the pieces, stagger siblings, reveal as they enter
if (!reduceMotion) {
  const groups = ['.row-text > *', '.row > .frame, .row > .caption-demo', '.deflist > div', '.stance-text', '.closer .wrap > *', '.page-head > *', '.osrow > .os', '.note',
    '.sec > .wrap > h2', '.flow > *', '.compare-wrap', '.fine', '.keys li', '.whatsnew > *', '.more-link', '.faq details', '.oss > *', '.hero-demo', '.crop-fig',
    '.prose > h1, .prose > h2, .prose > h3, .prose > p, .prose > ul, .prose > ol, .prose > table, .prose > pre, .prose > .grid2'];
  groups.forEach((sel) => {
    document.querySelectorAll(sel).forEach((el) => {
      el.setAttribute('data-reveal', '');
      const sibling = [...el.parentElement.children].indexOf(el);
      el.style.setProperty('--d', Math.min(sibling, 5) * 0.09 + 's');
    });
  });
  // Reveal by geometry: anything whose top has entered the viewport (or is above it) is shown.
  const check = () => {
    const edge = innerHeight * 0.94;
    document.querySelectorAll('[data-reveal]:not(.in)').forEach((el) => { if (el.getBoundingClientRect().top < edge) el.classList.add('in'); });
  };
  let queued = false;
  const queue = () => { if (queued) return; queued = true; setTimeout(() => { queued = false; check(); }, 40); };
  addEventListener('scroll', queue, { passive: true });
  addEventListener('resize', queue);
  addEventListener('load', check);
  check();
  // safety net: whatever happens, nothing stays hidden
  setTimeout(() => document.querySelectorAll('[data-reveal]:not(.in)').forEach((el) => { if (el.getBoundingClientRect().top < innerHeight * 2) el.classList.add('in'); }), 4000);
}

// copy buttons (the one-line installer, the MCP command)
document.querySelectorAll('[data-copy]').forEach((b) => b.addEventListener('click', async () => {
  const el = document.querySelector(b.dataset.copy);
  if (!el) return;
  try { await navigator.clipboard.writeText(el.textContent.trim()); b.textContent = '✓'; } catch { return; }
  setTimeout(() => { b.textContent = b.dataset.label || 'Copy'; }, 1500);
}));
