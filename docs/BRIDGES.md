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
   two other shapes.

So adding a medium is: pick `U`, set the MTU, write `recv`/`send`. Everything else
is already in the lib.

## The matrix

`U` = underlay address · **State**: stateless (fire-and-forget) / stateful (keep a
connection, drop the neighbour on disconnect via `Neighbors::forget`) / null
(`U = ()`, broadcast-only, filter by the envelope's own `dest`). **Shape**:
`dgram` (via `DatagramTransport`) · `stream` · `store`. **Status**: ✅ implemented ·
◑ partial (codec/framer present) · ▢ planned (thin shim to write).

| Medium | `U` | MTU | State | Shape | Status | Notes |
|---|---|---|---|---|---|---|
| UDP / IPv4 | `SocketAddr` | 1400 | stateless | dgram | ✅ | `bridge::udp` |
| Meshtastic (WiFi-UDP) | `u32` | 237 | stateless | dgram | ✅ | `bridge::meshtastic` |
| TCP (KISS) | conn | 64 K | stateful | stream | ✅ | `bridge::tcp` |
| Folder / USB / Syncthing | — | — | store | store | ✅ | `bridge::store` |
| HTTP bag | conn | 64 K | stateful | store | ✅ | `bridge::bag` (pull) |
| Text armor (SMS/paper/voice) | — | ~150 | null | store | ✅ | `armor::wrap/unwrap` |
| Ethernet 802.3 | `[u8;6]` | 1500 | stateless | dgram | ▢ | raw L2 socket per OS |
| Wi-Fi 802.11 | `[u8;6]` | 2304 | stateful | dgram | ▢ | raw/monitor mode |
| Wi-Fi Direct | `Ipv4Addr` | 1500 | stateful | dgram | ◑ | UDP bridge over the p2p iface |
| BATMAN-adv | `[u8;6]` | 1500 | stateless | dgram | ◑ | UDP broadcast on `bat0` |
| Yggdrasil / cjdns | `Ipv6Addr` | 1280 | stateless | dgram | ◑ | UDP over the tun; one interface |
| Thread | `Ipv6Addr` | 1280 | stateless | dgram | ◑ | UDP over 6LoWPAN |
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
| Ham AX.25 | `String` | 256 | stateless | dgram | ◑ | KISS framer present; TNC runner |
| DMR | `u32` | var | stateless | dgram | ▢ | IP-over-DMR |
| Reticulum | `[u8;16]` | 500 | stateless | dgram | ▢ | RNS destination |
| WebSocket | conn | 64 K | stateful | stream | ▢ | native + Web |
| WebTransport | conn | var | stateful | stream | ▢ | Web (QUIC) |
| WebRTC DataChannel | `String` | 16 K | stateful | stream | ▢ | Web / native, serverless signaling |
| Web Serial / USB | conn | var | stateful | stream | ▢ | drive a TNC from a browser tab |
| Nostr | relay | var | stateless | store | ▢ | one event per envelope on relays |
| ggwave / libquiet (audio) | `()` | 140/255 | null | dgram | ▢ | acoustic, sound card |
| JANUS (sonar) | `u8` | 32 | stateless | dgram | ▢ | underwater acoustic |
| QR stream | `()` | ~1 K | null | dgram | ◑ | armor present; camera/screen runner |
| Iridium SBD | `u32` | 340 | stateless | dgram | ▢ | satellite gateway |

## The three routing implications (all handled by `Neighbors<U>`)

- **Stateful** (WebSocket, BLE GATT, Wi-Fi): `U` is a live connection object. When
  it drops, the bridge calls `Neighbors::forget` so the router stops sending into a
  dead handle.
- **Stateless** (LoRa, ESP-NOW, Ethernet): no connection — blast bytes at `U`. A
  neighbour is "gone" only when its signed heartbeats stop and its binding ages out
  (`Neighbors::expire`).
- **Null address** (`U = ()`: audio, raw LoRa, QR): the hardware has no target
  field — everyone hears everything. Every SPORE address maps to `()`, `resolve`
  trivially succeeds, and the envelope's own `dest` filters mail for others.

## How the shapes map to code

- **`dgram`** → implement `driver::DatagramTransport` (`recv`, `send`, optional
  `mtu`). See `bridge::udp` (~40 lines) and `bridge::meshtastic` as templates.
- **`stream`** → a byte stream + framing (KISS). See `bridge::tcp`.
- **`store`** → a shared container polled for new items. See `bridge::store`.

Several rows are marked ◑ because they're **IP underlays** — BATMAN, Yggdrasil,
cjdns, Thread, Wi-Fi Direct all deliver IP, so the existing UDP bridge already
rides them today; you only point it at the right broadcast/multicast address
(§ "Routing across other networks" in the README). The rest are ▢: a thin
`DatagramTransport`/stream shim per medium, most of which belong in a
platform-specific crate (ESP32, Android, browser) rather than the portable core.
