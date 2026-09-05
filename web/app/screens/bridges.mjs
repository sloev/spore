// Bridges (M10-D IA) — the links this node has, and the ones it could ask for.
//
// Its own screen, reached from Settings, exactly as the design has it. It was
// folded *into* Settings when Settings was the only screen that could hold it;
// a node's links are the thing that decides whether it can reach anyone at all,
// which is too central to be a group inside a preferences page.
//
// Two lists, because they are two different things:
//
//   Bridges  links that exist — up or down, with what has crossed them
//   Devices  transports the browser could open but has not, because opening
//            them needs a user gesture the app cannot fake
//
// The second list is the honest version of "why is Bluetooth not here?" — the
// answer is that a permission prompt has to be triggered by a real click, so the
// row exists and says so rather than the transport silently not appearing.
//
// Pure render functions of a view model. No client, no store access.

import { el, icon } from '../ui/dom.mjs';
import { ICONS } from '../ui/icons.mjs';
import { formatBytes, shortWhen } from '../ui/format.mjs';
import { settingsGroup, settingsRow, DAEMON_ONLY } from '../ui/settings-list.mjs';


/**
 * @param {Object} vm
 * @param {Array}  vm.bridges    from SporeClient.bridges()
 * @param {Array}  vm.available  from SporeClient.availableTransports()
 * @param {Object|null} vm.adding
 * @param {string|null} vm.diagOpen  bridge id whose diagnostics sheet is open
 * @param {Object} vm.actions
 */
export function renderBridges(vm) {
  const { bridges, available, adding, actions } = vm;

  const up = bridges.filter((b) => b.up).length;
  const summary = bridges.length === 0
    ? 'No bridges. This node can reach nobody until one is attached.'
    : `${up} of ${bridges.length} up.`;

  // A transport that needs a gesture and is not already attached is a "device":
  // the browser will only hand it over from inside a real click.
  const attached = new Set(bridges.map((b) => b.kind));
  const devices = available.filter((t) => t.needsGesture && !attached.has(t.kind));

  return el('div', { class: 'pane-body scroll-y', style: { padding: 'var(--pad)' } },
    el('div', { class: 'settings' },

      settingsGroup('Bridges',
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

        ...bridges.map((b) => bridgeRow(b, actions)),

        el('p', { class: 'text-xs text-muted', style: { margin: '0', padding: 'var(--space-2) var(--pad-tight)' } },
          `11 bridges (${DAEMON_ONLY}) need a daemon and never appear here — a browser cannot open them.`),
      ),

      devices.length
        ? settingsGroup('Devices',
            ...devices.map((d) => el('div', { class: 'settings-row' },
              el('span', { class: 'settings-row-body' },
                el('b', {}, d.label),
                el('span', {}, 'Your browser will ask you to pick a device.'),
              ),
              el('span', { class: 'settings-row-control' },
                el('button', {
                  class: 'btn btn-sm btn-secondary', type: 'button',
                  onclick: () => actions.grantDevice(d.kind),
                }, 'Grant access'),
              ),
            )))
        : null,
    ),
  );
}

function bridgeRow(b, actions) {
  return el('button', {
    class: 'settings-row settings-row-nav', type: 'button',
    onclick: () => actions.openDiagnostics(b.id),
  },
    el('span', { class: 'settings-row-body' },
      el('b', {}, b.kind),
      el('span', {}, b.lastFrameAt
        ? `${b.sent} sent · ${b.received} received · last frame ${shortWhen(b.lastFrameAt)}`
        : `${b.sent} sent · ${b.received} received · no frames yet`),
    ),
    el('span', { class: 'settings-row-control' },
      el('span', { class: 'badge ' + (b.up ? 'badge-success' : 'badge-danger') + ' mono text-xs' },
        b.up ? 'UP' : 'DOWN'),
    ),
  );
}

/**
 * The contents of the diagnostics sheet for one bridge. Rendered by the shell's
 * `sheet()`; this is only what goes inside it.
 *
 * Bytes as well as frames: a frame count says whether anything is moving, and
 * bytes say what the link is costing, which is the question this view is opened
 * to answer. A LoRa frame and a WebSocket frame differ by two orders of
 * magnitude, so one number cannot stand for both.
 */
export function bridgeDiagnostics(b, actions) {
  if (!b) return null;
  return el('div', {},
    el('h2', { class: 'dialog-title', style: { marginBottom: 'var(--gap-tight)' } }, b.kind),
    el('p', { class: 'text-sm text-muted', style: { marginTop: '0' } },
      b.up ? 'Connected.' : 'Down. Nothing is crossing this link.'),

    el('div', { class: 'settings' },
      el('div', { class: 'settings-row' },
        el('span', { class: 'settings-row-body' }, el('b', {}, 'In'),
          el('span', {}, `${b.received} frames`)),
        el('span', { class: 'settings-row-control mono' }, formatBytes(b.bytesReceived || 0)),
      ),
      el('div', { class: 'settings-row' },
        el('span', { class: 'settings-row-body' }, el('b', {}, 'Out'),
          el('span', {}, `${b.sent} frames`)),
        el('span', { class: 'settings-row-control mono' }, formatBytes(b.bytesSent || 0)),
      ),
      el('div', { class: 'settings-row' },
        el('span', { class: 'settings-row-body' }, el('b', {}, 'Last frame'),
          el('span', {}, b.lastFrameAt ? shortWhen(b.lastFrameAt) : 'never')),
      ),
    ),

    el('div', { class: 'dialog-actions' },
      el('button', { class: 'btn btn-secondary', type: 'button', onclick: actions.closeDiagnostics }, 'Close'),
      el('button', { class: 'btn btn-danger', type: 'button', onclick: () => actions.removeBridge(b.id) }, 'Remove'),
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
