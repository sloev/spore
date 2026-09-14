# Manual hardware verification checklist

CI proves everything that runs without a device: the Rust core, the wire codecs
(KISS, Meshtastic protobuf, the audio modem DSP), the wasm node, the JS loopback
and WebSocket paths, and that the Android APK builds. What CI *cannot* prove is
the last hop through real hardware — a mic, a radio, a Bluetooth stack, a live
peer. This checklist is the repeatable procedure for those 🧪 paths. Each row is
independent; run the ones you have hardware for and note the date + result.

**A real NAT is hardware too.** Row 19 is here because every hole-punch test in
the tree runs on loopback, where there is no NAT to traverse and a punch that
never happened is indistinguishable from one that worked. Only two boxes behind
two different consumer routers can tell you whether the ladder actually reaches.

**The desktop node keeps its identity now.** It was not able to before: the
daemon called `Node::new` every start, so a restart produced a new address and an
empty store, and any row asking for a stable desktop address was unrunnable.
State lives in `$SPORE_HOME` (default `~/.local/share/spore`); delete that
directory to test as a fresh node, and set it per-shell to run two nodes on one
laptop.

**Demo A needs no hardware at all** — two processes on one laptop, which is why
it is the first thing to run and the thing to reproduce before blaming a radio.

```sh
printf 'bridges:\n  - tcp\n'                  > /tmp/a.yaml   # listens on :7373
printf 'bridges:\n  - tcp: 127.0.0.1:7373\n'  > /tmp/b.yaml   # connects to it

SPORE_HOME=/tmp/na spore /tmp/a.yaml     # terminal 1
SPORE_HOME=/tmp/nb spore /tmp/b.yaml     # terminal 2
```

Type into either. `/help` lists the commands; `/status` prints the address and
how much is held. A message typed in one appears in the other as
`[recv] <addr> (public) …`.

For the store-and-forward half: stop node A, type a message into B, start A
again, and type `/offer` into B. A receives a message it was never sent
directly. `/offer` is the diagnostic form of the `INV_OFFER_SECS` cadence, which
is five minutes — right in the field, far too slow to watch.

Two `SPORE_HOME`s because the identity lives there. Sharing one would make both
processes the same node, which fails in a way that looks like the network is
broken.

| # | Path | Setup | Pass looks like |
|---|---|---|---|
| 1 | **UDP LAN** (phone ⇄ desktop) | APK on a phone + `spore broadcast` on a laptop, same Wi-Fi | messages appear both ways in seconds; **both** addresses stable across restarts — stop and restart the laptop node too, and confirm it announces the same 16 hex digits |
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
| 12 | **Ham AX.25 / KISS** | A licensed operator + a TNC. Direwolf: set `KISSPORT 8001`, then `spore ax25:localhost:8001`; or a hardware TNC on serial (`stty` first). Second SPORE node on the frequency | envelopes cross the RF link; **`ENCRYPTED` must stay 0 on ham bands** — signing identifies, ciphering is illegal |
| 13 | **Tor onion** | `torrc` with `HiddenServiceDir` + `HiddenServicePort 7373 127.0.0.1:7373`, and `spore tcp` listening beside it; from another host `spore tor:<hostname>.onion` | circuit establishes (10–30 s is normal); envelopes flow with neither side exposing an IP |
| 14 | **ICMP / ping** (Linux) | Two Linux hosts; `sudo setcap cap_net_raw+ep ./spore` on both, then `spore icmp:<other-ip>` each way | a public message crosses carried in echo packets; `tcpdump icmp` shows echoes with the `SP` marker; ordinary `ping` between them still works |
| 15 | **Reticulum over TCP/UDP** | `python3 tools/reticulum_companion.py --listen tcp:4242` on the RNS host; `spore reticulum-tcp:<host>:4242` elsewhere | envelopes cross the RNS network with the companion on another machine — no local `mkfifo` |
| 16 | **I2P** | `i2pd` or Java I2P running with SAM enabled (`:7656`); on one host note its b32, on the other `spore i2p:<b32>.b32.i2p` | SAM session opens, tunnels build (30–60 s is normal), envelopes flow with neither end learning the other's IP |
| 17 | **Copyparty / WebDAV** | A copyparty share (`copyparty -v .::rw`); `spore copyparty:http://host:3923/bag/` on two machines | `<hexid>.spore` files appear in the share; each node imports the other's within one poll |
| 18 | **BLE generic (NUS)** | Chrome desktop web node → "Web Bluetooth", an nRF/ESP32 running Nordic UART | KISS frames cross; MTU chunking reassembles |
| 19 | **Direct across two real NATs** | Two daemons on different home connections (not the same LAN, not a VPN). Both `direct: 0.0.0.0:PORT` + `direct-stun: <server>`; one also `direct-to: <other's address>`, with a mesh path between them (a shared `tcp:` peer is enough) | each side prints a reflexive locator, then `pipe … up with … (punched)` — **`punched`, not `no punch, plain connect`**. Fallback on a reflexive candidate means the punch did not land: note the NAT type on both ends, since two symmetric NATs are the case no punch solves and are what `direct-iroh:` exists for |
| 21 | **ESP32 bring-up** (M8/E1) | `./esp32/build.sh s2mini --flash`, then `./esp32/diagnose.py`. No second device needed | all seven checks pass: identity derived, `sig=ok` (a signature the board made verifies **on** the board), `probe=ok`, uptime advancing, heap flat, no panics |
| 20 | **NFC tap** | Two Android phones on Chrome over **HTTPS** with the web node open, or one phone plus an NTAG213/216 tag; tap → "Web NFC" | the `application/x-spore` record is written and read back; an envelope crosses in one tap. A tag holding anything else (a URL, another app's record) is ignored rather than mis-parsed. Objects past the tag's capacity should take several taps and still reassemble |

**Recording results.** Append a dated line to the table's history below when a
row is verified (device, OS/firmware, result). A row with no history is still a
🧪 template — treat its code as faithful but unproven on your hardware.

## History

**Row 21 — ESP32 bring-up — 2026-08-25 — PASS.** LOLIN S2 Mini
(ESP32-S2FNR2 rev v1.0, single core 240 MHz, 4 MB flash, 2 MB PSRAM, USB-OTG,
MAC `80:65:99:49:8f:6e`), flashed from Linux with esptool 5.3.1.

```
up 11s · addr=52e36ddf70b65d9a · sig=ok · probe=ok · heap=226368 · due=0
up 26s · addr=52e36ddf70b65d9a · sig=ok · probe=ok · heap=226368 · due=0
up 41s · addr=52e36ddf70b65d9a · sig=ok · probe=ok · heap=226368 · due=0
```

The claim that matters is `sig=ok`: a signature this board produced verified on
this board, so ed25519 runs on Xtensa rather than merely compiling for it.
`probe=ok` confirms `Envelope::probe` agrees with `decode` on real silicon.
Free heap held at 226,368 bytes across all three readings — no drift, which is
the only way a leak would have shown, and not something static section sizes
can tell you.

**The identity did not survive a reboot when this run was made.** An earlier run
on the same board reported `addr=8a82bcbd735aed52`: nothing was wrong, the
runtime simply supplied no storage nutrient, so the seed was generated fresh
every boot.

The firmware now keeps the seed and the prekey ring in NVS, so this is a thing to
**re-run rather than a known limitation**. The row passes when two boots in a row
report the same `addr=` and the second says `restored from NVS`. Note both
addresses and the date.

NVS rather than the `spore` SPIFFS partition on purpose: that partition mounts
with `format_if_mount_failed`, which is what makes a never-flashed board work
out of the box and what would erase the identity along with a damaged store.
Envelopes are a cache and can be asked for again; a seed cannot.

