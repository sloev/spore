// Static-site generator for GitHub Pages. Renders README.md and docs/*.md into
// styled, cross-linked HTML with a shared nav, and leaves the live web demo to
// be copied alongside by the workflow. Depends only on `marked` (GFM tables).
//
//   npm ci && node build.mjs      # writes ../_site
import { marked } from 'marked';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { requireHardbrutCss } from '../web/hardbrut-import.mjs';

// HARDBRUT (M7): the framework is imported at build time and inlined, so a
// change to supernihil/hardbrut shows up on the next `npm ci && node build.mjs`.
const hardbrutCss = requireHardbrutCss();

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const out = path.join(root, '_site');
fs.rmSync(out, { recursive: true, force: true });
fs.mkdirSync(out, { recursive: true });

// Pages to render: [source md, output html, nav label]. `null` label hides it
// from the nav (still generated + linkable).
//
// Navigation: 4 primary items — Try it · How it works · Get a node · Developer
// — plus "Web node" (the live demo, appended below). Every dev-oriented
// document with real technical depth (spec, bridges, design, mission/charter,
// roadmap, security, ...) is reachable from one place, docs/DEVELOPER.md, not
// scattered across the top nav. Secondary guides are still rendered + linkable
// so nothing 404s, just kept off the primary nav.

// The front page is `site/home.md`, not the README. They have different jobs: a
// README opens on "what is this and how do I build it" for someone who already
// arrived at a repository, while the front page has to make sense to someone who
// has never heard of any of this and is deciding whether it is for them. Serving
// the README as `index.html` made the first screen a badge row and a feature
// table. The README stays as it is on GitHub.
const pages = [
  ['site/home.md', 'index.html', 'Try it'],
  ['docs/HOW_IT_WORKS.md', 'how-it-works.html', 'How it works'],
  ['docs/APPS.md', 'apps.html', 'Get a node'],
  ['docs/DEVELOPER.md', 'developer.html', 'Developer'],
  // Secondary guides: rendered + linkable, kept off the top nav
  ['docs/MISSION.md', 'mission.html', null],
  ['docs/SPEC.md', 'spec.html', null],
  ['docs/DESIGN.md', 'design.html', null],
  ['docs/BRIDGES.md', 'bridges.html', null],
  ['docs/REBUILD.md', 'rebuild.html', null],
  ['docs/CONTINUITY.md', 'continuity.html', null],
  ['docs/DIRECT.md', 'direct.html', null],
  ['docs/ROADMAP.md', 'roadmap.html', null],
  ['docs/SECURITY.md', 'security-policy.html', null],
  ['docs/SECURITY_FINDINGS.md', 'security.html', null],
  ['docs/CHANGELOG.md', 'changelog.html', null],
  ['docs/HARDWARE.md', 'hardware.html', null],
  ['android/TESTING.md', 'testing.html', null],
  ['bindings/README.md', 'bindings.html', null],
  ['reference/README.md', 'reference.html', null],
  ['web/README.md', 'webguide.html', null],
  ['docs/CONTRIBUTING.md', 'contributing.html', null],
  ['docs/DEV_GUIDE.md', 'dev-guide.html', null],
  ['docs/PROXY_SETUP.md', 'proxy-setup.html', null],
];

// Map a source .md path (as written in links) to its output page.
const linkMap = new Map();
for (const [src, dst] of pages) {
  const base = path.basename(src).toLowerCase();
  linkMap.set(src, dst); // exact repo path
  linkMap.set(base, dst); // bare filename
}
// README.md is ambiguous (root vs bindings vs web) and no longer a page of its
// own; exact paths above win, and a bare "readme.md" falls back to Home, which is
// where a reader following "see the README" wants to land on the site.
linkMap.set('README.md', 'index.html');
linkMap.set('readme.md', 'index.html');

// Per-page <meta name="description">, keyed by output page. Search results and
// link previews quote this, so it is the one line most people read before
// deciding whether to click; pages without an entry fall back to the site line.
const DESC_DEFAULT =
  'SPORE — a public-domain store-and-forward mesh: signed messages that travel ' +
  'over the internet, a shared folder, a USB stick, sound, paper, or radio.';
// Titles for pages the nav does not name. Without these the tab reads "SPORE —
// readme" for three different pages, which is no help to anyone with a dozen tabs
// open.
const titles = new Map([
  ['index.html', 'SPORE — messages that still get through'],
  ['security-policy.html', 'SPORE — reporting a vulnerability'],
  ['security.html', 'SPORE — security findings'],
  ['changelog.html', 'SPORE — changelog'],
  ['roadmap.html', 'SPORE — engineering roadmap'],
  ['hardware.html', 'SPORE — hardware verification'],
  ['testing.html', 'SPORE — Android device tests'],
  ['direct.html', 'SPORE — Direct: low-latency E2E pipes'],
  ['bindings.html', 'SPORE — language bindings'],
  ['reference.html', 'SPORE — reference decoders'],
  ['webguide.html', 'SPORE — the browser node'],
  ['contributing.html', 'SPORE — contributing'],
  ['dev-guide.html', 'SPORE — developer guide'],
  ['mission.html', 'SPORE — mission'],
  ['how-it-works.html', 'SPORE — how it works'],
  ['developer.html', 'SPORE — developer'],
]);

const descriptions = new Map([
  [
    'index.html',
    'Send notes, files and updates between your own devices with no account and ' +
      'no company in the middle — over Wi-Fi, a shared folder, a USB stick, ' +
      'Bluetooth, sound, or radio. Free and public domain.',
  ],
  [
    'spec.html',
    'The SPORE v1 wire format in full: envelope layout, addressing, routing, ' +
      'fragmentation, sealing and congestion rules — enough to reimplement it.',
  ],
  ['apps.html', 'Download SPORE: the browser node, the desktop daemon, the Android app, and the language bindings.'],
  ['how-it-works.html', 'Addressing, store-and-forward delivery, pluggable bridges, and privacy by default — the mechanism, briefly.'],
  ['developer.html', 'The wire format, every bridge, the reimplementation guide, and everything else with real technical depth.'],
  ['bridges.html', 'Every link SPORE speaks — internet, folder, serial, Bluetooth, audio, radio — and which have been verified on real hardware.'],
  ['security.html', 'The SPORE findings register: what was found, how it was reproduced, what was changed, and what is still open.'],
  ['continuity.html', 'How one surviving copy of SPORE — a file, a clone, a printed sheet — rebuilds the whole system with no server and no network.'],
]);

// "Web node" (the live demo) sits between the picker and the technical hub —
// spliced in rather than appended, so Developer stays the last, catch-all item.
const navLinks = pages
  .filter(([, , label]) => label)
  .map(([, dst, label]) => ({ dst, label }));
navLinks.splice(navLinks.length - 1, 0, { dst: 'demo/', label: 'Web node' });

const GITHUB_BLOB = 'https://github.com/sloev/spore/blob/master/';

function rewriteLinks(html, self) {
  // Rewrite href="...something.md" (with optional ../ and #anchor) to the built
  // page; leave external and anchor links alone. A repo-relative link to
  // anything that isn't a rendered page (source, scripts, LICENSE) has no
  // file to resolve to on Pages at all — those go to the GitHub blob view
  // instead of 404ing.
  return html.replace(/href="([^"]+)"/g, (m, href) => {
    if (/^https?:|^#|^mailto:/.test(href)) return m;
    const [pathPart, anchor] = href.split('#');
    const norm = pathPart.replace(/^\.\//, '').replace(/^\.\.\//, '');
    if (pathPart.toLowerCase().endsWith('.md')) {
      const dst =
        linkMap.get(norm) ||
        linkMap.get(path.basename(norm).toLowerCase()) ||
        norm.replace(/\.md$/i, '.html');
      return `href="${dst}${anchor ? '#' + anchor : ''}"`;
    }
    // Not a doc page. Only repo-relative paths (../ or ./) need rewriting —
    // a bare filename is a same-directory asset the build already copies in
    // (docs images, antenna-seed.svg), and must resolve locally, not on GitHub.
    if (!/^\.\.?\//.test(pathPart)) return m;
    return `href="${GITHUB_BLOB}${norm}${anchor ? '#' + anchor : ''}"`;
  });
}


// ---------------------------------------------------------------------------
// Share row (front page only).
//
// Most of these are plain intent URLs built at build time. Two are not:
// Mastodon has no central host, so the instance is asked for once and kept in
// localStorage; Signal and Matrix have no web share intent at all, so they go
// through the OS share sheet (navigator.share) and fall back to the clipboard.
// Saying so is better than shipping links that quietly do nothing.
// ---------------------------------------------------------------------------
const SHARE = {
  url: 'https://sloev.github.io/spore/',
  title: 'SPORE — messages that still deliver, with no servers',
  text:
    'SPORE: a signed postcard that travels over the internet, a folder, a USB ' +
    'stick, a QR code, or a person reading it aloud — with radio and Bluetooth ' +
    'paths for operators who verify them against their own hardware. ' +
    'Same delivery rules on all of them. Public domain.',
};

function shareBar() {
  const u = encodeURIComponent(SHARE.url);
  const t = encodeURIComponent(SHARE.title);
  const txt = encodeURIComponent(SHARE.text);
  const both = encodeURIComponent(`${SHARE.title} — ${SHARE.url}`);

  // [label, href, title]. `null` href = handled by the script below.
  const links = [
    ['Hacker News', `https://news.ycombinator.com/submitlink?u=${u}&t=${t}`, 'Submit to Hacker News'],
    ['Lobsters', `https://lobste.rs/stories/new?url=${u}&title=${t}`, 'Submit to Lobsters'],
    ['Reddit', `https://www.reddit.com/submit?url=${u}&title=${t}`, 'Submit to Reddit'],
    ['Mastodon', null, 'Toot it from your own instance'],
    ['Bluesky', `https://bsky.app/intent/compose?text=${both}`, 'Post to Bluesky'],
    ['Telegram', `https://t.me/share/url?url=${u}&text=${txt}`, 'Send on Telegram'],
    ['Signal', null, 'Signal has no web share link — uses your share sheet, or copies'],
    ['Matrix', null, 'Matrix has no web share link — uses your share sheet, or copies'],
    ['Delta Chat', `mailto:?subject=${t}&body=${txt}%0A%0A${u}`, 'Delta Chat (or any mail app)'],
    ['WhatsApp', `https://wa.me/?text=${both}`, 'Send on WhatsApp'],
    ['Facebook', `https://www.facebook.com/sharer/sharer.php?u=${u}`, 'Share on Facebook'],
    ['Copy link', null, 'Copy the link to the clipboard'],
  ];

  const buttons = links
    .map(([label, href, title]) =>
      href
        ? `<a class="btn btn-cancel" href="${href}" title="${title}" target="_blank" rel="noopener noreferrer">${label}</a>`
        : `<button class="btn btn-cancel" type="button" data-share="${label}" title="${title}">${label}</button>`
    )
    .join('');

  return `<section class="share card" aria-label="Share SPORE">
  <div class="card-body">
  <h2>Pass it on</h2>
  <p class="text-muted">Continuity is just redundancy that outlives its sources — and it only works
  if the copies are already scattered before they're needed.</p>
  <div class="cluster">${buttons}</div>
  </div>
</section>
<script>
(function () {
  var URL_ = ${JSON.stringify(SHARE.url)};
  var TITLE = ${JSON.stringify(SHARE.title)};
  var TEXT = ${JSON.stringify(SHARE.text)};

  function flash(btn, msg) {
    var was = btn.textContent;
    btn.textContent = msg;
    setTimeout(function () { btn.textContent = was; }, 1600);
  }
  function copy(btn) {
    var s = TITLE + ' — ' + URL_;
    var done = function () { flash(btn, 'Copied ✓'); };
    if (navigator.clipboard) navigator.clipboard.writeText(s).then(done, function () { prompt('Copy:', s); });
    else prompt('Copy:', s);
  }
  // Signal and Matrix: no intent URL exists, so offer the OS share sheet (which
  // lists them on a phone) and fall back to the clipboard on a desktop.
  function sheet(btn) {
    if (navigator.share) {
      navigator.share({ title: TITLE, text: TEXT, url: URL_ }).catch(function () {});
    } else {
      copy(btn);
    }
  }
  function mastodon(btn) {
    var host = localStorage.getItem('spore.mastodon') || '';
    host = prompt('Your Mastodon instance:', host || 'mastodon.social');
    if (!host) return;
    host = host.trim().replace(/^https?:\\/\\//, '').replace(/\\/.*$/, '');
    if (!host) return;
    localStorage.setItem('spore.mastodon', host);
    window.open('https://' + host + '/share?text=' +
      encodeURIComponent(TITLE + ' — ' + URL_ + '\\n\\n' + TEXT), '_blank', 'noopener');
  }

  document.querySelectorAll('[data-share]').forEach(function (btn) {
    btn.addEventListener('click', function () {
      var kind = btn.getAttribute('data-share');
      if (kind === 'Mastodon') mastodon(btn);
      else if (kind === 'Copy link') copy(btn);
      else sheet(btn);
    });
  });
})();
</script>`;
}

function nav(self) {
  const items = navLinks
    .map(({ dst, label }) => {
      const active = dst === self ? ' aria-current="page"' : '';
      return `<li><a href="${dst}"${active}>${label}</a></li>`;
    })
    .join('');
  return `<ul class="navbar-links" id="nav-links">${items}</ul>`;
}

// Thin SPORE-only layer over HARDBRUT: everything here is a rule HARDBRUT does
// not ship (docs reading width, the per-code-block copy button, print) — no
// component is redefined, and nothing here duplicates a HARDBRUT class.
const siteAdapterCss = `
/* SPORE adapter — docs site only. Uses HARDBRUT tokens, never redefines them. */
main.doc.container { max-width: 860px; }
main.doc { font-size: 0.95rem; }
main.doc .code-copy {
  position: absolute; top: 8px; right: 8px;
  font: 11px var(--font-mono); line-height: 1; padding: 4px 9px;
  background: var(--paper); color: var(--ink); border: var(--border);
  cursor: pointer; box-shadow: var(--shadow-sm);
}
main.doc pre { position: relative; }
@page { size: A4; margin: 11mm 10mm; }
@media print {
  .navbar, .site-footer, nav, .share, main.doc .code-copy { display: none !important; }
  main.doc { max-width: none; margin: 0; padding: 0; }
}
`;

// Minimal escaping for text going into an attribute value.
const attr = (s) => s.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');

// ---------------------------------------------------------------------------
// Heading anchors.
//
// The docs are written for GitHub, where every heading silently gets an `id` and
// `[Bridge index](#bridge-index)` just works. `marked` emits none, so all 116
// in-page links on the built site — including the whole bridge index, which is
// how anyone finds one bridge among fifty — pointed at nothing. Slugs are
// generated with GitHub's own algorithm so the same anchor resolves in both
// places, and `checkLinks` below fails the build if one ever does not.
// ---------------------------------------------------------------------------
function slugify(text) {
  return text
    .replace(/<[^>]*>/g, '') // heading text may hold <code>, <em>, <a>
    .replace(/&#(\d+);/g, (_, d) => String.fromCharCode(+d))
    .replace(/&quot;/g, '"')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&amp;/g, '&')
    .trim()
    .toLowerCase()
    .replace(/[^\w\- ]+/g, '') // drop punctuation, keep word chars, - and space
    .replace(/ /g, '-');
}

/// Add `id` to every heading, returning [html, Set<id>].
///
/// The set also carries ids the markdown placed itself — BRIDGES.md marks each of
/// its fifty bridges with an explicit `<a id="…"></a>` rather than relying on the
/// heading text, since a bridge's heading includes its status emoji.
function anchorHeadings(html) {
  const seen = new Map();
  const ids = new Set();
  for (const [, id] of html.matchAll(/\bid="([^"]+)"/g)) ids.add(id);
  for (const [, name] of html.matchAll(/<a[^>]*\bname="([^"]+)"/g)) ids.add(name);
  const out = html.replace(/<h([1-6])>([\s\S]*?)<\/h\1>/g, (m, depth, inner) => {
    const base = slugify(inner);
    if (!base) return m;
    // GitHub disambiguates repeats as `slug-1`, `slug-2`, …
    const n = seen.get(base) ?? 0;
    seen.set(base, n + 1);
    const id = n === 0 ? base : `${base}-${n}`;
    ids.add(id);
    return `<h${depth} id="${id}">${inner}</h${depth}>`;
  });
  return [out, ids];
}

function page(title, bodyHtml, self) {
  // A per-page body class, so a page can carry its own rules — the spec needs
  // print styling that would be wrong everywhere else.
  const cls = self.replace(/\.html$/, '');
  const desc = descriptions.get(self) || DESC_DEFAULT;
  const url = SHARE.url + (self === 'index.html' ? '' : self);
  return `<!doctype html>
<html lang="en" data-accent="yellow">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>${title}</title>
<meta name="description" content="${attr(desc)}" />
<link rel="canonical" href="${attr(url)}" />
<meta property="og:type" content="website" />
<meta property="og:site_name" content="SPORE" />
<meta property="og:title" content="${attr(title)}" />
<meta property="og:description" content="${attr(desc)}" />
<meta property="og:url" content="${attr(url)}" />
<meta name="twitter:card" content="summary" />
<style>
/* HARDBRUT (imported at build time from supernihil/hardbrut). */
${hardbrutCss}
${siteAdapterCss}
</style>
<script>
(function () {
  var h = document.documentElement;
  var t = localStorage.getItem('theme');
  if (!t) t = window.matchMedia('(prefers-color-scheme:dark)').matches ? 'dark' : 'light';
  h.setAttribute('data-theme', t);
})();
</script>
</head>
<body class="page-${cls}">
<nav class="navbar">
  <a class="navbar-brand" href="index.html">SPORE</a>
  <button class="navbar-toggle" type="button" aria-expanded="false" aria-controls="nav-links" aria-label="Menu">☰</button>
  ${nav(self)}
</nav>
<main class="doc container">
<section class="section">
${bodyHtml}
</section>
</main>
<footer class="site-footer">
  <span>SPORE — store-and-forward planetary opportunistic relay envelope · public domain (Unlicense)</span>
</footer>
<script>
(function () {
  // One "Copy" button per code block, on every page — added here rather than in
  // shareBar() because that script only ever runs on index.html, and every doc
  // page with a <pre> should get this, not just the front page.
  document.querySelectorAll('main.doc pre').forEach(function (pre) {
    var code = pre.querySelector('code');
    if (!code) return;
    var btn = document.createElement('button');
    btn.type = 'button';
    btn.className = 'code-copy';
    btn.textContent = 'Copy';
    btn.setAttribute('aria-label', 'Copy code to clipboard');
    btn.addEventListener('click', function () {
      var text = code.textContent;
      var was = btn.textContent;
      var flash = function (msg) {
        btn.textContent = msg;
        setTimeout(function () { btn.textContent = was; }, 1600);
      };
      if (navigator.clipboard) {
        navigator.clipboard.writeText(text).then(function () { flash('Copied ✓'); }, function () { flash('Select + copy'); });
      } else {
        flash('Select + copy');
      }
    });
    pre.insertBefore(btn, pre.firstChild);
  });
})();
</script>
<script>
(function () {
  // Mobile nav: HARDBRUT's .navbar-toggle/.navbar-links CSS contract is a
  // hidden toggle button + an .open class below 640px; this is the one line
  // of glue JS the framework leaves to the page.
  var toggle = document.querySelector('.navbar-toggle');
  var links = document.getElementById('nav-links');
  if (!toggle || !links) return;
  toggle.addEventListener('click', function () {
    var open = links.classList.toggle('open');
    toggle.setAttribute('aria-expanded', open ? 'true' : 'false');
  });
})();
</script>
</body>
</html>`;
}

marked.setOptions({ gfm: true, breaks: false });

// dst -> set of heading ids on that page, filled as we render and consumed by
// checkLinks once every page exists.
const anchors = new Map();

for (const [src, dst, label] of pages) {
  const abs = path.join(root, src);
  if (!fs.existsSync(abs)) {
    console.warn(`skip missing ${src}`);
    continue;
  }
  const md = fs.readFileSync(abs, 'utf8');
  // The front page's title is the promise, not the product name: it is what shows
  // in a search result and a browser tab, where "SPORE" alone means nothing to
  // someone who has not met it yet.
  const title = titles.get(dst) || (label ? `SPORE — ${label}` : 'SPORE');
  let html = rewriteLinks(marked.parse(md), dst);
  if (dst === 'index.html') {
    // At the foot of the page. Asking someone to pass it on before they know what
    // it is gets nothing; after the honest section it is a reasonable request.
    html += shareBar();
  }
  const [anchored, ids] = anchorHeadings(html);
  anchors.set(dst, ids);
  fs.writeFileSync(path.join(out, dst), page(title, anchored, dst));
  console.log(`rendered ${src} -> _site/${dst}`);
}

// Ship docs images. README (index.html, at the site root) references them as
// `docs/<img>`, while docs pages (also at the root) reference them bare, so copy
// each image to both `_site/` and `_site/docs/`.
const docsImgs = fs.readdirSync(path.join(root, 'docs')).filter((f) => /\.(png|jpe?g|svg|gif|webp)$/i.test(f));
fs.mkdirSync(path.join(out, 'docs'), { recursive: true });
for (const img of docsImgs) {
  const bytes = fs.readFileSync(path.join(root, 'docs', img));
  fs.writeFileSync(path.join(out, img), bytes);
  fs.writeFileSync(path.join(out, 'docs', img), bytes);
  console.log(`copied docs/${img} -> _site/${img} and _site/docs/${img}`);
}
// ---------------------------------------------------------------------------
// Link check.
//
// Every internal href must resolve to a file in `_site` and, if it carries an
// anchor, to a heading that exists on the target page. This is a build step
// rather than a separate script because the failure it catches is silent: a
// renamed heading or a doc that stops being a page leaves a link that still looks
// fine in the markdown and lands nowhere on the site. Being wrong here is
// cheap for us and expensive for the reader.
// ---------------------------------------------------------------------------

// Added after this script runs, by the Pages workflow (the standalone node and
// the Seed Sheet), so they are not on disk when the check runs.
const PROVIDED_LATER = new Set(['demo/', 'spore-seedsheet.html']);

function checkLinks() {
  const problems = [];
  for (const [, dst] of pages) {
    const file = path.join(out, dst);
    if (!fs.existsSync(file)) continue;
    const html = fs.readFileSync(file, 'utf8');
    for (const [, href] of html.matchAll(/href="([^"]+)"/g)) {
      if (/^(https?:|mailto:|data:)/.test(href)) continue;
      const [target, anchor] = href.split('#');
      const on = target === '' ? dst : target;
      if (target !== '' && !PROVIDED_LATER.has(target)) {
        if (!fs.existsSync(path.join(out, target))) {
          problems.push(`${dst}: -> ${href} (no such file in _site)`);
          continue;
        }
      }
      // An anchor into a page the workflow adds later cannot be checked here.
      if (!anchor || PROVIDED_LATER.has(on)) continue;
      const ids = anchors.get(on);
      if (!ids) continue; // an asset, not a rendered page
      if (!ids.has(anchor)) problems.push(`${dst}: -> ${href} (no heading "#${anchor}" on ${on})`);
    }
  }
  if (problems.length) {
    console.error(`\nBROKEN LINKS (${problems.length}):`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  console.log('links OK — every internal href and anchor resolves');
}

checkLinks();

console.log('done. build the standalone into _site/demo/index.html to finish.');
