import { defineConfig } from 'vite';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { marked } from 'marked';

const root = import.meta.dirname;
const site = JSON.parse(readFileSync(join(root, 'site.config.json'), 'utf8'));
// Version comes from the app's Cargo.toml when this folder lives inside the repo.
let appVersion = site.fallbackVersion;
try { appVersion = readFileSync(join(root, '../Cargo.toml'), 'utf8').match(/^version = "([^"]+)"/m)[1]; } catch { /* use fallback */ }
const dmgName = `OpenClip_${appVersion}.dmg`;
const dmgUrl = `https://github.com/${site.repo}/raw/main/${dmgName}`;
const partial = (n) => readFileSync(join(root, 'src/partials', n + '.html'), 'utf8');
const content = (n) => readFileSync(join(root, 'content', n), 'utf8');

// Every .html file (outside the skipped folders) becomes a page.
function pages(dir = root, out = {}) {
  for (const f of readdirSync(dir)) {
    if (['node_modules', 'dist', 'src', 'public', 'content', 'scripts', 'functions', '.wrangler'].includes(f)) continue;
    const p = join(dir, f);
    if (statSync(p).isDirectory()) pages(p, out);
    else if (f.endsWith('.html')) out[p.slice(root.length + 1).replace(/\.html$/, '')] = p;
  }
  return out;
}

const pick = (html, re) => (html.match(re) || [])[1];

// Shared header / footer / docs sidebar, SEO tags and the single-place host and version tokens.
const partials = {
  name: 'partials',
  transformIndexHtml: {
    order: 'pre',
    handler(html, ctx) {
      const urlPath = (ctx?.path || '/').replace(/index\.html$/, '').replace(/\.html$/, '');
      const title = pick(html, /<title>([^<]*)<\/title>/) || 'OpenClip';
      const desc = pick(html, /<meta name="description" content="([^"]*)"/) || '';
      const head = [
        `<link rel="canonical" href="%HOST%${urlPath}">`,
        `<link rel="icon" type="image/png" sizes="32x32" href="/favicon-32.png">`,
        `<link rel="icon" type="image/png" sizes="192x192" href="/icon-192.png">`,
        `<link rel="apple-touch-icon" href="/apple-touch-icon.png">`,
        `<meta name="theme-color" content="#090c12">`,
        `<meta property="og:type" content="website">`,
        `<meta property="og:site_name" content="OpenClip">`,
        `<meta property="og:title" content="${title}">`,
        desc && `<meta property="og:description" content="${desc}">`,
        `<meta property="og:url" content="%HOST%${urlPath}">`,
        `<meta property="og:image" content="%HOST%/og-image.png">`,
        `<meta name="twitter:card" content="summary_large_image">`,
      ].filter(Boolean).join('\n');
      return html
        .replace('</head>', head + '\n</head>')
        .replace('<!--@header-->', partial('header'))
        .replace('<!--@footer-->', partial('footer'))
        .replace('<!--@docs-nav-->', partial('docs-nav'))
        .replace('<!--@changelog-->', () => marked.parse(content('CHANGELOG.md').replace(/^# Changelog\s*/, '')))
        .replace('<!--@whatsnew-->', () => marked.parse(content('CHANGELOG.md').split(/\n(?=## )/).slice(1, 3).join('\n').replace(/^## /gm, '### ').replace(/^# Changelog\s*/, '')))
        .replace('<!--@notices-->', () => marked.parse(content('THIRD-PARTY-NOTICES.md')))
        .replaceAll('%HOST%', site.host)
        .replaceAll('%DMG_URL%', dmgUrl)
        .replaceAll('%DMG%', dmgName)
        .replaceAll('%VERSION%', appVersion)
        .replaceAll('%REPO%', site.repo);
    },
  },
};

export default defineConfig({
  plugins: [partials],
  build: { rollupOptions: { input: pages() } },
});
