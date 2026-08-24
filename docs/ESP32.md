# The ESP32 relay

<p><span class="badge">In development</span></p>

A ~$5 board, a battery and an antenna, left somewhere useful — carrying messages
for whoever comes past. No operating system, no Raspberry Pi, nothing to log in
to. It is the smallest thing that can be a full SPORE node rather than a client
of one.

**You cannot run this yet.** The toolchain and the core are done and building in
CI; the radio, the storage and the phone link are not written. The honest state
of each part is in the table further down, and the
[roadmap](roadmap.html#milestone-8--embedded-esp32-runtime-raw-80211-relay) tracks
the rest.

## What it is for

<div class="grid">

<div class="col-4"><div class="card"><div class="card-body">
<h2 class="text-h5">Somewhere with no network</h2>
<p class="text-muted">A valley, a building, a field. The board holds messages and
hands them on when a phone or another board comes within range.</p>
</div></div></div>

<div class="col-4"><div class="card"><div class="card-body">
<h2 class="text-h5">Cheap enough to leave behind</h2>
<p class="text-muted">Losing one costs a few dollars, so you can put them where
you would never leave a laptop — and put out enough that no single one matters.</p>
</div></div></div>

<div class="col-4"><div class="card"><div class="card-body">
<h2 class="text-h5">A peer, not a gateway</h2>
<p class="text-muted">It runs the same core as the phone app and the daemon, so
it stores and forwards on its own. Nothing routes through a server.</p>
</div></div></div>

</div>

## How it works

Three parts, and the middle one is why it works unattended:

<div class="grid">

<div class="col-4"><div class="card"><div class="card-body">
<h2 class="text-h5">1. It listens to the air</h2>
<p class="text-muted">Raw 802.11 — Wi-Fi frames with no access point and no
joining a network. It watches every frame that passes, keeps the ones that are
SPORE envelopes, and drops the rest immediately.</p>
</div></div></div>

<div class="col-4"><div class="card"><div class="card-body">
<h2 class="text-h5">2. It keeps what it hears</h2>
<p class="text-muted">Envelopes go to flash, so a board that loses power picks up
where it left off instead of forgetting. That is what makes it a relay rather
than a repeater.</p>
</div></div></div>

<div class="col-4"><div class="card"><div class="card-body">
<h2 class="text-h5">3. It hands them to you</h2>
<p class="text-muted">Plug in a USB cable, or connect over Bluetooth, and the
board and your phone exchange whatever each is missing.</p>
</div></div></div>

</div>

Because every envelope is signed and sealed by whoever sent it, the board never
needs to be trusted. It cannot read private messages it carries, and it cannot
forge one — so leaving a board somewhere you do not control costs you nothing but
the board. See [Bridges](bridges.html#wifi) for the radio's exact behaviour and
its regulatory notes.

## What actually works today

Being straight about this, because everything above except the first row is a
plan rather than a thing you can use:

| Part | Status |
|---|---|
| The SPORE core runs on ESP32-S3 | ✅ builds and links in CI, unmodified — no ESP-specific branches |
| Footprint measured | ✅ ~13% of a 4 MB flash part, ~13% of 512 KB RAM |
| Booting on a real board | ⬜ never flashed — the binary exists, nothing has run it |
| Raw 802.11 send/receive | ⬜ not written |
| Flash storage | ⬜ not written |
| USB and Bluetooth link to a phone | ⬜ not written |

No claim on this page has been checked on hardware. Everything green above is a
build-time fact, which is a different and much weaker thing —
[Hardware verification](hardware.html) is where a real device run gets recorded,
and there is not one for this yet.

## Building it yourself

The board is Xtensa, which ordinary Rust does not target, so the build runs in
Espressif's Docker image and needs nothing installed on your machine:

```sh
docker run --rm -v "$PWD":/work -w /work/esp32 \
  espressif/idf-rust:esp32s3_latest \
  bash -lc 'cargo build --release'
```

The first build compiles ESP-IDF itself and takes a while. Full instructions,
including flashing and the footprint report, are in
[`esp32/README.md`](https://github.com/sloev/spore/blob/master/esp32/README.md).

<p><a class="btn" href="apps.html">Other ways to run a node</a>
<a class="btn btn-cancel" href="roadmap.html#milestone-8--embedded-esp32-runtime-raw-80211-relay">Follow the work</a></p>
