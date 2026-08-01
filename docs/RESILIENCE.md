# Resilience — the pieces of "outlives us"

`MISSION.md` names two of its seven building pillars **Continuity** and
**Rebuild without us** — deliberately two, not one, because they answer
different questions. This page is the map between them: not a third document
duplicating either, but the index that makes the connection between them
legible, since neither page states it on its own.

## The question each piece answers

| Piece | Answers | Lives at |
|---|---|---|
| **Continuity** | If I lose this device, this app, this website — does *my* node come back? | [`CONTINUITY.md`](CONTINUITY.md) |
| **Rebuild** | If this codebase disappeared entirely, could someone who has never seen it write a compatible one? | [`REBUILD.md`](REBUILD.md) |
| **Reference decoders** | Can I check a real envelope against something I don't have to trust — no crypto library, no dependency? | [`reference/`](https://github.com/sloev/spore/tree/master/reference) |
| **Release artifacts** | Can I get a working node *and* the means to rebuild it from one download, with no live infrastructure involved? | Every [GitHub release](https://github.com/sloev/spore/releases) carries `spore-standalone.html` (the whole browser node, one file) and `spore-offline-bundle.tar.gz` (this source tree, dependencies vendored, `cargo build --offline` works immediately) alongside the APK — see [`APPS.md`](APPS.md) |
| **The frozen contract** | Will a node built from any of the above still speak to one built today, indefinitely? | `tests/api_freeze.rs` + `reference/vectors.json`, held in place by [`CONTRIBUTING.md`](../CONTRIBUTING.md)'s freeze rules |
| **Public domain** | Is there a license, a company, or a maintainer this depends on continuing to exist? | [`LICENSE`](../LICENSE) — no |

Together: a copy of SPORE that survives — a phone with the app, a saved HTML
file, a printed Seed Sheet, a cloned repo, or just a downloaded release
tarball — carries everything needed to keep working, verify it's genuine, and
regrow the rest, without this repository, this website, or any company
staying online.

## Why these stay separate documents

Merging `CONTINUITY.md` and `REBUILD.md` into one file was considered and
declined. They serve different readers in different voices: `REBUILD.md` is a
dense, byte-level implementer's guide (worked hex examples generated straight
from the reference code) for someone writing a Python/Go/C reimplementation;
`CONTINUITY.md` is an illustrated story-card page (cold-start playbooks, a
roadmap checklist) for someone asking "will this still work if the internet's
down, or if this project stops existing." Combining them would serve neither
reader as well as either page does alone — and both are already independent
top-nav destinations on the site. This page is the connective index instead:
short, off the top nav (like `MISSION.md` and `DEV_GUIDE.md`), linking out
rather than duplicating.
