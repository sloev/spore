# SPORE in the browser

The same SPORE node that runs on a laptop or an ESP32 also runs in a web page. The
Rust core is compiled to a small WebAssembly module, and a thin JavaScript layer
turns whatever the browser can talk over — a WebSocket, a WebRTC data channel, a
Nostr relay — into a SPORE link. A page becomes a full peer: it signs its own
messages, relays other people's, and needs no server of its own.

```js
import { loadSpore, Hub, ZERO_DEST } from './spore.mjs';
import { WebSocketTransport } from './transports/websocket.mjs';

const spore = await loadSpore(fetch('./spore.wasm'));
const hub = new Hub(spore.newNode());
hub.onDeliver = (env) => console.log('got:', new TextDecoder().decode(spore.payload(env)));

hub.addTransport(new WebSocketTransport('wss://relay.example/spore'));
hub.send(ZERO_DEST, new TextEncoder().encode('hello mesh'));
```

That's the whole surface: **load** the wasm, make a **node**, wrap it in a **hub**,
and **attach transports**. The hub is the browser twin of the Rust daemon's bridge
hub — a frame that arrives on one transport is fed to the router and its forwards
are relayed onto all the others.

<details>
<summary>Deep dive — how the wasm and the hub fit together</summary>

**No wasm-bindgen.** `spore.wasm` is a plain `wasm32-unknown-unknown` build with
exactly **one import**: `env.spore_fill_random(ptr, len)`. `loadSpore` supplies it
by calling `crypto.getRandomValues` over the module's own memory. On the Rust side
this is wired with getrandom's `custom` backend (`register_custom_getrandom!`), so
the whole crypto stack (ed25519, x25519, ChaCha20-Poly1305, blake2, sha2) runs
unchanged in the browser. You can confirm the single import with
`WebAssembly.Module.imports(new WebAssembly.Module(bytes))`.

That single import is the clearest example of the model in
[Design](../docs/DESIGN.md)'s "The spore and the soil": the core asks its
**runtime** for randomness rather than reaching for a source itself, which is
exactly why the same build runs in a browser tab and in a daemon. Time works the
same way (`now` is a parameter, never a clock the core reads). The browser is a
thin runtime in one respect worth knowing: there is no disk to spill the store
to, and the node stops when the last tab closes.

**Calling convention.** Rust hands variable-length results back as one `i64` that
packs a pointer and length: `(ptr << 32) | len`. JS reads the two halves with
`BigInt.asUintN(64, …)`, copies the bytes out of wasm memory, then calls
`spore_free`. Buffers going *in* are written into memory obtained from
`spore_alloc`. All of this is hidden inside `spore.mjs` — you only ever see
`Uint8Array`s.

**The node ABI** (`src/wasm.rs`) exposes: `spore_node_new/free`, `spore_node_addr`,
`spore_node_subscribe`, `spore_node_send(dest, payload, now)` and
`spore_node_recv(bytes, now)`. The two hot calls return a packed blob of two lists —
`{ forwards, delivered }` — where *forwards* are envelopes to relay onward and
*delivered* are envelopes addressed to (or subscribed by) this node. `spore.mjs`
parses that blob back into arrays of `Uint8Array`.

**Sealed mail** is a separate set of calls, and the distinction matters:
`spore_node_send` is the **raw, unsealed, unsigned** path — the one you use to
prove a transport carries bytes, not the one a person sends another person. For
that, `node.sendDirect(dest, payload)` seals to the peer's prekey (or through a §7
ratchet session, if one exists) and signs.

| Call | For |
|---|---|
| `node.announce()` | flood this node's prekey/topics. **Send one on connect** — until a peer has heard it, they have no key to seal to you and will fall back to cleartext |
| `node.sendDirect(dest, payload)` | a sealed, signed DM |
| `node.canSealTo(dest)` | *would* that DM actually be sealed? Ask before you promise it |
| `node.openDm(sender, payload, ratcheted)` | open a delivered DM; `null` if it does not open |
| `spore.src(env)` | the **authenticated** sender — `null` for unsigned, unverified, or `SRC8` |
| `spore.flags(env)` | `FLAG_ENCRYPTED`, `FLAG_RATCHET` — the latter picks which scheme `openDm` uses |

Two rules a UI has to follow. **Never draw a padlock unconditionally:**
`sendDirect` falls back to cleartext when no key is known, correctly and
silently, so `canSealTo` is what earns the icon. **Key threads on `spore.src`,
never on a claimed field** — a signed envelope proves its own sender, and
anything weaker is spoofable.

**The Hub** owns one node and N transports. On `addTransport(t)` it replaces
`t.receive` so inbound frames route through `node.recv`; deliveries fire
`onDeliver`, and forwards are `send()` onto every *other* transport (split-horizon).
`hub.send(dest, payload)` originates a message from this node onto all transports.
</details>

## Transports

A transport is any object with a `send(bytes)` method that calls `receive(bytes)`
when a frame arrives. Eleven are included; writing your own is a dozen lines.
The standalone build inlines ten of them — Web NFC is left out because it needs
Chrome on Android over HTTPS, which a `file://` page is not.

| Transport | File | Use |
|---|---|---|
| **WebSocket** | `transports/websocket.mjs` | relay or direct peer; works in browser and Node 22+ |
| **WebRTC** | `transports/webrtc.mjs` | direct browser-to-browser; manual copy/paste (`manualOffer`/`manualAnswer`) or your own signaling; no server after connect |
| **WebTorrent** | `transports/webtorrent.mjs` | join a swarm by name; real bittorrent-tracker rendezvous, then P2P over WebRTC |
| **Nostr** | `transports/nostr.mjs` | any Nostr relay becomes a SPORE bag (kind-30078, tag `spore-v1`) |
| **Meshtastic** | `transports/meshtastic.mjs` | a Meshtastic LoRa node over USB or Bluetooth; envelope rides a `MeshPacket` (portnum 256), codec ported from the Rust `bridge::meshtastic` |
| **Reticulum / RNode** | `transports/reticulum.mjs` | an RNode LoRa modem over USB or Bluetooth in host/KISS mode; you set the radio (freq/bw/sf/cr/tx) |
| **Web Serial** | `transports/webserial.mjs` | a generic USB KISS TNC/board; interops with the Rust serial bridge |
| **Web Bluetooth** | `transports/webbluetooth.mjs` | a generic BLE radio over the Nordic UART Service; KISS-framed |
| **Audio modem** | `transports/audio.mjs` | data-over-sound; a 16-FSK modem **bit-compatible** with the Rust `bridge::audio` |
| **Web NFC** | `transports/webnfc.mjs` | tap-to-transfer at a few centimetres; one envelope per `application/x-spore` NDEF record. Chrome on Android over HTTPS only, so it is **not** inlined into the standalone build — import it from `transports/` |
| **Loopback** | `transports/loopback.mjs` | in-memory link between two hubs, for tests and offline demos |

The KISS framing shared by the serial and Bluetooth transports lives in
`transports/kiss.mjs` and matches Rust's `src/kiss.rs` byte-for-byte, so a browser
tab and a physical board speak the same wire. The Meshtastic and Reticulum
transports are honest device drivers (Meshtastic protobuf / RNode host protocol)
but are not hardware-verified in CI — treat them as templates to confirm against
your firmware.

<details>
<summary>Deep dive — writing a transport, and the ones you'd add next</summary>

The base class is trivial:

```js
import { Transport } from './spore.mjs';
export class MyTransport extends Transport {
  send(bytes) { /* put bytes on the wire */ }
  // call this.receive(bytes) whenever a frame comes back
}
```

`send` may be async and should queue while the underlying channel is still opening
(the WebSocket and WebRTC transports show the pattern). Frames are whole SPORE
envelopes — one per message; don't split or concatenate them.

Browser media not yet wrapped but that fit the same shape: **WebTransport** (QUIC
datagrams — closest match to SPORE's datagram model) and **NFC** (`Web NFC`, for a
tap-to-seed). Each is a `send`/`receive` pair over its own API; the hub and node
above don't change.
</details>

## Run the tests

```sh
# build the wasm the JS loads
cargo build --release --lib --target wasm32-unknown-unknown

cd web
node test.mjs           # loopback: two hubs, one link, publish + verify
npm install             # pulls `ws` for the next one
node ws-test.mjs        # real WebSocket relay: A -> relay -> B
```

Both print an `OK` line. `test.mjs` needs nothing but Node (it runs the wasm);
`ws-test.mjs` stands up a throwaway `ws` relay on a random port and sends a signed
message through it.

## One-file node

`node build-standalone.mjs` inlines the wasm and **every transport** into a single
`spore-standalone.html` — a complete, functional node that runs from a `file://`
path, a USB stick, or an email attachment, making **zero network requests until you
add a bridge**. It boots one live node, then lets you wire it at runtime to a
WebSocket relay, a direct WebRTC peer, a Nostr relay, a Meshtastic or Reticulum
LoRa radio (USB or Bluetooth), the speakers/mic (audio modem), or a WebTorrent
swarm — signing, relaying, and delivering across all of them at once. Its identity
and its bridges are remembered in the browser's local storage (the 32-byte signing
seed via `node.seed()` / `newNode(seed)`), so it returns as the same node; network
bridges reconnect on load, device bridges wait for a click. It's the smallest
"a whole node in one file" seed; see [`docs/CONTINUITY.md`](../docs/CONTINUITY.md).

This same file *is* the **web node** (the site's `/demo/` page): the Pages workflow
builds it once and serves it at `/demo/` — one URL, not the same ~720 KB payload
shipped twice under two paths. `docs/APPS.md`'s "Download" button points at that
same URL with an HTML `download` attribute, so opening it runs the node and
downloading it saves the identical file. There is no separate demo page to keep
in sync — the thing you download and the thing you run in the browser are one file.
