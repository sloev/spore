// Chats (M10-D): conversation list beside a thread, with a composer.
//
// Pure render functions of a view model. Nothing here reads SporeClient or a
// store — main.mjs assembles the view model and passes callbacks down.
//
// Delivery status is three states, and the reasoning is `web/ui/delivery-
// status.mjs`'s, not restated here so the two cannot quietly diverge: the core
// has no "still travelling" and no "gave up" event, so "not yet delivered" is
// genuinely ambiguous between active resending and passive custody, and a status
// line claiming to tell them apart would invent precision the protocol does not
// have. What is observable: a receipt came back (acked), the envelope's own
// lifetime passed with no receipt (expired), or neither yet (still travelling).
// That module is the pre-M10 surface's copy and this is the HARDBRUT/3 one; both
// collapse into the Rust ThreadStore at M10-B.

import { el, icon } from '../ui/dom.mjs';
import { ICONS, KEYSTATE } from '../ui/icons.mjs';
import { formatAddr, shortWhen, truncate } from '../ui/format.mjs';

/** Receipt glyph + word. Never colour alone — the glyph and the text both carry it. */
function receipt(status) {
  if (status === 'acked') return el('span', { class: 'chat-receipt', 'data-state': 'read' }, '✓✓ Delivered');
  if (status === 'expired') return el('span', { class: 'chat-receipt' }, '⊘ Expired — undelivered');
  if (status === 'queued' || status === 'sent') return el('span', { class: 'chat-receipt' }, '· Still travelling');
  return null;
}

/**
 * The key-state badge. `canSealTo()` is false until a peer's ANNOUNCE has been
 * heard, and until then a DM genuinely goes out in the clear — so this says so
 * rather than drawing a padlock unconditionally.
 */
export function keyBadge(keyState) {
  const k = KEYSTATE[keyState] || KEYSTATE.cleartext;
  return el('span', { class: 'badge ' + k.badge + ' mono text-xs', style: { display: 'inline-flex', alignItems: 'center', gap: '4px' } },
    icon(k.icon, { size: '12px' }),
    k.label,
  );
}

// ---------------------------------------------------------------- list pane

/**
 * @param {Object} vm
 * @param {Array}  vm.conversations  [{addr, name, lastBody, lastAt, unread, keyState}]
 * @param {string|null} vm.selected
 * @param {Function} vm.onSelect
 * @param {Function} vm.onNewConversation
 * @param {number} vm.unauthenticatedCount
 */
export function renderChatList(vm) {
  const { conversations, selected, onSelect, onNewConversation, unauthenticatedCount } = vm;

  return el('div', { class: 'pane-body scroll-y', style: { padding: 'var(--pad-tight)' } },
    el('div', { class: 'cluster', style: { marginBottom: 'var(--gap)' } },
      el('button', { class: 'btn btn-sm btn-secondary', type: 'button', onclick: onNewConversation },
        icon(ICONS.plus, { size: '14px' }), 'New'),
    ),

    unauthenticatedCount > 0
      ? el('div', { class: 'alert alert-quiet text-xs', style: { marginBottom: 'var(--gap)' } },
          el('span', {}, unauthenticatedCount + ' message' + (unauthenticatedCount === 1 ? '' : 's') +
            ' arrived without a provable sender and were not filed into any conversation.'))
      : null,

    conversations.length === 0
      ? el('div', { class: 'empty' },
          el('div', { class: 'empty-mark' }, '○'),
          el('h3', {}, 'No conversations'),
          el('p', {}, 'A conversation starts when you message an address, or when someone messages you. Add an address in Contacts to begin.'),
        )
      : el('div', { class: 'list' },
          conversations.map((c) => el('button', {
            class: 'list-row',
            type: 'button',
            'aria-current': c.addr === selected ? 'true' : null,
            'data-unread': c.unread > 0 ? 'true' : null,
            onclick: () => onSelect(c.addr),
          },
            el('span', { class: 'list-row-avatar' }, (c.name || c.addr).slice(0, 2).toUpperCase()),
            el('span', { class: 'list-row-body' },
              el('span', { class: 'list-row-title' }, c.name || formatAddr(c.addr)),
              el('span', { class: 'list-row-subtitle' },
                (c.lastSelf ? 'You: ' : '') + truncate(c.lastBody || '', 48)),
            ),
            el('span', { class: 'list-row-meta' },
              el('span', {}, c.lastAt ? shortWhen(c.lastAt * 1000) : ''),
              c.unread > 0 ? el('span', { class: 'badge badge-count' }, String(c.unread)) : null,
            ),
          )),
        ),
  );
}

// -------------------------------------------------------------- thread pane

/**
 * @param {Object} vm
 * @param {string|null} vm.addr
 * @param {Array} vm.items       from groupThread()
 * @param {string} vm.keyState   'sealed' | 'ratchet' | 'cleartext'
 * @param {string} vm.draft
 * @param {Function} vm.onDraft
 * @param {Function} vm.onSend
 * @param {boolean} vm.sending
 */
export function renderChatThread(vm) {
  const { addr, items, keyState, draft, onDraft, onSend, sending } = vm;

  if (!addr) {
    return el('div', { class: 'pane-body scroll-y' },
      el('div', { class: 'empty' },
        el('div', { class: 'empty-mark' }, '○'),
        el('h3', {}, 'No conversation selected'),
        el('p', {}, 'Pick a conversation from the list.'),
      ),
    );
  }

  return el('div', { style: { display: 'contents' } },
    el('div', { class: 'pane-body scroll-y' },
      el('div', { class: 'chat', role: 'log', 'aria-live': 'polite' },
        items.length === 0
          ? el('div', { class: 'chat-system' }, 'No messages yet. Anything you send travels the mesh and may take a while to arrive.')
          : items.map(renderItem),
      ),
    ),

    // The key state sits directly above the composer, where it is read at the
    // moment of sending rather than buried in a header.
    el('div', {
      class: 'pane-foot',
      style: {
        padding: 'var(--space-1) var(--pad)', background: 'var(--paper)',
        borderTop: 'var(--border)', overflowX: 'auto', whiteSpace: 'nowrap',
      },
    }, keyBadge(keyState)),

    renderComposer({ draft, onDraft, onSend, sending }),
  );
}

function renderItem(item) {
  if (item.kind === 'day') return el('div', { class: 'chat-day' }, item.label);

  const m = item.message;
  const body = typeof m.body === 'string' ? m.body : new TextDecoder().decode(m.body);

  return el('div', {
    class: 'chat-msg' + (m.self ? ' chat-msg-self' : ''),
    'data-run': item.run,
  },
    // Only incoming messages carry an avatar. Rendering an empty one for your
    // own messages leaves a stray bordered box beside every bubble.
    m.self ? null : el('div', { class: 'chat-avatar' }, '··'),
    el('div', { class: 'chat-col' },
      // Body is a text node, never innerHTML: this is a peer's bytes.
      //
      // Deliberately NOT data-state="sending". The design system fades a bubble
      // at 60% while it is in flight, which reads correctly for a momentary
      // state — but "still travelling" here lasts until the envelope's TTL
      // expires, potentially hours, so every outgoing message would sit
      // permanently faded and look broken. The receipt line carries the state in
      // words instead, which is also the accessible form.
      el('div', { class: 'chat-bubble' }, body),
      el('div', { class: 'chat-meta' },
        el('span', {}, shortWhen(m.at * 1000)),
        m.self ? receipt(m.status) : null,
      ),
    ),
  );
}

/**
 * The composer owns its own enabled state rather than driving a re-render on
 * every keystroke. Re-rendering here would rebuild the textarea mid-typing and
 * throw away focus and the caret, so the draft is reported upward for sending
 * but the Send button is toggled locally, in place.
 */
function renderComposer({ draft, onDraft, onSend, sending }) {
  const button = el('button', {
    class: 'btn', type: 'submit',
    disabled: sending || !draft.trim(),
  }, sending ? 'Sending…' : 'Send');

  const form = el('form', { class: 'composer', 'data-draft': draft.trim() ? 'true' : null });

  const input = el('textarea', {
    value: draft,
    rows: 1,
    placeholder: 'Message',
    'aria-label': 'Message',
    oninput: (e) => {
      onDraft(e.target.value);
      button.disabled = sending || !e.target.value.trim();
      form.setAttribute('data-draft', e.target.value.trim() ? 'true' : 'false');
    },
    // Enter sends, Shift+Enter is a newline — the keyboard contract the design
    // system states for every thread.
    onkeydown: (e) => {
      if (e.key === 'Enter' && !e.shiftKey) { e.preventDefault(); if (!button.disabled) onSend(); }
    },
  });

  form.addEventListener('submit', (e) => { e.preventDefault(); onSend(); });
  form.appendChild(el('div', { class: 'composer-row' }, input, button));
  return form;
}
