// Audio-modem transport — send SPORE envelopes *as sound*. Two laptops on a
// table, a radio's speaker into another radio's mic, a room full of phones: the
// medium is broadcast-only and has no address, so everyone in earshot hears every
// frame and the envelope's own `dest` decides who cares.
//
// This is a faithful port of the Rust bridge (`src/bridge/audio.rs`): a 16-tone
// FSK modem with a Goertzel receiver, **bit-compatible** with it — a browser tab
// and a native `spore-audio` pipe exchange real envelopes over the air.
//
//   Wire (per frame):  SYNC(6 sym) · LEN(2 B) · PAYLOAD · CRC(4 B)
//   16 tones ⇒ 4 bits/symbol, high nibble first; CRC = SHA-256(LEN‖PAYLOAD)[0..4].
//
//   const t = await AudioModemTransport.open(); // asks for the mic
//   hub.addTransport(t);
import { Transport } from '../spore.mjs';

export const SAMPLE_RATE = 48000; // sound cards universally support 48 kHz
export const SYMBOL_LEN = 1024; // bin-aligned: SR/SYMBOL_LEN = 46.875 Hz
const TONES = 16; // 4 bits per symbol
const BASE_BIN = 32; // 1500 Hz
const SPACING_BIN = 4; // 187.5 Hz between tones
const SYNC = [15, 0, 15, 0, 12, 3];
const AMPLITUDE = 0.5;
export const MAX_FRAME_PAYLOAD = 4096;

const toneFreq = (s) => ((BASE_BIN + s * SPACING_BIN) * SAMPLE_RATE) / SYMBOL_LEN;

// ---- minimal synchronous SHA-256 (for the 4-byte frame CRC only) -------------
// The envelope itself is Ed25519-signed; this CRC only rejects a mis-heard frame,
// and must byte-match Rust's `Sha256::digest(frame)[..4]`.
const K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);
function sha256(bytes) {
  const h = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);
  const ml = bytes.length * 8;
  const withPad = new Uint8Array((((bytes.length + 8) >> 6) + 1) << 6);
  withPad.set(bytes);
  withPad[bytes.length] = 0x80;
  const dv = new DataView(withPad.buffer);
  dv.setUint32(withPad.length - 4, ml >>> 0);
  dv.setUint32(withPad.length - 8, Math.floor(ml / 0x100000000));
  const w = new Uint32Array(64);
  const rotr = (x, n) => (x >>> n) | (x << (32 - n));
  for (let i = 0; i < withPad.length; i += 64) {
    for (let t = 0; t < 16; t++) w[t] = dv.getUint32(i + t * 4);
    for (let t = 16; t < 64; t++) {
      const s0 = rotr(w[t - 15], 7) ^ rotr(w[t - 15], 18) ^ (w[t - 15] >>> 3);
      const s1 = rotr(w[t - 2], 17) ^ rotr(w[t - 2], 19) ^ (w[t - 2] >>> 10);
      w[t] = (w[t - 16] + s0 + w[t - 7] + s1) | 0;
    }
    let [a, b, c, d, e, f, g, hh] = h;
    for (let t = 0; t < 64; t++) {
      const S1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      const ch = (e & f) ^ (~e & g);
      const t1 = (hh + S1 + ch + K[t] + w[t]) | 0;
      const S0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const t2 = (S0 + maj) | 0;
      hh = g; g = f; f = e; e = (d + t1) | 0; d = c; c = b; b = a; a = (t1 + t2) | 0;
    }
    h[0] = (h[0] + a) | 0; h[1] = (h[1] + b) | 0; h[2] = (h[2] + c) | 0; h[3] = (h[3] + d) | 0;
    h[4] = (h[4] + e) | 0; h[5] = (h[5] + f) | 0; h[6] = (h[6] + g) | 0; h[7] = (h[7] + hh) | 0;
  }
  const out = new Uint8Array(32);
  new DataView(out.buffer).setUint32(0, h[0] >>> 0);
  for (let i = 0; i < 8; i++) new DataView(out.buffer).setUint32(i * 4, h[i] >>> 0);
  return out;
}

// ---- modulation (bytes -> PCM) ----------------------------------------------
export function modulate(payload) {
  const len = Math.min(payload.length, 0xffff);
  const frame = new Uint8Array(2 + len);
  frame[0] = len >> 8;
  frame[1] = len & 0xff;
  frame.set(payload.subarray(0, len), 2);
  const crc = sha256(frame).subarray(0, 4);
  const framed = new Uint8Array(frame.length + 4);
  framed.set(frame);
  framed.set(crc, frame.length);

  const symbols = SYNC.slice();
  for (const b of framed) symbols.push(b >> 4, b & 0x0f);

  const out = new Float32Array(symbols.length * SYMBOL_LEN);
  let o = 0;
  for (const s of symbols) {
    const f = toneFreq(s);
    for (let n = 0; n < SYMBOL_LEN; n++) {
      out[o++] = AMPLITUDE * Math.sin((2 * Math.PI * f * n) / SAMPLE_RATE);
    }
  }
  return out;
}

// ---- demodulation (PCM -> bytes) --------------------------------------------
function goertzel(buf, off, freq) {
  const k = Math.round((SYMBOL_LEN * freq) / SAMPLE_RATE);
  const w = (2 * Math.PI * k) / SYMBOL_LEN;
  const coeff = 2 * Math.cos(w);
  let s1 = 0, s2 = 0;
  for (let i = 0; i < SYMBOL_LEN; i++) {
    const s0 = buf[off + i] + coeff * s1 - s2;
    s2 = s1;
    s1 = s0;
  }
  return Math.sqrt(Math.max(s1 * s1 + s2 * s2 - coeff * s1 * s2, 0));
}
function symbolAt(buf, off) {
  if (off + SYMBOL_LEN > buf.length) return null;
  let best = 0, bestMag = 0, sum = 1e-9;
  for (let s = 0; s < TONES; s++) {
    const m = goertzel(buf, off, toneFreq(s));
    sum += m;
    if (m > bestMag) { bestMag = m; best = s; }
  }
  return [best, bestMag / sum];
}
function syncMatches(buf, off) {
  for (let i = 0; i < SYNC.length; i++) {
    const r = symbolAt(buf, off + i * SYMBOL_LEN);
    if (!r || r[0] !== SYNC[i] || r[1] <= 0.3) return false;
  }
  return true;
}
function readBytes(buf, start, count) {
  const out = new Uint8Array(count);
  for (let j = 0; j < count; j++) {
    const hi = symbolAt(buf, start + 2 * j * SYMBOL_LEN);
    const lo = symbolAt(buf, start + (2 * j + 1) * SYMBOL_LEN);
    if (!hi || !lo) return null;
    out[j] = (hi[0] << 4) | lo[0];
  }
  return out;
}
function decodeFrame(buf, off) {
  const dataOff = off + SYNC.length * SYMBOL_LEN;
  const lenB = readBytes(buf, dataOff, 2);
  if (!lenB) return null;
  const payloadLen = (lenB[0] << 8) | lenB[1];
  if (payloadLen > MAX_FRAME_PAYLOAD) return null;
  const frameBytes = 2 + payloadLen + 4;
  const framed = readBytes(buf, dataOff, frameBytes);
  if (!framed) return null;
  const body = framed.subarray(0, frameBytes - 4);
  const crc = sha256(body).subarray(0, 4);
  for (let i = 0; i < 4; i++) if (crc[i] !== framed[frameBytes - 4 + i]) return null;
  const end = dataOff + 2 * frameBytes * SYMBOL_LEN;
  return [body.subarray(2), end]; // strip the 2 LEN bytes
}

/** Streaming receiver: push PCM as it arrives, get back completed payloads. */
export class Demod {
  constructor() {
    this.buf = new Float32Array(0);
  }
  push(samples) {
    const merged = new Float32Array(this.buf.length + samples.length);
    merged.set(this.buf);
    merged.set(samples, this.buf.length);
    this.buf = merged;
    const out = [];
    const step = SYMBOL_LEN >> 3;
    let off = 0, consumed = 0;
    while (off + SYNC.length * SYMBOL_LEN <= this.buf.length) {
      if (syncMatches(this.buf, off)) {
        const r = decodeFrame(this.buf, off);
        if (r) { out.push(r[0]); off = r[1]; consumed = r[1]; continue; }
      }
      off += step;
    }
    const maxFrame = (SYNC.length + 2 * (2 + MAX_FRAME_PAYLOAD + 4)) * SYMBOL_LEN;
    const keepFrom = Math.max(consumed, this.buf.length - maxFrame);
    if (keepFrom > 0) this.buf = this.buf.slice(keepFrom);
    return out;
  }
}

// ---- browser transport (mic + speaker) --------------------------------------
export class AudioModemTransport extends Transport {
  constructor(ctx, stream) {
    super();
    this.ctx = ctx;
    this.stream = stream;
    this.demod = new Demod();

    const src = ctx.createMediaStreamSource(stream);
    // ScriptProcessor is deprecated but works everywhere including file://; the
    // 0-gain sink lets it fire without echoing the mic to the speakers.
    const proc = ctx.createScriptProcessor(4096, 1, 1);
    this.proc = proc;
    proc.onaudioprocess = (ev) => {
      const inp = ev.inputBuffer.getChannelData(0);
      for (const frame of this.demod.push(new Float32Array(inp))) this.receive(frame);
    };
    const mute = ctx.createGain();
    mute.gain.value = 0;
    src.connect(proc);
    proc.connect(mute);
    mute.connect(ctx.destination);
  }

  /** Open the mic (asks permission) at 48 kHz and start listening. */
  static async open() {
    if (!navigator.mediaDevices?.getUserMedia) throw new Error('microphone not available');
    const AC = window.AudioContext || window.webkitAudioContext;
    const ctx = new AC({ sampleRate: SAMPLE_RATE });
    if (ctx.sampleRate !== SAMPLE_RATE) {
      // A few platforms ignore the hint; the modem needs exactly 48 kHz.
      await ctx.close();
      throw new Error(`audio device is ${ctx.sampleRate} Hz; the modem needs ${SAMPLE_RATE} Hz`);
    }
    const stream = await navigator.mediaDevices.getUserMedia({
      audio: { echoCancellation: false, noiseSuppression: false, autoGainControl: false },
    });
    await ctx.resume();
    return new AudioModemTransport(ctx, stream);
  }

  send(bytes) {
    const pcm = modulate(bytes);
    const audioBuf = this.ctx.createBuffer(1, pcm.length, SAMPLE_RATE);
    audioBuf.getChannelData(0).set(pcm);
    const node = this.ctx.createBufferSource();
    node.buffer = audioBuf;
    node.connect(this.ctx.destination);
    node.start();
  }

  close() {
    try { this.proc.disconnect(); } catch { /* */ }
    try { this.stream.getTracks().forEach((t) => t.stop()); } catch { /* */ }
    try { this.ctx.close(); } catch { /* */ }
  }
}
