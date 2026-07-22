const s = require('./spore.js');
const assert = require('assert');

const [sk, pk] = s.keypair();
assert.strictEqual(sk.length, 32); assert.strictEqual(pk.length, 32);
const addr = s.addr_of(pk);
assert.strictEqual(addr.length, 8);

const wire = s.message_new(sk, addr, 1700000000, Buffer.from('hello from node'));
assert.strictEqual(s.message_verify(wire), true);
assert.deepStrictEqual(s.message_payload(wire), Buffer.from('hello from node'));
const id = s.message_id(wire);
assert(id && id.length === 16);

const bad = Buffer.from(wire); bad[bad.length-1] ^= 0xFF;
assert.strictEqual(s.message_verify(bad), false);

const [sec, pub] = s.prekey();
const sealed = s.seal(Buffer.from('secret'), pub);
assert.deepStrictEqual(s.open(sealed, sec), Buffer.from('secret'));
assert.strictEqual(s.open(sealed, Buffer.alloc(32)), null);

const psk = Buffer.from(Array.from({length:32}, (_,i)=>i));
const ct = s.topic_seal(Buffer.from('members'), psk);
assert.deepStrictEqual(s.topic_open(ct, psk), Buffer.from('members'));

const text = s.armor_wrap(wire);
assert(text.toString().startsWith('~S1.'));
assert.deepStrictEqual(s.armor_unwrap(Buffer.concat([Buffer.from('x '), text])), wire);

console.log('NODE OK — all roundtrips pass');
