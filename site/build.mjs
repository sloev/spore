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
  ['docs/APPS.md', 'apps.html', 'Apps'],
  ['docs/DESIGN.md', 'design.html', 'Design'],
  ['docs/BRIDGES.md', 'bridges.html', 'Bridges'],
  ['docs/REBUILD.md', 'rebuild.html', 'Rebuild'],
  ['docs/CONTINUITY.md', 'continuity.html', 'Continuity'],
  // Secondary guides: rendered + linkable, kept off the top nav to reduce clutter.
  // The findings register belongs on the site — anyone evaluating whether to trust
  // this with their mail should be able to read what was found and fixed without
  // cloning the repo — but households do not need it in the top bar.
  ['SECURITY.md', 'security-policy.html', null],
  ['docs/SECURITY_FINDINGS.md', 'security.html', null],
  ['CHANGELOG.md', 'changelog.html', null],
  ['docs/HARDWARE.md', 'hardware.html', null],
  ['bindings/README.md', 'bindings.html', null],
  ['reference/README.md', 'reference.html', null],
  ['web/README.md', 'webguide.html', null],
  ['CONTRIBUTING.md', 'contributing.html', null],
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
  .concat([{ dst: 'demo/', label: 'Web node' }]);

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
        ? `<a class="share-btn" href="${href}" title="${title}" target="_blank" rel="noopener noreferrer">${label}</a>`
        : `<button class="share-btn" type="button" data-share="${label}" title="${title}">${label}</button>`
    )
    .join('');

  return `<section class="share" aria-label="Share SPORE">
  <h2>Pass it on</h2>
  <p>Continuity is just redundancy that outlives its sources — and it only works
  if the copies are already scattered before they're needed.</p>
  <div class="share-row">${buttons}</div>
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
      const active = dst === self ? ' class="active"' : '';
      return `<a href="${dst}"${active}>${label}</a>`;
    })
    .join('');
  return `<nav>${items}</nav>`;
}

function page(title, bodyHtml, self) {
  // A per-page body class, so a page can carry its own rules — the spec needs
  // print styling that would be wrong everywhere else.
  const cls = self.replace(/\.html$/, '');
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>${title}</title>
<link rel="stylesheet" href="style.css" />
</head>
<body class="page-${cls}">
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
  let html = rewriteLinks(marked.parse(md), dst);
  if (dst === 'index.html') {
    // Right after the intro, before the first section — seen without scrolling,
    // without interrupting the opening pitch.
    html = html.replace('<h2', shareBar() + '<h2');
  }
  fs.writeFileSync(path.join(out, dst), page(title, html, dst));
  console.log(`rendered ${src} -> _site/${dst}`);
}

// Ship the stylesheet.
fs.writeFileSync(path.join(out, 'style.css'), fs.readFileSync(path.join(root, 'site/style.css')));
console.log('wrote _site/style.css');

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
console.log('done. build the standalone into _site/demo/index.html to finish.');
