// Web NFC transport — tap-to-transfer at a few centimetres.
//
// An envelope becomes an NDEF MIME record (`application/x-spore`) and moves by
// touching two phones together, or a phone to a tag. The gesture is the point:
// unlike every other bridge here, delivery requires a person to deliberately put
// two devices in contact, which makes it the natural way to seed a new device or
// hand a message to someone in front of you.
//
//   const t = await WebNfcTransport.open();
//   hub.addTransport(t);
//
// **Availability.** Web NFC is Chrome on Android only, over HTTPS, and needs a
// user gesture to start scanning. There is no desktop implementation. Everything
// below the transport class — the NDEF codec — is pure and tested; the reader
// itself cannot be exercised without a phone and a second device, so it is an
// honest template in the same sense as the ICMP raw-socket runner.
//
// **MTU.** A tag's capacity is its class: NTAG213 holds ~130 bytes of payload,
// NTAG216 ~850. Phone-to-phone is larger but still modest. SPORE's fountain
// fragmentation handles anything bigger — several taps reconstruct one object,
// and any ~K of N fragments suffice, so a mistimed tap costs a repeat rather than
// a restart.
import { Transport } from '../spore.mjs';

/// The MIME type an NDEF record carries a SPORE envelope under.
export const SPORE_MIME = 'application/x-spore';

// NDEF record header bits (NFC Forum NDEF 1.0 §3.2).
const MB = 0x80; // message begin
const ME = 0x40; // message end
const SR = 0x10; // short record — payload length is one byte
const TNF_MEDIA = 0x02; // type is a MIME media type

/// Encode one envelope as a complete single-record NDEF message.
///
/// Short-record form whenever the payload fits in a byte, which is the common
/// case for a fragment; longer payloads use the 4-byte length field. Pure — no
/// browser APIs — so it is testable off-device.
export function encodeNdef(envelope) {
  const type = new TextEncoder().encode(SPORE_MIME);
  const short = envelope.length < 256;
  const header = MB | ME | TNF_MEDIA | (short ? SR : 0);

  const lenBytes = short ? 1 : 4;
  const out = new Uint8Array(1 + 1 + lenBytes + type.length + envelope.length);
  let o = 0;
  out[o++] = header;
  out[o++] = type.length;
  if (short) {
    out[o++] = envelope.length;
  } else {
    // Payload length is big-endian, like every other length in NDEF.
    out[o++] = (envelope.length >>> 24) & 0xff;
    out[o++] = (envelope.length >>> 16) & 0xff;
    out[o++] = (envelope.length >>> 8) & 0xff;
    out[o++] = envelope.length & 0xff;
  }
  out.set(type, o);
  o += type.length;
  out.set(envelope, o);
  return out;
}

/// Recover the envelope from an NDEF message, or `null`.
///
/// Returns null for anything that is not a single SPORE MIME record: a URL tag, a
/// vCard, a truncated read, another app's record. A tag is written by whoever
/// held it last, so every length here is checked against what actually arrived
/// rather than believed.
export function decodeNdef(bytes) {
  if (!bytes || bytes.length < 3) return null;
  const header = bytes[0];
  if ((header & 0x07) !== TNF_MEDIA) return null;

  const typeLen = bytes[1];
  let o = 2;
  let payloadLen;
  if (header & SR) {
    payloadLen = bytes[o];
    o += 1;
  } else {
    if (bytes.length < 6) return null;
    payloadLen =
      bytes[o] * 0x1000000 + (bytes[o + 1] << 16) + (bytes[o + 2] << 8) + bytes[o + 3];
    o += 4;
  }
  // An ID field would sit here; a record we wrote never has one, and one we did
  // not write is not ours to interpret.
  if (header & 0x08) return null; // IL flag: ID present

  if (o + typeLen + payloadLen > bytes.length) return null; // truncated or lying
  const type = new TextDecoder().decode(bytes.subarray(o, o + typeLen));
  if (type !== SPORE_MIME) return null;
  o += typeLen;
  return bytes.subarray(o, o + payloadLen);
}

/// A tap-to-transfer transport over Web NFC.
///
/// Reading and writing are separate gestures in the platform API: scanning is
/// continuous once started, but a write completes on the next tag brought into
/// the field. Outbound envelopes therefore queue until a tag is present, and the
/// queue is bounded — a phone in a pocket must not accumulate an unbounded
/// backlog because nothing has been tapped for an hour.
export class WebNfcTransport extends Transport {
  static MAX_QUEUED = 32;

  constructor(reader) {
    super();
    this.reader = reader;
    this.pending = [];
    reader.addEventListener('reading', ({ message }) => {
      for (const record of message.records) {
        if (record.mediaType !== SPORE_MIME) continue;
        const data = new Uint8Array(
          record.data.buffer,
          record.data.byteOffset,
          record.data.byteLength,
        );
        this.receive(data);
      }
      // A tag in the field is also the moment we can hand something over.
      this.#flush();
    });
  }

  /// Begin scanning. Must be called from a user gesture; throws where Web NFC is
  /// unavailable, which is everywhere except Chrome on Android over HTTPS.
  static async open() {
    if (typeof NDEFReader === 'undefined') {
      throw new Error('Web NFC unavailable (Chrome on Android, HTTPS, user gesture)');
    }
    const reader = new NDEFReader();
    await reader.scan();
    return new WebNfcTransport(reader);
  }

  send(bytes) {
    // Oldest first: a queued envelope that never got tapped out is stale, and
    // the newest one is the one the user is most likely waiting to hand over.
    if (this.pending.length >= WebNfcTransport.MAX_QUEUED) this.pending.shift();
    this.pending.push(bytes);
    this.#flush();
  }

  async #flush() {
    while (this.pending.length) {
      const next = this.pending[0];
      try {
        await this.reader.write({
          records: [{ recordType: 'mime', mediaType: SPORE_MIME, data: next }],
        });
      } catch {
        return; // no tag in the field yet; keep it queued for the next tap
      }
      this.pending.shift();
    }
  }
}
