// KISS framing for byte-stream media (serial, BLE, RFCOMM, TNCs) — the browser
// twin of Rust's `src/kiss.rs`, byte-for-byte, so a page talks to a physical
// LoRa/ESP32 node over the same wire. A frame is `FEND cmd …escaped… FEND`;
// FEND/FESC bytes inside the payload are escaped. This lets a stream that split
// or merged reads resynchronise on the FEND boundary.
const FEND = 0xc0,
  FESC = 0xdb,
  TFEND = 0xdc,
  TFESC = 0xdd;

/** Frame one SPORE envelope for a byte stream: `C0 00 …escaped… C0`. */
export function kissFrame(bytes) {
  const out = [FEND, 0x00]; // FEND + command byte (data frame, port 0)
  for (const b of bytes) {
    if (b === FEND) out.push(FESC, TFEND);
    else if (b === FESC) out.push(FESC, TFESC);
    else out.push(b);
  }
  out.push(FEND);
  return new Uint8Array(out);
}

/**
 * Streaming KISS de-framer. Feed byte chunks as they arrive; get back whole
 * frames (command byte stripped). Keeps state across reads, so a frame split
 * over two reads still reassembles — the same contract as Rust's `KissStream`.
 */
export class KissDeframer {
  constructor() {
    this.cur = [];
    this.inFrame = false;
    this.gotCmd = false;
    this.esc = false;
  }
  push(chunk) {
    const frames = [];
    for (const b of chunk) {
      if (b === FEND) {
        if (this.inFrame && this.cur.length) frames.push(Uint8Array.from(this.cur));
        this.inFrame = true;
        this.gotCmd = false;
        this.esc = false;
        this.cur = [];
        continue;
      }
      if (!this.inFrame) continue;
      if (!this.gotCmd) {
        this.gotCmd = true; // drop the KISS command byte
        continue;
      }
      if (this.esc) {
        this.cur.push(b === TFEND ? FEND : b === TFESC ? FESC : b);
        this.esc = false;
      } else if (b === FESC) {
        this.esc = true;
      } else {
        this.cur.push(b);
      }
    }
    return frames;
  }
}
