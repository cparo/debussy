# ADR-0059 — The window imports, exports and picks one audio device, remembered in app settings

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #64
- **Architecture:** [Files and devices](../architecture/user-interface.md#files-and-devices)
  ([Application settings](../architecture/project.md#application-settings))

## Context

#64 needs import, export and device selection in the window (ADR-0057 put import and export on
the command line for the first cut). The file-dialog dependency (`flag-risk`, tag `license`),
how devices are chosen, and whether the choice is remembered (a new persisted format) were put to
the developer.

## Decision

- **Dialogs:** `rfd` 0.17.2 (MIT), with default features off, in `debussy` only, behind
  `debussy_ui::Services`. It adds three lock-file entries (`rfd`, `percent-encoding` and another
  `objc2-app-kit` version); the rest is already in the tree through `iced`. 0.18 needs Rust 1.92.
- **Import and export in the window:** an import button picks files, imports them and builds
  their waveform summaries; each file becomes a track with one clip at the start, as one undo
  step. Export and export mono pick a file and render the project; the extension picks the format
  as on the command line. The command-line options stay.
- **One device** for input and output, since they must share a clock (ADR-0036). Chosen in the
  transport bar while stopped.
- **Settings file:** `~/Library/Application Support/Debussy/settings.json`, format version 1,
  holding the device id. Read as untrusted input (64 KB limit, unknown fields refused); anything
  wrong gives the defaults. Written crash-safely. It lives in `project_storage`, the only crate
  that may use serde (ADR-0048).
- **Switching** (the agent's design, within the developer's choice): the real-time callback
  receives the engine's `Processor` over a lock-free queue only after its stream opens, and hands
  it back over another when asked, so the engine is not rebuilt and a device that fails to open
  never takes it. If the new device fails, the old one is reopened.
- **Errors:** a stream error (an unplugged device) stops playback and is reported on the status
  line; a failure to open any device at launch still opens the window, with a notice.

## Alternatives considered

- **Hand-written `NSOpenPanel` code, or typed paths:** more `unsafe` code, or a poor experience.
- **Separate input and output pickers:** would need clock-drift correction.
- **Not remembering the device:** the user would pick it again at every launch.

## Consequences

- Export runs on the UI thread; a long export holds the window until it finishes.
- Recording (#67) will open the chosen device's inputs as well.
