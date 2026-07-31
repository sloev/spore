# Messages that still get through

SPORE is a shared delivery layer for your own devices. Notes, files and updates
hop across phones, a folder, a USB stick or a radio link — without an account and
without a company's server in the middle.

- **No account.** Your device is the address.
- **Works when the network is bad.** Flaky Wi-Fi, local-only, or offline — it catches up later.
- **One set of rules.** Internet, cable, folder, or sound: same delivery.

<p class="cta">
  <a class="cta-primary" href="demo/">Try it in your browser</a>
  <a class="cta-secondary" href="apps.html">Get the app</a>
</p>

<p class="cta-note">Nothing to install to try it — the browser node is one page,
and it keeps working after you go offline.</p>

## How it feels

<!-- Illustrated story: short captions in the open; full prose in <details>. -->
<div class="story" role="list">

<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <!-- Postcard hops between three nodes -->
    <svg class="ill ill-hop" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <defs>
        <filter id="glow-a"><feGaussianBlur stdDeviation="1.2" result="b"/><feMerge><feMergeNode in="b"/><feMergeNode in="SourceGraphic"/></feMerge></filter>
      </defs>
      <!-- nodes -->
      <circle class="node n1" cx="40" cy="60" r="14" />
      <circle class="node n2" cx="140" cy="60" r="14" />
      <circle class="node n3" cx="240" cy="60" r="14" />
      <!-- path dashes -->
      <path class="path" d="M54 60 H126" fill="none" stroke-dasharray="4 4"/>
      <path class="path" d="M154 60 H226" fill="none" stroke-dasharray="4 4"/>
      <!-- postcard -->
      <g class="postcard">
        <rect x="-16" y="-10" width="32" height="20" rx="2"/>
        <line x1="-10" y1="-4" x2="10" y2="-4"/>
        <line x1="-10" y1="2" x2="6" y2="2"/>
      </g>
    </svg>
  </div>
  <figcaption>
    <strong>A signed postcard</strong>
    <span class="story-lead">To, from, expiry, payload — devices pass copies when they meet.</span>
  </figcaption>
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
</figure>

<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <!-- Medium icons orbit a core envelope -->
    <svg class="ill ill-media" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <rect class="env" x="118" y="42" width="44" height="36" rx="3"/>
      <path class="env-flap" d="M118 48 L140 62 L162 48" fill="none"/>
      <!-- orbiting mediums -->
      <g class="orbit">
        <g class="m wifi" transform="translate(140,22)"><circle r="8"/><path d="M-4 1 Q0 -3 4 1" fill="none"/></g>
        <g class="m folder" transform="translate(210,60)"><rect x="-7" y="-5" width="14" height="11" rx="1"/><path d="M-7 -5 L-3 -9 H3"/></g>
        <g class="m bt" transform="translate(140,98)"><path d="M0 -6 L0 6 L5 2 L-3 -2 L5 -6 Z" fill="none"/></g>
        <g class="m sound" transform="translate(70,60)"><circle r="3"/><path d="M4 -4 Q10 0 4 4" fill="none"/><path d="M7 -7 Q16 0 7 7" fill="none"/></g>
      </g>
    </svg>
  </div>
  <figcaption>
    <strong>Where it can travel</strong>
    <span class="story-lead">Wi-Fi · folder · USB · Bluetooth · sound · paper · radio when you verify it.</span>
  </figcaption>
  <details>
    <summary>Every kind of link</summary>
    <p>Home Wi-Fi · a shared folder or a USB stick · Bluetooth · sound between two
    laptops · a printed code or a message read aloud, for short notes · and optional
    long-range radio if you have the hardware.</p>
    <p>Radio paths are written and tested in software; until someone has run them on
    real hardware we mark them as such — see <a href="bridges.html">Bridges</a>.</p>
  </details>
</figure>

<figure class="story-card crate" role="listitem">
  <div class="story-art" aria-hidden="true">
    <!-- Lock vs open noticeboard -->
    <svg class="ill ill-privacy" viewBox="0 0 280 120" xmlns="http://www.w3.org/2000/svg">
      <g class="private" transform="translate(70,60)">
        <rect class="lock-body" x="-14" y="-4" width="28" height="22" rx="3"/>
        <path class="lock-shackle" d="M-8 -4 V-14 A8 8 0 0 1 8 -14 V-4" fill="none"/>
        <circle class="lock-key" cx="0" cy="8" r="3"/>
      </g>
      <g class="public" transform="translate(200,60)">
        <rect class="board" x="-28" y="-24" width="56" height="48" rx="2"/>
        <line x1="-18" y1="-10" x2="18" y2="-10"/>
        <line x1="-18" y1="0" x2="12" y2="0"/>
        <line x1="-18" y1="10" x2="16" y2="10"/>
      </g>
      <text class="label-priv" x="70" y="108" text-anchor="middle">sealed</text>
      <text class="label-pub" x="200" y="108" text-anchor="middle">open post</text>
    </svg>
  </div>
  <figcaption>
    <strong>Being straight with you</strong>
    <span class="story-lead">Private to one person stays private. Open group posts are public on purpose.</span>
  </figcaption>
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
</figure>

</div>

## For builders

The full rules are in the <a href="spec.html">Spec</a>; the reasoning is in
<a href="design.html">Design</a>, every link it speaks in
<a href="bridges.html">Bridges</a>, and how to rebuild it from scratch in
<a href="rebuild.html">Rebuild</a> and
<a href="continuity.html">Continuity</a>. Source on
<a href="https://github.com/sloev/spore">GitHub</a>.
