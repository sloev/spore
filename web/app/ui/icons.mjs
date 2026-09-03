// The interface icon set (M10-D).
//
// HARDBRUT/3 ships no icons — it substitutes Unicode glyphs in the mono face and
// flags that as an open decision ("replace the glyph set with a real one whose
// stroke weight matches the 2-3px border language; Lucide at stroke-width 2.5 is
// the closest match"). The prototype already made that call: these are the exact
// Lucide-style paths it drew, carried over verbatim as markup instead of React
// calls.
//
// They inherit currentColor and scale with font-size, so a button's icon is the
// button's colour without a second rule. Every icon-only control still needs its
// own aria-label — an icon is never the only label for a destructive action.

const svg = (paths) =>
  '<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24"' +
  ' fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"' +
  ' stroke-linejoin="round" style="display:block">' + paths + '</svg>';

const path = (d) => '<path d="' + d + '"/>';
const circle = (cx, cy, r) => '<circle cx="' + cx + '" cy="' + cy + '" r="' + r + '"/>';

export const ICONS = {
  plus: svg(path('M12 5v14') + path('M5 12h14')),
  x: svg(path('M18 6 6 18') + path('M6 6l12 12')),
  dots: svg(circle(5, 12, 1) + circle(12, 12, 1) + circle(19, 12, 1)),
  settings: svg(
    circle(12, 12, 3) +
    path('M12 2v4M12 18v4M4.2 4.2l2.8 2.8M17 17l2.8 2.8M2 12h4M18 12h4M4.2 19.8l2.8-2.8M17 7l2.8-2.8'),
  ),
  users: svg(
    circle(9, 7, 3) +
    path('M3 21v-2a4 4 0 0 1 4-4h4a4 4 0 0 1 4 4v2') +
    circle(17, 8, 2.2) +
    path('M16 21v-1.4a3.8 3.8 0 0 0-2.2-3.4'),
  ),
  messageCircle: svg(path('M21 11.5a8.5 8.5 0 0 1-8.5 8.5 8.4 8.4 0 0 1-3.8-.9L3 21l1.9-5.7a8.4 8.4 0 0 1-.9-3.8 8.5 8.5 0 1 1 17 0z')),
  article: svg('<rect x="4" y="4" width="16" height="16" rx="1"/>' + path('M7 9h10M7 13h10M7 17h6')),
  folder: svg(path('M3 7a2 2 0 0 1 2-2h3.5l2 2H19a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V7z')),
  share2: svg(circle(18, 5, 2) + circle(6, 12, 2) + circle(18, 19, 2) + path('M8.7 10.7l6.6-3.4M8.7 13.3l6.6 3.4')),
  trash: svg(path('M4 7h16M10 11v6M14 11v6M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12M9 7V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v3')),
  user: svg(circle(12, 8, 3.2) + path('M5 20c0-3.9 3.1-6 7-6s7 2.1 7 6')),
  check: svg(path('M5 12.5l4.5 4.5L19 7')),
  refresh: svg(path('M4 12a8 8 0 0 1 13.7-5.7L20 8M4 12a8 8 0 0 0 13.7 5.7L20 16M20 8v-4M20 8h-4M20 16v4M20 16h-4')),
  ban: svg(circle(12, 12, 8) + path('M6.3 6.3l11.4 11.4')),
  swap: svg(path('M4 9h13l-3-3M20 15H7l3 3')),
  chevronLeft: svg(path('M15 5l-7 7 7 7')),
  chevronRight: svg(path('M9 5l7 7-7 7')),
  help: svg(circle(12, 12, 9) + path('M9.5 9a2.5 2.5 0 1 1 3.5 2.3c-1 .5-1 1.2-1 2.2') + path('M12 17v.01')),
};

/**
 * How a conversation's key state is shown. The accent is decorative and must
 * never carry meaning, so each state pairs a status fill with its own glyph and
 * a word — colour is never the only signal.
 *
 * These map onto what the kernel can actually answer: `canSealTo()` is false
 * until a peer's ANNOUNCE has been heard (cleartext), true once there is a
 * prekey to seal to (sealed), and the RATCHET flag distinguishes a live §7
 * session (ratchet).
 */
export const KEYSTATE = {
  sealed: { icon: ICONS.check, label: 'SEALED', badge: 'badge-success' },
  ratchet: { icon: ICONS.refresh, label: 'RATCHET', badge: 'badge-info' },
  cleartext: { icon: ICONS.ban, label: 'CLEARTEXT', badge: 'badge-quiet' },
};
