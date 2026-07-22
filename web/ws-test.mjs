import { WebSocketServer } from 'ws';
import fs from 'node:fs';
import { loadSpore, Hub, ZERO_DEST } from './spore.mjs';
import { WebSocketTransport } from './transports/websocket.mjs';

try {
  const wasm = fs.readFileSync(new URL('../target/wasm32-unknown-unknown/release/spore.wasm', import.meta.url));
  const spore = await loadSpore(wasm);
  const wss = new WebSocketServer({ port: 0 });
  wss.on('connection', (sock) => sock.on('message', (data) => {
    for (const c of wss.clients) if (c !== sock && c.readyState === 1) c.send(data);
  }));
  await new Promise((r) => wss.on('listening', r));
  const url = `ws://localhost:${wss.address().port}`;

  const hubA = new Hub(spore.newNode());
  const hubB = new Hub(spore.newNode());
  let got = null;
  hubB.onDeliver = (env) => { if (spore.verify(env)) got = new TextDecoder().decode(spore.payload(env)); };
  hubA.addTransport(new WebSocketTransport(url));
  hubB.addTransport(new WebSocketTransport(url));

  await new Promise((r) => setTimeout(r, 300));
  hubA.send(ZERO_DEST, new TextEncoder().encode('hello over a websocket relay'));
  await new Promise((r) => setTimeout(r, 300));

  console.log(got === 'hello over a websocket relay'
    ? 'WEBSOCKET OK — A -> relay -> B, received + verified'
    : 'WEBSOCKET FAIL — got: ' + JSON.stringify(got));
} catch (e) {
  console.log('WEBSOCKET ERROR:', e.message);
}
process.exit(0);
