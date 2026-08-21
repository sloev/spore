// Client-side message formatting (W11/W12). This module is inlined into the
// standalone page as one of the shared-scope modules, so it can use raw regex
// syntax that the page's outer template literal would otherwise rewrite.
//
// Rendering is plain-text-first and injection-safe: the input is HTML-escaped
// *before* any markup is introduced, so a hostile message cannot emit raw HTML.
//
// Attachments (Appendix A of docs/DESIGN.md) are two application-level
// markers, kept canonical:
//   chat file  : 📎 <filename> | spore:<magnet> | <mime>
//   image      : ![name](spore:<magnet>)   (feed posts and any inline image)
// A client that does not parse them just sees the marker text — a safe fallback.

export function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}

// Minimal markdown inline spans: **bold**, *italic*, `code`, [text](url).
// No paragraphs, no raw HTML — just the spans a chat or feed post needs.
export function mdInline(text) {
  let s = escapeHtml(text);
  s = s.replace(/`([^`]+)`/g, (_, c) => '<code>' + c + '</code>');
  s = s.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');
  s = s.replace(/\*([^*]+)\*/g, '<em>$1</em>');
  s = s.replace(/\[([^\]]+)\]\((https?:\/\/[^)\s]+)\)/g,
    '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');
  return s;
}

// A raw magnet<:32hex> reference renders as a one-tap download link; the click
// handler is wired by the frame that renders the output.
function magnetLink(hex) {
  return '<a href="#" class="magnet-link" data-magnet="' + hex +
    '" title="fetch this file">\u21e9 ' + hex.slice(0, 8) + '\u2026</a>';
}

// Inline image (Appendix A): ![name](spore:<magnet>). Rendered as an <img>
// whose src the frame fills in once the bytes are local; until then it shows
// the filename + fetch state. data-magnet carries the digest.
function imageEmbed(name, hex) {
  return '<span class="img-embed" data-magnet="' + hex + '" data-name="' +
    escapeHtml(name) + '"><span class="img-thumb">\u2014 image \u2014</span><br>' +
    '<span class="img-name">' + escapeHtml(name) + '</span></span>';
}

// Chat file attachment (Appendix A): 📎 <filename> | spore:<magnet> | <mime>.
// A chip with the filename and a download affordance.
function fileChip(name, hex, mime) {
  return '<span class="file-chip" data-magnet="' + hex + '" data-name="' +
    escapeHtml(name) + '" data-mime="' + escapeHtml(mime) + '">\u{1F4CE} ' +
    escapeHtml(name) + '</span>';
}

// Full renderer used in both chats and the feed. The image/file markers produce
// HTML, so they are captured into sentinels first, the inline markdown runs on
// the remaining text (escaping it), and the sentinels are restored last so their
// generated markup is never re-escaped.
export function mdWithAttachments(text) {
  const embeds = [];
  let s = String(text);
  s = s.replace(/!\[([^\]]*)\]\(spore:([0-9a-fA-F]{32})\)/g,
    (_, name, hex) => { embeds.push(imageEmbed(name, hex)); return '\x01EMB' + (embeds.length - 1) + '\x02'; });
  s = s.replace(/(?:\n|^)\uD83D\uDCCE ([^\n|]+) \| spore:([0-9a-fA-F]{32}) \| (\S+)(?=\n|$)/g,
    (_, name, hex, mime) => { embeds.push(fileChip(name, hex, mime)); return '\x01EMB' + (embeds.length - 1) + '\x02'; });
  s = mdInline(s);
  s = s.replace(/magnet:([0-9a-fA-F]{32})/g, (_, hex) => magnetLink(hex));
  s = s.replace(/\x01EMB(\d+)\x02/g, (_, i) => embeds[+i]);
  return s;
}

// Backward-compatible alias: the pre-W12 renderer (markdown + raw-magnet links).
export function mdWithMagnet(text) {
  return mdInline(text).replace(/magnet:([0-9a-fA-F]{32})/g, (_, hex) => magnetLink(hex));
}
