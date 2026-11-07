# ADR-0057 — The first binary imports and exports from the command line and plays through the default output

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-07
- **Issue:** #64
- **Architecture:** [Application startup](../architecture/user-interface.md#application-startup)

## Context

The developer asked for a runnable application to start reviewing the interface. #64's full
acceptance (record, import, mix, play, export from the window) needs recording (#67) and import,
export and routing controls the interface does not have yet. A native file dialog would be a new
dependency, and device-rate conversion (ADR-0006) is not built.

## Decision

- `debussy PROJECT.debussy` opens the project, creating it at `--rate` (default 48 kHz) if it is
  missing, and opens the window.
- `--import FILE` (repeatable) imports each file before the window opens, as a new track named
  after the file, holding one clip of the whole file at frame 0 and connected to the output.
- `--export FILE [--mono]` renders the whole project, from frame 0 to the end of the last clip,
  and exits. The extension picks the format: `.wav` 24-bit, `.flac` 24-bit, `.m4a`/`.aac` AAC at
  256 kbit/s.
- Waveform summaries missing or invalid in `cache/` are built at startup.
- Playback uses the default output device, opened output-only at the project rate with a
  128-frame buffer (or the nearest it allows). If the device cannot run at the project rate,
  startup fails with a message.
- #64 stays open until the window covers the brief's first-release use case.

## Alternatives considered

- **Waiting for in-window import and export:** delays the first review.
- **A file-dialog crate (such as `rfd`):** a dependency decision of its own; it belongs with the
  in-window import ticket.
- **Picking a device rate and converting:** needs the device-boundary resampler.

## Consequences

- The interface can be reviewed now, with audio prepared from the command line.
- No unreferenced-audio cleanup runs on close yet: `debussy_ui::run` keeps the session. The next
  open cleans up (ADR-0049).
