// Contacts (M10-D): local labels for addresses, and the peers this node has
// heard from but the user has not kept.
//
// The screen's one job beyond listing is to keep a claim and a decision visibly
// apart. A name in an ANNOUNCE is what a node *says* it is called; anyone may
// announce anything. So a claimed name is always marked as claimed, and the
// label the user types is the one presented as theirs.
//
// Pure render functions of a view model. No client, no store access.

import { el, icon } from '../ui/dom.mjs';
import { ICONS } from '../ui/icons.mjs';
import { formatAddr, shortWhen, truncate } from '../ui/format.mjs';

/**
 * @param {Object} vm
 * @param {Array}  vm.rows      from contactRows()
 * @param {'contacts'|'seen'} vm.view
 * @param {string} vm.query
 * @param {string|null} vm.editing   address being edited, if any
 * @param {Object|null} vm.draft     { label, following, blocked } while editing
 * @param {boolean} vm.adding
 * @param {string|null} vm.addError
 * @param {string} vm.ownAddrHex
 * @param {Object} vm.actions
 */
export function renderContacts(vm) {
  const { rows, view, query, editing, adding, ownAddrHex, actions } = vm;

  return el('div', { class: 'pane-body scroll-y', style: { padding: 'var(--pad)' } },
    el('div', { style: { width: '100%', display: 'flex', flexDirection: 'column', gap: 'var(--gap)' } },

      el('p', { class: 'text-muted text-sm', style: { margin: '0' } },
        'A contact is a label you give an address. Names other nodes announce are claims, not proof — yours is the one that counts.'),

      el('div', { class: 'cluster-tight' },
        el('label', { class: 'search', style: { flex: '1 1 200px', minWidth: '0' } },
          el('span', { class: 'sr-only' }, 'Search contacts'),
          el('input', {
            type: 'search', placeholder: 'Search', value: query,
            oninput: (e) => actions.setQuery(e.target.value),
          }),
        ),
        // Hidden while the form is open: two visible controls both named "Add",
        // doing different things, is ambiguous to anyone navigating by
        // accessible name rather than by position.
        adding ? null : el('button', { class: 'btn btn-secondary btn-sm', type: 'button', onclick: actions.openAdd },
          icon(ICONS.plus, { size: '14px' }), 'Add'),
        el('button', {
          class: 'chip', type: 'button', 'aria-pressed': view === 'contacts' ? 'true' : 'false',
          onclick: () => actions.setView('contacts'),
        }, 'Contacts'),
        el('button', {
          class: 'chip', type: 'button', 'aria-pressed': view === 'seen' ? 'true' : 'false',
          onclick: () => actions.setView('seen'),
        }, 'Seen announces'),
      ),

      adding ? addForm(vm) : null,
      editing ? editForm(vm) : null,

      rows.length === 0 ? emptyFor(view, query) : el('div', { class: 'list' }, rows.map((r) => row(r, actions))),

      view === 'seen'
        ? el('p', { class: 'text-xs mono text-uppercase text-muted', style: { margin: '0' } },
            'Your address · ' + formatAddr(ownAddrHex))
        : null,
    ),
  );
}

function emptyFor(view, query) {
  if (query) {
    return el('div', { class: 'empty' },
      el('div', { class: 'empty-mark' }, '?'),
      el('h3', {}, 'Nothing found'),
      el('p', {}, 'Try a different search, or add one by address.'),
    );
  }
  if (view === 'seen') {
    return el('div', { class: 'empty' },
      el('div', { class: 'empty-mark' }, '○'),
      el('h3', {}, 'Nothing heard yet'),
      el('p', {}, 'Addresses appear here once this node has heard signed traffic from them. That needs a bridge, and a peer on the other side of it.'),
    );
  }
  return el('div', { class: 'empty' },
    el('div', { class: 'empty-mark' }, '○'),
    el('h3', {}, 'No contacts'),
    el('p', {}, 'Add an address to give it a label. There is no directory to browse — you get an address from the person it belongs to.'),
  );
}

function row(r, actions) {
  const initials = (r.name || r.addr).slice(0, 2).toUpperCase();

  return el('button', {
    class: 'list-row', type: 'button',
    onclick: () => actions.openEdit(r.addr),
  },
    el('span', { class: 'list-row-avatar' }, initials),
    el('span', { class: 'list-row-body' },
      el('span', { class: 'list-row-title' },
        r.name || formatAddr(r.addr),
        // A claimed name is never presented as settled.
        r.nameIsClaim ? el('span', { class: 'badge badge-quiet mono text-xs', style: { marginLeft: 'var(--space-2)' } }, 'CLAIMED') : null,
        r.blocked ? el('span', { class: 'badge badge-danger mono text-xs', style: { marginLeft: 'var(--space-2)' } }, 'BLOCKED') : null,
      ),
      el('span', { class: 'list-row-subtitle mono' }, formatAddr(r.addr)),
    ),
    el('span', { class: 'list-row-meta' },
      r.heard ? el('span', {}, shortWhen(Date.now() - r.ageSecs * 1000)) : el('span', {}, 'never heard'),
      r.following ? el('span', { class: 'badge badge-info mono text-xs' }, 'FOLLOWING') : null,
    ),
  );
}

function addForm({ addValue, addError, actions }) {
  let value = addValue || '';
  return el('div', { class: 'card' },
    el('div', { class: 'card-body', style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap-tight)' } },
      el('p', { class: 'text-sm text-muted', style: { margin: '0' } },
        'Paste an address. There is no directory to browse — you get one from the person it belongs to.'),
      el('div', { class: 'field' },
        el('label', { for: 'add-contact' }, 'Address'),
        el('input', {
          id: 'add-contact', type: 'text', placeholder: '3F2A9C1088E4001B', value,
          oninput: (e) => { value = e.target.value; },
          onkeydown: (e) => { if (e.key === 'Enter') { e.preventDefault(); actions.confirmAdd(value); } },
        }),
        el('div', { class: 'field-hint' }, '16 hexadecimal digits.'),
      ),
      addError ? el('div', { class: 'field-error', role: 'alert' }, addError) : null,
      el('div', { class: 'cluster-tight' },
        el('button', { class: 'btn btn-secondary btn-sm', type: 'button', onclick: actions.closeAdd }, 'Cancel'),
        el('button', { class: 'btn btn-sm', type: 'button', onclick: () => actions.confirmAdd(value) }, 'Add contact'),
      ),
    ),
  );
}

function editForm({ editing, draft, rows, actions }) {
  const r = rows.find((x) => x.addr === editing)
    || { addr: editing, claimedName: null, label: null, following: false, blocked: false };
  let label = draft.label;
  let following = draft.following;
  let blocked = draft.blocked;

  return el('div', { class: 'card' },
    el('div', { class: 'card-body', style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap)' } },
      el('span', { class: 'text-uppercase text-sm mono' }, 'Contact'),
      el('p', { class: 'mono text-sm', style: { margin: '0' } }, formatAddr(r.addr)),

      r.claimedName
        ? el('p', { class: 'text-sm text-muted', style: { margin: '0' } },
            'Claims to be ', el('strong', {}, truncate(r.claimedName, 40)),
            ' — unauthenticated. Give this address a private label instead, seen only by you.')
        : el('p', { class: 'text-sm text-muted', style: { margin: '0' } },
            'This address has announced no name. A label here is seen only by you.'),

      el('div', { class: 'field' },
        el('label', { for: 'edit-label' }, 'Your label'),
        el('input', {
          id: 'edit-label', type: 'text', value: label || '',
          placeholder: r.claimedName || 'No label',
          oninput: (e) => { label = e.target.value; },
        }),
      ),

      el('label', { class: 'choice' },
        el('input', {
          type: 'checkbox', class: 'switch', checked: following,
          onchange: (e) => { following = e.target.checked; },
        }),
        el('span', { class: 'text-sm' }, 'Following — see their wall in Blogs.'),
      ),
      el('label', { class: 'choice' },
        el('input', {
          type: 'checkbox', class: 'switch', checked: blocked,
          onchange: (e) => { blocked = e.target.checked; },
        }),
        el('span', { class: 'text-sm' }, 'Block — hide their messages and posts.'),
      ),

      el('div', { class: 'cluster-tight' },
        el('button', { class: 'btn btn-secondary btn-sm', type: 'button', onclick: actions.closeEdit }, 'Cancel'),
        el('button', {
          class: 'btn btn-sm', type: 'button',
          onclick: () => actions.saveEdit(r.addr, { label, following, blocked }),
        }, 'Save'),
        el('button', {
          class: 'btn btn-sm btn-secondary', type: 'button',
          onclick: () => actions.messageAddr(r.addr),
        }, 'Message'),
      ),
    ),
  );
}
