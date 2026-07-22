// In-memory loopback transport — links two hubs directly (for tests/demos).
import { Transport } from '../spore.mjs';

export class LoopbackTransport extends Transport {
  send(bytes) {
    if (this.peer) queueMicrotask(() => this.peer.receive(bytes.slice()));
  }
}

/** Make a linked pair; attach one to each hub. */
export function loopbackPair() {
  const a = new LoopbackTransport();
  const b = new LoopbackTransport();
  a.peer = b;
  b.peer = a;
  return [a, b];
}
