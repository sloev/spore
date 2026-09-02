// The browser transport registry (M10).
//
// One spec per transport the browser can actually run. `SporeClient.available
// Transports()` filters this by `available()` — feature detection, never a
// user-agent string — and the bridge-add screen renders only what survives.
// That is how the eleven daemon-only bridges (UDP, TCP, Tor, I2P, ICMP, iroh,
// Meshtastic-over-MQTT, SSB, copyparty, spool, foldersync) never appear in a
// browser: they are simply not in this file.
//
// Carried forward from the pre-M10 node's BRIDGES table, which was the proven
// acquisition logic — each transport already exposes a static `open()` that
// performs whatever user gesture it needs, so this table stays declarative.

import { LoopbackTransport, loopbackPair } from '../transports/loopback.mjs';
import { WebSocketTransport } from '../transports/websocket.mjs';
import { NostrTransport } from '../transports/nostr.mjs';
import { WebTorrentTransport } from '../transports/webtorrent.mjs';
import { WebSerialTransport } from '../transports/webserial.mjs';
import { WebBluetoothTransport } from '../transports/webbluetooth.mjs';
import { AudioModemTransport } from '../transports/audio.mjs';
import { MeshtasticSerialTransport, MeshtasticBLETransport } from '../transports/meshtastic.mjs';
import { ReticulumSerialTransport, ReticulumBLETransport } from '../transports/reticulum.mjs';

/** RNode radio parameters. Defaults are the EU 868 band the Rust bridge uses. */
export const RADIO_FIELDS = [
  { k: 'freq', label: 'Frequency', hint: 'MHz', value: '867.2' },
  { k: 'bw', label: 'Bandwidth', hint: 'kHz', value: '125' },
  { k: 'sf', label: 'Spreading factor', hint: '7-12', value: '8' },
  { k: 'cr', label: 'Coding rate', hint: '5-8', value: '5' },
  { k: 'tx', label: 'TX power', hint: 'dBm', value: '0' },
];

const radioOpts = (f) => ({
  freqHz: Math.round(parseFloat(f.freq) * 1e6),
  bwHz: Math.round(parseFloat(f.bw) * 1e3),
  sf: parseInt(f.sf, 10),
  cr: parseInt(f.cr, 10),
  txDbm: parseInt(f.tx, 10),
});

const hasSerial = () => Boolean(globalThis.navigator && navigator.serial);
const hasBluetooth = () => Boolean(globalThis.navigator && navigator.bluetooth);
const hasWebSocket = () => 'WebSocket' in globalThis;
const hasRtc = () => 'RTCPeerConnection' in globalThis;
const hasMic = () => Boolean(globalThis.navigator && navigator.mediaDevices && navigator.mediaDevices.getUserMedia);

/**
 * @typedef {Object} TransportSpec
 * @property {string}   kind         stable id, used in stored bridge configs
 * @property {string}   label        sentence-case description for the UI
 * @property {Array}    [fields]     config the screen must collect before open()
 * @property {() => boolean} available
 * @property {boolean}  [needsGesture] open() must run inside a user gesture
 * @property {boolean}  [manual]     no open(); needs a bespoke handshake screen
 * @property {boolean}  [persist]    safe to store and re-open on next launch
 * @property {(cfg: any) => Promise<any>} [open]
 */

/** @type {TransportSpec[]} */
export const TRANSPORTS = [
  {
    kind: 'websocket',
    label: 'WebSocket relay',
    fields: [{ k: 'url', label: 'Relay URL', hint: 'wss:// address of a public or private relay', value: '' }],
    available: hasWebSocket,
    persist: true,
    open: async (f) => new WebSocketTransport(f.url),
  },
  {
    kind: 'webtorrent',
    label: 'WebTorrent swarm — browser peers via tracker rendezvous',
    fields: [{ k: 'name', label: 'Swarm name', hint: 'Peers using the same name find each other', value: 'spore/public' }],
    available: () => hasRtc() && hasWebSocket(),
    persist: true,
    open: async (f) => WebTorrentTransport.join(f.name),
  },
  {
    kind: 'nostr',
    label: 'Nostr relay — kind-30078, tagged spore-v1',
    fields: [{ k: 'url', label: 'Relay URL', hint: 'wss:// address', value: '' }],
    available: hasWebSocket,
    persist: true,
    // window.nostr is a NIP-07 signer extension; without it the transport falls
    // back to an ephemeral key, which is a real difference worth not hiding.
    open: async (f) => new NostrTransport(f.url, globalThis.nostr ? (e) => globalThis.nostr.signEvent(e) : null),
  },
  {
    kind: 'meshtastic_serial',
    label: 'Meshtastic — USB serial (LoRa mesh)',
    available: hasSerial,
    needsGesture: true,
    open: async () => MeshtasticSerialTransport.open(),
  },
  {
    kind: 'meshtastic_ble',
    label: 'Meshtastic — Bluetooth (LoRa mesh)',
    available: hasBluetooth,
    needsGesture: true,
    open: async () => MeshtasticBLETransport.open(),
  },
  {
    kind: 'reticulum_serial',
    label: 'Reticulum / RNode — USB serial (LoRa)',
    fields: RADIO_FIELDS,
    available: hasSerial,
    needsGesture: true,
    open: async (f) => ReticulumSerialTransport.open(radioOpts(f)),
  },
  {
    kind: 'reticulum_ble',
    label: 'Reticulum / RNode — Bluetooth (LoRa)',
    fields: RADIO_FIELDS,
    available: hasBluetooth,
    needsGesture: true,
    open: async (f) => ReticulumBLETransport.open(radioOpts(f)),
  },
  {
    kind: 'webserial',
    label: 'Web Serial — generic KISS TNC',
    available: hasSerial,
    needsGesture: true,
    open: async () => WebSerialTransport.open({ baudRate: 115200 }),
  },
  {
    kind: 'webbluetooth',
    label: 'Web Bluetooth — generic Nordic UART (KISS)',
    available: hasBluetooth,
    needsGesture: true,
    open: async () => WebBluetoothTransport.open(),
  },
  {
    kind: 'audio',
    label: 'Audio modem — mic and speaker, 16-FSK',
    available: hasMic,
    needsGesture: true,
    open: async () => AudioModemTransport.open(),
  },
  {
    kind: 'webrtc',
    // No open(): this one cannot be created from a config object. It needs an
    // offer/answer exchanged out of band, so it gets its own handshake screen.
    // Listing it with a fake open() would be a control whose backend is missing.
    label: 'WebRTC — direct peer, copy/paste invite (no server)',
    available: hasRtc,
    manual: true,
    persist: false,
  },
  {
    kind: 'loopback',
    label: 'Loopback — offline self-test, no network',
    available: () => true,
    persist: false,
    // loopbackPair() already cross-links the two halves. Returns one of them;
    // its `.peer` is the other, for a second node to attach via
    // attachTransport(). Only useful in tests and the diagnostics screen.
    open: async () => loopbackPair()[0],
  },
];

export { LoopbackTransport };
