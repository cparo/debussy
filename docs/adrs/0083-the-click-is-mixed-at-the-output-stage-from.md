# ADR-0083 — The click is mixed at the output stage from bundled CC0 samples

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** #57
- **Architecture:** [Click track](../architecture/recording.md#click-track)

## Context

#57 asks for a click to record against. The click must be heard with the playback and line up
with takes (ADR-0022, ADR-0078), must never reach an export, and its sound needs a source we can
ship. The developer settled the routing, the sounds and where the preferences live.

## Decision

- The click is added to L and R in the engine's output stage, after the plan and after the mono
  fold, while playing with the click on. It is not a graph node: it is never on the patch board,
  exported or offline-rendered.
- Its timeline position is the playhead minus the plan latency, the position being heard, so a
  take placed by ADR-0078 lines up with the click the performer heard.
- It is stateless: each output frame sums the sound started at every beat whose start lies within
  the sound's length before that frame. Seeks and block sizes cannot change it.
- The downbeat plays at the click level, other beats at −6 dB.
- The sounds are three CC0 samples, Block, Click and Stick, supplied by Chris Paro, in
  `crates/engine_bridge/assets/clicks/` with a README recording their licence, compiled in with
  `include_bytes!`. `engine_bridge` decodes them with `wav_files`, folds to mono with
  `dsp::FOLD_DOWN_GAIN`, converts to the project rate with `dsp`'s import preset, normalises to
  −1 dBFS and trims below −60 dBFS. The engine gets an `Arc<[f32]>`, and a replaced click goes
  back over the retire queue to be dropped off the real-time thread.
- Click on/off is session state, like mono. The sound and level (−24 to +6 dB, default 0) are
  per-machine preferences in `settings.json` format 2; a version 1 file keeps its device and takes
  the defaults.
- `engine_bridge` gains a path dependency on the in-house `wav_files`; no third-party crate is
  added. The developer approved this and the settings format change through `flag-risk`
  (`license`, `product`).

## Alternatives considered

- **A click track node in the graph** — would appear on the patch board, could be routed into an
  export, and would need hiding everywhere the graph is shown.
- **Synthesised clicks** — no assets to license, but the developer preferred recorded sounds.
- **Clicking at the playhead itself** — the click would lead the heard playback by the plan
  latency and takes would land late against it.
- **A stateful click that counts frames since the last beat** — a seek or a dropped block would
  have to reset it, and it could drift from the grid.
- **Decoding the samples in `debussy`** — keeps `Cargo.lock` unchanged but threads PCM through
  the services interface for no gain.
- **Saving the sound and level in the project** — they suit the player's ears and room, not the
  song.

## Consequences

- Older builds read a version 2 settings file as invalid and use their defaults.
- The click is heard only from the main outputs; a separate click output for headphones is not
  built.
- Sample preparation runs on the interface thread when the engine is built or the sound changes:
  a few milliseconds.
