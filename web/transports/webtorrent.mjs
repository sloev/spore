// WebTorrent-swarm transport — join a swarm of browsers by an "infohash" and
// gossip SPORE envelopes over WebRTC data channels. It speaks the real
// bittorrent-tracker WebSocket protocol that WebTorrent uses in the browser, so
// the same public trackers (tracker.openwebtorrent.com, tracker.webtorrent.dev,
// …) rendezvous SPORE peers. There is no torrent file: the infohash is just a
// shared name, and once two peers are introduced the data channel carries signed
// envelopes directly — no server in the path after connect.
//
//   const t = await WebTorrentTransport.join('spore/public');
//   hub.addTransport(t);
//
// Every browser that joins the same name meets every other and floods envelopes
// through the mesh. Peer discovery touches the trackers; the actual traffic is
// peer-to-peer (WebRTC), so the swarm keeps working if a tracker goes down.
import { Transport } from '../spore.mjs';

const DEFAULT_TRACKERS = [
  'wss://tracker.openwebtorrent.com',
  'wss://tracker.webtorrent.dev',
  'wss://tracker.btorrent.xyz',
];
const ICE = [{ urls: 'stun:stun.l.google.com:19302' }];
const OFFERS_PER_ANNOUNCE = 4;
const REANNOUNCE_MS = 30_000;

const bin2str = (u8) => {
  let s = '';
  for (const b of u8) s += String.fromCharCode(b);
  return s;
};
const str2bin = (s) => {
  const u = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) u[i] = s.charCodeAt(i) & 0xff;
  return u;
};
const rand20 = () => {
  const u = new Uint8Array(20);
  crypto.getRandomValues(u);
  return u;
};

// Gather ICE fully (non-trickle) so a single SDP blob carries every candidate —
// that is what the tracker relays.
function completeSdp(pc) {
  return new Promise((resolve) => {
    if (pc.iceGatheringState === 'complete') return resolve();
    const check = () => {
      if (pc.iceGatheringState === 'complete') {
        pc.removeEventListener('icegatheringstatechange', check);
        resolve();
      }
    };
    pc.addEventListener('icegatheringstatechange', check);
    setTimeout(resolve, 4000); // don't hang forever behind a stubborn NAT
  });
}

export class WebTorrentTransport extends Transport {
  constructor(infoHash, { trackers = DEFAULT_TRACKERS } = {}) {
    super();
    this.infoHashStr = bin2str(infoHash);
    this.peerIdStr = bin2str(rand20());
    this.channels = new Set(); // open RTCDataChannels
    this.pendingOffers = new Map(); // offer_id(str) -> RTCPeerConnection
    this.sockets = [];
    this.onpeer = null; // (count) => void, for UI
    this.timers = [];
    for (const url of trackers) this._connectTracker(url);
  }

  /** Join a named swarm; the name is hashed to a 20-byte infohash. */
  static async join(name, opts = {}) {
    const digest = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(name));
    const infoHash = new Uint8Array(digest).subarray(0, 20);
    return new WebTorrentTransport(infoHash, opts);
  }

  _connectTracker(url) {
    let ws;
    try {
      ws = new WebSocket(url);
    } catch {
      return;
    }
    this.sockets.push(ws);
    ws.addEventListener('open', () => this._announce(ws));
    ws.addEventListener('message', (ev) => this._onTracker(ws, ev.data));
    ws.addEventListener('close', () => {
      this.sockets = this.sockets.filter((s) => s !== ws);
    });
    const t = setInterval(() => {
      if (ws.readyState === 1) this._announce(ws);
    }, REANNOUNCE_MS);
    this.timers.push(t);
  }

  // Build a batch of outbound offers and announce them to one tracker.
  async _announce(ws) {
    const offers = [];
    for (let i = 0; i < OFFERS_PER_ANNOUNCE; i++) {
      const pc = new RTCPeerConnection({ iceServers: ICE });
      const dc = pc.createDataChannel('spore', { ordered: false });
      this._wireChannel(pc, dc);
      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      await completeSdp(pc);
      const offerId = bin2str(rand20());
      this.pendingOffers.set(offerId, pc);
      offers.push({ offer_id: offerId, offer: { type: 'offer', sdp: pc.localDescription.sdp } });
    }
    this._sendJson(ws, {
      action: 'announce',
      info_hash: this.infoHashStr,
      peer_id: this.peerIdStr,
      numwant: OFFERS_PER_ANNOUNCE,
      uploaded: 0,
      downloaded: 0,
      left: 0,
      offers,
    });
  }

  async _onTracker(ws, data) {
    let msg;
    try {
      msg = JSON.parse(typeof data === 'string' ? data : await data.text());
    } catch {
      return;
    }
    if (msg.info_hash && msg.info_hash !== this.infoHashStr) return;
    // A remote peer's answer to one of our offers.
    if (msg.answer && msg.offer_id) {
      const pc = this.pendingOffers.get(msg.offer_id);
      if (pc) {
        this.pendingOffers.delete(msg.offer_id);
        try {
          await pc.setRemoteDescription(msg.answer);
        } catch {
          /* stale */
        }
      }
    }
    // A remote peer's offer wanting to connect to us.
    if (msg.offer && msg.offer_id && msg.peer_id) {
      const pc = new RTCPeerConnection({ iceServers: ICE });
      pc.ondatachannel = (e) => this._wireChannel(pc, e.channel);
      try {
        await pc.setRemoteDescription(msg.offer);
        const answer = await pc.createAnswer();
        await pc.setLocalDescription(answer);
        await completeSdp(pc);
        this._sendJson(ws, {
          action: 'announce',
          info_hash: this.infoHashStr,
          peer_id: this.peerIdStr,
          to_peer_id: msg.peer_id,
          offer_id: msg.offer_id,
          answer: { type: 'answer', sdp: pc.localDescription.sdp },
        });
      } catch {
        /* couldn't answer */
      }
    }
  }

  _wireChannel(pc, dc) {
    dc.binaryType = 'arraybuffer';
    dc.onopen = () => {
      this.channels.add(dc);
      if (this.onpeer) this.onpeer(this.channels.size);
    };
    dc.onmessage = (ev) => this.receive(new Uint8Array(ev.data));
    dc.onclose = () => {
      this.channels.delete(dc);
      if (this.onpeer) this.onpeer(this.channels.size);
    };
    pc.onconnectionstatechange = () => {
      if (['failed', 'closed', 'disconnected'].includes(pc.connectionState)) {
        this.channels.delete(dc);
        try {
          pc.close();
        } catch {
          /* */
        }
        if (this.onpeer) this.onpeer(this.channels.size);
      }
    };
  }

  _sendJson(ws, obj) {
    if (ws.readyState === 1) ws.send(JSON.stringify(obj));
  }

  get peerCount() {
    return this.channels.size;
  }

  send(bytes) {
    for (const dc of this.channels) {
      if (dc.readyState === 'open') {
        try {
          dc.send(bytes);
        } catch {
          /* */
        }
      }
    }
  }

  close() {
    for (const t of this.timers) clearInterval(t);
    for (const ws of this.sockets) try { ws.close(); } catch { /* */ }
    for (const dc of this.channels) try { dc.close(); } catch { /* */ }
    this.channels.clear();
  }
}
