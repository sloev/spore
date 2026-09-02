# Imported design reference (read-only)

`Spore Web App.dc.html` is the HARDBRUT/3 prototype that Milestone 10-D is built
from, exported from the Claude Design project
`8c7ed6be-6f3d-4123-b181-5d7ff8b86553` on 2026-09-02.

It is kept in the repo because the source project needs an authenticated
claude.ai session to read, and a spec nobody can open is not a spec. Nothing
builds from this file — it is a reference, not an input.

**It is not runnable here.** The markup is Claude Design's own prototype format:
107 `<x-import component-from-global-scope="DesignSystem_21e3b1.Button" …>`
elements with `{{expr}}` bindings and `<sc-if>` / `<sc-for>` control flow, driven
by a React bundle and a `support.js` runtime that are not vendored. It has to be
*translated* into the repo's plain-ESM idiom, not executed.

What it is good for: the information architecture (onboarding `step0..2`, chat,
blogs, contacts, per-contact files, settings, identity), the component budget,
and the exact HARDBRUT/3 classes each screen uses. The design system itself —
the part that *is* consumed — is vendored separately at `web/vendor/hardbrut3/`.
