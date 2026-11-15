# ADR-0138 — The first release is an unsigned GitHub build gated only on single-file project archives

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** #168
- **Architecture:** [Releases](../architecture/platform.md#releases)

## Context

Every section of the product brief before MIDI is built, and the roadmap (#20) held MIDI back until
"the first release" without defining one. Still open: punch-in/out and loop recording (#111), grid
snap (#114), single-file archives (#129) and effect follow-ups (#133, #134, #135, #137, #150).

## Decision

The first release (#168) waits only on #129. It is a `v0.1.0` tag and a GitHub Release carrying the
zipped, ad-hoc-signed `Debussy.app` with third-party licence notices inside; it is not Developer ID
signed or notarized. #111, #114 and the effect follow-ups land after it. MIDI work (#169–#177) is
blocked on it.

## Alternatives considered

- **Signed, notarized DMG** — the smoothest install, but needs an Apple Developer account and a
  signing pipeline before anyone can try the app.
- **Tag only, build from source** — no distribution work, but no build for a musician to try.
- **Wait for every open follow-up** — delays the release for features the brief does not list.

## Consequences

- Users must open the app past Gatekeeper themselves; the release notes say how.
- Signing and notarization stay open for a later release.
- MIDI work starts once #168 is closed.
