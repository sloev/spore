# SPORE v1 — Store-and-forward Planetary Opportunistic Relay Envelope
Front: the protocol (one page). Back: how it rides everything, and why not something else.

<p align="center"><a href="spore-v1.png"><img src="spore-v1.png" alt="SPORE v1 one-page visual reference" width="820" /></a></p>

*The whole thing at a glance ([full size](spore-v1.png)); the text below is the normative version.*

## 0. The whole protocol in one breath
A SPORE message is a **signed postcard**: to, from, expiry, payload, signature. Its SHA-256 fingerprint is its identity. Every node keeps postcards it hasn't seen, hands copies to anyone it meets who wants them, and drops duplicates and expired mail. That alone is a working planetary network. Everything below only makes it faster, safer, or quieter — and of the four hard features (forward secrecy, fountain fragmentation, congestion control, anonymity), only congestion control touches the router; the rest live inside payloads.

**Tiers** (all interoperate): **T0 carry** ≈60 lines: parse, dedup, store, deliver, damped flood · **T1 sync** +≈80: ANNOUNCE/INV/WANT, watermarks · **T2 route** +≈100: paths, directed unicast, custody. Endpoint extras (ratchet, fountain, mix) never change relays.

**Threat model, stated once:** every link is hostile — logged, spoofed, jammed, MITM'd. Links are trusted with *nothing*; authenticity and secrecy live only in the envelope. Attackers can drop or delay; redundancy and flood-fallback heal both.

## 1. Identity & addressing
Identity = one Ed25519 keypair. **Address** = first 8 B of SHA-256(pubkey). **Topic** = first 8 B of SHA-256(UTF-8 string). No global namespace; exchange keys by QR/paper/voice. Petnames are local.

## 2. Envelope (the only object; big-endian; fixed part = 16 B)
```
off len field
0   1   ver    = 0x01  (layout frozen; forward envelopes with unknown flag bits set)
1   1   type   0=DATA 1=INV 2=WANT 3=ANNOUNCE
2   1   flags  b0 ENCRYPTED b1 SIGNED b2 FRAGMENT b3 ACKREQ b4 FLOOD b5 SRC8
3   1   hops   remaining relays (default 16; relays clamp incoming to ≤ 16)
4   4   expiry unix seconds u32 (stores clamp horizon to 30 d)
8   8   dest   address | topic | 0x00×8 = public
-- if SIGNED: src = 32-B pubkey, or 8-B address if SRC8 --
    2   plen   u16
    N   payload
    64  sig    Ed25519 over all bytes above with hops zeroed
-- unsigned: no src, no sig; rides last everywhere --
```
**ID** = first 16 B of SHA-256(envelope, hops zeroed); computed, never transmitted (except inside INV/WANT/frag/ack). Overhead: 114 B signed, 90 B SRC8, 18 B unsigned. **SRC8** only toward peers that provably hold your key; relays never verify — endpoints do. No priority field: priority is bought, not claimed (§10 stamp).

## 3. Fragmentation — fountain coded
payload = `[orig_id:16][index:1][count:1][chunk]`; all chunks equal size (pad the original; the envelope self-delimits). Fragments are ordinary envelopes (own IDs, same dest/expiry).
- **index < count**: plain chunk *index* of the original envelope's bytes.
- **index ≥ count**: **repair chunk** = XOR of the data chunks selected by the first *count* bits of SHA-256(orig_id ‖ index); empty selection → chunk (index mod count). The sender can mint endless distinct repair chunks.
Receiver decodes when any received set reaches rank *count* (Gaussian elimination over GF(2) — a few lines of code); typically *count*+2 arrivals suffice at any loss rate, in any order, even one-way. Verify the reassembled signature; commit only what verifies. This is a rateless fountain: perfect for simplex radio, CW, paper tape.

## 4. Routing state (T2)
- **Neighbors** (per interface): point-to-point peers + anyone heard via hops=0 ANNOUNCE.
- **Paths**: `addr → up to 3 of (iface, neighbor, age)`. Learned: (a) the **first copy** of any new signed envelope raced every path and won — its src is reachable via what delivered it; (b) flooded ANNOUNCEs. On broadcast media the interface *is* the direction. Fresh < 3 h; purge 7 d (stale entries still guide custody).

**ANNOUNCE** (type 3, signed): payload = `[prekey:32][nt:1][topic×8 ea][np:1][(addr:8, age_min:2) ea][petname…]` — your current encryption prekey (§7), topics you collect, freshest paths. Link HELLO = hops 0; flooded = hops 16.

## 5. Forwarding rules (the entire router)
1. Envelope arrives: ID seen or expired → drop. Add ID (keep ≥ until expiry). Learn paths (§4).
2. dest ∈ {my addresses, followed topics, 0×8} → deliver (verify/decrypt per flags).
3. Store until expiry. Evict: expired → lowest stamp → largest → oldest. TX order: local origin, then stamp, then FIFO.
4. **Congestion control**, four rules: **(a)** token bucket — relayed traffic ≤ 10% of each interface's capacity (law on ISM bands, courtesy elsewhere; dedup makes dropped relays harmless); **(b)** Trickle timers — HELLO/ANNOUNCE interval doubles 5→80 min while nothing new is heard, resets to 5 on any novelty; **(c)** backpressure — HELLO carries one `busy` byte (queue fill); neighbors scale sending by (255−busy)/255 and defer unstamped relays to busy peers; **(d)** exponential backoff — FLOOD retries at 30 s ×2, cap 1 h, max 5.
5. hops = 0 → stop. Else decrement, then: **topic/0/FLOOD** → damped flood on all interfaces (on shared media wait random 1–5× airtime, cancel if the ID is overheard ≥ 2×; ≥ 1× for directed) · **unicast + fresh path** → that interface/neighbor only · **unicast, no path** → silent, unless you hold custody and the path died: set FLOOD, continue.
6. Originator: no echo/ACK → resend with FLOOD per 4d. Flooding **is** route discovery; replies teach reverse paths and heal blackholes.
7. Untrusted clock? Relay regardless of expiry; age by dwell, drop after 7 local days.

```
def on_rx(e, iface, nbr):
    if id(e) in seen or expired(e): return
    seen.add(id(e)); store.put(e)
    if e.SIGNED: paths.learn(addr(e.src), iface, nbr)   # first copy wins, keep 3
    if e.dest in my_addrs | topics | {ZERO}: deliver(e)
    if not e.hops: return
    e.hops -= 1
    if unicast(e.dest) and not e.FLOOD:
        p = paths.fresh(e.dest)
        if p: tx(p.iface, p.nbr, e)
        elif held_custody(e): e.FLOOD = 1; damped_tx_all(e)
    else: damped_tx_all(e)
```

## 6. Sync & custody (T1/T2)
On any meeting: ANNOUNCE, then **INV** (concatenated IDs, newest first, filtered by peer's topics + carriable unicast + per-neighbor watermark), peer replies **WANT**, send those. INV/WANT: hops=0, unsigned, consumed, never stored/relayed. (Huge stores MAY do negentropy-style range reconciliation.) **Custody:** push stored unicast to any peer that *is* the destination or announces a fresher path. A file or sheet of paper is concatenated envelopes; import = receive. Every boat, cyclist, or HF skywave contact merges two regions. That is the WAN.

## 7. Crypto & forward secrecy
- **Sign:** Ed25519 (§2).
- **Seal (baseline, one shot):** libsodium `crypto_box_seal` to the recipient's newest **prekey** (X25519, from ANNOUNCE, signed there). Rotate prekeys daily; **delete private prekeys after 7 d** → seized devices cannot read mail older than a week. Zero per-message state.
- **Sessions (state of the art): Double Ratchet.** Init: root = KDF(X25519 shared from the first seal). Message payload = `[dh_pub:32][n:2][pn:2][ct]`; ct = ChaCha20-Poly1305(mk, nonce=n, ad=header). On a new `dh_pub`: root,ck_in = KDF(root, DH); make a fresh ratchet key: root,ck_out = KDF(root, DH). Each mk = KDF(ck), then delete it; cache skipped mks ≤ 7 d for out-of-order arrival (normal in SPORE). KDF = libsodium crypto_kdf (BLAKE2b). Compromise leaks nothing older than one ratchet turn.
- **Encrypted topics:** pre-shared key, XChaCha20-Poly1305, 24-B nonce prefixed. **Key rotation:** flood `KEYROT <newpub>` signed by the old key. **Ham bands: ENCRYPTED=0** — signing identifies, ciphering is illegal there.

## 8. Receipts (ACKREQ)
Recipient floods a signed DATA to src, payload = `0x06` + orig ID. ACKs also teach reverse paths.

## 9. Anonymity — mix mode (Mixmaster/Loopix distilled)
An **onion** is just nested sealed envelopes. Sender picks 2–3 relays that follow topic `mix` (learned from ANNOUNCEs) and wraps inside-out: each layer = an envelope addressed to one mix, payload sealed to it = `'O'` + the next full envelope. A mix decrypts, waits a random Poisson 1–30 s, batches ≥ 3, re-injects the inner envelope as its own traffic.
- **Sender anonymity:** leave outer layers unsigned; sign only the innermost if at all.
- **Recipient anonymity:** make the innermost dest 0×8, payload sealed — everyone carries, one can open.
- **Unlinkability:** pad every onion payload to size classes 256 / 1024 / 4096 B so depth never shows; mixes SHOULD emit Poisson decoy onions at roughly their real rate.
Honest limit: beats local observers and any subset of mixes; a **global** passive observer is only beaten while decoy traffic flows.

## 10. Self-defense (local policy)
Quotas per src and per topic; prefer keys you've met or your operator vouches for. Unsigned mail rides last. **Stamp — the only cross-node priority:** n leading zero bits of ID = class n, mined via a payload nonce; priority is proof of work, unforgeable on every medium. Convention: topic `sos` outranks policy. Out-of-band handshakes: confirm peers by address emoji-hash.

## 11. Defaults
hops 16 · expiry 7 d · HELLO 5→80 min Trickle · ANNOUNCE flood ≤ 1/h · prekey rotate 1 d, delete 7 d · path fresh 3 h · seen-set ≥ 30 d · relay airtime ≤ 10% · payload UTF-8. T0 ≈ 60 lines; full T2 ≈ 400 with libsodium; one JS core runs everywhere below.

---

# Page 2 — Bindings: SPORE on everything
**Every medium on Earth has one of five shapes.** Bind by shape; the router never changes.
1. **Message pipe** → one envelope/fragment per message.
2. **Byte stream** → KISS: frames delimited 0xC0, escape 0xC0→0xDB 0xDC, 0xDB→0xDB 0xDD, command byte 0x00.
3. **Text channel** → armor: `~S1.` + Base32(envelope) + `.` + Base32(SHA-256[0:4]) + `~`, whitespace ignored.
4. **Shared bus** → KISS + CSMA: listen-before-talk, backoff 1–5× airtime; no native CRC → append SHA-256(envelope)[0:4], verify or drop.
5. **Shared store** → write envelopes as entries named by hex ID; reading = receiving; the store is a persistent INV.

**Underlays with their own routing = ONE interface** (decrement hops once; point-to-point may restore hops):
| Underlay | Tip |
|---|---|
| Plain internet | TCP/HTTPS bridge to static peers; any surviving VPS = supernode |
| **B.A.T.M.A.N.-adv / most Freifunk** | bat0 is one giant Ethernet: UDP broadcast :7373 and EtherType 0x7373 reach the whole mesh |
| **OLSR / older Freifunk** | routed L3: multicast 239.73.73.73 via olsrd BMF plugin, else static peers per segment |
| Yggdrasil, cjdns | IPv6 overlay: ff02::7373 per link + static global peers |
| WireGuard, Tailscale, Nebula, VPNs | just IP: UDP 7373 |
| Tor / I2P | hidden service :7373, KISS over the stream |
| Reticulum, Meshtastic, IP itself | rows below — their internal hops are invisible |

**1 — Message pipes**
| Medium | Parameters |
|---|---|
| UDP v4/v6 | port **7373**; discovery: bcast 255.255.255.255, mcast 239.73.73.73 + ff02::7373, mDNS `_spore._udp`; ≤ 1400 B |
| Raw Ethernet | EtherType 0x7373, dst ff:ff:ff:ff:ff:ff |
| WebRTC DataChannel | label `spore`, unordered; serverless signaling below |
| WebSocket | `ws://node:7373/ws`, binary; serve app same-origin (below) |
| Multipeer/AWDL (iOS/macOS) | serviceType `spore`; rides peer-WiFi+BT |
| ESP-NOW | 250 B, broadcast MAC, ch 6 |
| 802.15.4 raw | ≤ 100 B payload → FRAGMENT |
| LoRa raw | ≤ 255 B (use ≤ 200); EU 869.525 / US 915.0 MHz, SF9 BW125 CR4/5, sync 0x12, CRC on; duty/LBT per law |
| Meshtastic | portnum **256** (PRIVATE_APP), dst 0xFFFFFFFF, ≤ ≈230 B, want_ack off |
| Reticulum | PLAIN dest, app `spore`, aspect `v1`, ≤ ≈440 B; big via Link/Resource |
| AX.25 UI (ham packet) | dst `SPORE `, PID 0xF0, PACLEN 256, callsign = legal ID, ENCRYPTED=0 |
| MQTT | topic `spore/1`, QoS 0, binary payload |
| libp2p gossipsub (IPFS) | topic `spore/1`, raw envelope per message |
| Iridium SBD | 340 B up / 270 down; gateway bridges continents |
| NFC | NDEF media record `application/x-spore` |

**2 — Byte streams (KISS)**
| Medium | Parameters |
|---|---|
| TCP / ssh / any tunnel | port 7373 |
| Serial: **RNode**, any KISS TNC | zero glue — they already speak KISS; RNode MTU 500 |
| Web Serial / WebUSB | drive an RNode/TNC from a browser tab (desktop Chrome/Edge; Android via WebUSB CDC) |
| BT Classic RFCOMM | SPP UUID 0x1101, name `SPORE-<addr16hex>` |
| BLE GATT | svc `53504f52-4531-0000-0000-53504f524531`, RX `…4532` write-no-resp, TX `…4533` notify, chunk = MTU−3; Web Bluetooth = client of this (not iOS Safari) |
| HF modems (VARA, ARDOP) | env ≤ 200 B; minutes per envelope — stamp generously |

**3 — Text channels (armor)**
| Medium | Parameters |
|---|---|
| SMS | GSM-7-safe; ≈150 armor chars/segment, ≤ 3 segments, else FRAGMENT; p2p → may restore hops; auto-parse Android, share-sheet iOS |
| **Email / SMTP-IMAP / DeltaChat** | armor in body or attach `<id>.spore` (`application/x-spore`); poll IMAP = receive; DeltaChat adds Autocrypt e2e, and a **webxdc app can embed a full node in the chat** |
| **Usenet / NNTP** | one envelope per article in `alt.spore`; **Message-ID `<hexid@spore>` → servers dedup for you**; body = armor; Usenet's flooding becomes SPORE's |
| **Signal / Telegram / WhatsApp** | paste armor or attach `<id>.spore`; automate via signal-cli; their e2e wraps ours — layers independent |
| IRC / XMPP / Matrix | armor, ≤ 400 chars/line; Matrix alt: `m.file` |
| JS8Call, PSK31, RTTY, CW | ≤ 64-char lines, small fountain fragments — one-way decode is native now |
| Paper / QR | one env per QR (v40 ≈ 2.9 KB); print your pubkey QR too |
| A human voice | read the armor aloud; it was designed to survive that |

**4 — Shared buses (KISS+CSMA)**
| Medium | Parameters |
|---|---|
| **Walkie-talkies (PMR446/FRS/GMRS), CB, marine & ham FM (2 m/70 cm)** | AFSK 1200 Bd Bell 202 → KISS; couple acoustically (phone mic-to-speaker works) or 2-pin PTT cable; VOX keys TX; bursts ≤ 20 s; identify + ENCRYPTED=0 per band rules; software: Dire Wolf, `minimodem 1200` |
| Any audio path (cassette, phone line, intercom, PA) | same AFSK; CRC tail per rule 4 |
| Ultrasonic (ggwave) | ≤ 140 B/burst, ≈8–16 B/s, 15.5–19.5 kHz; browser WASM + native; ggwave's Reed-Solomon replaces the CRC tail; doubles as WebRTC signaling |
| RS-485 / field wire / powerline | 9600 8N1 default; CRC tail mandatory |

**5 — Shared stores**
| Medium | Parameters |
|---|---|
| Folder: USB, **Syncthing**, NFS, Dropbox | dir `spore/`, one file `<32hex>.spore` per envelope (or `*.spore` bags); filename = ID → the folder *is* the INV; prune expired; Syncthing merge = network |
| **Hypercore / Dat** | Hyperswarm topic = SHA-256(`spore:v1`); each node appends envelopes to its own core; replicate every peer core you meet; log = INV, block = envelope |
| **GunDB** | `gun.get('spore').get(<hexid>).put(armor)`; subscribe `.map().on()`; CRDT merge = sync |
| Scuttlebutt | message type `spore`, content = armor |
| Any BBS / forum / pastebin | armor posts; a moderator with a modem is a gateway |

## Browsers & phones: zero-rendezvous LAN peering
1. **Template SDP**: a WebRTC session reduces to ufrag(4)+pwd(22)+DTLS fingerprint(32)+mDNS host candidate(16) ≈ **90 B**; both sides rebuild full SDP from a hardcoded template.
2. **Ultrasonic handshake**: apps loop a ggwave HELLO = `addr(8)+descriptor(90)`; hear one → answer → `setRemoteDescription` → the browser's own mDNS resolves `<uuid>.local` → direct DataChannel. No server, no typed IP, ≈15 s.
3. **Static nodes**: native nodes run **ice-lite with static ufrag/pwd/fingerprint** — a constant descriptor: print as QR on the box, beep it, bake it into the app.
4. **QR camera** = same descriptors, instant fallback. Confirm peers by address emoji-hash (§10).
5. **App distribution**: every native node serves the PWA at `http://localhost:7373/` (secure context: installable, offline SW) and to the LAN at its IP (same-origin ws://). HTTP bag API: `POST /spore/push`, `GET /spore/inv?since=`, `POST /spore/want`, MIME `application/x-spore`, CORS `*` + `Access-Control-Allow-Private-Network: true`. The app store is every node.

**Platform matrix.** One JS core + libsodium-WASM. Browser/PWA: WebRTC, WS, HTTP bags, Web Bluetooth, Web Serial/USB, ultrasonic, QR. Android native: + UDP/TCP, BLE peripheral, WiFi Direct (GO 192.168.49.1 → UDP), SMS. iOS: + Multipeer, BLE (sync on open). Desktop daemon: everything + sound cards + TNCs.

## Positioning — why not just use…
**Reticulum**: superb routed crypto network; needs real software and bidirectional links on both ends. SPORE's floor is 60 lines, a sound card, or a human with paper — and it happily rides Reticulum. **DTN Bundle Protocol (RFC 9171)**: same ideas, IETF-complete and heavy; SPORE is BPv7's core on one page. **Meshtastic**: turnkey but one radio and its own app layer; SPORE is medium-plural and rides it. **SSB / Hypercore**: log replication couples you to whole feeds; SPORE dedups per message, so a 200-B link or a QR code is a valid peer. **Nostr**: the same signed-gossip idea minus JSON, relays and always-on internet. **Signal**: SPORE sessions use Signal's own Double Ratchet — over radios, paper and sneakernet. **Briar**: only friends relay; SPORE lets strangers carry mail, with mix mode when metadata matters. **Mixmaster / Loopix**: §9 is their core, built from one primitive. **BATMAN / OLSR / Babel / IP**: strictly better on stable dense powered networks — which is why they're underlays here. **FidoNet / UUCP / Usenet**: the ancestors; SPORE is netmail+echomail with modern crypto and self-forming routes.

**Known limits, on purpose:** no stream semantics; fountain fragments to ≈50 KB, bigger files ride stores/sneakernet; no permanence (expiry is a feature); mix-mode anonymity needs flowing decoys to beat a *global* observer; ratchet state is per-device — give each device its own key. Each limit buys the one-page spec. When you have fiber and stability, run IP and tunnel SPORE through it; SPORE's job begins where that ends.
