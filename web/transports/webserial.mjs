// Web Serial transport — talk to a physical SPORE node (a LoRa board, an ESP32,
// a TNC) plugged into a USB/serial port, straight from the page. Chrome-family
// browsers expose `navigator.serial`; the user picks the port with a gesture.
//
// Frames use KISS, matching the Rust serial/stream bridge byte-for-byte, so the
// page and the board speak the same wire.
//
//   const t = await WebSerialTransport.open({ baudRate: 115200 });
//   hub.addTransport(t);
import { Transport } from '../spore.mjs';
import { kissFrame, KissDeframer } from './kiss.mjs';

export class WebSerialTransport extends Transport {
  constructor(port) {
    super();
    this.port = port;
    this.deframer = new KissDeframer();
    this.writer = port.writable.getWriter();
    this._readLoop(port.readable.getReader());
  }

  /** Prompt for a port (or reuse a previously granted one) and open it. */
  static async open({ baudRate = 115200, port = null } = {}) {
    if (!navigator.serial) throw new Error('Web Serial not supported in this browser');
    const p = port || (await navigator.serial.requestPort());
    await p.open({ baudRate });
    return new WebSerialTransport(p);
  }

  async _readLoop(reader) {
    this.reader = reader;
    try {
      for (;;) {
        const { value, done } = await reader.read();
        if (done) break;
        if (value) for (const frame of this.deframer.push(value)) this.receive(frame);
      }
    } catch {
      /* port closed / unplugged */
    } finally {
      reader.releaseLock();
    }
  }

  send(bytes) {
    // Fire-and-forget; the writer serialises internally.
    this.writer.write(kissFrame(bytes)).catch(() => {});
  }

  async close() {
    try {
      await this.reader?.cancel();
    } catch {
      /* already gone */
    }
    try {
      this.writer.releaseLock();
      await this.port.close();
    } catch {
      /* already gone */
    }
  }
}
