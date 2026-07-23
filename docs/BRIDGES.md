# Bridge matrix — every medium SPORE can ride

A bridge is a thin adapter between one medium and the SPORE router. Because the
router is medium-independent, **every bridge reduces to the same three things**,
and all the shared logic already lives in the Rust lib (`src/bridge/`):

1. **An address type `U`** — how that medium names a peer (a MAC, an IP:port, a
   Meshtastic node number, or `()` for broadcast-only media). Learned and resolved
   by `bridge::Neighbors<U>` (SPORE's ARP), which is generic over `U`.
2. **A payload budget (MTU)** — the bridge clamps the shared node's `mtu`, and
   SPORE's fountain fragmentation (`send`) auto-splits to fit. No per-medium
   chunking code.
3. **`recv` / `send`** — the *only* platform-specific part. A datagram medium
   implements `bridge::driver::DatagramTransport` (two methods) and gets neighbour
   learning, resolution, relay, and MTU handling for free from
   `driver::run_datagram`. Stream media (TCP) and shared stores (folders) are the
   two other forms.

So adding a medium is: pick `U`, set the MTU, write `recv`/`send`. Everything else
is already in the lib.

> **Shapes vs. forms.** The spec (Page 2) names **five medium *shapes*** — message
> pipe, byte stream, text channel, shared bus, shared store. In this reference they
> collapse to **three driver *forms*** — `dgram`, `stream`, `store` — because
> message pipes and shared buses both ride the datagram driver, and text channels
> and shared stores both ride the store pattern. The tables below list the driver
> *form*; the *shape* is the spec category it comes from.

**Legend.** `U` = underlay address · **State**: stateless (fire-and-forget) /
stateful (keep a connection, drop the neighbour on disconnect via
`Neighbors::forget`) / null (`U = ()`, broadcast-only, filter by the envelope's
own `dest`). **Form** (the driver form the medium's Page-2 shape collapses to): `dgram` (via `DatagramTransport`) · `stream` · `store`.
**Status**: ✅ implemented · ◑ partial (codec/framer present) · ▢ planned (thin
shim to write).

## 1. Direct links & radios

Physical and link-layer media — the wire, the air, sound. SPORE floods across
whatever's in earshot.

| Medium | `U` | MTU | State | Form | Status | Notes |
|---|---|---|---|---|---|---|
| UDP / IPv4 (limited bcast) | `SocketAddr` | 1400 | stateless | dgram | ✅ | `bridge::udp::run` — `255.255.255.255:port` |
| UDP primary-subnet bcast | `SocketAddr` | 1400 | stateless | dgram | ✅ | `bridge::udp::run_primary` — auto-finds `192.168.x.255`; zero-config LAN |
| Ethernet 802.3 | `[u8;6]` | 1500 | stateless | dgram | ▢ | raw L2 socket per OS |
| Wi-Fi 802.11 | `[u8;6]` | 2304 | stateful | dgram | ▢ | raw/monitor mode |
| Wi-Fi Direct | `Ipv4Addr` | 1500 | stateful | dgram | ◑ | UDP bridge over the p2p iface |
| ESP-NOW | `[u8;6]` | 250 | stateless | dgram | ▢ | ESP32 (`esp-idf`) shim |
| BLE GATT | `[u8;6]` | ~247 | stateful | stream | ▢ | per-OS BLE / Web Bluetooth |
| BLE Mesh | `u16` | 380 | stateless | dgram | ▢ | stack segments |
| NFC (14443) | `[u8;7]` | ~255 | stateful | dgram | ▢ | NDEF `application/x-spore` |
| LiFi (802.15.7) | `[u8;6]` | var | stateless | dgram | ▢ | VLC modem |
| IrDA | `u32` | 2048 | stateful | dgram | ▢ | IrCOMM |
| Zigbee | `u64` | 104 | stateful | dgram | ▢ | APS fragmentation |
| Z-Wave | `u8` | 54 | stateful | dgram | ▢ | serial controller |
| LoRaWAN | `u32` | 51–222 | stateful | dgram | ▢ | via a network server |
| LoRa (P2P) | `()` | 255 | null | dgram | ▢ | RNode / SX127x |
| Ham AX.25 (KISS) | `String` | 256 | stateless | dgram | ◑ | `bridge::KissStream` framer; TNC runner |
| APRS | `String` (call-ssid) | ~200 | stateless | dgram | ▢ | messages over AX.25 / APRS-IS |
| DMR | `u32` | var | stateless | dgram | ▢ | IP-over-DMR |
| goTenna | `u32` (GID) | ~200 | stateless | dgram | ▢ | consumer mesh radio |
| **Audio (ggwave-style)** | `()` | 4 K/frame | null | dgram | ◑ | **`bridge::audio`** — 16-FSK modem, tested; sound-card via `run_pipe`; bit-compatible browser twin `web/transports/audio.mjs` |
| JANUS (sonar) | `u8` | 32 | stateless | dgram | ▢ | underwater acoustic |
| QR stream | `()` | ~1 K | null | dgram | ◑ | `armor` present; camera/screen runner |
| Iridium SBD | `u32` | 340 | stateless | dgram | ▢ | satellite gateway |

### Meshtastic (LoRa mesh) — one codec, several pipes

The Meshtastic `MeshPacket` protobuf codec (`bridge::meshtastic::encode`/`decode`)
is done and tested; each row below is just a different pipe carrying the same
frames, so `U` and MTU never change.

| Transport | `U` | MTU | State | Form | Status | Notes |
|---|---|---|---|---|---|---|
| Meshtastic — WiFi-UDP | `u32` | 237 | stateless | dgram | ✅ | `bridge::meshtastic::run` (multicast) |
| Meshtastic — USB serial | `u32` | 237 | stateful | stream | ◑ | same protobuf over the Serial API; framing runner TODO |
| Meshtastic — Web Serial | `u32` | 237 | stateful | stream | ◑ | JS `web/transports/meshtastic.mjs` (0x94c3 framing + ToRadio/FromRadio) |
| Meshtastic — BT/BLE | `u32` | 237 | stateful | stream | ◑ | JS `web/transports/meshtastic.mjs` (BLE service, ToRadio/FromRadio) |

### Reticulum (RNS) — destination-addressed, several pipes

Reticulum is itself a mesh with strong crypto; SPORE rides it as a payload,
addressed by the 16-byte RNS destination hash.

| Transport | `U` | MTU | State | Form | Status | Notes |
|---|---|---|---|---|---|---|
| Reticulum — TCP/UDP iface | `[u8;16]` | 500 | stateless | dgram | ▢ | RNS destination over an IP interface |
| Reticulum — RNode serial | `[u8;16]` | 500 | stateful | stream | ▢ | LoRa RNode over USB serial |
| Reticulum — Web Serial | `[u8;16]` | 500 | stateful | stream | ◑ | JS `web/transports/reticulum.mjs` — RNode host/KISS over USB |
| Reticulum — BT/BLE | `[u8;16]` | 500 | stateful | stream | ◑ | JS `web/transports/reticulum.mjs` — RNode host/KISS over BLE (Nordic UART) |

## 2. Internet overlays

Mesh-routing and anonymity networks that deliver packets over (or instead of) IP.
Most already carry IP, so the existing **UDP bridge rides them unchanged** — you
just point it at the right address on the overlay's interface.

| Overlay | `U` | MTU | State | Form | Status | Notes |
|---|---|---|---|---|---|---|
| BATMAN-adv | `[u8;6]` | 1500 | stateless | dgram | ◑ | UDP broadcast on `bat0` |
| Yggdrasil / cjdns | `Ipv6Addr` | 1280 | stateless | dgram | ◑ | UDP over the tun; one interface |
| Thread | `Ipv6Addr` | 1280 | stateless | dgram | ◑ | UDP over 6LoWPAN |
| Tor (onion service) | `.onion` | 64 K | stateful | stream | ▢ | hidden-service rendezvous |
| I2P | b32 dest | ~1200 | stateless | dgram | ▢ | garlic routing; SAM datagrams |
| Veilid | node id | var | stateless | dgram | ▢ | private-routed DHT |
| libp2p (gossipsub) | PeerId | var | stateful | stream | ▢ | pub/sub overlay; IPFS swarm |
| WebSocket | conn | 64 K | stateful | stream | ◑ | JS `web/transports/websocket.mjs`, tested; native shim TODO |
| WebTransport | conn | var | stateful | stream | ▢ | Web (QUIC datagrams) |
| WebRTC DataChannel | `String` | 16 K | stateful | stream | ◑ | JS `web/transports/webrtc.mjs`, serverless copy/paste signaling |
| WebTorrent swarm | `String` | 16 K | stateful | stream | ◑ | JS `web/transports/webtorrent.mjs` — bittorrent-tracker rendezvous, then WebRTC P2P |
| Web Serial / USB | conn | var | stateful | stream | ◑ | JS `web/transports/webserial.mjs` — KISS to a TNC/RNode/ESP32, interops with the serial bridge |
| Web Bluetooth | `String` | ~247 | stateful | stream | ◑ | JS `web/transports/webbluetooth.mjs` — Nordic UART, KISS-framed |

## 3. Store-and-forward & app carriers

Systems that already store messages and pass them on — the shape SPORE *is*, so
these compose cleanly. An envelope becomes a file, an event, a chat message, or a
bundle; the container replicates it, and the other side unwraps it.

| Carrier | `U` | MTU | State | Form | Status | Notes |
|---|---|---|---|---|---|---|
| Folder / USB / Syncthing | — | — | store | store | ✅ | `bridge::store` — `*.spore` files |
| HTTP bag | conn | 64 K | stateful | store | ✅ | `bridge::bag` (pull) |
| Copyparty | URL | 64 K | stateful | store | ▢ | envelopes in a copyparty share (HTTP/WebDAV upload) |
| Text armor (SMS/paper/voice) | — | ~150 | null | store | ✅ | `armor::wrap`/`unwrap` |
| Nostr | relay | var | stateless | store | ◑ | JS `web/transports/nostr.mjs` (kind-30078) |
| **SSB (Secure Scuttlebutt)** | feed | var | stateless | store | ◑ | **`bridge::ssb`** — `spore-v1` content; folder-log runner |
| Matrix | room id | large | stateless | store | ▢ | envelopes as room events (base64) |
| XMPP / Jabber | JID | large | stateful | stream | ▢ | message stanzas or PubSub |
| DeltaChat (email) | address | large | stateless | store | ▢ | envelopes as e-mail (IMAP/SMTP, Autocrypt) |
| Session (Oxen) | session id | var | stateless | store | ▢ | onion store-and-forward |
| Briar | contact | var | stateful | stream | ▢ | Tor + BLE friend-to-friend |
| Tox | ToxID | ~1200 | stateful | dgram | ▢ | P2P DHT messaging |
| DTN / Bundle Protocol (BPv7) | EID | var | stateless | store | ▢ | RFC 9171 delay-tolerant bundles — natural fit |
| NNCP | node id | var | stateless | store | ▢ | encrypted node-to-node copy |
| UUCP | host | var | stateless | store | ▢ | the original store-and-forward |
| Serval Rhizome | SID | var | stateless | store | ▢ | mesh store-and-forward |
| Hypercore / Hyperswarm | key | var | stateful | stream | ▢ | append-log replication |
| Earthstar / Willow | share | var | stateless | store | ▢ | offline-first sync |

## The three routing implications (all handled by `Neighbors<U>`)

- **Stateful** (WebSocket, BLE GATT, serial, Wi-Fi): `U` is a live connection
  object. When it drops, the bridge calls `Neighbors::forget` so the router stops
  sending into a dead handle.
- **Stateless** (LoRa, ESP-NOW, Ethernet, most store carriers): no connection —
  hand bytes to `U`. A neighbour is "gone" only when its signed heartbeats stop
  and its binding ages out (`Neighbors::expire`).
- **Null address** (`U = ()`: audio, raw LoRa, QR): the hardware has no target
  field — everyone hears everything. Every SPORE address maps to `()`, `resolve`
  trivially succeeds, and the envelope's own `dest` filters mail for others.

## How the forms map to code

- **`dgram`** → implement `driver::DatagramTransport` (`recv`, `send`, optional
  `mtu`). See `bridge::udp` (~40 lines) and `bridge::meshtastic` as templates.
- **`stream`** → a byte stream + framing (KISS). See `bridge::tcp` and
  `bridge::KissStream`.
- **`store`** → a shared container polled for new items. See `bridge::store`
  (files), `bridge::bag` (HTTP), and `bridge::ssb` (a log folder).

## Why so many rows share code

Several rows are ◑ because they're **IP underlays** — BATMAN, Yggdrasil, cjdns,
Thread, Wi-Fi Direct all deliver IP, so the existing UDP bridge already rides them
today; you only point it at the right broadcast/multicast address (§ "Routing
across other networks" in the README). Whole *families* also collapse to one
codec: every Meshtastic pipe reuses `bridge::meshtastic`'s protobuf, every
store-and-forward carrier reuses the "envelope in, envelope out" pattern of
`bridge::store`/`bridge::ssb`, and every browser transport is a `send`/`receive`
pair over the JS hub (`web/README.md`). The rest are ▢: a thin
`DatagramTransport`/stream/store shim per medium, most of which belong in a
platform-specific crate (ESP32, Android, browser) rather than the portable core.
