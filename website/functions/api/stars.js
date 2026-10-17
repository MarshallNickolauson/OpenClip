// GitHub star count, fetched server-side and cached at the edge for an hour, so visitors'
// browsers never contact GitHub and GitHub never sees visitors' IP addresses.
// While the repository is private (or GitHub is unreachable) this returns { stars: null } and
// the site simply shows no count.
const REPO = 'MarshallNickolauson/OpenClip';

export async function onRequestGet() {
  try {
    const r = await fetch(`https://api.github.com/repos/${REPO}`, {
      headers: { 'User-Agent': 'openclip-website', Accept: 'application/vnd.github+json' },
      cf: { cacheTtl: 3600, cacheEverything: true },
    });
    if (!r.ok) throw new Error(String(r.status));
    const { stargazers_count } = await r.json();
    return Response.json({ stars: typeof stargazers_count === 'number' ? stargazers_count : null }, { headers: { 'Cache-Control': 'public, max-age=900' } });
  } catch {
    return Response.json({ stars: null }, { status: 200, headers: { 'Cache-Control': 'public, max-age=60' } });
  }
}
