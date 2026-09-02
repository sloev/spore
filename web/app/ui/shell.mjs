// The application shell (M10-D): rail, sidebar, main pane, app bar, bottom nav.
//
// One route tree, and the shell decides how many panes a breakpoint shows —
// desktop renders list and detail side by side, phone renders one at a time and
// `data-pane` says which. That is why back is a real navigation on mobile rather
// than app-managed slide state.
//
// Pure render functions: everything comes in as a view model, every interaction
// goes out as a callback. No screen reads the client.

import { el, icon } from './dom.mjs';
import { ICONS } from './icons.mjs';

/**
 * The five destinations, in nav order. This list is the whole top-level IA —
 * a screen that is not here is not reachable from the nav by design.
 */
export const DESTINATIONS = [
  { id: 'contacts', label: 'Contacts', icon: ICONS.users },
  { id: 'chat', label: 'Chats', icon: ICONS.messageCircle },
  { id: 'blogs', label: 'Blogs', icon: ICONS.article },
  { id: 'files', label: 'Files', icon: ICONS.folder },
  { id: 'settings', label: 'Settings', icon: ICONS.settings },
];

/**
 * Chats and blogs are two-pane (a list beside a thread); the rest render only
 * into main. On a phone main is hidden while the pane is 'list', so a
 * single-pane screen has to open straight to 'detail' or it shows nothing.
 */
export function defaultPaneFor(screen) {
  return screen === 'chat' || screen === 'blogs' ? 'list' : 'detail';
}

/** Whether the app bar shows a back affordance for this screen on mobile. */
export function canGoBack(screen) {
  return ['chat', 'blogs', 'files'].includes(screen);
}

/**
 * The top app bar. `onBack` is rendered only when there is somewhere to go —
 * a back button that does nothing is worse than no back button.
 */
export function appBar({ title, subtitle, onBack = null, actions = [] }) {
  return el('header', { class: 'app-bar' },
    onBack && el('button', {
      class: 'btn btn-icon btn-tertiary app-bar-back',
      'aria-label': 'Back',
      onclick: onBack,
    }, icon(ICONS.swap, { size: '18px' })),
    el('div', { class: 'app-bar-title' },
      el('b', {}, title || ''),
      subtitle ? el('span', {}, subtitle) : null,
    ),
    el('div', { class: 'app-bar-actions' }, actions),
  );
}

/**
 * Phone and small-tablet navigation. The label is present for screen readers
 * but visually hidden — the icon carries it at this size, and every item still
 * has an accessible name.
 */
export function bottomNav({ screen, onNavigate }) {
  return el('nav', { class: 'bottom-nav', 'aria-label': 'Primary' },
    DESTINATIONS.map((d) => el('button', {
      class: 'bottom-nav-item',
      type: 'button',
      'aria-current': d.id === screen ? 'page' : null,
      onclick: () => onNavigate(d.id),
    },
      icon(d.icon, { size: 'var(--icon-lg)' }),
      el('span', { class: 'sr-only' }, d.label),
    )),
  );
}

/** Desktop vertical rail. Hidden below 1180px, where these move to bottom nav. */
export function navRail({ screen, onNavigate }) {
  return el('nav', { class: 'nav-rail', 'aria-label': 'Primary' },
    // The wordmark, not a monogram. HARDBRUT/3 suggests a single letter in the
    // 88px rail, but SPORE's own rule is absolute: the brand is the word SPORE
    // on every surface, and nothing stands in for it — "not even a monogram".
    // It is set small rather than abbreviated.
    el('div', { class: 'nav-rail-brand', title: 'SPORE' }, 'SPORE'),
    DESTINATIONS.map((d) => el('button', {
      class: 'nav-rail-item',
      type: 'button',
      'aria-current': d.id === screen ? 'page' : null,
      onclick: () => onNavigate(d.id),
    },
      icon(d.icon, { size: 'var(--icon-lg)' }),
      el('span', {}, d.label),
    )),
    el('div', { class: 'nav-rail-spacer' }),
  );
}

/**
 * Assemble the shell. `side` is optional — screens without a list pane pass
 * null, and the app-level CSS collapses the grid to rail + main.
 */
export function appShell({ screen, pane, side, main, onNavigate }) {
  return el('div', { style: { display: 'flex', flexDirection: 'column', height: '100dvh' } },
    el('div', { class: 'app-shell', 'data-pane': pane, style: { flex: '1 1 auto', minHeight: '0' } },
      el('div', { class: 'app-shell-rail' }, navRail({ screen, onNavigate })),
      side ? el('div', { class: 'app-shell-side' }, side) : null,
      el('div', { class: 'app-shell-main' }, main),
    ),
    bottomNav({ screen, onNavigate }),
  );
}
