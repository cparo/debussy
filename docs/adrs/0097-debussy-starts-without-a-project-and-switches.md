# ADR-0097 — debussy starts without a project and switches projects by replacing the App

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-10
- **Issue:** —
- **Architecture:** [Application startup](../architecture/user-interface.md#application-startup)

## Context

`debussy` needed a project path on its command line: without one it printed the usage text and
no window opened, and the ≡ menu's New project and Open entries were disabled
([ADR-0060](0060-the-window-is-laid-out-as-top-bar-track-pane.md)) because the interface could
not switch projects. One `App` owns one session, bridge and engine; its `AppServices` owns one
bundle and an audio output opened at that project's sample rate. `debussy_ui` may not do I/O
(`docs/architecture/crates.md`), yet opening a project reads the bundle, builds waveform caches
and opens the audio device.

## Decision

- With no arguments, `debussy` opens a start screen offering New project (with a 44.1, 48, 88.2
  or 96 kHz picker, 48 kHz by default) and Open project. `--import` and `--export` need a project.
  This amends the startup in
  [ADR-0057](0057-the-first-binary-imports-and-exports-from-the.md), which otherwise stands.
- Switching projects replaces the whole `App`, with its services and audio output, rather than
  resetting one in place. The window's shell holds the `App` and does the swap.
- Opening and creating go through a `Launcher` trait in `debussy_ui`, implemented by `debussy`.
  It is split in two: `prepare` does everything that can fail while the current project still
  runs, and returns a `Start` closure, which cannot fail, that opens the audio output and builds
  the `App`. The current `App` is dropped between the two, so a failed open leaves the current
  project untouched and the two audio outputs are never open at once.
- New project refuses an existing path rather than overwriting or opening it. Open never creates.
- Switching needs the transport stopped, no track armed, and no take still being saved; the old
  project is saved first.

## Alternatives considered

- **Reset the `App` in place**: give `App` and `Services` a method that swaps the session,
  bridge, engine, bundle and output. It would have to reset dozens of fields of view state, and
  every new field would be another one to forget, and it would reopen the audio device at the
  new rate inside `Services`. Replacing the `App` gets a fresh state by construction.
- **Build the new `App` in one fallible call**: simpler, but the new audio output would have to
  open while the old one still runs, possibly on the same device at another rate, or the old
  project would have to close before the new one is known to load.
- **A sample rate chosen in the save dialog, or a fixed 48 kHz**: the native save dialog has no
  room for a picker, and a fixed rate would make the command line the only way to choose one.
- **Opening the existing project when New is given its path**: surprising; a New that silently
  opens something else hides a mistake.

## Consequences

- Command-line import and export remain until the interface's own import covers them, as
  ADR-0057 says.
- The window's title stays "Debussy"; the project's name shows in the top bar.
- No recent projects list yet; the start screen offers only New and Open.
- A project opened in the window whose rate the device cannot run shows "No audio output" in the
  status line, as at launch.
