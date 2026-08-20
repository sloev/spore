// Client-side message formatting (W11). This module is inlined into the
// standalone page as one of the shared-scope modules, so it can use raw regex
// syntax that the page's outer template literal would otherwise rewrite.
//
// Rendering is plain-text-first and injection-safe: the input is HTML-escaped
// *before* any markup is introduced, so a hostile message cannot emit raw HTML.

export function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}

// Minimal markdown: **bold**, *italic*, `code`, [text](url). No paragraphs, no
// raw HTML — just the inline spans a chat or feed post needs.
export function mdInline(text) {
  let s = escapeHtml(text);
  // Inline code first so its contents survive the other replacements.
  s = s.replace(/`([^`]+)`/g, (_, c) => '<code>' + c + '</code>');
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/\*([^*]+)\*/g, '<em>$1</em>');
  s = s.replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
    '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');
  return s;
}

// A `magnet:<32hex>` reference renders as a link that downloads once fetched;
// the click handler is wired by the frame that renders the output.
export function mdWithMagnet(text) {
  return mdInline(text).replace(/magnet:([0-9a-fA-F]{32})/g, (full, hex) => {
    return '<a href="#" class="magnet-link" data-magnet="' + hex +
      '" title="fetch this file">\u21e9 ' + hex.slice(0, 8) + '&hellip;</a>';
  });
}
