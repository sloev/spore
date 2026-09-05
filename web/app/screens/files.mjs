// Files (M10-D): what this node holds, and what it is still pulling.
//
// The screen the kernel could not support until now. `listFiles()` answers only
// *complete* files, so before `spore_node_files` a transfer in flight looked
// exactly like one that was never requested — there was nothing to draw a
// progress bar from. This renders the real transfer list.
//
// **Two honesty rules this screen exists under.**
//
// A file published from a browser is published *in the clear*. The core has a
// sealed path (`Node::publish_file_sealed`), but the wasm ABI does not export it
// and `spore_node_publish_file` calls the plain one, so every byte is readable
// by anyone relaying it. The screen says so rather than showing a padlock it has
// not earned — the same rule `spore_node_send_direct_sealed` was added for.
//
// And progress is a lower bound, never an estimate. Chunks under an interior
// manifest we do not hold yet cannot be named, so they cannot be counted: a bar
// may jump forward when an interior node lands. So the number is shown as chunks
// held out of chunks known, and there is no time remaining anywhere on this
// screen, because we genuinely do not know it.
//
// Pure render functions of a view model. No client, no store access.

import { el, icon } from '../ui/dom.mjs';
import { ICONS } from '../ui/icons.mjs';
import { formatBytes, fileKind } from '../ui/format.mjs';

const MAGNET_RE = /^[0-9a-f]{32}$/i;

/** True for a magnet a user could plausibly have pasted. */
export function looksLikeMagnet(s) {
  return MAGNET_RE.test((s || '').trim());
}

/**
 * @param {Object} vm
 * @param {Array}  vm.transfers   from SporeClient.transfers()
 * @param {string} vm.fetchValue  the magnet input
 * @param {string|null} vm.fetchError
 * @param {boolean} vm.publishing
 * @param {string|null} vm.publishError
 * @param {Object} vm.actions
 */
export function renderFiles(vm) {
  const { transfers, fetchValue, fetchError, publishing, publishError, actions } = vm;

  // In-flight first: the one thing on this screen that is changing is the one
  // thing worth putting at the top.
  const rows = [...transfers].sort((a, b) => {
    if (a.complete !== b.complete) return a.complete ? 1 : -1;
    return a.name.localeCompare(b.name);
  });

  return el('div', { class: 'pane-body scroll-y', style: { padding: 'var(--pad)' } },
    el('div', { style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap)' } },

      publishPanel({ publishing, publishError, actions }),
      fetchPanel({ fetchValue, fetchError, actions }),

      rows.length === 0
        ? el('div', { class: 'empty' },
            el('div', { class: 'empty-mark' }, '□'),
            el('h3', {}, 'No files'),
            el('p', {}, 'Publish one to offer it to the mesh, or paste a magnet to pull one. A file needs a bridge and a peer holding it.'))
        : el('div', { class: 'list' }, rows.map((t) => fileRow(t, actions))),
    ),
  );
}

/** One file: kind block, name, size, and either progress or what you can do. */
function fileRow(t, actions) {
  // Not the design system's "uploading": the node is pulling chunks, and the
  // kernel does not distinguish publishing from fetching. app.css gives
  // "incomplete" the same treatment.
  const state = t.complete ? 'complete' : 'incomplete';
  const pct = t.chunksTotal > 0 ? Math.min(100, Math.round((t.chunksHeld / t.chunksTotal) * 100)) : 0;

  const body = el('span', { class: 'list-row-body' },
    el('span', { class: 'list-row-title' }, t.name),
    el('span', { class: 'list-row-subtitle' },
      t.complete
        ? formatBytes(t.bytes)
        : `${formatBytes(t.bytes)} · ${t.chunksHeld} of ${t.chunksTotal} chunks`),
    t.complete
      ? null
      : el('span', { class: 'progress progress-sm', role: 'progressbar',
                     'aria-valuenow': String(pct), 'aria-valuemin': '0', 'aria-valuemax': '100',
                     'aria-label': 'Chunks held', style: { marginTop: 'var(--space-1)' } },
          el('span', { class: 'progress-fill', style: { width: pct + '%' } })),
  );

  return el('div', { class: 'list-row file-row', 'data-state': state, style: { cursor: 'default' } },
    el('span', { class: 'file-kind' }, fileKind(t.name)),
    body,
    el('span', { class: 'list-row-meta' },
      el('span', { class: 'cluster-tight' },
        el('button', {
          class: 'btn btn-sm btn-secondary', type: 'button',
          onclick: () => actions.copyMagnet(t.magnet),
        }, 'Magnet'),
        t.complete
          ? el('button', { class: 'btn btn-sm', type: 'button', onclick: () => actions.save(t.magnet, t.name) }, 'Save')
          : null,
      ),
    ),
  );
}

function publishPanel({ publishing, publishError, actions }) {
  return el('div', { class: 'card' },
    el('div', { class: 'card-body', style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap-tight)' } },
      el('div', { class: 'field' },
        el('label', { for: 'file-publish' }, 'Publish a file'),
        el('input', {
          id: 'file-publish', type: 'file', disabled: publishing,
          onchange: (e) => {
            const f = e.target.files && e.target.files[0];
            if (f) actions.publish(f);
            e.target.value = '';
          },
        }),
        // Stated plainly, because the alternative is a UI that implies privacy
        // the kernel is not providing on this path.
        el('div', { class: 'field-hint' },
          'Published in the clear. Anyone relaying it can read it — the sealed path exists in the core but is not reachable from a browser yet.'),
      ),
      publishing ? el('p', { class: 'text-sm text-muted', style: { margin: '0' } }, 'Chunking and signing…') : null,
      publishError ? el('div', { class: 'field-error', role: 'alert' }, publishError) : null,
    ),
  );
}

function fetchPanel({ fetchValue, fetchError, actions }) {
  const ready = looksLikeMagnet(fetchValue);
  return el('div', { class: 'card' },
    el('div', { class: 'card-body', style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap-tight)' } },
      el('div', { class: 'field' },
        el('label', { for: 'file-magnet' }, 'Fetch by magnet'),
        el('input', {
          id: 'file-magnet', type: 'text', class: 'mono', value: fetchValue,
          placeholder: '32 hex characters', spellcheck: 'false',
          oninput: (e) => actions.setFetchValue(e.target.value),
          onkeydown: (e) => { if (e.key === 'Enter' && ready) actions.fetch(fetchValue.trim()); },
        }),
        el('div', { class: 'field-hint' },
          'The id of a file someone else published. Chunks arrive as peers that hold them answer.'),
      ),
      fetchError ? el('div', { class: 'field-error', role: 'alert' }, fetchError) : null,
      el('div', { class: 'cluster-tight' },
        el('button', {
          class: 'btn btn-sm', type: 'button', disabled: !ready,
          onclick: () => actions.fetch(fetchValue.trim()),
        }, icon(ICONS.folder, { size: '14px' }), 'Fetch'),
      ),
    ),
  );
}
