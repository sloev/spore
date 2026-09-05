// Formatting and shell-routing rules (M10-D). Pure logic, no DOM, so it runs in
// plain Node like every other suite here.
//
//   node web/app/ui/format.test.mjs

import assert from 'node:assert';
import { formatAddr, truncate, formatBytes, clock, dayLabel, shortWhen, fileKind } from './format.mjs';
import { defaultPaneFor, canGoBack, navValueFor, DESTINATIONS } from './shell.mjs';

let failures = 0;
function test(name, fn) {
  try { fn(); console.log('  ok  ' + name); }
  catch (err) { failures++; console.log('FAIL  ' + name + '\n      ' + (err && err.message)); }
}

// ------------------------------------------------------------------ addresses

test('an address is grouped in fours and upper-cased', () => {
  assert.strictEqual(formatAddr('3f2a9c1088e4001b'), '3F2A · 9C10 · 88E4 · 001B');
});

test('formatAddr tolerates an absent address rather than throwing', () => {
  assert.strictEqual(formatAddr(null), '');
  assert.strictEqual(formatAddr(''), '');
});

// ---------------------------------------------------------------------- sizes

test('sizes carry a real space before the unit', () => {
  assert.strictEqual(formatBytes(512), '512 B');
  assert.strictEqual(formatBytes(2517), '2.5 KB');
  assert.strictEqual(formatBytes(2.4 * 1024 * 1024), '2.4 MB');
});

test('sizes drop the decimal once they are big enough not to need it', () => {
  assert.strictEqual(formatBytes(64 * 1024 * 1024), '64 MB');
});

test('a negative or non-finite size formats to nothing, not to NaN', () => {
  assert.strictEqual(formatBytes(-1), '');
  assert.strictEqual(formatBytes(NaN), '');
});

// ----------------------------------------------------------------------- time

test('the clock is always two digits and local', () => {
  const d = new Date(2026, 2, 15, 9, 4);
  assert.strictEqual(clock(d.getTime()), '09:04');
});

test('dates are relative for two days then absolute', () => {
  const now = new Date(2026, 2, 15, 12, 0).getTime();
  const at = (dayOffset) => new Date(2026, 2, 15 - dayOffset, 10, 0).getTime();
  assert.strictEqual(dayLabel(at(0), now), 'Today');
  assert.strictEqual(dayLabel(at(1), now), 'Yesterday');
  assert.strictEqual(dayLabel(at(2), now), '13 Mar');
});

test('a list row shows a clock today and a day count after that', () => {
  const now = new Date(2026, 2, 15, 12, 0).getTime();
  assert.strictEqual(shortWhen(new Date(2026, 2, 15, 9, 41).getTime(), now), '09:41');
  assert.strictEqual(shortWhen(new Date(2026, 2, 14, 9, 41).getTime(), now), '1d');
  assert.strictEqual(shortWhen(new Date(2026, 2, 12, 9, 41).getTime(), now), '3d');
});

// ---------------------------------------------------------------------- misc

test('truncate only cuts when it must', () => {
  assert.strictEqual(truncate('short', 10), 'short');
  assert.strictEqual(truncate('a much longer string', 10), 'a much lo…');
});

test('a file kind is the extension, upper-case, never longer than four', () => {
  assert.strictEqual(fileKind('audit.pdf'), 'PDF');
  assert.strictEqual(fileKind('archive.tar.gz'), 'GZ');
  assert.strictEqual(fileKind('field-recordings.jpeg'), 'JPEG');
  assert.strictEqual(fileKind('README'), 'BIN', 'no extension is BIN, not empty');
  assert.strictEqual(fileKind('trailing.'), 'BIN');
});

// ------------------------------------------------------------- shell routing

test('the six destinations match the design, in its order', () => {
  const ids = DESTINATIONS.map((d) => d.id);
  assert.deepStrictEqual(ids, ['contacts', 'chat', 'blogs', 'files', 'identity', 'settings']);
  assert.strictEqual(new Set(ids).size, 6);
  // The design's own labels, which are not the screen ids: a rename here is a
  // change to the product's vocabulary, not a tidy-up.
  assert.deepStrictEqual(DESTINATIONS.map((d) => d.label),
    ['Contacts', 'Chat', 'Blogs', 'Fileshare', 'Identity', 'Settings']);
});

test('every destination has a label and an icon', () => {
  for (const d of DESTINATIONS) {
    assert.ok(d.label, d.id + ' needs a label');
    assert.ok(d.icon && d.icon.startsWith('<svg'), d.id + ' needs an icon');
  }
});

test('bridges keeps Settings lit, because that is where you came from', () => {
  // Bridges is a screen with no nav item. Without this the nav goes blank in a
  // place the user reached from Settings, which reads as "you have left the app".
  assert.strictEqual(navValueFor('bridges'), 'settings');
  assert.strictEqual(navValueFor('files'), 'files');
});

test('the identity sheet is what is current while it is open', () => {
  // Identity is a nav item that opens a sheet instead of navigating, so the
  // screen underneath is unchanged and cannot be what reads as current.
  assert.strictEqual(navValueFor('chat', true), 'identity');
  assert.strictEqual(navValueFor('chat', false), 'chat');
});

test('two-pane screens open to the list, one-pane screens to the detail', () => {
  // On a phone the main pane is hidden while pane is 'list', so a screen with
  // no sidebar must open to 'detail' or it renders nothing at all.
  assert.strictEqual(defaultPaneFor('chat'), 'list');
  assert.strictEqual(defaultPaneFor('blogs'), 'list');
  for (const one of ['contacts', 'files', 'settings', 'bridges', 'identity']) {
    assert.strictEqual(defaultPaneFor(one), 'detail', one + ' would be blank on a phone otherwise');
  }
});

test('back is offered only where there is somewhere to go back to', () => {
  assert.ok(canGoBack('chat'));
  // Bridges is entered from Settings, so it has a destination on every width —
  // not only where the mobile list/detail flip exists.
  assert.ok(canGoBack('bridges'));
  assert.ok(!canGoBack('settings'), 'settings has no list pane to return to');
  assert.ok(!canGoBack('files'), 'a top-level screen with no list pane has no back');
});

console.log(failures ? '\n' + failures + ' failing' : '\nall passing');
process.exit(failures ? 1 : 0);
