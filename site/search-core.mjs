// Ranking for the docs search. Pure, dependency-free, and shared: `build.mjs`
// inlines this file into `search.html`, and `search-core.test.mjs` runs it
// directly. One implementation, so the thing that ships is the thing that is
// tested.
//
// No Lunr, no CDN. The audit suggested Lunr; two things ruled it out. The docs
// are a few hundred sections, where a linear scan is instant and an inverted
// index is machinery to maintain. And a CDN script cannot be used at all here —
// the site must work from disk, so the index is inlined rather than fetched.

/** Split a query or a document into lowercase terms. */
export function terms(s) {
  return (s || '')
    .toLowerCase()
    .split(/[^a-z0-9_]+/)
    .filter((t) => t.length > 1);
}

/**
 * Score one entry against the query terms.
 *
 * Every term must appear somewhere, so a two-word query narrows rather than
 * widens — searching "link fragmentation" should not return every page that
 * says "link". A hit in a heading counts for more than one in the body, because
 * a section *about* a thing is what someone searching for it wants.
 *
 * Returns 0 when the entry does not match at all.
 */
export function score(entry, qterms) {
  if (!qterms.length) return 0;
  const title = (entry.t || '').toLowerCase();
  const body = (entry.s || '').toLowerCase();
  let total = 0;
  for (const q of qterms) {
    const inTitle = title.includes(q);
    // Count body occurrences, capped: a page that says "envelope" forty times
    // is not forty times more relevant, and without the cap the spec wins
    // every query by being long.
    let n = 0;
    let i = body.indexOf(q);
    while (i !== -1 && n < 4) {
      n++;
      i = body.indexOf(q, i + q.length);
    }
    if (!inTitle && n === 0) return 0; // every term must appear
    // An exact word match in a title beats a substring of a longer word.
    const exactTitle = inTitle && new RegExp(`\\b${q}\\b`).test(title);
    total += (exactTitle ? 12 : inTitle ? 6 : 0) + n;
  }
  return total;
}

/** Best matches first, at most `limit`. */
export function rank(index, query, limit = 30) {
  const q = terms(query);
  if (!q.length) return [];
  const hits = [];
  for (const e of index) {
    const s = score(e, q);
    if (s > 0) hits.push({ e, s });
  }
  // Ties broken by shorter title, so "Envelope" outranks a section that merely
  // mentions it in a longer name. Stable otherwise, so results do not shuffle
  // between keystrokes.
  hits.sort((a, b) => b.s - a.s || (a.e.t || '').length - (b.e.t || '').length);
  return hits.slice(0, limit).map((h) => h.e);
}
