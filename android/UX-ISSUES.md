# Android UX issues & conventions

Running notes on Android-app UX decisions that aren't obvious from the code, so a
later change doesn't undo them by accident. Paired with `docs/ROADMAP.md`
(which tracks status) and `docs/VISUALDESIGN.md` (which is normative for look).

## Chat attachments (PR1)

**Problem.** The release APK published a file the instant it was picked: no staging,
so a mis-tap sent a file with no message; the file then arrived as its own
contextless bubble with no preview and no way to open it; and there was no
`FileProvider`, so even a saved file couldn't be handed to another app.

**Target.** Pick → the file stages in the composer (nothing on the wire yet) →
Send produces **one** bubble carrying the text *and* the attachment, the same on
both sides → an image previews inline, any file opens via a chooser.

### The marker (application convention)

An attachment travels as two envelopes: the file's manifest+chunks (the existing
publish path, sealed to the peer when known), and a normal DATA body whose **last
line** is the canonical marker:

```
📎 <filename> | spore:<hex-magnet> | <mime>
```

- Matched by `Markdown.parseAttach` (`(?m)^📎 (.+) \| spore:([0-9a-fA-F]{16,}) \| (\S+)$`).
- Application-level only: relays and non-SPORE clients see opaque UTF-8, and a
  client that doesn't parse it just shows the marker text — a reasonable fallback.
- Distinct from the feed's image form `![name](spore:<magnet>)`, which has nowhere
  to carry a mime type. Chat needs the mime to decide image-preview vs file-chip.

### One bubble, both sides

- **Sender:** `sendTextWithAttachment` publishes the file, then sends the marker
  body via the shared `sendBody`, stamping `magnet`+`mime` onto its own `Msg`. The
  local bytes are cached immediately so the sender can preview/Open their own file
  (our own file never comes back through the mesh).
- **Receiver:** the marker body arrives as ordinary text; `route()` parses it and
  stamps `magnet`+`mime` onto the received `Msg`. The manifest envelope's
  "incoming file…" bubble is **suppressed for sealed files** (they now always carry
  a marker bubble), and `pumpFiles`' "received…" bubble is suppressed when a message
  already references the magnet — so the attachment is one bubble, not three.

### Preview & Open

- Images decode with `inSampleSize` (cap 1080 px) on `Dispatchers.IO` via
  `produceState` — a phone photo decoded whole for a 220 dp row costs ~100 MB of
  heap and would stall the list.
- Open copies the bytes into `cacheDir/attachments/<magnet>` (reclaimable) and
  shares a `FileProvider` `content://` URI with a one-shot read grant. **Never** a
  `file://` path and never the private store directly — the provider vends only the
  copy (`res/xml/file_paths.xml` lists exactly `attachments/`).

### Acceptance

- [ ] Pick → composer chip; thread unchanged until Send.
- [ ] Remove staged (✕) → text-only Send.
- [ ] Send → one bubble text+attachment for sender and receiver.
- [ ] LED while fetching; inline preview when an image is decodable.
- [ ] Open via FileProvider chooser; no crash when the file isn't here yet.
- [ ] Sealed-publish path preserved (contents *and* filename sealed to a known peer).
- [ ] No pink-on-olive; `contentDescription` on the image.

### Non-goals (v1)

Multiple files per send; ExoPlayer audio/video playback; editing after send. A
received **public/unsealed** file with no marker still shows the legacy
"incoming/received" status bubbles — only sealed DM attachments get the single
merged bubble, because only they are guaranteed a marker sender.

**Not device-verified.** Written without an Android SDK in the build environment;
the `apk` CI job compiles it. Manual QA (stage/remove/send/open, large image,
peer-without-file) is a device-matrix item (PR6).
