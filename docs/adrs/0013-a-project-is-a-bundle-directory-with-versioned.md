# ADR-0013 — A project is a bundle directory with versioned JSON metadata

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Project bundle](../architecture/project.md#project-bundle)

## Context

A project holds metadata (tracks, clips, mixer and effect settings, named versions), audio files,
and regenerable caches. This is a persisted format, so it was put to the developer as a `product`
risk before any user data exists.

## Decision

A project is a `<name>.debussy` bundle directory holding `project.json` (with an integer
`format_version`), `versions/`, `audio/` and `cache/`. Readers refuse newer format versions and
migrate older ones.

## Alternatives considered

- **Bundle with TOML metadata** — friendlier to edit by hand, but verbose for nested clip lists,
  and nobody is expected to hand-edit projects.
- **A single SQLite file** — gives atomic transactions, but it is not diffable, the audio would sit
  inside a database or awkwardly beside it, and the SQLite C library is a heavy dependency.

## Consequences

- Metadata is human-readable and diffable, which helps debugging and bug reports.
- Saving must be crash-safe without transactions: write a temporary file, then rename it over
  `project.json`.
- JSON needs `serde` and `serde_json`. Each is a separate buy/build decision, made when the crate
  that saves projects is created.
- The bundle needs a document type declared in the app's `Info.plist`.
