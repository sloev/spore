// ContactStore rules (M10-D).
//
//   node web/app/stores/contacts.test.mjs

import assert from 'node:assert';
import { ContactStore, contactRows } from './contacts.mjs';
import { memoryAdapter } from '../spore-client.mjs';

let failures = 0;
async function test(name, fn) {
  try { await fn(); console.log('  ok  ' + name); }
  catch (err) { failures++; console.log('FAIL  ' + name + '\n      ' + (err && err.message)); }
}

const ADA = '3f2a9c1088e4001b';
const RAE = '9c10e4883f2a001b';
const JO = '88e4001b3f2a9c10';

const peer = (addrHex, claimedName, ageSecs = 10, hasPrekey = true) =>
  ({ addrHex, claimedName, ageSecs, hasPrekey });

// ------------------------------------------------- the claim/label separation

await test('a claimed name is never stored as if the user chose it', async () => {
  const s = new ContactStore();
  s.setFollowing(ADA, true); // touching the row must not import any claim
  assert.strictEqual(s.labelFor(ADA), null, 'no label was typed, so there is none');
});

await test('labelFor does not fall back to an announced name', async () => {
  // The fallback is the screen's job, and the screen has to mark it as a claim.
  // Doing it here would let an unauthenticated name become "the contact's name".
  const s = new ContactStore();
  const rows = contactRows([peer(ADA, 'Ada Lovelace')], s, { view: 'seen' });
  assert.strictEqual(s.labelFor(ADA), null);
  assert.strictEqual(rows[0].name, 'Ada Lovelace');
  assert.strictEqual(rows[0].nameIsClaim, true, 'and it must be flagged as a claim');
});

await test('a user label wins over a claim and stops being a claim', async () => {
  const s = new ContactStore();
  s.setLabel(ADA, 'Ada (work)');
  const [row] = contactRows([peer(ADA, 'Ada Lovelace')], s, { view: 'contacts' });
  assert.strictEqual(row.name, 'Ada (work)');
  assert.strictEqual(row.claimedName, 'Ada Lovelace', 'the claim is still visible for context');
  assert.strictEqual(row.nameIsClaim, false);
});

await test('an address with no label and no claim falls back to the address', async () => {
  const s = new ContactStore();
  s.setFollowing(JO, true);
  const [row] = contactRows([], s, { view: 'contacts' });
  assert.strictEqual(row.name, null, 'the screen formats the address; the row does not invent a name');
  assert.strictEqual(row.addr, JO);
});

// -------------------------------------------------------------------- writes

await test('an empty label clears it without dropping the row', async () => {
  const s = new ContactStore();
  s.setLabel(ADA, 'Ada');
  s.setBlocked(ADA, true);
  s.setLabel(ADA, '   ');
  assert.strictEqual(s.labelFor(ADA), null);
  assert.strictEqual(s.isBlocked(ADA), true, 'blocking must survive clearing the label');
});

await test('following and blocking are independent', async () => {
  const s = new ContactStore();
  s.setFollowing(ADA, true);
  s.setBlocked(ADA, true);
  assert.strictEqual(s.isFollowing(ADA), true);
  assert.strictEqual(s.isBlocked(ADA), true);
  s.setBlocked(ADA, false);
  assert.strictEqual(s.isFollowing(ADA), true);
});

await test('following() is the Blogs subscription list', async () => {
  const s = new ContactStore();
  s.setFollowing(ADA, true);
  s.setLabel(RAE, 'Rae');
  assert.deepStrictEqual(s.following().map((c) => c.addr), [ADA]);
});

// ----------------------------------------------------------------- the views

await test('contacts and seen are different lists, not a filter of one', async () => {
  const s = new ContactStore();
  s.setLabel(ADA, 'Ada');
  const peers = [peer(ADA, 'Ada Lovelace'), peer(RAE, 'Rae Kim')];

  const contacts = contactRows(peers, s, { view: 'contacts' }).map((r) => r.addr);
  const seen = contactRows(peers, s, { view: 'seen' }).map((r) => r.addr);

  assert.deepStrictEqual(contacts, [ADA], 'only what the user kept');
  assert.deepStrictEqual(seen, [RAE], 'heard from, but not yet a contact');
});

await test('a contact never heard from still appears under contacts', async () => {
  // You can add an address before that node has ever announced to you.
  const s = new ContactStore();
  s.setLabel(JO, 'Jo');
  const [row] = contactRows([], s, { view: 'contacts' });
  assert.strictEqual(row.addr, JO);
  assert.strictEqual(row.heard, false);
  assert.strictEqual(row.hasPrekey, false, 'so nothing to them can be sealed yet');
});

await test('seen is ordered freshest first', async () => {
  const s = new ContactStore();
  const peers = [peer(ADA, 'Ada', 300), peer(RAE, 'Rae', 5)];
  assert.deepStrictEqual(contactRows(peers, s, { view: 'seen' }).map((r) => r.addr), [RAE, ADA]);
});

await test('search matches label, claimed name and address', async () => {
  const s = new ContactStore();
  s.setLabel(ADA, 'Ada (work)');
  s.setLabel(RAE, null);
  s.setFollowing(RAE, true);
  const peers = [peer(ADA, 'Ada Lovelace'), peer(RAE, 'Rae Kim')];

  const byLabel = contactRows(peers, s, { view: 'contacts', query: 'work' }).map((r) => r.addr);
  const byClaim = contactRows(peers, s, { view: 'contacts', query: 'rae kim' }).map((r) => r.addr);
  const byAddr = contactRows(peers, s, { view: 'contacts', query: '9c10e' }).map((r) => r.addr);

  assert.deepStrictEqual(byLabel, [ADA]);
  assert.deepStrictEqual(byClaim, [RAE]);
  assert.deepStrictEqual(byAddr, [RAE]);
});

// -------------------------------------------------------------- persistence

await test('labels survive a reload', async () => {
  const storage = memoryAdapter();
  const a = new ContactStore({ storage });
  a.setLabel(ADA, 'Ada');
  a.setBlocked(RAE, true);
  await a.save();

  const b = new ContactStore({ storage });
  await b.load();
  assert.strictEqual(b.labelFor(ADA), 'Ada');
  assert.strictEqual(b.isBlocked(RAE), true);
});

await test('a corrupt blob starts empty rather than wiping the user\'s labels', async () => {
  const storage = memoryAdapter({ 'spore.contacts': 'not json at all' });
  const s = new ContactStore({ storage });
  await s.load();
  assert.strictEqual(s.all().length, 0);
  assert.strictEqual(await storage.get('spore.contacts'), 'not json at all');
});

console.log(failures ? '\n' + failures + ' failing' : '\nall passing');
process.exit(failures ? 1 : 0);
