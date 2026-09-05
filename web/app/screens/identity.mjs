// Identity (M10-D IA) — who this node is, and the seed that is the only copy.
//
// A top-level destination in the design, and it earns one: the address is what
// every other screen is *about*, and the seed is the single piece of data in the
// whole app that cannot be recovered from anywhere else.
//
// **On multiple identities.** The design switches between several, and this
// screen shows one. That is not an omission in the UI — it is where the kernel
// currently stops. A node *is* its seed: `spore.newNode(seed)` builds one node,
// `SporeClient` owns one node, and every store here (threads, contacts, topics,
// custody) is keyed by app rather than by identity. Switching identities means
// either several live nodes with namespaced stores, or a teardown and rebuild
// per switch, and the choice between them belongs with the six domain stores in
// M10-B rather than being improvised behind a nav item. So the switcher shows
// the identity that exists and says plainly that there is one — rather than
// offering a control that would either lie or lose data.
//
// Pure render functions of a view model. No client, no store access.

import { el } from '../ui/dom.mjs';
import { formatAddr } from '../ui/format.mjs';
import { settingsGroup, settingsRow } from '../ui/settings-list.mjs';

/**
 * @param {Object} vm
 * @param {string} vm.addrHex
 * @param {string} vm.petname
 * @param {boolean} vm.seedShown
 * @param {string|null} vm.seedHex   only while revealed
 * @param {Object} vm.actions
 */
export function renderIdentity(vm) {
  const { addrHex, petname, seedShown, seedHex, actions } = vm;

  return el('div', { class: 'pane-body scroll-y', style: { padding: 'var(--pad)' } },
    el('div', { class: 'settings' },

      settingsGroup('This node',
        settingsRow({
          title: 'Address',
          description: formatAddr(addrHex),
          control: el('button', {
            class: 'btn btn-sm btn-secondary', type: 'button',
            onclick: () => actions.copyAddress(addrHex),
          }, 'Copy'),
        }),
        settingsRow({
          title: 'Name',
          description: 'What you announce. Anyone may announce any name, so a peer sees this as a claim and keeps their own label for you.',
          control: el('input', {
            type: 'text', value: petname, style: { maxWidth: '160px' },
            'aria-label': 'Your announced name',
            onchange: (e) => actions.setPetname(e.target.value),
          }),
        }),
      ),

      settingsGroup('Seed',
        seedShown
          ? el('div', { class: 'settings-row', style: { flexDirection: 'column', alignItems: 'stretch', gap: 'var(--gap-tight)' } },
              el('div', { class: 'alert alert-danger', role: 'alert' },
                el('span', {},
                  el('strong', {}, 'Anyone with these 32 bytes is this node. '),
                  'Not a copy of it — it. They can read what is sealed to you and sign as you, and there is no revocation.')),
              el('code', { class: 'mono', style: { wordBreak: 'break-all', display: 'block', padding: 'var(--pad-tight)', background: 'var(--sunken)', border: 'var(--border-thin)' } },
                seedHex || ''),
              el('div', { class: 'cluster-tight' },
                el('button', { class: 'btn btn-secondary btn-sm', type: 'button', onclick: actions.hideSeed }, 'Hide'),
                el('button', { class: 'btn btn-sm', type: 'button', onclick: () => actions.copySeed(seedHex) }, 'Copy'),
              ))
          : settingsRow({
              title: 'Show the seed',
              description: 'The only thing that can ever be this address again. It is not written down anywhere else.',
              control: el('button', { class: 'btn btn-sm btn-secondary', type: 'button', onclick: actions.showSeed }, 'Reveal'),
            }),
      ),

      // Stated rather than left as a missing feature the user has to notice.
      settingsGroup('Other identities',
        settingsRow({
          title: 'One identity on this device',
          description: 'A node is its seed, and this tab runs one node. Holding several at once needs each conversation, contact and file store to be keyed by identity — that work is Milestone 10-B, and until it lands a second identity means a second browser profile.',
        }),
      ),
    ),
  );
}

/**
 * Contents of the identity sheet — the nav item opens this rather than
 * navigating, because switching who you are should not lose where you were.
 */
export function identitySheet({ addrHex, petname, actions }) {
  return el('div', {},
    el('h2', { class: 'dialog-title', style: { marginBottom: 'var(--gap)' } }, 'Identity'),
    el('div', { class: 'list' },
      el('div', { class: 'list-row', style: { cursor: 'default' } },
        el('span', { class: 'list-row-body' },
          el('span', { class: 'list-row-title' }, petname || 'This node'),
          el('span', { class: 'list-row-subtitle mono' }, formatAddr(addrHex)),
        ),
        el('span', { class: 'list-row-meta' },
          el('span', { class: 'badge badge-success mono text-xs' }, 'ACTIVE')),
      ),
    ),
    el('p', { class: 'text-xs text-muted', style: { marginTop: 'var(--gap-tight)' } },
      'One node per tab. A second identity needs a second browser profile until M10-B keys the stores by identity.'),
    el('div', { class: 'dialog-actions' },
      el('button', { class: 'btn btn-secondary', type: 'button', onclick: actions.closeIdentitySheet }, 'Close'),
      el('button', { class: 'btn', type: 'button', onclick: actions.manageIdentity }, 'Manage'),
    ),
  );
}
