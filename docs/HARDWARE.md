# Manual hardware verification checklist

CI proves everything that runs without a device: the Rust core, the wire codecs
(KISS, Meshtastic protobuf, the audio modem DSP), the wasm node, the JS loopback
and WebSocket paths, and that the Android APK builds. What CI *cannot* prove is
the last hop through real hardware — a mic, a radio, a Bluetooth stack, a live
peer. This checklist is the repeatable procedure for those 🧪 paths. Each row is
independent; run the ones you have hardware for and note the date + result.

| # | Path | Setup | Pass looks like |
|---|---|---|---|
| 1 | **UDP LAN** (phone ⇄ desktop) | APK on a phone + `spore broadcast` on a laptop, same Wi-Fi | messages appear both ways in seconds; addresses stable across app restarts |
| 2 | **Audio modem** (phone ⇄ desktop) | Enable audio modem in the app; on the laptop `sox -d -t f32 -r 48000 -c 1 - \| spore audio \| sox -t f32 -r 48000 -c 1 - -d`; devices ~30 cm apart, moderate volume | a short public message crosses by sound alone (expect ~1 s/25 bytes); sig OK on arrival |
| 3 | **Audio modem** (tab ⇄ phone) | Web node in a desktop browser with the audio bridge + the app's audio modem | same as #2, browser ⇄ phone |
| 4 | **Web Serial → board** | Chrome/Edge **desktop**, web node → "Web Serial — generic KISS TNC"; a board running the KISS echo/firmware | frames echo; ↑/↓ counters advance; unplug flips the row to closed |
| 5 | **Meshtastic BLE** (app) | A paired Meshtastic node (unencrypted channel), app → Bridges → "Add Meshtastic radio" | bridge row goes `open`; a public message from the phone appears on another Meshtastic-bridged SPORE node; **confirm the firmware's field numbers match `mesh.proto`** |
| 5b | **Meshtastic USB serial** (desktop) | A node on USB. `stty -F /dev/ttyUSB0 115200 raw -echo`, then `spore meshtastic-serial:/dev/ttyUSB0` (or pipe it: `socat /dev/ttyUSB0,b115200,raw - \| spore meshtastic-serial`); a second Meshtastic-bridged SPORE node in range | the radio's own log lines appear and are skipped; a public message crosses the LoRa mesh; **confirm the firmware's stream-API framing (`0x94 0xc3`) and `mesh.proto` field numbers** |
| 6 | **Meshtastic Web Serial/BLE** (browser) | Chrome desktop, web node → Meshtastic bridge, node on USB/BLE | as #5 from a tab |
| 7 | **RNode BLE / Serial** | An RNode (or T-Beam w/ RNode firmware); set region-legal freq/bw/SF/CR/power; second RNode-bridged node in range | envelopes cross the LoRa air; radio config visibly applied (RNode LED/console) |
| 8 | **Wi-Fi Direct** | Two phones, both enable the Wi-Fi Direct bridge; accept the P2P prompt | group forms; messages flow with **no AP present** |
| 9 | **WebTorrent swarm** | Two devices (any mix of app/web node) join the same swarm name, default trackers reachable | peer count ≥ 1 on both; messages relay P2P; killing the tracker afterwards does not drop the link |
| 10 | **Nostr relay** | Web node or app pointed at a public relay (kind-30078 accepted) | envelopes published from a signer-equipped web node arrive on an rx-only listener |
| 11 | **RNS payload** | `mkfifo up down; python3 tools/reticulum_companion.py <up >down & spore reticulum <down >up` on two RNS-connected hosts (`pip install rns`) | envelopes cross the Reticulum network via the shared `spore.mesh` PLAIN destination |
| 13 | **Ham AX.25 / KISS** | A licensed operator + a TNC. Direwolf: set `KISSPORT 8001`, then `spore ax25:localhost:8001`; or a hardware TNC on serial (`stty` first). Second SPORE node on the frequency | envelopes cross the RF link; **`ENCRYPTED` must stay 0 on ham bands** — signing identifies, ciphering is illegal |
| 14 | **Tor onion** | `torrc` with `HiddenServiceDir` + `HiddenServicePort 7373 127.0.0.1:7373`, and `spore tcp` listening beside it; from another host `spore tor:<hostname>.onion` | circuit establishes (10–30 s is normal); envelopes flow with neither side exposing an IP |
| 12 | **BLE generic (NUS)** | Chrome desktop web node → "Web Bluetooth", an nRF/ESP32 running Nordic UART | KISS frames cross; MTU chunking reassembles |

**Recording results.** Append a dated line to the table's history below when a
row is verified (device, OS/firmware, result). A row with no history is still a
🧪 template — treat its code as faithful but unproven on your hardware.

## History

*(none recorded yet)*
