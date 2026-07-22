// Static-site generator for GitHub Pages. Renders README.md and docs/*.md into
// styled, cross-linked HTML with a shared nav, and leaves the live web demo to
// be copied alongside by the workflow. Depends only on `marked` (GFM tables).
//
//   npm ci && node build.mjs      # writes ../_site
import { marked } from 'marked';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(root, '_site');
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });

// Pages to render: [source md, output html, nav label]. `null` label hides it
// from the nav (still generated + linkable).
const pages = [
  ['README.md', 'index.html', 'Home'],
  ['docs/SPEC.md', 'spec.html', 'Spec'],
  ['docs/DESIGN.md', 'design.html', 'Design'],
  ['docs/BRIDGES.md', 'bridges.html', 'Bridges'],
  ['docs/AUDIT.md', 'audit.html', 'Audit'],
  ['bindings/README.md', 'bindings.html', 'Bindings'],
  ['web/README.md', 'webguide.html', 'Web guide'],
];

// Map a source .md path (as written in links) to its output page.
const linkMap = new Map();
for (const [src, dst] of pages) {
  const base = path.basename(src).toLowerCase();
  linkMap.set(src, dst); // exact repo path
  linkMap.set(base, dst); // bare filename
}
// README.md is ambiguous (root vs bindings vs web); exact paths above win, and a
// bare "readme.md" falls back to Home.
linkMap.set('readme.md', 'index.html');

const navLinks = pages
  .filter(([, , label]) => label)
  .map(([, dst, label]) => ({ dst, label }))
  .concat([{ dst: 'demo/', label: 'Live demo' }]);

function rewriteLinks(html, self) {
  // Rewrite href="...something.md" (with optional ../ and #anchor) to the built
  // page; leave external and non-md links alone.
  return html.replace(/href="([^"]+)"/g, (m, href) => {
    if (/^https?:|^#|^mailto:/.test(href)) return m;
    const [pathPart, anchor] = href.split('#');
    if (!pathPart.toLowerCase().endsWith('.md')) return m;
    const norm = pathPart.replace(/^\.\//, '').replace(/^\.\.\//, '');
    const dst =
      linkMap.get(norm) ||
      linkMap.get(path.basename(norm).toLowerCase()) ||
      norm.replace(/\.md$/i, '.html');
    return `href="${dst}${anchor ? '#' + anchor : ''}"`;
  });
}

function nav(self) {
  const items = navLinks
    .map(({ dst, label }) => {
      const active = dst === self ? ' class="active"' : '';
      return `<a href="${dst}"${active}>${label}</a>`;
    })
    .join('');
  return `<nav>${items}</nav>`;
}

function page(title, bodyHtml, self) {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>${title}</title>
<link rel="stylesheet" href="style.css" />
</head>
<body>
<header class="site">
  <a class="brand" href="index.html">SPORE</a>
  ${nav(self)}
</header>
<main class="doc">
${bodyHtml}
</main>
<footer class="site">
  <span>SPORE — store-and-forward planetary opportunistic relay envelope · public domain (Unlicense)</span>
</footer>
</body>
</html>`;
}

marked.setOptions({ gfm: true, breaks: false });

for (const [src, dst, label] of pages) {
  const abs = path.join(root, src);
  if (!fs.existsSync(abs)) {
    console.warn(`skip missing ${src}`);
    continue;
  }
  const md = fs.readFileSync(abs, 'utf8');
  const title = label && label !== 'Home' ? `SPORE — ${label}` : 'SPORE';
  const html = rewriteLinks(marked.parse(md), dst);
  fs.writeFileSync(path.join(out, dst), page(title, html, dst));
  console.log(`rendered ${src} -> _site/${dst}`);
}

// Ship the stylesheet.
fs.writeFileSync(path.join(out, 'style.css'), fs.readFileSync(path.join(root, 'site/style.css')));
console.log('wrote _site/style.css');
console.log('done. copy web/ + the built wasm into _site/demo/ to finish.');
