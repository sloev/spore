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

import { el, icon } from '../ui/dom.mjs';
import { ICONS } from '../ui/icons.mjs';
import { formatAddr, shortWhen } from '../ui/format.mjs';

/** The eleven bridges a browser can never open. Named so their absence is a
 *  stated fact rather than a gap the reader has to notice. */
const DAEMON_ONLY = 'UDP, TCP, folder, HTTP bag, copyparty, SSB, AX.25, Tor, I2P, ICMP and iroh';

const ACCENTS = ['yellow', 'red', 'blue', 'green', 'pink', 'purple'];

function group(title, ...rows) {
  return el('section', { class: 'settings-group' },
    el('h2', { class: 'settings-group-title' }, title),
    ...rows,
  );
}

function row({ title, description, control = null, onClick = null, nav = false, danger = false }) {
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

/**
 * @param {Object} vm
 * @param {Array}  vm.bridges        from SporeClient.bridges()
 * @param {Array}  vm.available      from SporeClient.availableTransports()
 * @param {Object|null} vm.adding    { kind, fields, error, busy } while the form is open
 * @param {string} vm.accent
 * @param {string} vm.theme          'system' | 'light' | 'dark'
 * @param {boolean} vm.keepHistory
 * @param {number|null} vm.lastAnnounceAt
 * @param {string} vm.addrHex
 * @param {boolean} vm.confirmingWipe
 * @param {Object} vm.actions
 */
export function renderSettings(vm) {
  const { bridges, available, adding, accent, theme, keepHistory,
          lastAnnounceAt, addrHex, confirmingWipe, actions } = vm;

  const up = bridges.filter((b) => b.up).length;
  const summary = bridges.length === 0
    ? 'No bridges. This node can reach nobody until one is attached.'
    : `${up} of ${bridges.length} up.`;

  return el('div', { class: 'pane-body scroll-y', style: { padding: 'var(--pad)' } },
    el('div', { class: 'settings' },

      // ---- Bridges
      group('Bridges',
        el('div', {
          style: {
            padding: 'var(--space-3) var(--pad-tight)', display: 'flex', alignItems: 'center',
            justifyContent: 'space-between', gap: 'var(--gap-tight)', flexWrap: 'wrap',
            borderBottom: 'var(--border-thin)',
          },
        },
          el('p', { class: 'text-muted text-sm', style: { margin: '0' } }, summary),
          adding ? null : el('button', { class: 'btn btn-sm', type: 'button', onclick: actions.openAddBridge },
            icon(ICONS.plus, { size: '14px' }), 'Add bridge'),
        ),

        adding ? addBridgeForm(vm) : null,

        ...bridges.map((b) => row({
          title: b.kind,
          description: b.lastFrameAt
            ? `${b.sent} sent · ${b.received} received · last frame ${shortWhen(b.lastFrameAt)}`
            : `${b.sent} sent · ${b.received} received · no frames yet`,
          control: el('span', { class: 'cluster-tight' },
            el('span', { class: 'badge ' + (b.up ? 'badge-success' : 'badge-danger') + ' mono text-xs' },
              b.up ? 'UP' : 'DOWN'),
            el('button', {
              class: 'btn btn-sm btn-secondary', type: 'button',
              onclick: () => actions.removeBridge(b.id),
            }, 'Remove'),
          ),
        })),

        // Sentence case, not the uppercase label style: the design system
        // reserves uppercase for labels, buttons and badges, and this is a
        // sentence of running text — the longest on the screen.
        el('p', { class: 'text-xs text-muted', style: { margin: '0', padding: 'var(--space-2) var(--pad-tight)' } },
          `11 bridges (${DAEMON_ONLY}) need a daemon and never appear here — a browser cannot open them.`),
      ),

      // ---- Appearance
      group('Appearance',
        row({
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
        row({
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
      group('Data',
        row({
          title: 'Kept on this device',
          description: 'Seed, prekey ring, contacts and conversations. Nothing leaves except what you send.',
        }),
        row({
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
          : row({
              title: 'Wipe this tab',
              description: 'Drops the seed and stops this node. Cannot be undone.',
              danger: true,
              onClick: actions.startWipe,
            }),
      ),

      // ---- Diagnostics
      group('Diagnostics',
        row({
          title: 'This node',
          description: formatAddr(addrHex),
          control: el('button', {
            class: 'btn btn-sm btn-secondary', type: 'button',
            onclick: () => actions.copyAddress(addrHex),
          }, 'Copy'),
        }),
        row({
          title: 'Manual announce',
          description: lastAnnounceAt
            ? `Last announced ${shortWhen(lastAnnounceAt)}. Announcing again restarts the backoff.`
            : 'Not announced yet. A peer cannot seal to you until it has heard one.',
          nav: true,
          onClick: actions.announceNow,
        }),
        row({
          title: 'Native bridges',
          description: `${DAEMON_ONLY} — available on the desktop daemon only.`,
        }),
      ),
    ),
  );
}

/** The add-bridge form. Only kinds the host can actually open are offered. */
function addBridgeForm({ available, adding, actions }) {
  const spec = available.find((t) => t.kind === adding.kind) || available[0];
  const fields = adding.fields || {};

  return el('div', { class: 'card', style: { margin: 'var(--pad-tight)' } },
    el('div', { class: 'card-body', style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap-tight)' } },

      el('div', { class: 'field' },
        el('label', { for: 'bridge-kind' }, 'Kind'),
        el('select', {
          id: 'bridge-kind',
          onchange: (e) => actions.setAddBridgeKind(e.target.value),
        }, available.map((t) => el('option', {
          value: t.kind, selected: spec && t.kind === spec.kind,
        }, t.label))),
      ),

      // A transport that cannot be built from a config says so instead of
      // offering a button that would fail.
      spec && spec.manual
        ? el('div', { class: 'alert alert-quiet' },
            el('span', { class: 'text-sm' },
              'This one needs an offer and an answer exchanged out of band, so it cannot be opened from a form. Its handshake screen is not built yet.'))
        : null,

      ...(spec && spec.fields ? spec.fields : []).map((f) => el('div', { class: 'field' },
        el('label', { for: 'bf-' + f.k }, f.label),
        el('input', {
          id: 'bf-' + f.k, type: 'text',
          value: fields[f.k] !== undefined ? fields[f.k] : (f.value || ''),
          placeholder: f.hint || '',
          oninput: (e) => actions.setAddBridgeField(f.k, e.target.value),
        }),
        f.hint ? el('div', { class: 'field-hint' }, f.hint) : null,
      )),

      spec && spec.needsGesture
        ? el('p', { class: 'text-xs text-muted', style: { margin: '0' } },
            'Your browser will ask you to pick a device.')
        : null,

      adding.error ? el('div', { class: 'field-error', role: 'alert' }, adding.error) : null,

      el('div', { class: 'cluster-tight' },
        el('button', { class: 'btn btn-secondary btn-sm', type: 'button', onclick: actions.closeAddBridge }, 'Cancel'),
        el('button', {
          class: 'btn btn-sm', type: 'button',
          disabled: adding.busy || (spec && spec.manual),
          onclick: actions.confirmAddBridge,
        }, adding.busy ? 'Connecting…' : 'Attach'),
      ),
    ),
  );
}
