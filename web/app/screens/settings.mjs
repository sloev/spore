// Settings (M10-D): bridges, appearance, data, diagnostics.
//
// This is the screen the standalone lost when the pre-M10 app was deleted, and
// the one that makes the node useful rather than merely correct — without it
// there is no way to attach a transport after onboarding.
//
// Every control here is wired to something real. The design's prototype also had
// a "Replay onboarding" row, described in its own copy as "Demo — restart the
// first-run flow"; that is scaffolding for a design demo, not a product control,
// and it is dropped for the same reason the onboarding "Skip demo" button was.
//
// Pure render functions of a view model. No client, no store access.

import { el } from '../ui/dom.mjs';
import { formatAddr, shortWhen } from '../ui/format.mjs';
import { settingsGroup, settingsRow, DAEMON_ONLY } from '../ui/settings-list.mjs';


const ACCENTS = ['yellow', 'red', 'blue', 'green', 'pink', 'purple'];

/**
 * @param {Object} vm
 * @param {Array}  vm.bridges        from SporeClient.bridges() — for the count only
 * @param {string} vm.accent
 * @param {string} vm.theme          'system' | 'light' | 'dark'
 * @param {boolean} vm.keepHistory
 * @param {number|null} vm.lastAnnounceAt
 * @param {string} vm.addrHex
 * @param {boolean} vm.confirmingWipe
 * @param {Object} vm.actions
 */
export function renderSettings(vm) {
  const { bridges, accent, theme, keepHistory,
          lastAnnounceAt, addrHex, confirmingWipe, actions } = vm;

  const up = bridges.filter((b) => b.up).length;
  const bridgeSummary = bridges.length === 0
    ? 'None. This node can reach nobody until one is attached.'
    : `${up} of ${bridges.length} up.`;

  return el('div', { class: 'pane-body scroll-y', style: { padding: 'var(--pad)' } },
    el('div', { class: 'settings' },

      // ---- Bridges: a row that goes to the screen, not the screen inlined.
      // A node's links decide whether it can reach anyone at all, which is too
      // central to be a group inside a preferences page.
      settingsGroup('Network',
        settingsRow({
          title: 'Bridges',
          description: bridgeSummary,
          nav: true,
          onClick: actions.openBridges,
        }),
      ),

      // ---- Appearance
      settingsGroup('Appearance',
        settingsRow({
          title: 'Accent',
          description: 'Six values, swaps instantly',
          control: el('span', { class: 'cluster-tight' }, ACCENTS.map((a) => el('button', {
            class: 'btn btn-sm', type: 'button',
            'aria-label': a, 'aria-pressed': a === accent ? 'true' : 'false',
            style: {
              background: `var(--${a})`, width: '28px', minHeight: '28px', padding: '0',
              boxShadow: a === accent ? 'var(--e2)' : 'var(--e0)',
              borderWidth: a === accent ? 'var(--bw-heavy)' : 'var(--bw-thin)',
            },
            onclick: () => actions.setAccent(a),
          }))),
        }),
        settingsRow({
          title: 'Theme',
          description: 'Follows the system until you choose',
          control: el('div', { class: 'segmented', role: 'group', 'aria-label': 'Theme' },
            ['system', 'light', 'dark'].map((t) => el('button', {
              class: 'segmented-item', type: 'button',
              'aria-pressed': t === theme ? 'true' : 'false',
              onclick: () => actions.setTheme(t),
            }, t))),
        }),
      ),

      // ---- Data
      settingsGroup('Data',
        settingsRow({
          title: 'Kept on this device',
          description: 'Seed, prekey ring, contacts and conversations. Nothing leaves except what you send.',
        }),
        settingsRow({
          title: 'Keep history on this device',
          description: keepHistory
            ? 'Conversations survive a reload.'
            : 'Conversations are held in memory only and are lost on reload.',
          control: el('input', {
            type: 'checkbox', class: 'switch', checked: keepHistory,
            'aria-label': 'Keep history on this device',
            onchange: (e) => actions.setKeepHistory(e.target.checked),
          }),
        }),
        confirmingWipe
          ? el('div', { class: 'settings-row', style: { flexDirection: 'column', alignItems: 'stretch', gap: 'var(--gap-tight)' } },
              el('div', { class: 'alert alert-danger', role: 'alert' },
                el('span', {},
                  el('strong', {}, 'This deletes the identity itself. '),
                  'The seed is the only thing that can ever be this address again, and it is not written down anywhere else. Anyone holding messages for you will keep sending them into the void.')),
              el('div', { class: 'cluster-tight' },
                el('button', { class: 'btn btn-secondary btn-sm', type: 'button', onclick: actions.cancelWipe }, 'Cancel'),
                el('button', { class: 'btn btn-danger btn-sm', type: 'button', onclick: actions.confirmWipe }, 'Delete this node forever'),
              ))
          : settingsRow({
              title: 'Wipe this tab',
              description: 'Drops the seed and stops this node. Cannot be undone.',
              danger: true,
              onClick: actions.startWipe,
            }),
      ),

      // ---- Diagnostics
      settingsGroup('Diagnostics',
        settingsRow({
          title: 'This node',
          description: formatAddr(addrHex),
          control: el('button', {
            class: 'btn btn-sm btn-secondary', type: 'button',
            onclick: () => actions.copyAddress(addrHex),
          }, 'Copy'),
        }),
        settingsRow({
          title: 'Manual announce',
          description: lastAnnounceAt
            ? `Last announced ${shortWhen(lastAnnounceAt)}. Announcing again restarts the backoff.`
            : 'Not announced yet. A peer cannot seal to you until it has heard one.',
          nav: true,
          onClick: actions.announceNow,
        }),
        settingsRow({
          title: 'Native bridges',
          description: `${DAEMON_ONLY} — available on the desktop daemon only.`,
        }),
      ),
    ),
  );
}
