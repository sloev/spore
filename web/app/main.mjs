// Boot and state ownership (M10-D).
//
// This module owns all application state and is the only thing that talks to
// SporeClient. Screens are pure render functions of a view model; they get data
// down and send intent back up as callbacks. That is what makes the M10-B store
// migration invisible later — when a store moves into Rust, only this file
// changes.
//
// The onboarding gate is enforced here, once: nothing below it renders until an
// identity exists. Hiding buttons is not a gate.

import { SporeClient, localStorageAdapter } from './spore-client.mjs';
import { el, mount } from './ui/dom.mjs';
import { appShell, appBar, defaultPaneFor, canGoBack, DESTINATIONS } from './ui/shell.mjs';
import { renderOnboarding } from './screens/onboarding.mjs';
import { formatAddr } from './ui/format.mjs';

const K_ONBOARDED = 'spore.onboarded';

const state = {
  booted: false,
  onboarding: true,
  step: 0,
  seedHex: null,
  seedConfirmed: false,
  relayUrl: '',
  busy: false,
  error: null,

  screen: 'chat',
  pane: 'list',

  identity: null,
  bridges: [],
};

let client = null;
let root = null;

// ------------------------------------------------------------------- helpers

function setState(patch) {
  Object.assign(state, patch);
  render();
}

function fault(message) {
  setState({ error: message, busy: false });
}

/** Never let a rejected promise vanish into the console. */
async function guard(fn) {
  try {
    setState({ busy: true, error: null });
    await fn();
    setState({ busy: false });
  } catch (err) {
    fault(String((err && err.message) || err));
  }
}

// --------------------------------------------------------------- onboarding

const onboardingActions = {
  generate: () => guard(async () => {
    const identity = await client.init(wasmSource());
    setState({ identity, seedHex: client.exportSeed() });
  }),

  next: () => setState({ step: state.step + 1, error: null }),

  toggleSeedConfirmed: () => setState({ seedConfirmed: !state.seedConfirmed }),

  copySeed: (seed) => {
    if (navigator.clipboard) navigator.clipboard.writeText(seed);
  },

  setRelayUrl: (v) => { state.relayUrl = v; }, // no re-render: the input owns its own text

  connectAndFinish: () => guard(async () => {
    await client.addBridge({ kind: 'websocket', url: state.relayUrl.trim() });
    finishOnboarding();
  }),

  finishWithoutBridge: () => finishOnboarding(),
};

function finishOnboarding() {
  localStorage.setItem(K_ONBOARDED, '1');
  // The seed is dropped from memory here: step 2 said it could not be shown
  // again, and keeping it around would make that a lie.
  setState({ onboarding: false, seedHex: null, step: 0 });
}

// -------------------------------------------------------------------- routing

function navigate(screen) {
  if (!DESTINATIONS.some((d) => d.id === screen)) return;
  setState({ screen, pane: defaultPaneFor(screen) });
}

function goBack() {
  setState({ pane: 'list' });
}

// ---------------------------------------------------------------- rendering

/**
 * Screens still to be built in M10-D. Rendered as an explicit, labelled gap
 * rather than a plausible-looking empty state — an empty Contacts list would
 * claim "you have no contacts", which is a different and false statement.
 */
function notBuiltYet(name) {
  return el('div', { class: 'pane-body scroll-y' },
    el('div', { class: 'empty' },
      el('div', { class: 'empty-mark' }, '·'),
      el('h3', {}, name),
      el('p', {}, 'This screen is not built yet. The kernel, the client contract and the shell are in place; the screen itself lands later in Milestone 10-D.'),
    ),
  );
}

function screenTitle() {
  const d = DESTINATIONS.find((x) => x.id === state.screen);
  return d ? d.label : 'SPORE';
}

function renderApp() {
  const main = el('div', { style: { display: 'contents' } },
    appBar({
      title: screenTitle(),
      subtitle: state.identity ? formatAddr(state.identity.addrHex) : null,
      onBack: canGoBack(state.screen) && state.pane === 'detail' ? goBack : null,
      actions: [],
    }),
    notBuiltYet(screenTitle()),
  );

  return appShell({
    screen: state.screen,
    pane: state.pane,
    side: null,
    main,
    onNavigate: navigate,
  });
}

function render() {
  if (!root) return;
  if (!state.booted) {
    mount(root, el('div', { class: 'empty' }, el('p', {}, 'Starting…')));
    return;
  }
  mount(root, state.onboarding
    ? renderOnboarding({
        step: state.step,
        addrHex: state.identity ? state.identity.addrHex : null,
        seedHex: state.seedHex,
        seedConfirmed: state.seedConfirmed,
        relayUrl: state.relayUrl,
        busy: state.busy,
        error: state.error,
        actions: onboardingActions,
      })
    : renderApp());
}

// -------------------------------------------------------------------- boot

/**
 * Where the wasm comes from: inlined base64 in the standalone, a fetch in dev.
 *
 * Dev deliberately reads the whole body rather than handing the Response to
 * instantiateStreaming — that path rejects unless the server sends
 * application/wasm, and the usual dev servers do not agree about .wasm. The
 * standalone never takes this branch, so nothing ships the extra copy.
 */
function wasmSource() {
  if (globalThis.SPORE_WASM_BYTES) return globalThis.SPORE_WASM_BYTES;
  const url = new URL('../../target/wasm32-unknown-unknown/release/spore.wasm', import.meta.url);
  return fetch(url).then((r) => {
    if (!r.ok) throw new Error('could not load spore.wasm (' + r.status + ') — run: cargo build --release --lib --target wasm32-unknown-unknown');
    return r.arrayBuffer();
  });
}

export async function boot(container) {
  root = container;
  client = new SporeClient({ storage: localStorageAdapter() });

  client.on((e) => {
    if (e.type === 'BridgeStateChanged') setState({ bridges: client.bridges() });
    if (e.type === 'ClientFault') setState({ error: e.message });
  });

  // The gate: an identity already on disk means onboarding is done. We check
  // storage rather than initialising first, so the "Generate identity" button
  // genuinely generates rather than reporting something already created.
  const hasSeed = Boolean(localStorage.getItem('spore.seed'));
  const onboarded = Boolean(localStorage.getItem(K_ONBOARDED));

  if (hasSeed) {
    const identity = await client.init(wasmSource());
    setState({ booted: true, identity, onboarding: !onboarded });
  } else {
    setState({ booted: true, onboarding: true });
  }
}

if (typeof document !== 'undefined') {
  const container = document.getElementById('app');
  if (container) boot(container).catch((err) => {
    container.replaceChildren();
    container.appendChild(el('div', { class: 'alert alert-danger', role: 'alert' },
      'Could not start the node: ' + String((err && err.message) || err)));
  });
}
