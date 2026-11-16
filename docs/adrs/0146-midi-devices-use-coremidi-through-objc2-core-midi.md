# ADR-0146 — MIDI devices use CoreMIDI through objc2-core-midi, with validated, bounded input stamped one block late

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-15
- **Issue:** #171
- **Architecture:** [MIDI devices](../architecture/platform.md#midi-devices)

## Context

Issue #171, part of the MIDI scope (ADR-0139), needs MIDI endpoints listed, hot-plug followed,
time-stamped input received and output sent, behind a `platform_interfaces` trait with the macOS
implementation in `macos_platform`. Live input has to reach the audio thread without locks and be
placed at frames in a block. Both a new dependency and a new source of untrusted input were raised
with `flag-risk` and answered by Chris Paro on 2026-11-15.

## Decision

- **Bindings:** CoreMIDI is reached through `objc2-core-midi` 0.3.2 (Zlib OR Apache-2.0 OR MIT),
  from the same `objc2` family as the AudioToolbox bindings (ADR-0051), adding no transitive
  crates. `objc2-core-audio` becomes a direct dependency for the host-time functions. The new
  `unsafe` code stays in `macos_platform::sys`, every block with a `// SAFETY:` comment.
- **Client:** one CoreMIDI client per process, never disposed, made on a dedicated thread running a
  `CFRunLoop`, so that notifications do not depend on the UI's run loop. Ports use the UMP MIDI 1.0
  protocol, so the system converts MIDI 2.0 devices and the parser sees one format.
- **Identity:** an endpoint is named by its `kMIDIPropertyUniqueID`, so a replugged device
  keeps its name. Inputs reconnect on the setup notification; outputs look their endpoint up on
  each send.
- **Input validation:** only channel voice messages that `midi::Message` accepts are kept; SysEx,
  system, utility and MIDI 2.0 messages are dropped. Each input queues at most 1024 messages, drops
  the overflow, and counts every drop. Timestamps are clamped when stamped. No input panics.
- **Stamping:** input is stamped one block late, mapping the block before the callback's host time
  onto the block, so that the spacing between messages survives. Audio callbacks get their host
  time from `cpal`'s stream instant, which on macOS counts host-clock nanoseconds, so this needs no
  new `unsafe` in the stream code.

## Alternatives considered

- **`midir`** — portable, but it hides timestamps behind its own clock, has no hot-plug
  notifications, and would bring a second set of CoreMIDI bindings.
- **The `coremidi` crate** — a safe wrapper, but built on its own `coremidi-sys` bindings beside
  the `objc2` ones we already have, and slower to follow new CoreMIDI APIs.
- **Hand-written FFI** — no dependency, but every declaration would be ours to get right, with no
  gain over generated bindings under an allowed licence.
- **A per-port rate limit** instead of a fixed queue bound — protects against floods too, but needs
  a clock and tuning; the bound already caps memory and work per block.
- **Passing system messages** (clock, start, stop) through — wanted later for sync, but nothing
  consumes them yet; they can be admitted by a later decision.
- **Notifications on the main run loop** — would stop whenever the UI is busy or absent, as in
  tests and command-line tools.
- **Stamping each message at frame 0, or as soon as it arrives** — adds up to one block of jitter
  where the one-block delay adds a constant one block of latency.

## Consequences

- Live MIDI input has one block of added latency, constant, on top of the audio buffer's.
- System real-time messages are unavailable until a later decision admits them; MIDI clock sync
  needs one.
- The virtual endpoints used by tests exist only in `macos_platform`'s unit tests.
- Recording and monitoring live input are #175; sending MIDI from tracks, with the latency offset
  the scheduled `send` allows for, is #176.
