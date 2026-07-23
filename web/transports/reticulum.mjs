// Reticulum / RNode transports — bridge a page to an RNode LoRa modem (the radio
// Reticulum uses) over USB (Web Serial) or Bluetooth (Nordic UART), straight from
// the browser. An RNode in host/KISS mode is a raw LoRa pipe: whatever bytes you
// hand it in a DATA frame it transmits, and every DATA frame it hears it hands
// back. We put a SPORE envelope in each DATA frame, so envelopes flood over the
// same LoRa air Reticulum runs on. The device has no address (everyone in range
// hears every frame); the envelope's own `dest` decides who cares.
//
// The radio must be configured before it will transmit — frequency, bandwidth,
// spreading factor, coding rate, TX power — so pass those to `open()` (match your
// region and the other radios). This is an honest RNode host-mode driver, not a
// full Reticulum transport node: it moves envelopes over the radio, it does not
// route RNS packets.
//
//   const t = await ReticulumSerialTransport.open({
//     freqHz: 867200000, bwHz: 125000, sf: 8, cr: 5, txDbm: 0,
//   });
//   hub.addTransport(t);
import { Transport } from '../spore.mjs';

// Uniquely named (this module shares scope with the other transports when the
// standalone inlines them all).
const RN_FEND = 0xc0, RN_FESC = 0xdb, RN_TFEND = 0xdc, RN_TFESC = 0xdd;
// RNode host-protocol commands (subset needed to configure + move data).
const CMD_DATA = 0x00;
const CMD_FREQUENCY = 0x01;
const CMD_BANDWIDTH = 0x02;
const CMD_TXPOWER = 0x03;
const CMD_SF = 0x04;
const CMD_CR = 0x05;
const CMD_RADIO_STATE = 0x06;

// One KISS frame: FEND CMD …escaped… FEND.
function kissCmd(cmd, data) {
  const out = [RN_FEND, cmd];
  for (const b of data) {
    if (b === RN_FEND) out.push(RN_FESC, RN_TFEND);
    else if (b === RN_FESC) out.push(RN_FESC, RN_TFESC);
    else out.push(b);
  }
  out.push(RN_FEND);
  return Uint8Array.from(out);
}
const be32 = (n) => [(n >>> 24) & 0xff, (n >>> 16) & 0xff, (n >>> 8) & 0xff, n & 0xff];

// Streaming KISS de-framer that KEEPS the command byte (RNode multiplexes data
// and status on the command), yielding { cmd, data } per frame.
class RNodeDeframer {
  constructor() { this.cur = []; this.inFrame = false; this.cmd = -1; this.gotCmd = false; this.esc = false; }
  push(chunk) {
    const frames = [];
    for (const b of chunk) {
      if (b === RN_FEND) {
        if (this.inFrame && this.gotCmd) frames.push({ cmd: this.cmd, data: Uint8Array.from(this.cur) });
        this.inFrame = true; this.gotCmd = false; this.esc = false; this.cur = [];
        continue;
      }
      if (!this.inFrame) continue;
      if (!this.gotCmd) { this.cmd = b; this.gotCmd = true; continue; }
      if (this.esc) { this.cur.push(b === RN_TFEND ? RN_FEND : b === RN_TFESC ? RN_FESC : b); this.esc = false; }
      else if (b === RN_FESC) this.esc = true;
      else this.cur.push(b);
    }
    return frames;
  }
}

// ---- shared behaviour: subclasses provide _write(bytes) and call _feed(bytes)-
class RNodeBase extends Transport {
  constructor(radio) {
    super();
    this.radio = radio;
    this.deframer = new RNodeDeframer();
  }
  // Bring the radio up for TX/RX. Called once the link is open.
  async configure() {
    const r = this.radio || {};
    if (r.freqHz) await this._write(kissCmd(CMD_FREQUENCY, be32(r.freqHz >>> 0)));
    if (r.bwHz) await this._write(kissCmd(CMD_BANDWIDTH, be32(r.bwHz >>> 0)));
    if (r.txDbm != null) await this._write(kissCmd(CMD_TXPOWER, [r.txDbm & 0xff]));
    if (r.sf) await this._write(kissCmd(CMD_SF, [r.sf & 0xff]));
    if (r.cr) await this._write(kissCmd(CMD_CR, [r.cr & 0xff]));
    await this._write(kissCmd(CMD_RADIO_STATE, [1])); // radio on
  }
  _feed(bytes) {
    for (const f of this.deframer.push(bytes)) {
      if (f.cmd === CMD_DATA && f.data.length) this.receive(f.data);
    }
  }
  send(bytes) {
    this._write(kissCmd(CMD_DATA, bytes)).catch(() => {});
  }
}

// ---- USB (Web Serial) --------------------------------------------------------
export class ReticulumSerialTransport extends RNodeBase {
  constructor(port, radio) {
    super(radio);
    this.port = port;
    this.writer = port.writable.getWriter();
    this._read(port.readable.getReader());
  }
  static async open({ port = null, baudRate = 115200, ...radio } = {}) {
    if (!navigator.serial) throw new Error('Web Serial not supported in this browser');
    const p = port || (await navigator.serial.requestPort());
    await p.open({ baudRate });
    const t = new ReticulumSerialTransport(p, radio);
    await t.configure();
    return t;
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
  _write(bytes) { return this.writer.write(bytes); }
  async close() {
    try { await this.reader?.cancel(); } catch { /* */ }
    try { this.writer.releaseLock(); await this.port.close(); } catch { /* */ }
  }
}

// ---- Bluetooth (Nordic UART, as RNode BLE exposes) ---------------------------
const RN_NUS = '6e400001-b5a3-f393-e0a9-e50e24dcca9e';
const RN_NUS_RX = '6e400002-b5a3-f393-e0a9-e50e24dcca9e'; // write to device
const RN_NUS_TX = '6e400003-b5a3-f393-e0a9-e50e24dcca9e'; // notify from device
const RN_BLE_CHUNK = 20;

export class ReticulumBLETransport extends RNodeBase {
  constructor(device, rxChar, radio) {
    super(radio);
    this.device = device;
    this.rx = rxChar;
    this._q = Promise.resolve();
  }
  static async open({ ...radio } = {}) {
    if (!navigator.bluetooth) throw new Error('Web Bluetooth not supported in this browser');
    const device = await navigator.bluetooth.requestDevice({
      filters: [{ services: [RN_NUS] }],
      optionalServices: [RN_NUS],
    });
    const gatt = await device.gatt.connect();
    const svc = await gatt.getPrimaryService(RN_NUS);
    const rxChar = await svc.getCharacteristic(RN_NUS_RX);
    const txChar = await svc.getCharacteristic(RN_NUS_TX);
    const t = new ReticulumBLETransport(device, rxChar, radio);
    txChar.addEventListener('characteristicvaluechanged', (ev) => {
      const dv = ev.target.value;
      t._feed(new Uint8Array(dv.buffer, dv.byteOffset, dv.byteLength));
    });
    await txChar.startNotifications();
    await t.configure();
    return t;
  }
  _write(bytes) {
    this._q = this._q.then(async () => {
      for (let i = 0; i < bytes.length; i += RN_BLE_CHUNK) {
        const slice = bytes.subarray(i, i + RN_BLE_CHUNK);
        try {
          if (this.rx.writeValueWithoutResponse) await this.rx.writeValueWithoutResponse(slice);
          else await this.rx.writeValue(slice);
        } catch { break; }
      }
    });
    return this._q;
  }
  close() { try { this.device.gatt.disconnect(); } catch { /* */ } }
}
