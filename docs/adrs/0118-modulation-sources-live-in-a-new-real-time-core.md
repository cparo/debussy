# ADR-0118 — Modulation sources live in a new real-time-core modulation crate

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-13
- **Issue:** #62
- **Architecture:** [Crate map](../architecture/crates.md#crate-map)

## Context

Issue #62 adds chorus and flanger, both driven by low-frequency oscillators. Later MIDI and
modular-synthesis work will need the same oscillators as modulation sources, outside any one
effect. `audio_effects` holds effect nodes and `dsp` holds signal primitives; neither is a home
for modulation sources a future graph node could wrap. The delay's note and feel arithmetic for
tempo sync also lives privately in `audio_effects::delay`, and the LFOs need it too.

## Decision

A new real-time-core crate, `modulation`, holds modulation sources as plain DSP primitives,
starting with `Lfo` and `Shape`, and the tempo-sync arithmetic in `modulation::sync`
(`whole_note`, `synced_frames`, `period_frames`). It depends only on `dsp` (for `Noise`) and
carries the real-time `clippy.toml` and `#![forbid(unsafe_code)]`. `audio_effects` depends on it;
the delay calls `modulation::sync` instead of its own copies, unchanged in behaviour. There is no
graph LFO node or control port yet. The workspace grows to 19 crates; the `Cargo.lock` change is
only the new path crate, with no third-party dependency.

## Alternatives considered

- **LFOs in `dsp`.** `dsp` is the home for stateless and small stateful primitives; modulation
  sources will grow (envelopes, step sequences, a graph node's state) and should not swell it.
  The user asked for a separate crate.
- **LFOs private to `audio_effects`.** The cheapest now, but a future modular node or MIDI
  modulation would have to depend on the effect crate or copy the code.
- **Keep the sync arithmetic in `delay`.** The LFO would need a second copy of the feel factors,
  which could drift.

## Consequences

- A graph LFO node and control ports, deferred with MIDI in ADR-0020, can wrap `Lfo` without
  moving it.
- The delay's sync tests move with the functions and must pass unchanged; the delay's own tests
  still cover its use of them.
- One more crate to build and to list in `crate_rules.rs`.
