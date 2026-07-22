// WebSocket transport — binary frames to a relay or peer. Works in the browser
// (global WebSocket) and in Node 22 (global WebSocket client). One SPORE
// envelope per message.
import { Transport } from '../spore.mjs';

export class WebSocketTransport extends Transport {
  // `wsOrUrl` is a URL string (we open it) or an existing WebSocket (e.g. a
  // server-accepted connection).
  constructor(wsOrUrl) {
    super();
    const ws = typeof wsOrUrl === 'string' ? new WebSocket(wsOrUrl) : wsOrUrl;
    ws.binaryType = 'arraybuffer';
    this.ws = ws;
    this._q = [];
    ws.addEventListener('open', () => {
      for (const b of this._q) ws.send(b);
      this._q = [];
    });
    ws.addEventListener('message', (ev) => {
      const data = ev.data;
      const bytes = data instanceof ArrayBuffer ? new Uint8Array(data) : new Uint8Array(data);
      this.receive(bytes);
    });
  }
  send(bytes) {
    if (this.ws.readyState === 1) this.ws.send(bytes);
    else this._q.push(bytes.slice());
  }
}
