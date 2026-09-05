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
import { ThreadStore, groupThread } from './stores/threads.mjs';
import { ContactStore, contactRows } from './stores/contacts.mjs';
import { el, mount } from './ui/dom.mjs';
import { appShell, appBar, defaultPaneFor, canGoBack, DESTINATIONS } from './ui/shell.mjs';
import { renderOnboarding } from './screens/onboarding.mjs';
import { renderChatList, renderChatThread } from './screens/chat.mjs';
import { renderContacts } from './screens/contacts.mjs';
import { renderSettings } from './screens/settings.mjs';
import { renderFiles } from './screens/files.mjs';
import { formatAddr, dayLabel } from './ui/format.mjs';

const K_ONBOARDED = 'spore.onboarded';
const ADDR_RE = /^[0-9a-f]{16}$/i;

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

  chatOpen: null,
  chatDraft: '',
  sending: false,
  newConvoOpen: false,
  newConvoError: null,

  accent: 'red',
  theme: 'system',
  keepHistory: true,
  lastAnnounceAt: null,
  addingBridge: null,
  confirmingWipe: false,

  contactsView: 'contacts',
  contactsQuery: '',
  editingContact: null,
  contactDraft: null,
  addingContact: false,
  addContactError: null,

  transfers: [],
  fetchValue: '',
  fetchError: null,
  publishing: false,
  publishError: null,
};

let client = null;
let threads = null;
let contacts = null;
let root = null;

// ------------------------------------------------------------------- helpers

function setState(patch) {
  Object.assign(state, patch);
  render();
}

async function guard(fn) {
  try {
    setState({ busy: true, error: null });
    await fn();
    setState({ busy: false });
  } catch (err) {
    setState({ error: String((err && err.message) || err), busy: false });
  }
}

// --------------------------------------------------------------- onboarding

const onboardingActions = {
  generate: () => guard(async () => {
    const identity = await client.init(wasmSource());
    await threads.load();
    await contacts.load();
    setState({ identity, seedHex: client.exportSeed() });
  }),
  next: () => setState({ step: state.step + 1, error: null }),
  toggleSeedConfirmed: () => setState({ seedConfirmed: !state.seedConfirmed }),
  copySeed: (seed) => { if (navigator.clipboard) navigator.clipboard.writeText(seed); },
  setRelayUrl: (v) => { state.relayUrl = v; }, // the input owns its own text
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
  state.onboarding = false;
  state.seedHex = null;
  state.step = 0;
  if ((location.hash || '') === '') location.hash = '#/chats';
  applyHash();
}

// -------------------------------------------------------------------- routing
//
// The URL is the source of truth, not a mirror of it: navigation writes a hash
// and the hashchange handler is the only thing that moves the app. That buys
// three things at once — a thread is refresh-safe and shareable because the
// param IS the 16-hex address the domain layer uses, the browser back button is
// a real navigation rather than app-managed slide state, and there is exactly
// one code path into a screen instead of two that can disagree.

/** Screen id -> URL segment. Only these five are routable. */
const SEGMENTS = { contacts: 'contacts', chat: 'chats', blogs: 'blogs', files: 'files', settings: 'settings' };
const SCREEN_OF = Object.fromEntries(Object.entries(SEGMENTS).map(([k, v]) => [v, k]));

function hashFor(screen, chatOpen) {
  if (screen === 'chat' && chatOpen) return '#/chats/' + chatOpen;
  return '#/' + (SEGMENTS[screen] || 'chats');
}

function navigate(screen) {
  if (!SEGMENTS[screen]) return;
  // Leaving chat drops the open thread from the URL; coming back to chat lands
  // on the list, which is what the pane rules expect.
  location.hash = hashFor(screen, screen === 'chat' ? state.chatOpen : null);
}

function openChat(addr) {
  location.hash = hashFor('chat', addr);
}

function goBack() {
  // On a phone this is the list/detail step. Going "back" from a thread means
  // dropping the address from the URL, so the browser's own back button and
  // this button do the same thing rather than diverging.
  if (state.screen === 'chat' && state.chatOpen) location.hash = '#/chats';
  else setState({ pane: 'list' });
}

/** Parse the hash and move the app to it. The only writer of screen/chatOpen. */
function applyHash() {
  const raw = (location.hash || '').replace(/^#\/?/, '');
  const [seg, param] = raw.split('/');
  const screen = SCREEN_OF[seg] || 'chat';

  let chatOpen = null;
  if (screen === 'chat' && param && ADDR_RE.test(param)) chatOpen = param.toLowerCase();

  if (chatOpen) {
    threads.markRead(chatOpen);
    threads.save();
  }
  setState({
    screen,
    chatOpen,
    pane: chatOpen ? 'detail' : defaultPaneFor(screen),
    chatDraft: chatOpen === state.chatOpen ? state.chatDraft : '',
    newConvoOpen: false,
    newConvoError: null,
  });
}

// ----------------------------------------------------------------- chat

const chatActions = {
  select: (addr) => openChat(addr),

  setDraft: (v) => { state.chatDraft = v; }, // no re-render: see renderComposer

  send: () => guard(async () => {
    const body = state.chatDraft.trim();
    if (!body || !state.chatOpen) return;
    setState({ sending: true });
    const envelope = client.sendDirect(state.chatOpen, new TextEncoder().encode(body));
    threads.send({ ...envelope, body });
    await threads.save();
    setState({ chatDraft: '', sending: false });
  }),

  openNew: () => setState({ newConvoOpen: true, newConvoError: null }),
  closeNew: () => setState({ newConvoOpen: false, newConvoError: null }),

  startNew: (value) => {
    const addr = (value || '').trim().toLowerCase().replace(/[^0-9a-f]/g, '');
    if (!ADDR_RE.test(addr)) {
      setState({ newConvoError: 'An address is 16 hexadecimal digits. Check what you pasted.' });
      return;
    }
    if (addr === state.identity.addrHex) {
      setState({ newConvoError: 'That is this node’s own address.' });
      return;
    }
    setState({ newConvoOpen: false, newConvoError: null });
    openChat(addr);
  },
};

// ------------------------------------------------------------------ contacts

const contactsActions = {
  setView: (view) => setState({ contactsView: view, editingContact: null, addingContact: false }),
  setQuery: (q) => setState({ contactsQuery: q }),

  openAdd: () => setState({ addingContact: true, addContactError: null, editingContact: null }),
  closeAdd: () => setState({ addingContact: false, addContactError: null }),

  confirmAdd: (value) => {
    const addr = normaliseAddr(value);
    if (!addr) { setState({ addContactError: 'An address is 16 hexadecimal digits. Check what you pasted.' }); return; }
    if (addr === state.identity.addrHex) { setState({ addContactError: 'That is this node\u2019s own address.' }); return; }
    contacts.setLabel(addr, '');
    contacts.save();
    setState({ addingContact: false, addContactError: null });
    contactsActions.openEdit(addr);
  },

  openEdit: (addr) => {
    const c = contacts.get(addr);
    setState({
      editingContact: addr,
      addingContact: false,
      contactDraft: {
        label: c && c.label ? c.label : '',
        following: Boolean(c && c.following),
        blocked: Boolean(c && c.blocked),
      },
    });
  },
  closeEdit: () => setState({ editingContact: null, contactDraft: null }),

  saveEdit: (addr, { label, following, blocked }) => {
    contacts.setLabel(addr, label);
    contacts.setFollowing(addr, following);
    contacts.setBlocked(addr, blocked);
    contacts.save();
    setState({ editingContact: null, contactDraft: null });
  },

  messageAddr: (addr) => openChat(addr),
};

/** 16 hex digits, tolerant of spacing and the middot the UI formats with. */
function normaliseAddr(value) {
  const a = (value || '').trim().toLowerCase().replace(/[^0-9a-f]/g, '');
  return ADDR_RE.test(a) ? a : null;
}

/**
 * The display name for an address, and whether it is only a claim. A label the
 * user typed wins; an announced name is a fallback that must be marked.
 */
function nameFor(addr) {
  const label = contacts.labelFor(addr);
  if (label) return { name: label, isClaim: false };
  const p = client.peers().find((x) => x.addrHex === addr);
  if (p && p.claimedName) return { name: p.claimedName, isClaim: true };
  return { name: null, isClaim: false };
}

// ------------------------------------------------------------------ settings

const K_ACCENT = 'spore.accent';
const K_THEME = 'spore.theme';
const K_KEEP_HISTORY = 'spore.keepHistory';

/** Appearance is applied to <html>, which is where the design system reads it. */
function applyAppearance() {
  const root = document.documentElement;
  root.setAttribute('data-accent', state.accent);
  if (state.theme === 'system') root.removeAttribute('data-theme');
  else root.setAttribute('data-theme', state.theme);
}

const filesActions = {
  setFetchValue: (v) => setState({ fetchValue: v, fetchError: null }),

  fetch: (magnetHex) => {
    try {
      client.fetchFile(magnetHex);
      // No transfer appears until a peer answers with the manifest, so say what
      // was asked rather than pretending the file is already on its way.
      setState({ fetchValue: '', fetchError: null });
    } catch (err) {
      setState({ fetchError: String((err && err.message) || err) });
    }
  },

  publish: async (file) => {
    setState({ publishing: true, publishError: null });
    try {
      const bytes = new Uint8Array(await file.arrayBuffer());
      client.publishFile(file.name, bytes);
      setState({ publishing: false, transfers: client.transfers() });
    } catch (err) {
      // A file past what the node can chunk or store fails here, and the core's
      // message says which — pass it through rather than replacing it.
      setState({ publishing: false, publishError: String((err && err.message) || err) });
    }
  },

  copyMagnet: (magnet) => { if (navigator.clipboard) navigator.clipboard.writeText(magnet); },

  save: (magnet, name) => {
    const bytes = client.fileBytes(magnet);
    if (!bytes) {
      setState({ publishError: 'That file is not complete yet.' });
      return;
    }
    const url = URL.createObjectURL(new Blob([bytes]));
    const a = document.createElement('a');
    a.href = url;
    a.download = name;
    a.click();
    URL.revokeObjectURL(url);
  },
};

const settingsActions = {
  setAccent: (a) => {
    state.accent = a;
    localStorage.setItem(K_ACCENT, a);
    applyAppearance();
    render();
  },
  setTheme: (t) => {
    state.theme = t;
    localStorage.setItem(K_THEME, t);
    applyAppearance();
    render();
  },

  setKeepHistory: (on) => {
    localStorage.setItem(K_KEEP_HISTORY, on ? '1' : '0');
    threads.storage = on ? localStorageAdapter() : null;
    // Turning it off must actually forget, not just stop writing -- otherwise
    // the switch claims something the disk contradicts.
    if (!on) localStorage.removeItem('spore.threads');
    setState({ keepHistory: on });
  },

  startWipe: () => setState({ confirmingWipe: true }),
  cancelWipe: () => setState({ confirmingWipe: false }),
  confirmWipe: () => {
    client.dispose();
    localStorage.clear();
    location.hash = '';
    location.reload();
  },

  copyAddress: (addr) => { if (navigator.clipboard) navigator.clipboard.writeText(addr); },

  announceNow: () => {
    client.announceNow();
    setState({ lastAnnounceAt: Date.now() });
  },

  openAddBridge: () => {
    const first = client.availableTransports()[0];
    setState({ addingBridge: { kind: first ? first.kind : null, fields: {}, error: null, busy: false } });
  },
  closeAddBridge: () => setState({ addingBridge: null }),
  setAddBridgeKind: (kind) => setState({ addingBridge: { ...state.addingBridge, kind, fields: {}, error: null } }),
  setAddBridgeField: (k, v) => {
    // No re-render: the input owns its own text, as in the composer.
    state.addingBridge.fields[k] = v;
  },

  confirmAddBridge: async () => {
    const a = state.addingBridge;
    if (!a || !a.kind) return;
    const spec = client.availableTransports().find((t) => t.kind === a.kind);
    const cfg = { kind: a.kind };
    for (const f of (spec && spec.fields) || []) {
      cfg[f.k] = a.fields[f.k] !== undefined ? a.fields[f.k] : (f.value || '');
    }
    setState({ addingBridge: { ...a, busy: true, error: null } });
    try {
      await client.addBridge(cfg);
      setState({ addingBridge: null, bridges: client.bridges() });
    } catch (err) {
      setState({ addingBridge: { ...state.addingBridge, busy: false, error: String((err && err.message) || err) } });
    }
  },

  removeBridge: (id) => {
    client.removeBridge(id);
    setState({ bridges: client.bridges() });
  },
};

// ---------------------------------------------------------------- rendering

function notBuiltYet(name) {
  return el('div', { class: 'pane-body scroll-y' },
    el('div', { class: 'empty' },
      el('div', { class: 'empty-mark' }, '·'),
      el('h3', {}, name),
      el('p', {}, 'This screen is not built yet. The kernel, the client contract and the shell are in place; the screen itself lands later in Milestone 10-D.'),
    ),
  );
}

function newConversationForm() {
  let value = '';
  return el('div', { class: 'card', style: { marginBottom: 'var(--gap)' } },
    el('div', { class: 'card-body', style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap-tight)' } },
      el('div', { class: 'field' },
        el('label', { for: 'new-convo' }, 'Address'),
        el('input', {
          id: 'new-convo', type: 'text', placeholder: '3F2A9C1088E4001B',
          oninput: (e) => { value = e.target.value; },
          onkeydown: (e) => { if (e.key === 'Enter') { e.preventDefault(); chatActions.startNew(value); } },
        }),
        el('div', { class: 'field-hint' }, '16 hexadecimal digits. There is no directory to search.'),
      ),
      state.newConvoError ? el('div', { class: 'field-error', role: 'alert' }, state.newConvoError) : null,
      el('div', { class: 'cluster-tight' },
        el('button', { class: 'btn btn-secondary btn-sm', type: 'button', onclick: chatActions.closeNew }, 'Cancel'),
        el('button', { class: 'btn btn-sm', type: 'button', onclick: () => chatActions.startNew(value) }, 'Open'),
      ),
    ),
  );
}

function chatConversations() {
  const rows = threads.conversations();
  // An open-but-empty conversation is real state: the user opened an address
  // and has not sent anything yet. It belongs in the list.
  if (state.chatOpen && !rows.some((r) => r.addr === state.chatOpen)) {
    rows.unshift({ addr: state.chatOpen, lastBody: '', lastAt: 0, lastSelf: false, unread: 0 });
  }
  return rows
    // A blocked address is hidden from the conversation list; the thread is not
    // deleted, because blocking is a display decision and not a data one.
    .filter((r) => !contacts.isBlocked(r.addr))
    .map((r) => ({ ...r, ...nameFor(r.addr), keyState: client.keyStateFor(r.addr) }));
}

function renderChatScreen() {
  const side = el('div', { style: { display: 'contents' } },
    state.newConvoOpen
      ? el('div', { class: 'pane-body scroll-y', style: { padding: 'var(--pad-tight)' } }, newConversationForm())
      : renderChatList({
          conversations: chatConversations(),
          selected: state.chatOpen,
          onSelect: chatActions.select,
          onNewConversation: chatActions.openNew,
          unauthenticatedCount: threads.unauthenticatedCount,
        }),
  );

  const main = el('div', { style: { display: 'contents' } },
    appBar({
      title: state.chatOpen ? formatAddr(state.chatOpen) : 'Chats',
      subtitle: state.chatOpen ? null : 'Direct messages',
      onBack: state.pane === 'detail' ? goBack : null,
      actions: [],
    }),
    renderChatThread({
      addr: state.chatOpen,
      items: state.chatOpen ? groupThread(threads.messages(state.chatOpen), (s) => dayLabel(s * 1000)) : [],
      keyState: state.chatOpen ? client.keyStateFor(state.chatOpen) : 'cleartext',
      draft: state.chatDraft,
      onDraft: chatActions.setDraft,
      onSend: chatActions.send,
      sending: state.sending,
    }),
  );

  return { side, main };
}

function screenTitle() {
  const d = DESTINATIONS.find((x) => x.id === state.screen);
  return d ? d.label : 'SPORE';
}

function renderApp() {
  let side = null;
  let main;

  if (state.screen === 'chat') {
    ({ side, main } = renderChatScreen());
  } else if (state.screen === 'settings') {
    main = el('div', { style: { display: 'contents' } },
      appBar({ title: 'Settings', subtitle: null, onBack: null, actions: [] }),
      renderSettings({
        bridges: state.bridges,
        available: client.availableTransports(),
        adding: state.addingBridge,
        accent: state.accent,
        theme: state.theme,
        keepHistory: state.keepHistory,
        lastAnnounceAt: state.lastAnnounceAt,
        addrHex: state.identity ? state.identity.addrHex : '',
        confirmingWipe: state.confirmingWipe,
        actions: settingsActions,
      }),
    );
  } else if (state.screen === 'files') {
    main = el('div', { style: { display: 'contents' } },
      appBar({ title: 'Files', subtitle: null, onBack: null, actions: [] }),
      renderFiles({
        transfers: state.transfers,
        fetchValue: state.fetchValue,
        fetchError: state.fetchError,
        publishing: state.publishing,
        publishError: state.publishError,
        actions: filesActions,
      }),
    );
  } else if (state.screen === 'contacts') {
    main = el('div', { style: { display: 'contents' } },
      appBar({ title: 'Contacts', subtitle: null, onBack: null, actions: [] }),
      renderContacts({
        rows: contactRows(client.peers(), contacts, { view: state.contactsView, query: state.contactsQuery }),
        view: state.contactsView,
        query: state.contactsQuery,
        editing: state.editingContact,
        draft: state.contactDraft || { label: '', following: false, blocked: false },
        adding: state.addingContact,
        addError: state.addContactError,
        addValue: '',
        ownAddrHex: state.identity ? state.identity.addrHex : '',
        actions: contactsActions,
      }),
    );
  } else {
    main = el('div', { style: { display: 'contents' } },
      appBar({
        title: screenTitle(),
        subtitle: state.identity ? formatAddr(state.identity.addrHex) : null,
        onBack: canGoBack(state.screen) && state.pane === 'detail' ? goBack : null,
        actions: [],
      }),
      notBuiltYet(screenTitle()),
    );
  }

  return appShell({ screen: state.screen, pane: state.pane, side, main, onNavigate: navigate });
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
  // Resolved against the document, not `import.meta.url`. The standalone
  // concatenates every module into one *classic* script, where `import.meta` is
  // a syntax error — and a syntax error there kills the whole script silently,
  // leaving a page that simply never initialises. `document.baseURI` gives the
  // same answer in the dev harness and costs the standalone nothing, since it
  // never reaches this branch.
  const url = new URL('../../target/wasm32-unknown-unknown/release/spore.wasm', document.baseURI);
  return fetch(url).then((r) => {
    if (!r.ok) throw new Error('could not load spore.wasm (' + r.status + ') — run: cargo build --release --lib --target wasm32-unknown-unknown');
    return r.arrayBuffer();
  });
}

export async function boot(container) {
  root = container;
  const storage = localStorageAdapter();
  client = new SporeClient({ storage });
  threads = new ThreadStore({ storage });
  contacts = new ContactStore({ storage });

  client.on((e) => {
    switch (e.type) {
      case 'EnvelopeReceived': {
        // Blocking hides a sender rather than discarding their traffic: the
        // envelope was still authenticated and still cost the mesh a relay, so
        // it is recorded and simply not surfaced.
        const addr = threads.receive(e);
        // Reading a thread you are looking at should not leave it unread.
        if (addr && addr === state.chatOpen && state.pane === 'detail') threads.markRead(addr);
        threads.save();
        render();
        break;
      }
      case 'EnvelopeAcked':
        if (threads.setStatus(e.id, 'acked')) { threads.save(); render(); }
        break;
      case 'EnvelopeExpired':
        if (threads.setStatus(e.id, 'expired')) { threads.save(); render(); }
        break;
      case 'BridgeStateChanged':
        setState({ bridges: client.bridges() });
        break;
      case 'AnnounceSent':
        setState({ lastAnnounceAt: Date.now() });
        break;
      case 'TransferProgress':
        // The client only emits this when a chunk count actually moved, so
        // re-reading the whole list here costs one tree walk per real change,
        // not one per tick.
        setState({ transfers: client.transfers() });
        break;
      case 'ClientFault':
        setState({ error: e.message });
        break;
      default:
        break;
    }
  });

  // The gate: an identity already on disk means onboarding is done. We check
  // storage rather than initialising first, so the "Generate identity" button
  // genuinely generates rather than reporting something already created.
  const hasSeed = Boolean(localStorage.getItem('spore.seed'));
  const onboarded = Boolean(localStorage.getItem(K_ONBOARDED));

  state.accent = localStorage.getItem(K_ACCENT) || 'red';
  state.theme = localStorage.getItem(K_THEME) || 'system';
  state.keepHistory = localStorage.getItem(K_KEEP_HISTORY) !== '0';
  applyAppearance();
  if (!state.keepHistory) threads.storage = null;

  window.addEventListener('hashchange', () => { if (!state.onboarding) applyHash(); });

  if (hasSeed) {
    const identity = await client.init(wasmSource());
    await threads.load();
    await contacts.load();
    state.booted = true;
    state.identity = identity;
    // Adopted custody can include manifests, so the list is not necessarily
    // empty on a cold start; the first TransferProgress would otherwise be the
    // only thing that ever filled it.
    state.transfers = client.transfers();
    state.onboarding = !onboarded;
    if (state.onboarding) render();
    else applyHash(); // restores the open thread from the URL on a reload
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
