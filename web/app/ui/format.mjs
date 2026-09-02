// Presentation formatting (M10-D). Pure functions, no DOM — so they are
// testable in Node and shared by every screen.
//
// HARDBRUT/3's content rules, applied here rather than repeated per screen:
// digits always, even below ten; sizes pre-formatted with a space; times short
// and local; dates relative for two days then absolute; no ellipsis except for a
// genuine in-flight state.

/** 16-hex address as 3F2A · 9C10 · 88E4 · 001B — grouped so it can be read aloud. */
export function formatAddr(hex) {
  if (!hex) return '';
  return (hex.match(/.{1,4}/g) || []).join(' · ').toUpperCase();
}

/** Shorten to n characters with an ellipsis. Never used on an address. */
export function truncate(s, n) {
  if (!s) return '';
  return s.length > n ? s.slice(0, n - 1) + '…' : s;
}

/** 2.4 MB — one decimal, a real space before the unit. */
export function formatBytes(n) {
  if (!Number.isFinite(n) || n < 0) return '';
  if (n < 1024) return n + ' B';
  const units = ['KB', 'MB', 'GB', 'TB'];
  let v = n / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return (v < 10 ? v.toFixed(1) : Math.round(v)) + ' ' + units[i];
}

/** 09:41, always two digits, always local. */
export function clock(ms) {
  const d = new Date(ms);
  return String(d.getHours()).padStart(2, '0') + ':' + String(d.getMinutes()).padStart(2, '0');
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/**
 * Today / Yesterday for two days, then an absolute date — the rule the design
 * system states, so a day separator never says "3 days ago".
 */
export function dayLabel(ms, now = Date.now()) {
  const d = new Date(ms);
  const t = new Date(now);
  const midnight = (x) => new Date(x.getFullYear(), x.getMonth(), x.getDate()).getTime();
  const days = Math.round((midnight(t) - midnight(d)) / 86400000);
  if (days === 0) return 'Today';
  if (days === 1) return 'Yesterday';
  return d.getDate() + ' ' + MONTHS[d.getMonth()];
}

/** A short relative stamp for list rows: 09:41 today, else 2d, else a date. */
export function shortWhen(ms, now = Date.now()) {
  const label = dayLabel(ms, now);
  if (label === 'Today') return clock(ms);
  if (label === 'Yesterday') return '1d';
  const days = Math.round((now - ms) / 86400000);
  return days < 7 ? days + 'd' : label;
}

/** The file-kind block in an attachment row: the extension, upper-case, max 4. */
export function fileKind(name) {
  const dot = (name || '').lastIndexOf('.');
  if (dot < 0 || dot === name.length - 1) return 'BIN';
  return name.slice(dot + 1).toUpperCase().slice(0, 4);
}
