# SPORE v1 — Store-and-forward Planetary Opportunistic Relay Envelope
Front: the protocol. Back: how it rides everything. Two sides of one sheet — the
normative whole. Per-medium parameters live in [BRIDGES.md](BRIDGES.md).

<p align="center"><a href="spore-v1.png"><img src="spore-v1.png" alt="SPORE v1 one-page visual reference" width="820" /></a></p>

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

Receiver decodes when any received set reaches rank *count* (Gaussian elimination over GF(2)); typically *count*+2 arrivals suffice at any loss rate, in any order, even one-way. Verify the reassembled signature; commit only what verifies. Rateless: perfect for simplex radio, CW, paper tape.

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
On any meeting: ANNOUNCE, then **INV** (concatenated IDs, newest first, filtered by peer's topics + carriable unicast + per-neighbor watermark), peer replies **WANT**, send those. INV/WANT: hops=0, unsigned, consumed, never stored/relayed. **Custody:** push stored unicast to any peer that *is* the destination or announces a fresher path. A file or sheet of paper is concatenated envelopes; import = receive. Every boat, cyclist, or HF skywave contact merges two regions. That is the WAN.

**Files** are content-addressed: chunks `[0x07][file_id:16][index:4][bytes]`, indexed by a signed **manifest** `[0x01]…[chunk_id:16 × count]` whose own ID is the shareable **magnet**. A manifest that outgrows one envelope nests — interior nodes `[0x08][depth:1]…` name manifests a level down, so the root stays one frame and one signature at any file size, and only the root is signed (an ID is the hash of its bytes, so the tree authenticates itself). Sealed to one recipient: `[0x09][depth:1][hdr_len:2][hdr]…`, `hdr` = the file key + real name sealed to their prekey, each chunk then encrypted under that key with the chunk index as nonce.

## 7. Crypto & forward secrecy
- **Sign:** Ed25519 (§2).
- **Seal (baseline, one shot):** libsodium `crypto_box_seal` to the recipient's newest **prekey** (X25519, from ANNOUNCE, signed there). Zero per-message state. Implementations **MUST** hold a ring of prekeys, mint a fresh one daily, try every live one when opening, and **delete secrets after 7 d** — so a seized device cannot read mail older than a week. See §7.2 for the ring's shape and its one hard requirement.
- **Sessions: Double Ratchet.** Init: the moment both sides know each other's current prekey (i.e. right after ANNOUNCE), each derives the same root independently via a static-static X25519 DH (own current prekey secret × peer's advertised prekey public) — no message exchange needed. Whichever address sorts numerically lower is always that pair's initiator ("Alice": has a sending chain immediately); the other is the responder ("Bob": no sending chain until it actually receives Alice's first ratchet message — so Bob's own first send, before that, still goes out as a plain one-shot seal). A session is bootstrapped once per peer and never re-seeded from a later ANNOUNCE (prekeys rotate daily; the ratchet's own evolution is what's trusted from then on) — implementations **MUST** settle any due prekey rotation before both building an ANNOUNCE and bootstrapping a session from one, or the two sides can derive different roots. Sessions are local, in-memory state — not required to be persisted; a restart may lose one and re-bootstrap from the next ANNOUNCE exchange. Wire discriminator: an ENCRYPTED `DATA` envelope's payload is ratchet-shaped rather than the one-shot seal shape iff a reserved envelope flag bit is set (implementations SHOULD use bit `0x40`, unused by v1). Payload = `[dh_pub:32][n:2][pn:2][ct]`; ct = ChaCha20-Poly1305(mk, nonce=n, ad=header). On a new `dh_pub`: root,ck_in = KDF(root, DH); fresh ratchet key: root,ck_out = KDF(root, DH). Each mk = KDF(ck), then drop it; skipped mks are cached for out-of-order arrival, bounded both by count (`MAX_SKIPPED_KEYS`) and by age (purged once older than the session's skip TTL), and zeroised on drop. A session's skip TTL is set at bootstrap from the same configurable value that governs prekey lifetime (the "offline window", PR0 Part B; §7.2, default 7 d) — one knob, not two that could drift apart. KDF = BLAKE2b. Compromise leaks nothing older than one ratchet turn.
- **Encrypted topics:** pre-shared key, XChaCha20-Poly1305, 24-B nonce prefixed. **Rotation:** flood `KEYROT <newpub>` signed by the old key. **Ham bands: ENCRYPTED=0** — signing identifies, ciphering is illegal there.
- **Topic key schedule (§7.1):** `rotate(k) = SHA-256(k ‖ "spore-keyrot-v1")` advances an epoch — forward secrecy, but a stolen key follows the chain, so it never heals. **Healing rotation:** draw 32 random bytes `c`, seal a copy to each member's prekey, and everyone folds it in: `mix(k,c) = SHA-256("spore-topic-mix-v1" ‖ k ‖ c)`. Message = `[0x01][count:2 BE][80-B sealed box]×count`, no recipient hints (so it does not enumerate the group); a member trial-decrypts, ≤ 256 boxes. Mixing never replaces, so an injected contribution cannot cancel an honest one. `key_id(k) = SHA-256("spore-topic-keyid-v1" ‖ k)[..4]` names a key so a receiver holding several candidates picks the right one — the group has no roster and no arbiter, so members can diverge; this makes divergence readable rather than fatal.

- **§7.2 Prekey ring.** A node holds up to 16 prekeys, oldest first, and advertises the newest in ANNOUNCE. It mints one every 24 h and **deletes** any secret older than the configured lifetime (the "offline window"; default 7 d, PR0 Part B); the newest is never deleted, so a node is always sealable. Opening tries every live entry newest-first — a sender who last heard an older ANNOUNCE still reaches you until that secret expires. The nonce mixes the *recipient's* public key, so an entry stores both halves. This same lifetime value is handed to every ratchet session bootstrapped from this node's ANNOUNCEs, as its skip-key TTL (§7 above) — one implementation-level knob drives both, since letting them disagree would make one of the two windows a lie.
  **The requirement that makes it mean anything: prekey secrets MUST be random and MUST NOT be derivable from the identity seed.** Restoring from a seed recovers the address, the signing key and the ability to mint new prekeys — it must *not* recover a deleted secret, or "delete" is a word with no referent. Implementations therefore persist the ring separately from the seed and must state that a *backup* of the ring defeats the 7-day window, as it would for any forward-secret keystore. This was the substance of S-022: the property was specified, and the reference implementation derived one prekey from the seed and kept it forever.

## 8. Receipts (ACKREQ)
Recipient floods a signed DATA to src, payload = `0x06` + orig ID. ACKs also teach reverse paths.

## 9. Anonymity — mix mode (Mixmaster/Loopix distilled)
An **onion** is nested sealed envelopes. Sender picks 2–3 relays that follow topic `mix` (learned from ANNOUNCEs) and wraps inside-out: each layer = an envelope addressed to one mix, payload sealed to it = `'O'` + the next full envelope. A mix decrypts, waits a random Poisson 1–30 s, batches ≥ 3, re-injects the inner envelope as its own traffic.
- **Sender anonymity:** leave outer layers unsigned; sign only the innermost if at all.
- **Recipient anonymity:** innermost dest 0×8, payload sealed — everyone carries, one can open.
- **Unlinkability:** pad every onion payload to size classes 256 / 1024 / 4096 B so depth never shows; mixes SHOULD emit Poisson decoy onions at roughly their real rate.

Honest limit: beats local observers and any subset of mixes; a **global** passive observer is only beaten while decoy traffic flows.

## 10. Self-defense (local policy)
Quotas per src and per topic; prefer keys you've met or your operator vouches for. Unsigned mail rides last. **Stamp — the only cross-node priority:** n leading zero bits of ID = class n, mined via a payload nonce; priority is proof of work, unforgeable on every medium. Convention: topic `sos` outranks policy. Confirm peers out of band by address emoji-hash.

## 11. Defaults
hops 16 · expiry 7 d · HELLO 5→80 min Trickle · ANNOUNCE flood ≤ 1/h · path fresh 3 h · seen-set ≥ 30 d (received; see §7.2/S-023 for what this implementation does with prekeys and beacons) · relay airtime ≤ 10% · payload UTF-8. T0 ≈ 60 lines; full T2 ≈ 400 with libsodium.

---

# Page 2 — Bindings: SPORE on everything

**Every medium on Earth has one of five shapes.** Bind by shape; the router never changes. This page is normative for the *shapes*; the per-medium parameter tables (≈70 media: frequencies, port numbers, UUIDs, MTUs, firmware caveats) are the manual, [BRIDGES.md](BRIDGES.md).

1. **Message pipe** → one envelope/fragment per message.
2. **Byte stream** → KISS: frames delimited `0xC0`, escape `0xC0`→`0xDB 0xDC`, `0xDB`→`0xDB 0xDD`, command byte `0x00`.
3. **Text channel** → armor: `~S1.` + Base32(envelope) + `.` + Base32(SHA-256[0:4]) + `~`, whitespace ignored.
4. **Shared bus** → KISS + CSMA: listen-before-talk, backoff 1–5× airtime; no native CRC → append SHA-256(envelope)[0:4], verify or drop.
5. **Shared store** → write envelopes as entries named by hex ID; reading = receiving; the store is a persistent INV.

**Underlays with their own routing = ONE interface.** Meshtastic, Reticulum, Yggdrasil, cjdns, BATMAN/OLSR, Tor/I2P, WireGuard, plain IP — each already moves bytes across many physical hops. Hand it one frame and decrement `hops` **once** for the whole crossing; its internal hops are invisible and free. Point-to-point backbone links may *restore* the hop so long hauls don't burn the budget. SPORE hops therefore count **gateways between networks**, not hops inside them — exactly IP over Ethernet.

**Numbers worth memorising.** Port **7373** (UDP/TCP/WS), the same value as EtherType `0x7373` and multicast `239.73.73.73` / `ff02::7373`. Meshtastic portnum **256**. BLE service `53504f52-4531-…`. Everything else: look it up.

**Two address spaces.** *Who* = the SPORE address or topic — end-to-end, cryptographic, identical on every medium. *How* = the underlay's own naming (a node number, a destination hash, an `IP:port`, or nothing at all) — local to one link. A bridge owns exactly one interface and translates between them; the router never learns underlay addresses, the way an OS's ARP table maps IP→MAC. Bindings are learned by **snooping signed frames** — a signed envelope proves its own sender, so no handshake is needed — and a stale binding costs nothing, because flood-fallback (§5.6) routes around it.

**Zero-rendezvous peering (browsers & phones).** A WebRTC session reduces to ufrag(4) + pwd(22) + DTLS fingerprint(32) + mDNS host candidate(16) ≈ **90 B**; both sides rebuild full SDP from a hardcoded template. Beep that descriptor over ultrasound or show it as a QR, answer, and the browser's own mDNS completes a direct DataChannel — no server, no typed IP, ≈15 s. Native nodes run **ice-lite with static ufrag/pwd/fingerprint**, so their descriptor is a constant you can print on the box. **App distribution:** every native node serves the PWA at `http://localhost:7373/` and to the LAN at its IP, with an HTTP bag API (`POST /spore/push`, `GET /spore/inv?since=`, `POST /spore/want`, MIME `application/x-spore`). The app store is every node.

## Positioning — why not just use…
**Reticulum**: superb routed crypto network; needs real software and bidirectional links on both ends. SPORE's floor is 60 lines, a sound card, or a human with paper — and it happily rides Reticulum. **DTN Bundle Protocol (RFC 9171)**: same ideas, IETF-complete and heavy; SPORE is BPv7's core on one page. **Meshtastic**: turnkey but one radio and its own app layer; SPORE is medium-plural and rides it. **SSB / Hypercore**: log replication couples you to whole feeds; SPORE dedups per message, so a 200-B link or a QR code is a valid peer. **Nostr**: the same signed-gossip idea minus JSON, relays and always-on internet. **Signal**: SPORE sessions use Signal's own Double Ratchet — over radios, paper and sneakernet. **Briar**: only friends relay; SPORE lets strangers carry mail, with mix mode when metadata matters. **Mixmaster / Loopix**: §9 is their core, built from one primitive. **BATMAN / OLSR / Babel / IP**: strictly better on stable dense powered networks — which is why they're underlays here. **FidoNet / UUCP / Usenet**: the ancestors; SPORE is netmail+echomail with modern crypto and self-forming routes.

**Known limits, on purpose:** no stream semantics; a single envelope fountain-fragments to ≈50 KB and larger objects ride the file layer (§6), where what bounds them is storage and what each link agrees to carry, not the format; no permanence (expiry is a feature); mix-mode anonymity needs flowing decoys to beat a *global* observer; ratchet state is per-device — give each device its own key. Each limit buys the two-page spec. When you have fiber and stability, run IP and tunnel SPORE through it; SPORE's job begins where that ends.
