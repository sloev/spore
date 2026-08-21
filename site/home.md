# Messages that still get through

SPORE is a shared delivery layer for your own devices. Notes, files and updates
hop across phones, a folder, a USB stick or a radio link — without an account and
without a company's server in the middle.

- **No account.** Your device is the address.
- **Works when the network is bad.** Flaky Wi-Fi, local-only, or offline — it catches up later.
- **One set of rules.** Internet, cable, folder, or sound: same delivery.

<p><a class="btn" href="demo/">Try it in your browser</a>
<a class="btn btn-cancel" href="apps.html">Get the app</a></p>

<p class="text-muted">Nothing to install to try it — the browser node is one page,
and it keeps working after you go offline.</p>

## How it feels

<div class="grid">

<div class="col-4"><div class="card"><div class="card-body">
<strong>A signed postcard</strong>
<p class="text-muted">To, from, expiry, payload — devices pass copies when they meet.</p>
<details>
<summary>How delivery works</summary>
<p>Think of a signed postcard. It says who it is for, when it stops being worth
carrying, and what is inside. Devices keep copies of the ones they have not seen
and pass them along when they meet. Duplicates fall away, old mail expires, and a
message finds a route you never had to plan.</p>
<p>That is enough for a household or a neighbourhood to keep talking, with no
directory and nobody's server to ask.</p>
<p>The postcard is how the delivery works, not a limit on what you send. The same
envelope carries chat, files, a live session between two apps, a feed you follow,
or a sensor's readings.</p>
</details>
</div></div></div>

<div class="col-4"><div class="card"><div class="card-body">
<strong>Where it can travel</strong>
<p class="text-muted">Wi-Fi · folder · USB · Bluetooth · sound · paper · radio when you verify it.</p>
<details>
<summary>Every kind of link</summary>
<p>Home Wi-Fi · a shared folder or a USB stick · Bluetooth · sound between two
laptops · a printed code or a message read aloud, for short notes · and optional
long-range radio if you have the hardware.</p>
<p>Radio paths are written and tested in software; until someone has run them on
real hardware we mark them as such — see <a href="bridges.html">Bridges</a>.</p>
</details>
</div></div></div>

<div class="col-4"><div class="card"><div class="card-body">
<strong>Being straight with you</strong>
<p class="text-muted">Private to one person stays private. Open group posts are public on purpose.</p>
<details>
<summary>Privacy in plain words</summary>
<p>SPORE is open source and public domain — no company owns it and there is nothing
to buy.</p>
<p><strong>Private messages are private. Posts to an open group are public — on purpose.</strong>
A message to one person can only be read by that person. A closed group can have a
key too, and then only its members can read it. But a post to an <em>open</em> group
travels in the clear, deliberately: that is what lets a device you have never met
pass it along, exactly like a postcard on a noticeboard. If it matters who reads
it, send it to a person or a closed group.</p>
<p>Some parts are further along than others. See
<a href="bridges.html">Bridges</a> and
<a href="security.html">what we have found and fixed</a>.</p>
</details>
</div></div></div>

</div>

The name is the idea: the protocol is a **spore** — a small seed carrying
everything needed to regrow the whole thing from one copy — and every device it
lands in is **soil**. A laptop, a phone, a browser tab or a cheap radio board
each feed it what they have — a clock, some randomness, room to keep messages, a
way to reach other devices — and the same node grows in all of them.

## For builders

The full rules are in the <a href="spec.html">Spec</a>; the reasoning is in
<a href="design.html">Design</a>, every link it speaks in
<a href="bridges.html">Bridges</a>, and how to rebuild it from scratch in
<a href="rebuild.html">Rebuild</a> and
<a href="continuity.html">Continuity</a>. Source on
Source on <a href="https://github.com/sloev/spore">GitHub</a>. Curious how it
works? See <a href="mission.html">How it works</a>.
