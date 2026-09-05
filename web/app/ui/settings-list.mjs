// The settings-list vocabulary, shared by Settings, Bridges and Identity.
//
// HARDBRUT/3's `settings.css` gives one row shape — title, description, and an
// optional control or chevron — and three screens use it. They each had their
// own copy of these two helpers, which is how the standalone build found them:
// it concatenates every module into a single classic-script scope, so a second
// top-level `function row` is not a duplicate, it is a syntax error.
//
// Sharing them is the fix, and the better shape anyway: a settings row that
// looks different on the Bridges screen than on the Settings screen would be a
// bug nobody filed.

import { el } from './dom.mjs';

/** The eleven bridges a browser can never open, named so their absence is a
 *  stated fact rather than a gap the reader has to notice. */
export const DAEMON_ONLY = 'UDP, TCP, folder, HTTP bag, copyparty, SSB, AX.25, Tor, I2P, ICMP and iroh';

/** A titled group of rows. */
export function settingsGroup(title, ...rows) {
  return el('section', { class: 'settings-group' },
    el('h2', { class: 'settings-group-title' }, title),
    ...rows,
  );
}

/**
 * One row. Pass `onClick` to make it a button; `nav: true` adds the chevron
 * that means "this goes somewhere", so a row that navigates never looks the
 * same as a row that only states a fact.
 */
export function settingsRow({ title, description, control = null, onClick = null, nav = false, danger = false }) {
  const body = el('span', { class: 'settings-row-body' },
    el('b', {}, title),
    description ? el('span', {}, description) : null,
  );
  const cls = 'settings-row' + (nav ? ' settings-row-nav' : '') + (danger ? ' settings-row-danger' : '');
  if (onClick) {
    return el('button', { class: cls, type: 'button', onclick: onClick },
      body,
      control ? el('span', { class: 'settings-row-control' }, control) : null);
  }
  return el('div', { class: cls },
    body,
    control ? el('span', { class: 'settings-row-control' }, control) : null);
}
