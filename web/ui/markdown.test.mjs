// Unit tests for the inline markdown + magnet + attachment renderer (W11/W12).
// Pure functions, no DOM: escapeHtml / mdInline / mdWithMagnet / mdWithAttachments.
//
//   node web/ui/markdown.test.mjs
import assert from 'node:assert';
import { escapeHtml, mdInline, mdWithMagnet, mdWithAttachments } from './markdown.mjs';

// HTML is escaped before any markup is introduced — XSS-safe by construction.
assert.strictEqual(escapeHtml('<script>alert(1)</script>'),
  '&lt;script&gt;alert(1)&lt;/script&gt;', 'escapes angle brackets + quotes');
assert.strictEqual(mdInline('<img src=x onerror=alert(1)>'),
  '&lt;img src=x onerror=alert(1)&gt;', 'a hostile message cannot emit raw HTML');

// The four inline spans render.
assert.strictEqual(mdInline('**bold**'), '<strong>bold</strong>', 'bold');
assert.strictEqual(mdInline('*italic*'), '<em>italic</em>', 'italic');
assert.strictEqual(mdInline('`code`'), '<code>code</code>', 'code');
assert.strictEqual(mdInline('[site](https://example.com)'),
  '<a href="https://example.com" target="_blank" rel="noopener noreferrer">site</a>', 'link');

// Mixed, in one message.
assert.strictEqual(mdInline('**b** and *i* and `c` and [l](https://x.y)'),
  '<strong>b</strong> and <em>i</em> and <code>c</code> and <a href="https://x.y" target="_blank" rel="noopener noreferrer">l</a>',
  'mixed spans');

// A magnet reference becomes a download link with its hex preserved.
const out = mdWithMagnet('here magnet:aaaabbbbccccddddeeeeffff00001111 done');
assert.ok(out.includes('class="magnet-link"'), 'magnet link class present');
assert.ok(out.includes('data-magnet="aaaabbbbccccddddeeeeffff00001111"'), 'hex preserved');
assert.ok(!out.includes('magnet:aaaabbbb'), 'the raw token is replaced');

// Inline image (Appendix A): ![name](spore:<magnet>) → .img-embed.
const imgOut = mdWithAttachments('look ![pic.jpg](spore:aaaabbbbccccddddeeeeffff00001111) nice');
assert.ok(imgOut.includes('class="img-embed"'), 'image marker becomes an embed');
assert.ok(imgOut.includes('data-magnet="aaaabbbbccccddddeeeeffff00001111"'), 'image magnet preserved');
assert.ok(imgOut.includes('pic.jpg'), 'image name kept');
assert.ok(!imgOut.includes('!['), 'raw image marker replaced');

// Chat file marker (Appendix A): 📎 name | spore:<magnet> | mime → .file-chip.
const fileOut = mdWithAttachments('📎 paper.pdf | spore:aaaabbbbccccddddeeeeffff00001111 | application/pdf');
assert.ok(fileOut.includes('class="file-chip"'), 'file marker becomes a chip');
assert.ok(fileOut.includes('data-mime="application/pdf"'), 'mime preserved');
assert.ok(fileOut.includes('paper.pdf'), 'filename kept');

// A hostile filename inside the image marker is still escaped (no raw HTML).
const evil = mdWithAttachments('![<b>x</b>](spore:aaaabbbbccccddddeeeeffff00001111)');
assert.ok(!evil.includes('<b>'), 'hostile name is escaped, not emitted as HTML');

// A link that is not http(s) is left escaped as plain text, never made clickable.
const js = mdInline('[x](javascript:alert(1))');
assert.ok(!js.includes('<a '), 'non-http links are never made clickable');

console.log('MARKDOWN OK — spans + magnet + image/file attachments, XSS-safe');
