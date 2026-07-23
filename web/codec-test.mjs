// Codec tests for the browser transports' pure, off-device logic — the parts
// that can be exercised without real hardware or a live peer, so the 🧪 bridges
// still have a regression net for their wire formats. Run: `node web/codec-test.mjs`.
//
//   * KISS framing (matches src/kiss.rs)
//   * Meshtastic MeshPacket protobuf (byte-identical to the Rust bridge)
//   * the 16-FSK audio modem (bit-compatible with bridge::audio)
import { kissFrame, KissDeframer } from './transports/kiss.mjs';
import { modulate, Demod } from './transports/audio.mjs';
import { encodeMeshPacket, decodeMeshPacket } from './transports/meshtastic.mjs';

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

console.log(fails ? `\nCODEC TESTS FAILED (${fails})` : '\nCODEC OK — kiss, meshtastic, and audio wire formats verified');
process.exit(fails ? 1 : 0);
