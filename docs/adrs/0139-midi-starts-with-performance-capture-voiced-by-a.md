# ADR-0139 — MIDI starts with performance capture voiced by a built-in synth, a sampler or external hardware

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-14
- **Issue:** #20
- **Architecture:** [Instruments](../architecture/signal-graph.md#instruments)

## Context

The brief lists MIDI, synthesis and sampling last, for a user who records live performances and is
not a producer of programmed or sample-based music. ADR-0020 typed ports so MIDI could be added
later, but left the scope open. That scope decides which sound sources, editing tools and platform
code get built.

## Decision

MIDI serves performance capture first: a keyboardist or e-drummer plays live, hears an instrument,
and the notes are recorded as clips. Clips get clip-level editing and quantize to the bars/beats
grid, with no per-note editing in the first cut. Notes are voiced by a built-in subtractive synth,
a sampler playing the user's own WAV files, or external hardware through MIDI out (notes and CC
with a per-port latency offset, no clock). Third-party plug-ins are not hosted. Basic sequencing
stays open as a later extension.

The work is #169 (event ports), #170 (MIDI clips), #171 (CoreMIDI), #172 (clip playback), #173
(synth), #174 (sampler), #175 (recording and monitoring), #176 (MIDI out) and #177 (clip editing).

## Alternatives considered

- **Capture plus programming** (full piano roll, step input) — serves producers of programmed
  music, whom the brief puts out of scope for now.
- **Controllers only** — no notes or instruments, so no use for a keyboardist or e-drummer.
- **Audio Unit hosting** — the largest choice of sounds, but macOS-only, a large untrusted-code
  surface, and crash isolation to build.
- **Bundled samples or SFZ/SF2 import** — sounds out of the box, but brings an asset-licence review
  or a new untrusted-input parser.

## Consequences

- Without plug-ins, the instruments are only as good as the synth and sampler we build.
- Still not ticketed: per-note editing and step input, MIDI learn, Standard MIDI File import and
  export, MIDI clock, bundled or SFZ/SF2 samples, Audio Unit hosting, MPE and MIDI 2.0.
