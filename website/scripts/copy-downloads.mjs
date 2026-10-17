// Prebuild step: publish release metadata and (optionally) mirror the disk image.
//  - copies ../OpenClip_<version>.dmg into public/downloads/ when it exists and fits Cloudflare Pages' 25 MiB file limit
//  - writes public/latest.json, sitemap.xml, robots.txt and install.sh from site.config.json
// Never fails the build when no dmg exists yet.
import { copyFileSync, mkdirSync, existsSync, readFileSync, writeFileSync, readdirSync, rmSync, statSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const site = JSON.parse(readFileSync(join(here, '..', 'site.config.json'), 'utf8'));
const repoRoot = join(here, '..', '..');
const pub = join(here, '..', 'public');
const out = join(pub, 'downloads');
mkdirSync(out, { recursive: true });

let version = site.fallbackVersion;
try { version = readFileSync(join(repoRoot, 'Cargo.toml'), 'utf8').match(/^version = "([^"]+)"/m)[1]; } catch { /* standalone checkout */ }
const file = `OpenClip_${version}.dmg`;
const url = `https://github.com/${site.repo}/raw/main/${file}`;

// drop mirrored images of other versions
for (const f of readdirSync(out)) if (/^OpenClip_.*\.dmg$/.test(f) && f !== file) rmSync(join(out, f));
let mirror = null;
const src = join(repoRoot, file);
if (existsSync(src)) {
  if (statSync(src).size <= 25 * 1024 * 1024) { copyFileSync(src, join(out, file)); mirror = `${site.host}/downloads/${file}`; }
  else console.warn(`NOTE: ${file} is over 25 MiB, so it is not mirrored on the site; the GitHub link is used.`);
} else console.warn(`NOTE: ${file} not found in the repository root; the download button points at GitHub (${url}).`);

// changelog bullets for the manifest
let notes = [];
try {
  const log = readFileSync(join(here, '..', 'content', 'CHANGELOG.md'), 'utf8');
  const section = log.split(/\n(?=## )/).find((b) => b.startsWith(`## ${version}`)) || '';
  notes = [...section.matchAll(/^- \*\*(.+?)\*\*/gm)].map((m) => m[1].replace(/[.:]$/, '')).slice(0, 5);
} catch { /* optional */ }

writeFileSync(join(pub, 'latest.json'), JSON.stringify({ version, file, url, ...(mirror ? { mirror } : {}), notes, page: `${site.host}/download` }, null, 2) + '\n');

writeFileSync(join(pub, 'robots.txt'), `User-agent: *\nAllow: /\n\nSitemap: ${site.host}/sitemap.xml\n`);

// sitemap: every page except 404
const pages = [];
const walk = (dir, rel = '') => {
  for (const f of readdirSync(dir)) {
    if (['node_modules', 'dist', 'src', 'public', 'content', 'scripts', 'functions', '.wrangler'].includes(f)) continue;
    const p = join(dir, f);
    if (statSync(p).isDirectory()) walk(p, rel + f + '/');
    else if (f.endsWith('.html') && f !== '404.html') pages.push(f === 'index.html' ? rel : rel + f.replace(/\.html$/, ''));
  }
};
walk(join(here, '..'));
pages.sort((a, b) => (a === '' ? -1 : b === '' ? 1 : a.localeCompare(b)));
writeFileSync(join(pub, 'sitemap.xml'), `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${pages.map((p) => `  <url><loc>${site.host}/${p}</loc></url>`).join('\n')}\n</urlset>\n`);

// install.sh
writeFileSync(join(pub, 'install.sh'), `#!/bin/bash
# OpenClip installer for macOS.
#   curl -fsSL ${site.host}/install.sh | bash
#
# Downloads the latest OpenClip disk image with curl (so macOS does not tag it as "downloaded from
# the internet"), copies the app to Applications and opens it. Read it first if you like: it only
# downloads the disk image named in ${site.host}/latest.json and copies the app out of it.
set -euo pipefail

SITE="${site.host}"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "OpenClip is macOS only for now." >&2
  exit 1
fi

# the manifest names the exact, versioned file for the newest release
URL="$(curl -fsSL -m 10 "$SITE/latest.json" 2>/dev/null | sed -n 's/.*"url": *"\\([^"]*\\)".*/\\1/p' | head -1 || true)"
[ -n "$URL" ] || { echo "Could not reach the OpenClip website. Check your connection and try again." >&2; exit 1; }

tmp="$(mktemp -d)"
mnt=""
cleanup() {
  [ -n "$mnt" ] && hdiutil detach -quiet "$mnt" 2>/dev/null || true
  rm -rf "$tmp"
}
trap cleanup EXIT

echo "Downloading OpenClip..."
curl -fL --progress-bar -o "$tmp/OpenClip.dmg" "$URL" || { echo "Could not download OpenClip from $URL" >&2; exit 1; }

mnt="$(hdiutil attach -nobrowse -readonly -noautoopen "$tmp/OpenClip.dmg" | awk -F'\\t' '/\\/Volumes\\//{print $NF}' | tail -1)"
[ -d "$mnt/OpenClip.app" ] || { echo "The disk image did not contain OpenClip.app." >&2; exit 1; }

# quit a running copy so it can be replaced
osascript -e 'tell application "OpenClip" to quit' >/dev/null 2>&1 || true
sleep 1

dest="/Applications"
[ -w "$dest" ] || { dest="$HOME/Applications"; mkdir -p "$dest"; }
rm -rf "$dest/OpenClip.app"
cp -R "$mnt/OpenClip.app" "$dest/"
xattr -dr com.apple.quarantine "$dest/OpenClip.app" 2>/dev/null || true

echo "Installed to $dest/OpenClip.app"
open "$dest/OpenClip.app"
`);
console.log(`site metadata written for OpenClip ${version} (${pages.length} pages)`);
