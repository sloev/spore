// Web Bluetooth transport — talk to a BLE SPORE node (an ESP32, an nRF board, a
// LoRa "walkie-talkie") over the Nordic UART Service, the de-facto serial-over-
// BLE profile most hobby radios expose. Chrome-family browsers only; the user
// picks the device with a gesture.
//
// NUS carries a raw byte stream, so we frame with KISS exactly like the wired
// serial bridge — same wire, no address (`dest` in the envelope decides who
// cares). BLE characteristics cap a write at the negotiated MTU (commonly 20 B),
// so outbound frames are chunked; the deframer reassembles on the other side.
//
//   const t = await WebBluetoothTransport.open();
//   hub.addTransport(t);
import { Transport } from '../spore.mjs';
import { kissFrame, KissDeframer } from './kiss.mjs';

// Nordic UART Service + its RX (write) and TX (notify) characteristics.
const NUS = '6e400001-b5a3-f393-e0a9-e50e24dcca9e';
const NUS_RX = '6e400002-b5a3-f393-e0a9-e50e24dcca9e'; // phone -> device (write)
const NUS_TX = '6e400003-b5a3-f393-e0a9-e50e24dcca9e'; // device -> phone (notify)
const CHUNK = 20; // conservative BLE payload; safe before MTU negotiation

export class WebBluetoothTransport extends Transport {
  constructor(device, rxChar, txChar) {
    super();
    this.device = device;
    this.rx = rxChar; // we write here
    this.deframer = new KissDeframer();
    txChar.addEventListener('characteristicvaluechanged', (ev) => {
      const dv = ev.target.value; // DataView
      const bytes = new Uint8Array(dv.buffer, dv.byteOffset, dv.byteLength);
      for (const frame of this.deframer.push(bytes)) this.receive(frame);
    });
    this._q = Promise.resolve();
  }

  /** Prompt for a NUS device, connect GATT, and start notifications. */
  static async open({ service = NUS } = {}) {
    if (!navigator.bluetooth) throw new Error('Web Bluetooth not supported in this browser');
    const device = await navigator.bluetooth.requestDevice({
      filters: [{ services: [service] }],
      optionalServices: [service],
    });
    const gatt = await device.gatt.connect();
    const svc = await gatt.getPrimaryService(service);
    const rxChar = await svc.getCharacteristic(NUS_RX);
    const txChar = await svc.getCharacteristic(NUS_TX);
    await txChar.startNotifications();
    return new WebBluetoothTransport(device, rxChar, txChar);
  }

  send(bytes) {
    const framed = kissFrame(bytes);
    // Serialise writes and split to the BLE payload cap.
    this._q = this._q.then(async () => {
      for (let i = 0; i < framed.length; i += CHUNK) {
        const slice = framed.subarray(i, i + CHUNK);
        try {
          if (this.rx.writeValueWithoutResponse) await this.rx.writeValueWithoutResponse(slice);
          else await this.rx.writeValue(slice);
        } catch {
          break; // link dropped
        }
      }
    });
  }

  close() {
    try {
      this.device.gatt.disconnect();
    } catch {
      /* already gone */
    }
  }
}
