# Crates

## Crate map

Every crate lives at `crates/<name>/`. Modules within each crate are listed in that crate's
RustDoc, not here ([ADR-0026](../adrs/0026-the-workspace-is-split-into-seventeen-crates-in.md)).

| Crate | Kind | Holds |
|---|---|---|
| `audio_buffers` | real-time core | Sample blocks, preallocated block pools, lock-free single-producer/single-consumer queues |
| `dsp` | real-time core | Resampling, FFT, dither, filter and oscillator primitives |
| `audio_effects` | real-time core | The built-in effect nodes, built from `dsp` |
| `signal_graph` | real-time core | The processing graph: nodes, typed ports, cycle rules, processing plans, delay compensation, panning and summing |
| `audio_engine` | real-time core | Running a processing plan each block: transport, clip playback from streamed buffers, recording capture, input monitoring, offline rendering |
| `audio_analysis` | core | Waveform summaries and spectrogram data |
| `project_model` | core | Tracks, clips, routing, edits as undoable commands, named versions |
| `wav_files` | core | WAV and RF64 reading and writing over `Read`, `Write` and `Seek` |
| `platform_interfaces` | core | The traits a platform implements: audio devices, compressed-audio codecs |
| `disk_streaming` | I/O | Read-ahead of project audio from disk into buffers for `audio_engine` |
| `project_storage` | I/O | The project bundle: `project.json`, versions, migrations, crash-safe saving, cleanup |
| `import_export` | I/O | Import (decode, resample, split into mono files) and export (render, dither, encode) |
| `macos_platform` | platform | `platform_interfaces` for macOS: `cpal` devices and AudioToolbox codecs |
| `debussy_ui` | UI | The `iced` user interface |
| `debussy` | binary | The application: wires the platform, engine, storage and UI together |
| `realtime_guard` | test support | The real-time guard (see `docs/architecture/engine.md` "Real-time thread") |
| `xtask` | tooling | Repository tasks (see `docs/architecture/tooling.md` "Repository tasks (xtask)") |

## Dependency rules

- A crate may depend only on crates of its own kind or of a kind above it in this order: real-time
  core, core, I/O, platform and UI, binary. Within a kind, dependencies must not form a cycle.
- Real-time core crates must depend on no third-party crate other than `thiserror`.
- Only `macos_platform` may contain platform-specific code or depend on `cpal` or Apple
  frameworks. Only `debussy` may depend on `macos_platform`.
- `debussy_ui` must not depend on I/O or platform crates; it reaches them through interfaces that
  `debussy` supplies.
- Only `project_storage` may depend on `serde` and `serde_json` ([ADR-0048](../adrs/0048-project-files-are-serialized-with-serde-and-serde.md)).
- `realtime_guard` may be used only as a dev-dependency.
- Each real-time core crate carries the `clippy.toml` required by `docs/architecture/engine.md`
  "Real-time thread".
- `unsafe` code is allowed only in `macos_platform` and `realtime_guard`. Every other crate sets
  `#![forbid(unsafe_code)]` ([ADR-0026](../adrs/0026-the-workspace-is-split-into-seventeen-crates-in.md)).
- In `macos_platform`, `unsafe` code is confined to its `sys` module; the crate sets
  `#![deny(unsafe_code)]` and only `sys` allows it ([ADR-0051](../adrs/0051-audiotoolbox-codecs-use-objc2-bindings-unsafe.md)).
- The one exception is `audio_buffers`' queue module: the crate sets `#![deny(unsafe_code)]` and
  only that module allows it ([ADR-0028](../adrs/0028-unsafe-is-allowed-in-audio-buffers-queue-module.md)).
