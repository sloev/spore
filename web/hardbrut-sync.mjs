// Update the committed vendored copy of hardbrut.css from the pinned remote.
//
//   node web/hardbrut-sync.mjs
//
// Pulls supernihil/hardbrut @ pinned ref, strips the Google-Fonts @import, and
// writes web/vendor/hardbrut/hardbrut.css. The build itself never fetches — it
// reads the local checkout first, then this vendored copy — so CI stays
// deterministic and offline.

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { HARDBRUT, fetchHardbrutCssRemote } from './hardbrut-import.mjs';

const here = path.dirname(fileURLToPath(import.meta.url));
const out = path.join(here, 'vendor/hardbrut/hardbrut.css');

const css = await fetchHardbrutCssRemote();
fs.mkdirSync(path.dirname(out), { recursive: true });
fs.writeFileSync(out, css);
console.log('vendored ' + out + ' (' + css.length + ' bytes) from ' +
  HARDBRUT.remote + '/' + HARDBRUT.ref + '/' + HARDBRUT.file);
