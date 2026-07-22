// WebRTC DataChannel transport — direct browser-to-browser, no server once the
// peers are connected. Signaling (exchanging the SDP offer/answer) is out of
// band: paste it, beep it over ultrasonic, or show a QR (spec §"zero-rendezvous
// LAN peering"). Pass an already-open RTCDataChannel, or use `connect()` to make
// a pair for testing.
import { Transport } from '../spore.mjs';

export class WebRTCTransport extends Transport {
  constructor(channel) {
    super();
    this.dc = channel;
    channel.binaryType = 'arraybuffer';
    channel.onmessage = (ev) => this.receive(new Uint8Array(ev.data));
    this._q = [];
    channel.onopen = () => {
      for (const b of this._q) channel.send(b);
      this._q = [];
    };
    if (channel.readyState === 'open') channel.onopen();
  }
  send(bytes) {
    if (this.dc.readyState === 'open') this.dc.send(bytes);
    else this._q.push(bytes.slice());
  }
}

/**
 * Convenience: open one DataChannel between two RTCPeerConnections you've wired
 * with your own signaling. Returns a promise of an open `RTCDataChannel`.
 */
export function openChannel(pc, { initiator }) {
  return new Promise((resolve) => {
    if (initiator) {
      const dc = pc.createDataChannel('spore', { ordered: false });
      dc.onopen = () => resolve(dc);
    } else {
      pc.ondatachannel = (ev) => {
        const dc = ev.channel;
        if (dc.readyState === 'open') resolve(dc);
        else dc.onopen = () => resolve(dc);
      };
    }
  });
}
