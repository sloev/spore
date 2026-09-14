# Developer

Everything with real technical depth lives here rather than scattered across the
site. The groups below are ordered the way a question usually arrives: what is
this, what are the rules, how do I build it, what is actually proved, and where
is it going.

Looking for a specific word — "custody", "fountain code", "watermark"? Use
[Search](search.html) in the menu; it returns sections rather than pages, so a
hit lands on the paragraph.

## Start here

New to this, or coming back to it. Each answers a different first question.

<div class="list-rows">
<a class="list-row" href="concepts.html"><div class="list-row-body"><span class="list-row-title">Concepts</span><span class="list-row-subtitle">Each metaphor on the landing page, and the thing it is actually made of.</span></div></a>
<a class="list-row" href="glossary.html"><div class="list-row-body"><span class="list-row-title">Glossary</span><span class="list-row-subtitle">Words this project uses in a particular way, each pointing at where it lives in the code.</span></div></a>
<a class="list-row" href="kernel-flows.html"><div class="list-row-body"><span class="list-row-title">Kernel flows</span><span class="list-row-subtitle">A message moving — four scenarios, every arrow recorded from a real node call.</span></div></a>
<a class="list-row" href="status.html"><div class="list-row-body"><span class="list-row-title">What works today</span><span class="list-row-subtitle">Counted from the tree — what is tested, what has never met an antenna, and what is not written yet.</span></div></a>
</div>

## The protocol

What binds an implementation. Read the reference for the rules and the rest for the parts that are easy to get wrong.

<div class="list-rows">
<a class="list-row" href="spec.html"><div class="list-row-body"><span class="list-row-title">Technical reference</span><span class="list-row-subtitle">The wire format, the application layer on top, and where the core runs.</span></div></a>
<a class="list-row" href="bridges.html"><div class="list-row-body"><span class="list-row-title">Bridges</span><span class="list-row-subtitle">Every medium SPORE speaks, and which are verified on real hardware.</span></div></a>
<a class="list-row" href="direct.html"><div class="list-row-body"><span class="list-row-title">Direct</span><span class="list-row-subtitle">The low-latency peer-to-peer pipe, and how NAT traversal works.</span></div></a>
<a class="list-row" href="state-machines.html"><div class="list-row-body"><span class="list-row-title">State machines</span><span class="list-row-subtitle">The ratchet, the mix and the hub, drawn from the transitions they actually take.</span></div></a>
<a class="list-row" href="rebuild.html"><div class="list-row-body"><span class="list-row-title">Rebuild</span><span class="list-row-subtitle">Reimplementing SPORE from scratch, with worked byte examples.</span></div></a>
<a class="list-row" href="reference.html"><div class="list-row-body"><span class="list-row-title">Reference decoders</span><span class="list-row-subtitle">Dependency-free Tier-0 decoders and the cross-language test vectors.</span></div></a>
<a class="list-row" href="bindings.html"><div class="list-row-body"><span class="list-row-title">Language bindings</span><span class="list-row-subtitle">Python, Go and JS wrappers generated from one C ABI.</span></div></a>
</div>

## Build and run it

Working on the code, or on a device.

<div class="list-rows">
<a class="list-row" href="dev-guide.html"><div class="list-row-body"><span class="list-row-title">Dev guide</span><span class="list-row-subtitle">Repo map, build commands per area, and where to look for what.</span></div></a>
<a class="list-row" href="webguide.html"><div class="list-row-body"><span class="list-row-title">The browser node</span><span class="list-row-subtitle">Building and hacking on the single-file web node.</span></div></a>
<a class="list-row" href="testing.html"><div class="list-row-body"><span class="list-row-title">Android device tests</span><span class="list-row-subtitle">The device-matrix checklist for the Android app.</span></div></a>
</div>

## What is actually proved

Claims with evidence behind them, and the places where there is none yet. Start here if you are deciding whether to depend on any of this.

<div class="list-rows">
<a class="list-row" href="security-matrix.html"><div class="list-row-body"><span class="list-row-title">Security maturity</span><span class="list-row-subtitle">What has actually been tested, per component — and the column that is empty for all of them.</span></div></a>
<a class="list-row" href="simulations.html"><div class="list-row-body"><span class="list-row-title">Simulations</span><span class="list-row-subtitle">Every scenario the simulator runs, what each is for, and the numbers from the last run.</span></div></a>
<a class="list-row" href="hardware.html"><div class="list-row-body"><span class="list-row-title">Hardware verification</span><span class="list-row-subtitle">What's been run on real radios and real devices, not just CI.</span></div></a>
<a class="list-row" href="threat-model.html"><div class="list-row-body"><span class="list-row-title">Threat model</span><span class="list-row-subtitle">Six chapters of adversary, what stops them, and the residual risk where nothing fully does.</span></div></a>
<a class="list-row" href="security-policy.html"><div class="list-row-body"><span class="list-row-title">Reporting a vulnerability</span><span class="list-row-subtitle">How to report, and what happens next.</span></div></a>
</div>

## The project

Where it is going, how it is run, and what happens if it stops.

<div class="list-rows">
<a class="list-row" href="roadmap.html"><div class="list-row-body"><span class="list-row-title">Roadmap</span><span class="list-row-subtitle">The engineering plan, milestone by milestone.</span></div></a>
<a class="list-row" href="https://github.com/sloev/spore/releases"><div class="list-row-body"><span class="list-row-title">Releases</span><span class="list-row-subtitle">What shipped, in order, with notes generated from the commits.</span></div></a>
<a class="list-row" href="mission.html"><div class="list-row-body"><span class="list-row-title">Mission</span><span class="list-row-subtitle">The project charter and the decision test every change is held to.</span></div></a>
<a class="list-row" href="contributing.html"><div class="list-row-body"><span class="list-row-title">Contributing</span><span class="list-row-subtitle">Freeze rules, CI, branches, releases.</span></div></a>
<a class="list-row" href="continuity.html"><div class="list-row-body"><span class="list-row-title">Continuity</span><span class="list-row-subtitle">What survives if you lose a device, this codebase, or this website.</span></div></a>
</div>
