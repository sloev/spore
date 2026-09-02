// Minimal DOM helpers (M10-D). No framework: the web node ships as one file
// with zero external requests, and a runtime would be most of that budget.
//
// The rule that keeps this honest: screens are pure render functions of a view
// model. They never read SporeClient directly and never hold state — main.mjs
// owns state, calls render, and hands the result down.

/**
 * Build an element. Children may be nodes, strings, or nested arrays; null and
 * undefined are skipped so `cond && el(...)` reads naturally.
 *
 *   el('div', { class: 'card' }, el('p', {}, 'hello'))
 *
 * Attributes are set as attributes except: `class`, `style` (object or string),
 * `on*` handlers, and boolean DOM props (checked, disabled, value).
 */
export function el(tag, attrs = {}, ...children) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v === null || v === undefined || v === false) continue;
    if (k.startsWith('on') && typeof v === 'function') {
      node.addEventListener(k.slice(2).toLowerCase(), v);
    } else if (k === 'style' && typeof v === 'object') {
      Object.assign(node.style, v);
    } else if (k === 'value' || k === 'checked' || k === 'disabled') {
      node[k] = v;
    } else if (k === 'html') {
      // Only ever called with markup this app generated (icons, rendered
      // markdown that ui/markdown.mjs already escaped) — never with a peer's
      // bytes. Anything from the mesh goes through text nodes.
      node.innerHTML = v;
    } else {
      node.setAttribute(k, v === true ? '' : String(v));
    }
  }
  append(node, children);
  return node;
}

function append(node, children) {
  for (const c of children) {
    if (c === null || c === undefined || c === false) continue;
    if (Array.isArray(c)) append(node, c);
    else node.appendChild(typeof c === 'string' || typeof c === 'number' ? document.createTextNode(String(c)) : c);
  }
}

/** Replace a container's children in one shot. */
export function mount(container, ...children) {
  container.replaceChildren();
  append(container, children);
  return container;
}

/** An SVG icon from the set in icons.mjs, sized to the current font. */
export function icon(markup, { size = '1em', label = null } = {}) {
  const span = el('span', {
    class: 'icon',
    'aria-hidden': label ? null : 'true',
    'aria-label': label,
    role: label ? 'img' : null,
    style: { width: size, height: size, display: 'inline-flex', flex: 'none' },
    html: markup,
  });
  return span;
}
