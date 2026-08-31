// The web node's M9 delivery-status pass. This module is inlined into the
// standalone page (build-standalone.mjs's shared-scope `modules` list) and
// also imported directly by codec-test.mjs — a pure function, so both work
// off the same source rather than a copy that can drift.
//
// Three states, chosen to be exactly what's observable rather than what
// sounds reassuring. The core has no "still travelling" event and no "gave
// up" event: SPEC §5.4d's resend backoff exhausts in minutes and silently
// drops its `Pending` entry either way, long before the envelope's own
// expiry. So "not yet delivered" is genuinely ambiguous between "still
// actively resending" and "relying on passive custody until it expires," and
// both are real possibilities that can't be told apart from outside the
// core. What *can* be told apart honestly: delivered (a receipt came back),
// expired (the envelope's own lifetime has passed with no receipt, ever), or
// still within that lifetime — "still travelling" for all of it, because
// claiming to know the difference between active resending and passive
// custody would be a status line inventing precision the protocol doesn't
// have.

/**
 * @param {{fromMe?: boolean, id?: string|null, delivered?: boolean, ts: number}} m
 * @param {number} expirySecs - `spore.defaultMessageExpirySecs()`
 * @param {number} [nowMs] - injectable for tests; defaults to wall clock
 * @returns {string} an HTML fragment, or '' for a message this doesn't apply to
 */
export function deliveryStatus(m, expirySecs, nowMs = Date.now()) {
  if (!m.fromMe || !m.id) return '';
  if (m.delivered) return ' <span class="cnt" style="color:var(--ok)">✓ delivered</span>';
  const expired = nowMs > m.ts + expirySecs * 1000;
  return expired
    ? ' <span class="cnt" style="color:var(--warn)">expired — undelivered</span>'
    : ' <span class="cnt">· still travelling</span>';
}
