// First-run flow (M10-D): generate an identity, back up its seed, add a bridge.
//
// Three steps, and the middle one is the only place the seed is ever shown.
// Everything below onboarding is unreachable until an identity exists — that is
// enforced in main.mjs's guard, not by hiding buttons here.
//
// Two deliberate departures from the prototype, both because the kernel cannot
// honestly back the prototype's version:
//
//   1. The prototype shows a seed *phrase*. The kernel's seed is 32 raw bytes
//      and there is no mnemonic wordlist anywhere in the crate, so words would
//      have to be invented — and a backup that cannot be restored is worse than
//      no backup. The real 64-hex seed is shown instead, which `newNode(seed)`
//      genuinely accepts back.
//   2. The prototype's "Skip demo" control is scaffolding for the design demo
//      (its sibling is a settings row labelled "Demo — restart the first-run
//      flow"). A shipped product has no demo to skip. Skipping the *bridge* step
//      is real and kept, as "Do this later".

import { el } from '../ui/dom.mjs';
import { formatAddr } from '../ui/format.mjs';

const STEP_COUNT = 3;

function stepBars(step) {
  return el('div', { style: { display: 'flex', gap: 'var(--space-2)' } },
    // Flat 4px fills, no border: at this height a 2px stroke swallows the bar
    // and every step reads as complete. Pending uses an ink mix rather than
    // --sunken, which is too close to --bg to register as a track.
    Array.from({ length: STEP_COUNT }, (_, i) => el('div', {
      style: {
        height: '4px',
        flex: '1',
        background: i <= step ? 'var(--accent)' : 'color-mix(in srgb, var(--ink) 20%, var(--bg))',
      },
    })),
  );
}

function card(...children) {
  return el('div', { class: 'card' },
    el('div', { class: 'card-body', style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap)' } }, ...children));
}

/**
 * @param {Object} vm
 * @param {number} vm.step            0, 1 or 2
 * @param {string|null} vm.addrHex    present once the identity exists
 * @param {string|null} vm.seedHex    shown on step 1 only
 * @param {boolean} vm.seedConfirmed
 * @param {string} vm.relayUrl
 * @param {boolean} vm.busy
 * @param {string|null} vm.error
 * @param {Object} vm.actions
 */
export function renderOnboarding(vm) {
  const { step, addrHex, seedHex, seedConfirmed, relayUrl, busy, error, actions } = vm;

  return el('div', {
    style: {
      background: 'var(--bg)', height: '100dvh', overflowY: 'auto', display: 'flex',
      flexDirection: 'column', alignItems: 'center', padding: 'var(--section) var(--pad)',
    },
  },
    el('div', {
      style: {
        maxWidth: '480px', width: '100%', display: 'flex', flexDirection: 'column',
        gap: 'var(--gap)', marginTop: 'var(--space-8)',
      },
    },
      el('div', { style: { display: 'flex', alignItems: 'center', justifyContent: 'space-between' } },
        el('span', {
          style: {
            display: 'inline-block', background: 'var(--accent)', color: 'var(--accent-ink)',
            border: 'var(--border)', boxShadow: 'var(--e2)', padding: 'var(--space-1) var(--space-3)',
            fontFamily: 'var(--font-display)', textTransform: 'uppercase', letterSpacing: '0.06em',
          },
        }, 'SPORE'),
      ),
      stepBars(step),
      error ? el('div', { class: 'alert alert-danger', role: 'alert' },
        el('p', { class: 'text-sm', style: { marginBottom: '0' } }, error)) : null,

      step === 0 ? stepIdentity({ addrHex, busy, actions }) : null,
      step === 1 ? stepBackup({ seedHex, seedConfirmed, actions }) : null,
      step === 2 ? stepBridge({ relayUrl, busy, actions }) : null,
    ),
  );
}

function stepIdentity({ addrHex, busy, actions }) {
  return card(
    el('span', { class: 'text-uppercase text-sm mono' }, 'Step 1 of 3'),
    el('h1', { style: { margin: '0' } }, 'This tab is your node'),
    el('p', { class: 'text-muted', style: { margin: '0' } },
      'No account, no server. A 32-byte seed generated right here becomes your identity — a 16-digit address derived from it, nothing more.'),
    el('div', {
      style: {
        background: 'var(--sunken)', border: 'var(--border)', padding: 'var(--pad-tight)',
        display: 'flex', flexDirection: 'column', gap: 'var(--space-1)',
      },
    },
      el('span', { class: 'text-xs mono text-uppercase text-muted' }, 'Your address'),
      el('span', { class: 'mono', style: { fontSize: 'var(--text-h4)' } },
        addrHex ? formatAddr(addrHex) : el('span', { class: 'text-muted' }, 'Not generated yet')),
    ),
    addrHex
      ? el('button', { class: 'btn', type: 'button', onclick: actions.next }, 'Continue')
      : el('button', { class: 'btn', type: 'button', disabled: busy, onclick: actions.generate },
          busy ? 'Generating…' : 'Generate identity'),
  );
}

function stepBackup({ seedHex, seedConfirmed, actions }) {
  return card(
    el('span', { class: 'text-uppercase text-sm mono' }, 'Step 2 of 3'),
    el('h1', { style: { margin: '0' } }, 'Back up your seed'),
    el('p', { class: 'text-muted', style: { margin: '0' } },
      'This is the only copy. Losing it loses the identity — there is no recovery, no support desk, no reset link.'),
    el('div', { class: 'alert alert-danger' },
      el('p', { class: 'text-sm', style: { marginBottom: '0' } },
        'Write it down or store it in a password manager. This cannot be shown again after you continue.')),
    el('div', { style: { background: 'var(--sunken)', border: 'var(--border)', padding: 'var(--pad-tight)' } },
      el('span', { class: 'mono text-sm', style: { wordBreak: 'break-all', lineHeight: '1.8' } }, seedHex || '')),
    el('button', {
      class: 'btn btn-secondary btn-sm', type: 'button',
      onclick: () => actions.copySeed(seedHex),
    }, 'Copy seed'),
    el('label', { class: 'choice' },
      el('input', {
        type: 'checkbox',
        checked: seedConfirmed,
        onchange: actions.toggleSeedConfirmed,
      }),
      el('span', { class: 'text-sm' }, 'I have backed this up somewhere safe.'),
    ),
    el('button', { class: 'btn', type: 'button', disabled: !seedConfirmed, onclick: actions.next }, 'Continue'),
  );
}

function stepBridge({ relayUrl, busy, actions }) {
  return card(
    el('span', { class: 'text-uppercase text-sm mono' }, 'Step 3 of 3'),
    el('h1', { style: { margin: '0' } }, 'Add your first bridge'),
    el('p', { class: 'text-muted', style: { margin: '0' } },
      "Without a bridge this node can't reach anyone. A relay is the easiest start — add more, or different kinds, any time from Settings."),
    el('div', { class: 'field' },
      el('label', { for: 'ob-relay' }, 'Relay URL'),
      el('input', {
        id: 'ob-relay', type: 'url', value: relayUrl,
        placeholder: 'wss://relay.spore.example',
        oninput: (e) => actions.setRelayUrl(e.target.value),
      }),
      el('div', { class: 'field-hint' }, 'wss:// address of a public or private relay'),
    ),
    el('div', { style: { display: 'flex', gap: 'var(--gap-tight)' } },
      el('button', { class: 'btn btn-secondary', type: 'button', onclick: actions.finishWithoutBridge }, 'Do this later'),
      el('button', {
        class: 'btn', type: 'button',
        disabled: busy || !relayUrl.trim(),
        onclick: actions.connectAndFinish,
      }, busy ? 'Connecting…' : 'Connect & finish'),
    ),
  );
}
