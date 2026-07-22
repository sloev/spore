// Nostr transport — carry SPORE envelopes as events on Nostr relays. Each
// envelope is base64'd into the `content` of a kind-30078 (app-data) event on a
// shared tag, so any relay becomes a SPORE bag. Pull-style: you receive whatever
// other nodes have published under the tag.
//
// This is a browser/Node shim over a WebSocket to a relay; it needs a Nostr
// signer for outbound events (window.nostr / NIP-07, or a local key). Inbound
// works with no key. Left as a documented template — plug in your signer.
import { Transport } from '../spore.mjs';

const KIND = 30078; // parameterized replaceable app-data
const TAG = 'spore-v1';

function b64(bytes) {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}
function unb64(str) {
  const s = atob(str);
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
  return out;
}

export class NostrTransport extends Transport {
  // `relayUrl` e.g. 'wss://relay.damus.io'; `signer` async (event)->signedEvent
  // (window.nostr.signEvent, or null for receive-only).
  constructor(relayUrl, signer = null) {
    super();
    this.signer = signer;
    const ws = new WebSocket(relayUrl);
    this.ws = ws;
    ws.addEventListener('open', () => {
      ws.send(JSON.stringify(['REQ', 'spore', { kinds: [KIND], '#d': [TAG] }]));
    });
    ws.addEventListener('message', (ev) => {
      try {
        const msg = JSON.parse(ev.data);
        if (msg[0] === 'EVENT' && msg[2] && msg[2].content) {
          this.receive(unb64(msg[2].content));
        }
      } catch {
        /* ignore malformed */
      }
    });
  }
  async send(bytes) {
    if (!this.signer) return; // receive-only without a key
    const event = {
      kind: KIND,
      created_at: Math.floor(Date.now() / 1000),
      tags: [['d', TAG]],
      content: b64(bytes),
    };
    const signed = await this.signer(event);
    if (this.ws.readyState === 1) this.ws.send(JSON.stringify(['EVENT', signed]));
  }
}
