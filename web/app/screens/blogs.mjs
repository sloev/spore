// Blogs (M10-D) — feeds this node follows, and what has arrived on them.
//
// The last screen of M10-D. A feed is SPORE's one-to-many primitive: a topic is
// `topic_of(name)`, an 8-byte hash, and anything published to it floods to every
// node that follows it.
//
// **Two facts the screen states rather than hides.**
//
// Following is public. §4's ANNOUNCE carries the whole topic set, so every node
// that hears this one learns what it reads. That is not an implementation detail
// a UI may quietly omit — it is how a neighbour knows the traffic is worth
// relaying here, and it cannot be turned off while still receiving the feed.
// Someone deciding whether to follow a topic deserves to know that following it
// is itself broadcast.
//
// And a post is cleartext and unauthenticated unless the core says otherwise.
// Anyone may publish to any topic; there is no ownership of a feed name. A post
// whose sender the core did not authenticate shows as "unsigned" and never
// borrows a name from anywhere.
//
// Pure render functions of a view model. No client, no store access.

import { el, icon } from '../ui/dom.mjs';
import { ICONS } from '../ui/icons.mjs';
import { formatAddr, shortWhen, truncate } from '../ui/format.mjs';

/**
 * @param {Object} vm
 * @param {Array}  vm.topics      [{ topicHex, name, posts, latest }]
 * @param {string|null} vm.openTopic  topicHex of the open feed
 * @param {Array}  vm.posts       posts on the open feed, oldest first
 * @param {string} vm.followValue
 * @param {string|null} vm.followError
 * @param {string} vm.draft
 * @param {boolean} vm.posting
 * @param {Object} vm.actions
 */
export function renderBlogs(vm) {
  const { topics, openTopic, posts, followValue, followError, draft, posting, actions } = vm;
  const open = topics.find((t) => t.topicHex === openTopic) || null;

  return el('div', { class: 'pane-body scroll-y', style: { padding: 'var(--pad)' } },
    el('div', { style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap)' } },
      open ? feedView(open, posts, draft, posting, actions) : feedList(vm, topics, followValue, followError, actions),
    ),
  );
}

// ------------------------------------------------------------------ the list

function feedList(vm, topics, followValue, followError, actions) {
  const name = (followValue || '').trim();
  return el('div', { style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap)' } },

    el('div', { class: 'card' },
      el('div', { class: 'card-body', style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap-tight)' } },
        el('div', { class: 'field' },
          el('label', { for: 'topic-follow' }, 'Follow a feed'),
          el('input', {
            id: 'topic-follow', type: 'text', value: followValue,
            placeholder: 'a topic name, e.g. ridge-weather', spellcheck: 'false',
            oninput: (e) => actions.setFollowValue(e.target.value),
            onkeydown: (e) => { if (e.key === 'Enter' && name) actions.follow(name); },
          }),
          // The one thing someone deciding to follow a feed most needs to know.
          el('div', { class: 'field-hint' },
            'Every node that hears this one learns which feeds it follows — the topic set travels in each announce. There is no private way to follow.'),
        ),
        followError ? el('div', { class: 'field-error', role: 'alert' }, followError) : null,
        el('div', { class: 'cluster-tight' },
          el('button', {
            class: 'btn btn-sm', type: 'button', disabled: !name,
            onclick: () => actions.follow(name),
          }, icon(ICONS.plus, { size: '14px' }), 'Follow'),
        ),
      ),
    ),

    topics.length === 0
      ? el('div', { class: 'empty' },
          el('div', { class: 'empty-mark' }, '≡'),
          el('h3', {}, 'No feeds'),
          el('p', {}, 'Follow a topic by name. Anyone who publishes to that name reaches everyone following it — a feed has no owner.'))
      : el('div', { class: 'list' }, topics.map((t) => topicRow(t, actions))),
  );
}

function topicRow(t, actions) {
  const latest = t.latest;
  return el('button', {
    class: 'list-row', type: 'button', onclick: () => actions.open(t.topicHex),
  },
    el('span', { class: 'list-row-body' },
      // A topic followed on another device has no local name, and the bare
      // address is the honest thing to show rather than a fabricated label.
      el('span', { class: 'list-row-title' + (t.name ? '' : ' mono') },
        t.name || formatAddr(t.topicHex)),
      el('span', { class: 'list-row-subtitle' },
        latest ? truncate(latest.body, 60) : 'Nothing received yet'),
    ),
    el('span', { class: 'list-row-meta' },
      latest ? el('span', {}, shortWhen(latest.at * 1000)) : null,
      t.posts > 0 ? el('span', { class: 'badge badge-quiet mono text-xs' }, String(t.posts)) : null,
    ),
  );
}

// ------------------------------------------------------------------ one feed

function feedView(topic, posts, draft, posting, actions) {
  return el('div', { style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap)' } },

    el('div', { class: 'cluster-tight', style: { justifyContent: 'space-between', flexWrap: 'wrap' } },
      el('button', { class: 'btn btn-sm btn-secondary', type: 'button', onclick: actions.close },
        icon(ICONS.chevronLeft, { size: '14px' }), 'All feeds'),
      el('button', { class: 'btn btn-sm btn-danger', type: 'button', onclick: () => actions.unfollow(topic.topicHex) },
        'Unfollow'),
    ),

    el('div', { class: 'card' },
      el('div', { class: 'card-body', style: { display: 'flex', flexDirection: 'column', gap: 'var(--gap-tight)' } },
        el('h3', { style: { margin: '0' } }, topic.name || formatAddr(topic.topicHex)),
        el('p', { class: 'text-xs mono text-muted', style: { margin: '0' } }, formatAddr(topic.topicHex)),
        composer(topic, draft, posting, actions),
      ),
    ),

    posts.length === 0
      ? el('div', { class: 'empty' },
          el('div', { class: 'empty-mark' }, '·'),
          el('h3', {}, 'Nothing yet'),
          el('p', {}, 'Posts appear as they arrive. That needs a bridge, and someone publishing to this topic.'))
      : el('div', { class: 'list' }, [...posts].reverse().map(postRow)),
  );
}

function postRow(p) {
  return el('div', { class: 'list-row', style: { cursor: 'default', alignItems: 'flex-start' } },
    el('span', { class: 'list-row-body' },
      el('span', { class: 'list-row-subtitle mono text-xs' },
        // Never a claimed name: a feed post is flooded and need not be signed,
        // so if the core did not authenticate a sender there is not one to show.
        p.from ? formatAddr(p.from) : 'unsigned'),
      el('span', { style: { whiteSpace: 'pre-wrap', wordBreak: 'break-word' } }, p.body),
    ),
    el('span', { class: 'list-row-meta' }, shortWhen(p.at * 1000)),
  );
}

/**
 * Same contract as the chat composer: the textarea owns its own text and toggles
 * the button in place, instead of driving a re-render on every keystroke that
 * would rebuild the field mid-typing and throw away focus and the caret.
 *
 * It also has to *read* its own value when publishing. The button's handler
 * closes over the draft as it was at render time, which is the empty string —
 * so a version that published `draft` published nothing, and a version that
 * derived `disabled` from it could never be enabled at all.
 */
function composer(topic, draft, posting, actions) {
  const button = el('button', {
    class: 'btn btn-sm', type: 'button', disabled: posting || !draft.trim(),
  }, posting ? 'Publishing…' : 'Publish');

  const area = el('textarea', {
    id: 'topic-post', rows: '3', value: draft, disabled: posting,
    placeholder: 'Anyone following this topic will receive it',
    oninput: (e) => {
      actions.setDraft(e.target.value);
      button.disabled = posting || !e.target.value.trim();
    },
  });

  button.addEventListener('click', () => {
    const body = area.value.trim();
    if (body) actions.post(topic.topicHex, body);
  });

  return el('div', {},
    el('div', { class: 'field' },
      el('label', { for: 'topic-post' }, 'Post to this feed'),
      area,
      el('div', { class: 'field-hint' },
        'Cleartext. A feed is one-to-many, so there is nobody in particular to seal it to.'),
    ),
    el('div', { class: 'cluster-tight' }, button),
  );
}
