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

const STUN = [{ urls: 'stun:stun.l.google.com:19302' }];

// Gather ICE fully so one copy-pasteable blob carries every candidate.
function iceComplete(pc) {
  return new Promise((resolve) => {
    if (pc.iceGatheringState === 'complete') return resolve();
    const check = () => {
      if (pc.iceGatheringState === 'complete') {
        pc.removeEventListener('icegatheringstatechange', check);
        resolve();
      }
    };
    pc.addEventListener('icegatheringstatechange', check);
    setTimeout(resolve, 4000);
  });
}
const packSdp = (d) => btoa(JSON.stringify({ type: d.type, sdp: d.sdp }));
const unpackSdp = (s) => JSON.parse(atob(s.trim()));

/**
 * Manual (copy-paste / QR / ultrasonic) WebRTC signaling — no server. Two people
 * pass two short blobs by any out-of-band channel and get a direct link:
 *
 *   A: const { offer, transport, accept } = await manualOffer();
 *      // send `offer` to B; when B returns their answer: await accept(answer)
 *   B: const { answer, transport } = await manualAnswer(offer);
 *      // send `answer` back to A
 *
 * Each side's `transport` resolves/attaches once the channel opens. Attach the
 * transport to your hub immediately; it queues sends until the link is up.
 */
export async function manualOffer() {
  const pc = new RTCPeerConnection({ iceServers: STUN });
  const dc = pc.createDataChannel('spore', { ordered: false });
  const transport = new WebRTCTransport(dc);
  await pc.setLocalDescription(await pc.createOffer());
  await iceComplete(pc);
  return {
    offer: packSdp(pc.localDescription),
    transport,
    accept: (answerBlob) => pc.setRemoteDescription(unpackSdp(answerBlob)),
  };
}

export async function manualAnswer(offerBlob) {
  const pc = new RTCPeerConnection({ iceServers: STUN });
  let transport;
  const ready = new Promise((res) => {
    pc.ondatachannel = (ev) => {
      transport = new WebRTCTransport(ev.channel);
      res(transport);
    };
  });
  await pc.setRemoteDescription(unpackSdp(offerBlob));
  await pc.setLocalDescription(await pc.createAnswer());
  await iceComplete(pc);
  return { answer: packSdp(pc.localDescription), transport: ready };
}
