# How it works

SPORE nodes pass signed envelopes to each other whenever they can reach one
another — over Wi-Fi, a cable, a shared folder, Bluetooth, sound, or radio.
No node needs to be always-on, and no server sits in the middle.

<div class="grid">

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">Your device is the address</h2>
<p class="text-muted">An address is the hash of a public key, not an account.
There is nothing to sign up for and nothing a company can suspend.</p>
<p><a href="spec.html#1-identity--addressing">Address format →</a></p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">Delivery is store-and-forward</h2>
<p class="text-muted">Devices hold envelopes they haven't delivered yet and
pass them on when they meet another node, so a message still arrives after
you've been offline.</p>
<p class="text-muted">Nobody plans the route. A node drops anything it has
already seen, keeps the rest until it expires, and passes each one on with a
hop count one lower — so copies spread outward and die out instead of looping.
Sending is how routes are found: the first copy to arrive teaches everyone
along the way which direction the sender lies in, and replies come back that
way until the path stops working, at which point it spreads out again.</p>
<p><a href="spec.html#5-forwarding-rules-the-entire-router">The forwarding rules →</a>
· <a href="continuity.html">Why this survives outages →</a></p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">Bridges are pluggable</h2>
<p class="text-muted">The envelope format doesn't change between mediums —
the same message crosses Wi-Fi, a USB stick, Bluetooth, an audio modem, or a
radio link.</p>
<p><a href="bridges.html">Full bridge list →</a></p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">Private by default, public on purpose</h2>
<p class="text-muted">A message to one person is sealed and only they can
read it. A post to an open group travels in the clear, deliberately — that's
what lets a stranger's device carry it forward.</p>
<p><a href="spec.html#7-crypto--forward-secrecy">Privacy model →</a></p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">Junk mail costs the sender something</h2>
<p class="text-muted">Priority is proof-of-work, not a claim — anyone can mint a
high-priority envelope, but not for free. Congestion control caps how much of a
link any relayed traffic can use, and a node re-checks a stored copy against its
own content hash before trusting it, so tampering after the fact just makes the
copy disappear rather than serve corrupted.</p>
<p><a href="threat-model.html#4-resources--storage-cpu-battery-bandwidth-spam">What stops flooding →</a></p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">Radio networks become one interface</h2>
<p class="text-muted">Meshtastic, Reticulum, Tor, WireGuard, plain IP — each
already moves bytes across many physical hops. SPORE hands one of them a frame
and treats the whole crossing as a single hop. They aren't rivals to replace;
they're transports it can ride.</p>
<p><a href="spec.html#page-2--bindings-spore-on-everything">Every medium is one of five shapes →</a></p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">Direct, when a path exists</h2>
<p class="text-muted">For a live chat or file transfer, two nodes can open a
low-latency pipe straight to each other. When no path exists, the message
still gets there — just store-and-forward instead of instant.</p>
<p><a href="direct.html">How Direct works →</a></p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">One protocol, many runtimes</h2>
<p class="text-muted">A phone, a browser tab, a daemon, or a cheap radio
board all speak the same wire format. Runtimes differ; what they say to each
other doesn't.</p>
<p><a href="apps.html">Get a node →</a></p>
</div></div></div>

</div>

<p><a class="btn" href="developer.html">Developer docs</a>
<a class="btn btn-cancel" href="apps.html">Get a node</a></p>
