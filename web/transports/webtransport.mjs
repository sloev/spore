import { Transport } from '../spore.mjs';

export class WebTransportBridge extends Transport {
  constructor(url) {
    super();
    this.url = url;
    this.transport = null;
    this.writer = null;
    this.reader = null;
    this._queue = [];
    this._connected = false;
  }

  async connect() {
    try {
      this.transport = new WebTransport(this.url);
      await this.transport.ready;
      this._connected = true;
      
      this.writer = this.transport.datagrams.writable.getWriter();
      this.reader = this.transport.datagrams.readable.getReader();
      
      // Start reading datagrams
      const readLoop = async () => {
        try {
          while (true) {
            const { value, done } = await this.reader.read();
            if (done) break;
            if (value) this.receive(value);
          }
        } catch (error) {
          console.error('WebTransport read error:', error);
        }
      };
      readLoop();
      
      // Flush queued messages
      this._flushQueue();
    } catch (error) {
      console.error('WebTransport connection failed:', error);
      this._connected = false;
    }
  }

  send(bytes) {
    if (this._connected && this.writer) {
      this.writer.write(bytes)
        .catch(err => console.error('WebTransport send error:', err));
    } else {
      this._queue.push(bytes);
    }
  }

  _flushQueue() {
    while (this._queue.length > 0) {
      const bytes = this._queue.shift();
      this.writer.write(bytes)
        .catch(err => console.error('WebTransport queue flush error:', err));
    }
  }

  close() {
    if (this.writer) this.writer.close();
    if (this.transport) this.transport.close();
  }
}