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
<p class="text-muted">Files work the other way round. Their index floods, but
the pieces are only ever sent to someone who asked — and a node asked for a
piece it does not have will go and fetch it, so wanting a file pulls it toward
you across nodes that never had it, without anyone routing a request back to
whoever published it.</p>
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
<p><a href="spec.html#bindings--spore-on-everything">Every medium is one of five shapes →</a></p>
</div></div></div>

<div class="col-6"><div class="card"><div class="card-body">
<h2 class="text-h5">A slow radio is still a hop</h2>
<p class="text-muted">Links disagree wildly about how big a frame can be — a
Wi-Fi frame is 1400 bytes, a LoRa one 237, a Zigbee one 54. A bridge whose link
cannot carry a message splits it for that link alone and the far side puts it
back, so the sender never has to know what the far end of the path is made of.
Because a radio drops frames, a few repair pieces go with it: any complete-enough
subset rebuilds the message, and nothing has to be asked for again.</p>
<p><a href="spec.html#link-fragmentation--crossing-a-hop-that-cannot-carry-the-frame">Link fragmentation →</a></p>
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

<h2>Walked through</h2>

<p class="text-muted">The cards above are the shape. These are four things that
actually happen, with the real numbers — each one is a scenario in the
simulator, so if the behaviour changes the description fails with it.</p>

<div class="card"><div class="card-body">
<h3 class="text-h5">A 4 kB message, over Wi-Fi, then Wi-Fi, then LoRa</h3>
<p class="text-muted">Ada sends Rae 4 kB. The path runs across two Wi-Fi hops
and then a LoRa radio, and Ada has no idea the radio is there.</p>
<ol class="text-muted">
<li>The envelope is about 4 114 bytes signed. Ada's own link takes 1 400, so she
splits it into four pieces of roughly 1 362 and sends those.</li>
<li>Both Wi-Fi hops carry each piece whole.</li>
<li>The node holding the LoRa link cannot: its frames are 237 bytes. It cuts each
arriving piece into six, adds a couple of repair pieces, and sends those. The far
end reassembles before its router ever sees a fragment.</li>
<li>Rae's node puts the four pieces back and checks one signature.</li>
</ol>
<p class="text-muted">54 frames, 25 kB on the wire, first delivery at 130 ms.
Before per-hop splitting existed the same send produced 18 frames, 25 kB, and
<strong>nothing arrived</strong> — the pieces were cut for a link three hops away
and no node on the path could re-cut them.</p>
</div></div>

<div class="card"><div class="card-body">
<h3 class="text-h5">A file three hops away, from someone who has gone home</h3>
<p class="text-muted">Someone published a file. Its index flooded, so everyone
knows it exists; the pieces did not, so nobody between you and the publisher has
them.</p>
<ol class="text-muted">
<li>You ask your neighbour for the pieces. It has none.</li>
<li>Rather than forward your request, it <em>adopts</em> it — it now wants those
pieces itself — and asks its own neighbours. That repeats, up to a depth limit.</li>
<li>Somewhere along the line a node has them. They come back the way the interest
went, and every node that carried them keeps a copy, so the next person on that
path is faster.</li>
</ol>
<p class="text-muted">Nothing routed a request to the publisher, so it works when
the publisher is offline and ten caches are not. A node only ever adopts interest
in pieces some index it already holds names — otherwise "want this id" would be a
request to search the mesh on a stranger's behalf.</p>
</div></div>

<div class="card"><div class="card-body">
<h3 class="text-h5">A small file, with no round trip at all</h3>
<p class="text-muted">A 2 kB note is published. The index goes out, and the first
few pieces go with it — enough that the whole file arrives in one shot. The
receiver has nothing left to ask for, so it asks for nothing.</p>
<p class="text-muted">How many pieces ride along is the sender's choice alone.
The receiver ignores what it already has and asks for the rest either way, so the
two ends never have to agree, and a node that would rather not spend the airtime
can send none.</p>
</div></div>

<div class="card"><div class="card-body">
<h3 class="text-h5">A message on a link that drops one frame in ten</h3>
<p class="text-muted">A 900-byte envelope over a 237-byte radio is five pieces,
and all five have to arrive — so a link losing 10% of frames loses closer to half
of messages. Adding one repair piece takes that from 54% delivered to 90%; two
take it to 96%.</p>
<p class="text-muted">Repair, rather than asking again, because asking needs a
way back. A one-way radio has none, and on a shared channel a complaint collides
with the traffic it is complaining about.</p>
</div></div>

<p><a class="btn" href="developer.html">Developer docs</a>
<a class="btn btn-cancel" href="apps.html">Get a node</a></p>
