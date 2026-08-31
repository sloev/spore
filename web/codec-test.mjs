// Codec tests for the browser transports' pure, off-device logic — the parts
// that can be exercised without real hardware or a live peer, so the 🧪 bridges
// still have a regression net for their wire formats. Run: `node web/codec-test.mjs`.
//
//   * KISS framing (matches src/kiss.rs)
//   * Meshtastic MeshPacket protobuf (byte-identical to the Rust bridge)
//   * the 16-FSK audio modem (bit-compatible with bridge::audio)
//   * NDEF records for Web NFC — browser-only, since a dependency-free NFC
//     bridge for the daemon is not possible, so there is no Rust twin to match
import { kissFrame, KissDeframer } from './transports/kiss.mjs';
import { modulate, Demod } from './transports/audio.mjs';
import { encodeMeshPacket, decodeMeshPacket } from './transports/meshtastic.mjs';
import { encodeNdef, decodeNdef, SPORE_MIME } from './transports/webnfc.mjs';
import { deliveryStatus } from './ui/delivery-status.mjs';

let fails = 0;
const eq = (a, b) => Buffer.compare(Buffer.from(a), Buffer.from(b)) === 0;
function check(name, cond) {
  console.log((cond ? 'ok   ' : 'FAIL ') + name);
  if (!cond) fails++;
}
const startsWith = (u8, prefix) => prefix.every((b, i) => u8[i] === b);

// --- KISS: escapes + split reads --------------------------------------------
{
  const env = new Uint8Array([1, 2, 0xc0, 3, 0xdb, 0xdc, 0, 0xff, 0xc0]);
  const framed = kissFrame(env);
  check('kiss frame is C0 00 … C0', framed[0] === 0xc0 && framed[1] === 0x00 && framed[framed.length - 1] === 0xc0);
  const d = new KissDeframer();
  const out = [...d.push(framed.subarray(0, 4)), ...d.push(framed.subarray(4))];
  check('kiss roundtrip w/ escapes across split reads', out.length === 1 && eq(out[0], env));
}

// --- Meshtastic MeshPacket: exact Rust byte layout + roundtrip ---------------
{
  const env = new TextEncoder().encode('the dam holds');
  const pkt = Uint8Array.from(encodeMeshPacket(Array.from(env), 0x1234abcd, 0xffffffff, 0xdeadbeef));
  // from(field1 varint) + to(field2 varint) + decoded(field4 len-delim) prefix —
  // locked against the Rust bridge::meshtastic encoder.
  check('meshtastic exact wire prefix (Rust interop)',
    startsWith(pkt, [0x08, 0xcd, 0xd7, 0xd2, 0x91, 0x01, 0x10, 0xff, 0xff, 0xff, 0xff, 0x0f, 0x22]));
  const mp = decodeMeshPacket(pkt);
  check('meshtastic portnum = 256 (PRIVATE_APP)', mp && mp.portnum === 256);
  check('meshtastic from node preserved', mp && mp.from === 0x1234abcd);
  check('meshtastic payload roundtrip', mp && eq(mp.payload, env));
}

// --- Audio modem: roundtrip under lead-in silence + noise --------------------
{
  for (const p of [new TextEncoder().encode('SPORE over sound'), new Uint8Array([]), new Uint8Array([42, 7])]) {
    const pcm = modulate(p);
    const lead = 500, tail = 500;
    const sig = new Float32Array(lead + pcm.length + tail);
    for (let i = 0; i < pcm.length; i++) sig[lead + i] = pcm[i] + (Math.random() - 0.5) * 0.02;
    const d = new Demod();
    const got = [];
    for (let i = 0; i < sig.length; i += 4096) got.push(...d.push(sig.subarray(i, i + 4096)));
    check('audio modem roundtrip ' + p.length + 'B under noise', got.length === 1 && eq(got[0], p));
  }
}


// --- NDEF: short + long records, and everything a tag might hold instead ------
{
  const env = Uint8Array.from({ length: 40 }, (_, i) => (i * 3) & 0xff);
  const msg = encodeNdef(env);
  check('ndef: short record round-trips', eq(decodeNdef(msg), env));
  check('ndef: short-record flag set below 256 B', (msg[0] & 0x10) !== 0);

  // Past 255 bytes NDEF switches to the 4-byte length field.
  const big = Uint8Array.from({ length: 700 }, (_, i) => i & 0xff);
  const bigMsg = encodeNdef(big);
  check('ndef: long record round-trips', eq(decodeNdef(bigMsg), big));
  check('ndef: short-record flag clear at 700 B', (bigMsg[0] & 0x10) === 0);

  // A tag is written by whoever held it last, so the decoder has to survive
  // whatever is on it — not just records we wrote.
  check('ndef: empty input rejected', decodeNdef(new Uint8Array(0)) === null);
  check('ndef: null rejected', decodeNdef(null) === null);

  const url = new Uint8Array([0xd1, 0x01, 0x05, 0x55, 0x01, 0x61, 0x62, 0x63, 0x64]);
  check('ndef: a URL tag is not ours', decodeNdef(url) === null);

  const wrongMime = encodeNdef(env);
  wrongMime[4] = 'X'.charCodeAt(0); // corrupt the type string
  check('ndef: another MIME type is not ours', decodeNdef(wrongMime) === null);

  // A length that runs past the buffer must be refused, not trusted.
  const lying = encodeNdef(env);
  lying[2] = 0xff;
  check('ndef: payload length past the end refused', decodeNdef(lying) === null);

  let truncOk = true;
  for (let cut = 0; cut < msg.length; cut++) {
    if (decodeNdef(msg.subarray(0, cut)) !== null) truncOk = false;
  }
  check('ndef: every truncation rejected', truncOk);
  check('ndef: mime type is the documented one', SPORE_MIME === 'application/x-spore');
}

// ---------------------------------------------------------------------------
// Private-group invite (W7) — the same kind of Rust twin as the codecs above:
// the string a browser hands a person must be the string `src/invite.rs` will
// accept, or the invite opens a room with one member in it. The binding is a
// thin marshal over wasm, which is precisely where an offset bug would hide.
// Skipped rather than failed when the wasm is not built, so this file still
// runs on its own; CI always builds it first.
// ---------------------------------------------------------------------------
{
  const wasmPath = new URL('../target/wasm32-unknown-unknown/release/spore.wasm', import.meta.url);
  const fs = await import('node:fs');
  if (!fs.existsSync(wasmPath)) {
    console.log('skip group invite — wasm not built (cargo build --release --lib --target wasm32-unknown-unknown)');
  } else {
    const { loadSpore } = await import('./spore.mjs');
    const spore = await loadSpore(fs.readFileSync(wasmPath));
    const key = Uint8Array.from({ length: 32 }, (_, i) => i * 7 & 0xff);
    const name = 'Book club & 🍄';
    const line = spore.groupInviteEncode(name, key);

    check('group invite: prefix is spore-group:', line.startsWith('spore-group:'));
    const g = spore.groupInviteDecode(line);
    check('group invite: round-trips the name through percent-encoding', g && g.name === name);
    check('group invite: carries the key byte-for-byte', g && eq(g.key, key));
    check('group invite: a flipped key nibble fails the checksum',
      spore.groupInviteDecode(line.replace('spore-group:00', 'spore-group:01')) === null);
    check('group invite: a renamed invite fails the checksum',
      spore.groupInviteDecode(line.replace('n=Book', 'n=Payroll')) === null);
    check('group invite: truncation is refused',
      spore.groupInviteDecode(line.slice(0, -2)) === null);
    check('group invite: an address invite is not a group invite',
      spore.groupInviteDecode('spore:1c9a4f0e77b32d51?n=Jo&k=3f2a') === null);
    check('group invite: junk is refused', spore.groupInviteDecode('hello') === null);
    // What a paste out of a chat client actually looks like.
    check('group invite: surrounding whitespace still decodes',
      spore.groupInviteDecode(`  ${line}\n`) !== null);
  }
}

// ---------------------------------------------------------------------------
// Delivery status (M9) — the three states a DM's status line can honestly
// show. `nowMs` is injected so the expiry boundary is tested deterministically
// rather than racing the wall clock.
// ---------------------------------------------------------------------------
{
  const DAY = 86400 * 1000;
  const EXPIRY_SECS = 7 * 86400;
  const now = 1_700_000_000_000;
  const has = (s, t) => s.includes(t);

  check('delivery: not mine renders nothing', deliveryStatus({ fromMe: false, id: 'x', ts: now }, EXPIRY_SECS, now) === '');
  check('delivery: no id (group/legacy) renders nothing', deliveryStatus({ fromMe: true, id: null, ts: now }, EXPIRY_SECS, now) === '');
  check('delivery: delivered shows the checkmark', has(deliveryStatus({ fromMe: true, id: 'x', delivered: true, ts: now }, EXPIRY_SECS, now), '✓ delivered'));
  check('delivery: just sent is still travelling', has(deliveryStatus({ fromMe: true, id: 'x', ts: now }, EXPIRY_SECS, now), 'still travelling'));
  check('delivery: 6 days old, within the 7-day window, still travelling', has(deliveryStatus({ fromMe: true, id: 'x', ts: now - 6 * DAY }, EXPIRY_SECS, now), 'still travelling'));
  check('delivery: 8 days old, past the window, expired', has(deliveryStatus({ fromMe: true, id: 'x', ts: now - 8 * DAY }, EXPIRY_SECS, now), 'expired'));
  check('delivery: 1 second before the boundary, still travelling', has(deliveryStatus({ fromMe: true, id: 'x', ts: now - EXPIRY_SECS * 1000 + 1000 }, EXPIRY_SECS, now), 'still travelling'));
  check('delivery: 1 second after the boundary, expired', has(deliveryStatus({ fromMe: true, id: 'x', ts: now - EXPIRY_SECS * 1000 - 1000 }, EXPIRY_SECS, now), 'expired'));
  check('delivery: delivered wins even past the expiry boundary', has(deliveryStatus({ fromMe: true, id: 'x', delivered: true, ts: now - 8 * DAY }, EXPIRY_SECS, now), '✓ delivered'));
}

console.log(fails ? `\nCODEC TESTS FAILED (${fails})` : '\nCODEC OK — kiss, meshtastic, audio, NDEF, group-invite and delivery-status formats verified');
process.exit(fails ? 1 : 0);
