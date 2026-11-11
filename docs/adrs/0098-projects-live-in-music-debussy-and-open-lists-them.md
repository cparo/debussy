# ADR-0098 — Projects live in ~/Music/Debussy and Open lists them in the window

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-10
- **Issue:** —
- **Architecture:** [Projects folder](../architecture/project.md#projects-folder)

## Context

The New and Open dialogs added in
[ADR-0097](0097-debussy-starts-without-a-project-and-switches.md) start wherever the system dialog
last was, so every New and Open began with finding the projects again. The settings path was also
worked out in `debussy/src/main.rs`, a macOS path outside the platform crate
(`docs/architecture/crates.md`). Separately, the user wants a project to travel as one file,
which the bundle directory ([ADR-0013](0013-a-project-is-a-bundle-directory-with-versioned.md))
does not do until macOS knows the type.

## Decision

- New projects go in `~/Music/Debussy` by default; the save dialog opens there, created if
  missing, with the first free `Untitled.debussy`, `Untitled 2.debussy`, … suggested.
- Open (⌘O, and the start screen) shows an in-window list of the projects in that folder, newest
  first, with ages in words worked out from a duration ("5 min ago"), and Browse… for a project
  anywhere else through the folder dialog. A listed project is a `*.debussy` directory directly in
  the folder holding a `project.json`; its age is that file's modification time; the list stops at
  1000.
- No recents list or other new persisted state.
- The folder lookups (projects folder, settings file) live in `macos_platform::folders`.
- The working format stays the bundle directory. Declaring it as a macOS package and a
  single-file archive for sharing are separate work.

## Alternatives considered

- **`~/Documents/Debussy`** — "Desktop & Documents" in iCloud Drive sweeps it up, and macOS may
  evict a project's audio to the cloud, so playback and recording would stall on downloads.
  `~/Music` is where Logic and GarageBand keep projects and is not synced.
- **Remember the last folder used** — new persisted state for little gain over a fixed default,
  and the projects still scatter.
- **A recents list** — persisted state (a settings format change) that goes stale as projects move
  or are deleted; listing the folder is always current.
- **Keep the native Open dialog only** — still a dialog to navigate each time, and it shows
  bundles as folders with no ages. Browse… keeps it for anything outside the folder.
- **Make projects single files now** — a zip working format would break streaming playback,
  recording to disk and save-after-every-edit
  ([ADR-0067](0067-the-project-is-saved-after-every-edit.md)); the single-file need is met at
  the edges instead.

## Consequences

- Projects saved outside `~/Music/Debussy` are not listed and need Browse… each time.
- The list reads the folder each time the browser or start screen opens; a folder of thousands of
  projects shows only the newest 1000.
- Follow-ups: declare the bundle as a macOS package so Finder, AirDrop and Mail treat a project as
  one file ([#128](https://github.com/cparo/debussy/issues/128)); share and open a project as a
  single archive ([#129](https://github.com/cparo/debussy/issues/129)).
