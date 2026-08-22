# How it works

SPORE nodes pass signed envelopes to each other whenever they can reach one
another — over Wi-Fi, a cable, a shared folder, Bluetooth, sound, or radio.
No node needs to be always-on, and no server sits in the middle.

<div class="grid">

<div class="col-6"><div class="card"><div class="card-body">
<strong>Your device is the address</strong>
<p class="text-muted">An address is the hash of a public key, not an account.
There is nothing to sign up for and nothing a company can suspend.</p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<strong>Delivery is store-and-forward</strong>
<p class="text-muted">Devices hold envelopes they haven't delivered yet and
pass them on when they meet another node. A message finds a route without
anyone planning one, and it still arrives after you've been offline.</p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<strong>Bridges are pluggable</strong>
<p class="text-muted">The envelope format doesn't change between mediums —
the same message crosses Wi-Fi, a USB stick, Bluetooth, an audio modem, or a
radio link. See <a href="bridges.html">Bridges</a> for the full list.</p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<strong>Private by default, public on purpose</strong>
<p class="text-muted">A message to one person is sealed and only they can
read it. A post to an open group travels in the clear, deliberately — that's
what lets a stranger's device carry it forward.</p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<strong>Direct, when a path exists</strong>
<p class="text-muted">For a live chat or file transfer, two nodes can open a
low-latency pipe straight to each other. When no path exists, the message
still gets there — just store-and-forward instead of instant.</p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<strong>One protocol, many runtimes</strong>
<p class="text-muted">A phone, a browser tab, a daemon, or a cheap radio
board all speak the same wire format. Runtimes differ; what they say to each
other doesn't.</p>
</div></div></div>

</div>

Want the full wire format, every bridge's exact behaviour, and the reasoning
behind each design choice? See the <a href="developer.html">Developer</a> page.
