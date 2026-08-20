// Unit tests for the inline markdown + magnet renderer (W11). Pure functions, no
// DOM: escapeHtml / mdInline / mdWithMagnet.
//
//   node web/ui/markdown.test.mjs
import assert from 'node:assert';
import { escapeHtml, mdInline, mdWithMagnet } from './markdown.mjs';

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

// A link that is not http(s) is left escaped as plain text, never made clickable.
const js = mdInline('[x](javascript:alert(1))');
assert.ok(!js.includes('<a '), 'non-http links are never made clickable');
assert.ok(js.includes('&lt;') === false, 'plain text is not double-escaped here');

console.log('MARKDOWN OK — inline renderer is XSS-safe and formats all four spans');
