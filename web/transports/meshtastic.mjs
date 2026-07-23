// Meshtastic transports — bridge a page to a Meshtastic LoRa node over USB (Web
// Serial) or Bluetooth (Web BLE). A SPORE envelope rides inside a Meshtastic
// `MeshPacket` on portnum 256 (PRIVATE_APP); the device floods it across the LoRa
// mesh, and packets from the mesh are unwrapped back into envelopes. The wrap/
// unwrap codec is a JS port of the Rust `bridge::meshtastic`, so browser and
// native agree on the wire.
//
// CAVEATS (same as the Rust template — not hardware-verified here):
//  - Field numbers follow Meshtastic `mesh.proto`; confirm against your firmware.
//  - Only the *unencrypted* `decoded` payload is handled. Use an unencrypted
//    channel, or add the AES-CTR channel key to interoperate on an encrypted one.
//
//   const t = await MeshtasticSerialTransport.open();     // USB
//   const t = await MeshtasticBLETransport.open();        // Bluetooth
//   hub.addTransport(t);
import { Transport } from '../spore.mjs';

const PORT_PRIVATE_APP = 256;
const BROADCAST = 0xffffffff;
const DEFAULT_HOP_LIMIT = 3;

// ---- minimal protobuf (varints kept < 2^53, so no 32-bit shift hazards) ------
function putVarint(a, n) {
  while (true) {
    const b = n & 0x7f;
    n = Math.floor(n / 128);
    if (n !== 0) a.push(b | 0x80);
    else { a.push(b); break; }
  }
}
const putTag = (a, field, wire) => putVarint(a, field * 8 + wire);
const putUint = (a, field, val) => { putTag(a, field, 0); putVarint(a, val); };
const putBytes = (a, field, data) => { putTag(a, field, 2); putVarint(a, data.length); for (const b of data) a.push(b); };
const putFixed32 = (a, field, val) => { putTag(a, field, 5); a.push(val & 0xff, (val >>> 8) & 0xff, (val >>> 16) & 0xff, (val >>> 24) & 0xff); };
function getVarint(buf, o) {
  let val = 0, shift = 0;
  while (o < buf.length && shift < 64) {
    const b = buf[o++];
    val += (b & 0x7f) * Math.pow(2, shift);
    if ((b & 0x80) === 0) return [val, o];
    shift += 7;
  }
  return null;
}

// MeshPacket: from=1 to=2 decoded=4 encrypted=5 id=6 hop_limit=9
// Data:       portnum=1 payload=2
// (exported for the codec tests in web/codec-test.mjs; the standalone build
// strips `export` when it inlines this module, so this is test-only surface.)
export function encodeMeshPacket(envWire, fromNode, toNode, packetId) {
  const data = [];
  putUint(data, 1, PORT_PRIVATE_APP);
  putBytes(data, 2, envWire);
  const pkt = [];
  putUint(pkt, 1, fromNode >>> 0);
  putUint(pkt, 2, toNode >>> 0);
  putBytes(pkt, 4, data);
  putFixed32(pkt, 6, packetId >>> 0);
  putUint(pkt, 9, DEFAULT_HOP_LIMIT);
  return pkt; // array of bytes
}

// Walk a protobuf, returning a map of fieldNumber -> value(s) we care about.
function scanFields(buf, start, end) {
  const out = { uint: {}, bytes: {} };
  let o = start;
  while (o < end) {
    const t = getVarint(buf, o);
    if (!t) return null;
    o = t[1];
    const field = Math.floor(t[0] / 8);
    const wire = t[0] & 7;
    if (wire === 0) {
      const v = getVarint(buf, o);
      if (!v) return null;
      out.uint[field] = v[0];
      o = v[1];
    } else if (wire === 2) {
      const l = getVarint(buf, o);
      if (!l) return null;
      const s = l[1], e = s + l[0];
      if (e > end) return null;
      out.bytes[field] = buf.subarray(s, e);
      o = e;
    } else if (wire === 5) o += 4;
    else if (wire === 1) o += 8;
    else return null;
  }
  return out;
}

// Decode a bare MeshPacket -> {from, portnum, payload} or null.
export function decodeMeshPacket(buf) {
  const p = scanFields(buf, 0, buf.length);
  if (!p || !p.bytes[4]) return null; // no `decoded`
  const from = p.uint[1] || 0;
  const d = scanFields(p.bytes[4], 0, p.bytes[4].length);
  if (!d) return null;
  return { from, portnum: d.uint[1] || 0, payload: d.bytes[2] || new Uint8Array(0) };
}

// A rolling node id for our own packets (derived from the SPORE address).
function nodeIdFrom(addr8) {
  return ((addr8[0] << 24) | (addr8[1] << 16) | (addr8[2] << 8) | addr8[3]) >>> 0;
}

// ---- shared behaviour --------------------------------------------------------
class MeshtasticBase extends Transport {
  constructor() {
    super();
    this.myNode = 0xffffffff; // set once we know our address (see setAddr)
    this.rng = (Math.random() * 0xffffffff) >>> 0;
  }
  setAddr(addr8) { this.myNode = nodeIdFrom(addr8); }
  _nextId() { this.rng = (Math.imul(this.rng, 1664525) + 1013904223) >>> 0; return this.rng; }
  _wrapToRadio(envBytes) {
    const pkt = encodeMeshPacket(Array.from(envBytes), this.myNode, BROADCAST, this._nextId());
    const toRadio = [];
    putBytes(toRadio, 1, pkt); // ToRadio.packet = 1
    return Uint8Array.from(toRadio);
  }
  _onFromRadio(bytes) {
    // FromRadio.packet = field 2 (a MeshPacket).
    const fr = scanFields(bytes, 0, bytes.length);
    if (!fr || !fr.bytes[2]) return;
    const mp = decodeMeshPacket(fr.bytes[2]);
    if (mp && mp.portnum === PORT_PRIVATE_APP && mp.payload.length) this.receive(mp.payload);
  }
}

// ---- USB (Web Serial): 0x94 0xc3 <len16be> <ToRadio|FromRadio> ---------------
export class MeshtasticSerialTransport extends MeshtasticBase {
  constructor(port) {
    super();
    this.port = port;
    this.writer = port.writable.getWriter();
    this.buf = [];
    this._read(port.readable.getReader());
  }
  static async open({ port = null } = {}) {
    if (!navigator.serial) throw new Error('Web Serial not supported in this browser');
    const p = port || (await navigator.serial.requestPort());
    await p.open({ baudRate: 115200 });
    return new MeshtasticSerialTransport(p);
  }
  async _read(reader) {
    this.reader = reader;
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        if (value) this._feed(value);
      }
    } catch { /* closed */ } finally { reader.releaseLock(); }
  }
  _feed(chunk) {
    for (const b of chunk) this.buf.push(b);
    // Parse as many complete frames as are buffered.
    for (;;) {
      // Find the 0x94 0xc3 magic.
      let i = 0;
      while (i + 1 < this.buf.length && !(this.buf[i] === 0x94 && this.buf[i + 1] === 0xc3)) i++;
      if (i + 4 > this.buf.length) { if (i > 0) this.buf.splice(0, i); return; }
      const len = (this.buf[i + 2] << 8) | this.buf[i + 3];
      if (i + 4 + len > this.buf.length) { if (i > 0) this.buf.splice(0, i); return; }
      const frame = Uint8Array.from(this.buf.slice(i + 4, i + 4 + len));
      this.buf.splice(0, i + 4 + len);
      this._onFromRadio(frame);
    }
  }
  send(bytes) {
    const body = this._wrapToRadio(bytes);
    const hdr = Uint8Array.from([0x94, 0xc3, (body.length >> 8) & 0xff, body.length & 0xff]);
    const out = new Uint8Array(hdr.length + body.length);
    out.set(hdr); out.set(body, hdr.length);
    this.writer.write(out).catch(() => {});
  }
  async close() {
    try { await this.reader?.cancel(); } catch { /* */ }
    try { this.writer.releaseLock(); await this.port.close(); } catch { /* */ }
  }
}

// ---- Bluetooth (Web BLE): ToRadio (write) / FromRadio (read on FromNum) ------
const MT_SERVICE = '6ba1b218-15a8-461f-9fa8-5dcae273eafd';
const MT_TORADIO = 'f75c76d2-129e-4dad-a1dd-7866124401e7';
const MT_FROMRADIO = '2c55e69e-4993-11ed-b878-0242ac120002';
const MT_FROMNUM = 'ed9da18c-a800-4f66-a670-aa7547e34453';

export class MeshtasticBLETransport extends MeshtasticBase {
  constructor(device, toRadio, fromRadio) {
    super();
    this.device = device;
    this.toRadio = toRadio;
    this.fromRadio = fromRadio;
    this._q = Promise.resolve();
  }
  static async open() {
    if (!navigator.bluetooth) throw new Error('Web Bluetooth not supported in this browser');
    const device = await navigator.bluetooth.requestDevice({
      filters: [{ services: [MT_SERVICE] }],
      optionalServices: [MT_SERVICE],
    });
    const gatt = await device.gatt.connect();
    const svc = await gatt.getPrimaryService(MT_SERVICE);
    const toRadio = await svc.getCharacteristic(MT_TORADIO);
    const fromRadio = await svc.getCharacteristic(MT_FROMRADIO);
    const t = new MeshtasticBLETransport(device, toRadio, fromRadio);
    // A FromNum notification means "packets are waiting" — drain FromRadio.
    const fromNum = await svc.getCharacteristic(MT_FROMNUM);
    fromNum.addEventListener('characteristicvaluechanged', () => t._drain());
    await fromNum.startNotifications();
    // Kick the config/stream by asking for a config id.
    const wc = [];
    putUint(wc, 3, (Math.random() * 0xffffffff) >>> 0); // ToRadio.want_config_id = 3
    await toRadio.writeValue(Uint8Array.from(wc));
    t._drain();
    return t;
  }
  async _drain() {
    // Read FromRadio repeatedly until it returns empty.
    for (let i = 0; i < 64; i++) {
      let dv;
      try { dv = await this.fromRadio.readValue(); } catch { return; }
      if (!dv || dv.byteLength === 0) return;
      this._onFromRadio(new Uint8Array(dv.buffer, dv.byteOffset, dv.byteLength));
    }
  }
  send(bytes) {
    const body = this._wrapToRadio(bytes);
    this._q = this._q.then(() => this.toRadio.writeValue(body).catch(() => {}));
  }
  close() { try { this.device.gatt.disconnect(); } catch { /* */ } }
}
