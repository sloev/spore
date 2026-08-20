// Build-time HARDBRUT import (Milestone 7).
//
// Resolves the canonical `hardbrut.css` from a local checkout first (fast dev
// loop — edit /home/nihil/Developer/css/hardbrut/hardbrut.css and rebuild), then
// falls back to the pinned remote ref. The returned CSS is inlined into the
// built artefact, so the standalone web node still makes ZERO external requests
// at runtime.
//
// The Google-Fonts `@import` that HARDBRUT ships is stripped so the standalone
// stays zero-request; `--font` degrades to the system stack (Inter is the first
// choice in the stack, and the system UI face fills in when Inter is absent).

import fs from 'node:fs';
import https from 'node:https';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const here = path.dirname(fileURLToPath(import.meta.url));

// Pinned source of truth. Bump the ref deliberately when HARDBRUT moves.
export const HARDBRUT = {
  remote: 'https://raw.githubusercontent.com/supernihil/hardbrut',
  ref: 'main',
  file: 'hardbrut.css',
  // Committed vendored copy — CI and reproducible builds read this when no
  // local checkout exists, so the build never depends on a live network fetch.
  vendored: path.join(here, 'vendor/hardbrut/hardbrut.css'),
  // Local checkouts, in priority order. The user's known checkout is first so
  // editing it and rebuilding reflects the change with zero extra steps.
  localPaths: [
    '/home/nihil/Developer/css/hardbrut/hardbrut.css',
  ],
};

// Strip the external font import so the built artefact keeps zero external
// requests; the font stack falls back to the system UI face.
const FONT_IMPORT = /@import\s+url\(['"]?https?:\/\/[^'")]+['"]?\)\s*;/gi;

function fetchRemote(url) {
  return new Promise((resolve, reject) => {
    https.get(url, (res) => {
      if (res.statusCode === 301 || res.statusCode === 302) {
        res.resume();
        fetchRemote(res.headers.location).then(resolve, reject);
        return;
      }
      if (res.statusCode !== 200) {
        reject(new Error('HARDBRUT fetch: HTTP ' + res.statusCode + ' for ' + url));
        return;
      }
      const chunks = [];
      res.on('data', (c) => chunks.push(c));
      res.on('end', () => resolve(Buffer.concat(chunks).toString('utf8')));
    }).on('error', reject);
  });
}

function stripFontImport(css) { return css.replace(FONT_IMPORT, ''); }

// Resolution order: 1) local checkout (fast dev loop)  2) committed vendor copy
// (CI / reproducible)  3) pinned remote (only on explicit demand).
export function requireHardbrutCss() {
  for (const p of HARDBRUT.localPaths) {
    if (fs.existsSync(p)) return stripFontImport(fs.readFileSync(p, 'utf8'));
  }
  if (fs.existsSync(HARDBRUT.vendored)) {
    return stripFontImport(fs.readFileSync(HARDBRUT.vendored, 'utf8'));
  }
  throw new Error(
    'HARDBRUT css not found (no local checkout, no vendored copy). Clone ' +
    'https://github.com/supernihil/hardbrut to /home/nihil/Developer/css/hardbrut ' +
    'or run `node web/hardbrut-sync.mjs` to vendor it.'
  );
}

export async function fetchHardbrutCssRemote() {
  const url = `${HARDBRUT.remote}/${HARDBRUT.ref}/${HARDBRUT.file}`;
  const css = await fetchRemote(url);
  return stripFontImport(css);
}
