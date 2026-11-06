# ADR-0026 — The workspace is split into seventeen crates in five dependency tiers

- **Status:** Accepted
- **Decided by:** agent, unratified
- **Date:** 2026-11-06
- **Issue:** —
- **Architecture:** [Crate map](../architecture/crates.md#crate-map)

## Context

ADRs 0001–0025 fixed the engine, graph, project model, formats and platform seams, but not where
code lives. The architecture README's design principles require crates to map to a subsystem or a
family of similar modules, platform-dependent code kept apart from platform-independent code, and
no cycles between crates. The dependency policy treats the real-time core differently (default
**build**, no third-party crates).

## Decision

Seventeen crates in five tiers, each depending only on its own tier or earlier ones: real-time core
(`audio_buffers`, `dsp`, `audio_effects`, `signal_graph`, `audio_engine`), core (`audio_analysis`,
`project_model`, `wav_files`, `platform_interfaces`), I/O (`disk_streaming`, `project_storage`,
`import_export`), platform and UI (`macos_platform`, `debussy_ui`), and the `debussy` binary,
plus `realtime_guard` for tests and `xtask` for tooling.

## Alternatives considered

- **Few large crates** (engine, project, platform, app) — fewer manifests, but the real-time rules
  (`clippy.toml`, no dependencies) would cover code that does not need them, and changes to one
  subsystem would rebuild and retest unrelated ones.
- **Platform traits inside `audio_engine`** — saves a crate, but `import_export` needs the codec
  traits without the engine's real-time constraints, and the platform would depend on the engine.
- **Disk streaming inside `audio_engine`** — the engine would do file I/O, against the real-time
  rules; read-ahead stays in an I/O crate that feeds the engine through `audio_buffers` queues.
- **One crate per effect** — a flat peer set of similar modules belongs in one crate under the
  design principles.

## Consequences

- The tiers keep the real-time core free of I/O, platform code and third-party dependencies by
  construction.
- `debussy` is the composition root: the UI and engine see the platform only through
  `platform_interfaces`, so a second platform crate needs no changes elsewhere.
- Crates are created as work reaches them, not all at once. Each new crate's module list goes in
  its RustDoc.
- Tier membership is not yet checked by tooling; a `cargo xtask check` rule reading `cargo
  metadata` could enforce the dependency rules later.
- Decided by the agent; awaiting ratification in review.
