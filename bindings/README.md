# SPORE language bindings

Use the SPORE core from **Python, Go, and JavaScript** — not by reimplementing it,
but by calling the same Rust library through a small C interface. The wrappers are
*generated*, so all three languages stay in lock-step with the Rust code.

```
src/ffi.rs   ──►  libspore.so   ──►  Python (ctypes)
   (C ABI)      (cargo build)        Go     (cgo)
                                     JS     (koffi)
```

## Quick start

```sh
cargo build --release            # builds target/release/libspore.{so,dylib,dll}
python3 bindings/generate.py     # (re)generate the wrappers from spec.json
```

Then, in each language:

<details>
<summary>Python (ctypes — no dependencies)</summary>

```python
import sys; sys.path.insert(0, "bindings/python")
import spore

sk, pk = spore.keypair()
addr   = spore.addr_of(pk)
wire   = spore.message_new(sk, addr, 1_700_000_000, b"hello")
assert spore.message_verify(wire)
assert spore.message_payload(wire) == b"hello"
```

Run the smoke test: `python3 bindings/python/test.py`.
</details>

<details>
<summary>Go (cgo)</summary>

```go
import spore "spore"          // bindings/go

sk, pk := spore.Keypair()
addr := spore.AddrOf(pk)
wire := spore.MessageNew(sk, addr, 1700000000, []byte("hello"))
_ = spore.MessageVerify(wire)
```

Run the test: `cd bindings/go && LD_LIBRARY_PATH=../../target/release go test ./...`.
</details>

<details>
<summary>JavaScript / Node (koffi)</summary>

```js
const s = require("./bindings/node/spore.js");   // npm install koffi

const [sk, pk] = s.keypair();
const addr = s.addr_of(pk);
const wire = s.message_new(sk, addr, 1700000000, Buffer.from("hello"));
s.message_verify(wire); // true
```

Run the test: `cd bindings/node && npm install && node test.js`. In the browser,
load the `wasm32` build of the same crate instead.
</details>

The wrappers all share one core, so they interoperate: a message signed in Python
verifies in Go or Node unchanged.

## What's exposed

The C ABI (`src/ffi.rs`) covers the byte-in/byte-out primitives — identity, signed
messages, sealing, encrypted topics, and text armor. Everything takes and returns
bytes; functions that can fail return `None`/`nil`/`null`.

<details>
<summary>Deep dive: the surface and how generation works</summary>

`spec.json` lists each function with a tiny type vocabulary (`bytes`, `in8/16/32`,
`out8/16/32`, `u32`; returns `void` / `bytes` / `bool`). `generate.py` reads it and
emits `spore.h`, `python/spore.py`, `go/spore.go`, and `node/spore.js`. To change
the surface: edit `src/ffi.rs` **and** `spec.json` together, then rerun the
generator. The Rust side has its own test (`cargo test ffi`); each wrapper has a
smoke test.

| Function | Purpose |
|---|---|
| `keypair` / `prekey` | Ed25519 signing pair / X25519 encryption prekey pair |
| `addr_of` / `topic_of` | SPORE address / topic address (8 bytes) |
| `message_new` / `message_verify` | build+sign a DATA envelope / check its signature |
| `message_id` / `message_payload` | its 16-byte content ID / its payload |
| `seal` / `open` | anonymous sealed box to/from a prekey |
| `topic_seal` / `topic_open` | encrypted-topic pre-shared-key crypto |
| `armor_wrap` / `armor_unwrap` | `~S1.…~` Base32 text you can paste or print |

Buffers returned across the ABI are freed by the wrapper automatically
(`spore_bytes_free`); callers never manage memory. The full router, sessions,
files, and bridges stay on the Rust side — the bindings are for the portable
message/crypto primitives an app in another language actually needs.
</details>
