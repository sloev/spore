// Auto-generated SPORE bindings (koffi) — do not edit; run bindings/generate.py.
//
//   npm install koffi
//   SPORE_LIB=../../target/release/libspore.so node your.js
//
// Every function takes/returns Node Buffers; failing functions return null.
'use strict';
const koffi = require('koffi');
const path = require('path');

function loadLib() {
  const names = ['libspore.so', 'libspore.dylib', 'spore.dll'];
  const tried = [];
  if (process.env.SPORE_LIB) tried.push(process.env.SPORE_LIB);
  for (const n of names) {
    tried.push(path.join(__dirname, '..', '..', 'target', 'release', n));
    tried.push(n);
  }
  let last;
  for (const p of tried) {
    try { return koffi.load(p); } catch (e) { last = e; }
  }
  throw last;
}
const _l = loadLib();

const SporeBytes = koffi.struct('SporeBytes', { data: 'uint8_t *', len: 'size_t' });
const _free = _l.func('void spore_bytes_free(SporeBytes b)');

function take(b) {
  if (!b.data) return null;
  const out = Buffer.from(koffi.decode(b.data, koffi.array('uint8_t', b.len, 'Array')));
  _free(b);
  return out;
}


const _keypair = _l.func('void spore_keypair(_Out_ uint8_t *out_sk, _Out_ uint8_t *out_pk)');

function keypair() {
  const out_sk = Buffer.alloc(32);
  const out_pk = Buffer.alloc(32);
  _keypair(out_sk, out_pk);
  return [out_sk, out_pk];
}

const _prekey = _l.func('void spore_prekey(_Out_ uint8_t *out_sec, _Out_ uint8_t *out_pub)');

function prekey() {
  const out_sec = Buffer.alloc(32);
  const out_pub = Buffer.alloc(32);
  _prekey(out_sec, out_pub);
  return [out_sec, out_pub];
}

const _addr_of = _l.func('void spore_addr_of(uint8_t *pk, _Out_ uint8_t *out)');

function addr_of(pk) {
  const out = Buffer.alloc(8);
  _addr_of(pk, out);
  return out;
}

const _topic_of = _l.func('void spore_topic_of(uint8_t *s, size_t s_len, _Out_ uint8_t *out)');

function topic_of(s) {
  const out = Buffer.alloc(8);
  _topic_of(s, s.length, out);
  return out;
}

const _seal = _l.func('SporeBytes spore_seal(uint8_t *msg, size_t msg_len, uint8_t *recip_prekey)');

function seal(msg, recip_prekey) {
  return take(_seal(msg, msg.length, recip_prekey));
}

const _open = _l.func('SporeBytes spore_open(uint8_t *sealed, size_t sealed_len, uint8_t *prekey_sec)');

function open(sealed, prekey_sec) {
  return take(_open(sealed, sealed.length, prekey_sec));
}

const _topic_seal = _l.func('SporeBytes spore_topic_seal(uint8_t *msg, size_t msg_len, uint8_t *psk)');

function topic_seal(msg, psk) {
  return take(_topic_seal(msg, msg.length, psk));
}

const _topic_open = _l.func('SporeBytes spore_topic_open(uint8_t *ct, size_t ct_len, uint8_t *psk)');

function topic_open(ct, psk) {
  return take(_topic_open(ct, ct.length, psk));
}

const _armor_wrap = _l.func('SporeBytes spore_armor_wrap(uint8_t *env, size_t env_len)');

function armor_wrap(env) {
  return take(_armor_wrap(env, env.length));
}

const _armor_unwrap = _l.func('SporeBytes spore_armor_unwrap(uint8_t *text, size_t text_len)');

function armor_unwrap(text) {
  return take(_armor_unwrap(text, text.length));
}

const _message_new = _l.func('SporeBytes spore_message_new(uint8_t *sk, uint8_t *dest, uint32_t expiry, uint8_t *payload, size_t payload_len)');

function message_new(sk, dest, expiry, payload) {
  return take(_message_new(sk, dest, expiry, payload, payload.length));
}

const _message_verify = _l.func('uint8_t spore_message_verify(uint8_t *wire, size_t wire_len)');

function message_verify(wire) {
  return _message_verify(wire, wire.length) !== 0;
}

const _message_id = _l.func('uint8_t spore_message_id(uint8_t *wire, size_t wire_len, _Out_ uint8_t *out)');

function message_id(wire) {
  const out = Buffer.alloc(16);
  const ok = _message_id(wire, wire.length, out) !== 0;
  return ok ? out : null;
}

const _message_payload = _l.func('SporeBytes spore_message_payload(uint8_t *wire, size_t wire_len)');

function message_payload(wire) {
  return take(_message_payload(wire, wire.length));
}

module.exports = { keypair, prekey, addr_of, topic_of, seal, open, topic_seal, topic_open, armor_wrap, armor_unwrap, message_new, message_verify, message_id, message_payload };
