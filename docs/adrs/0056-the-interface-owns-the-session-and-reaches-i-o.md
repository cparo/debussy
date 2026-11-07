# ADR-0056 — The interface owns the session and reaches I/O through a Services trait

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #55
- **Architecture:** [Interface structure](../architecture/user-interface.md#interface-structure)
  ([Gestures](../architecture/user-interface.md#gestures))

## Context

#55 asked which screens come first, how `debussy_ui` reaches I/O when it may not depend on I/O or
platform crates (`docs/architecture/crates.md` "Dependency rules"), and how gestures map to undo
steps (ADR-0037, ADR-0043). The first two were put to the developer; the third follows from
those ADRs.

## Decision

- **First cut:** the app shell, the transport bar, the track list with waveform lanes, and the
  mixer. The patch board and the spectrogram view follow in their own tickets (#73, #74).
- **Ownership and seam:** `debussy_ui`'s `App` owns the `Session`, the `engine_bridge::Bridge`
  and the `audio_engine::Engine` control side. Everything else goes through a `Services` trait
  that `debussy` implements: track feeds from disk streams, moving the streams, waveform
  summaries, saving. `App::update` needs no window, so it is tested with a fake `Services`.
- **Gestures:** a slider sends its value to the running parameter while dragged
  (`Bridge::parameter`) and records one `set_parameter` step on release. Mute and solo are one
  step each. Undo and redo are refused while playing, since clip edits wait for the transport to
  stop (ADR-0046).
- **Track feeds** are rebuilt on play only when the tracks changed since they were last built.
- **Waveform lanes** are 1024 columns per track, computed when the view or the project changes,
  from the stored files' summaries. Zoom runs from 64 frames per column (the summaries' finest)
  to 2^24. Where clips overlap the top one shows, scaled by its gain, and each clip's columns
  start at its first visible frame, so they may sit up to a column off the timeline grid.

## Alternatives considered

- **A controller in `debussy` owning the session and sending view state back:** a cleaner split,
  but every interaction becomes a message round trip and the UI cannot read the model directly.
- **Everything in the ticket at once:** one very large PR; the patch board is a design of its own.
- **Recording every slider step:** floods the undo history, against ADR-0037.

## Consequences

- `debussy` (#64) must implement `Services` and run `debussy_ui::run(App)`.
- The UI has not yet been opened against a real project; that happens once #64 wires it up.
