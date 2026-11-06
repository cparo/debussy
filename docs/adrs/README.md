# Architectural Decision Records

This directory records **why** a concrete architectural decision was made: the alternatives
considered, the trade-offs accepted, and the findings that forced a choice. It complements, and is
deliberately separate from, the [architectural specs](../architecture/README.md) which answer
**how** the system is built and link back to these ADRs rather than restating their reasoning.

## Conventions

- **One file per decision**, at `NNNN-slug.md`, numbered zero-padded and monotonically increasing.
  Numbers are **never reused**. New records append to the end of the sequence regardless of which
  architecture section they support.
- Start new records with `cargo xtask new-adr "<decision>" --decided-by <value>`, which fills
  [`0000-template.md`](0000-template.md), takes the next number, and adds the index row.
- **Decided by** is the name of the person who decided, `agent, ratified by <who>`,
  or `agent, unratified`: so it is clear who signed off on each decision.
  `cargo xtask new-adr --decided-by human` fills in the name from `git config user.name`.
- **Status** is `Accepted` or `Superseded by ADR-NNNN`. A record is never deleted or edited to
  reverse its own decision — write a new record and mark the old one superseded.
- Link related records with `[ADR-NNNN](NNNN-slug.md)`; link back to the section under
  [`docs/architecture/`](../architecture/README.md) that the decision supports.

## Index

| ADR | Title | Status | Architecture section |
|---|---|---|---|
| [ADR-0001](0001-debussy-is-licensed-mit-or-apache-2-0-and-accepts.md) | Debussy is licensed MIT OR Apache-2.0 and accepts only permissively licensed dependencies | Accepted | [Project licence](../architecture/licensing.md#project-licence) |
| [ADR-0002](0002-repository-tasks-are-built-in-house-on-std-and.md) | Repository tasks are built in-house on std and anyhow | Accepted | [Repository tasks (xtask)](../architecture/tooling.md#repository-tasks-xtask) |
| [ADR-0003](0003-the-user-interface-is-built-with-iced.md) | The user interface is built with iced | Accepted | [UI toolkit](../architecture/user-interface.md#ui-toolkit) |
| [ADR-0004](0004-mp3-and-aac-go-through-apple-audiotoolbox.md) | MP3 and AAC go through Apple AudioToolbox | Superseded by ADR-0024 | [Compressed audio codecs](../architecture/platform.md#compressed-audio-codecs) |
| [ADR-0005](0005-the-engine-processes-audio-as-32-bit-float.md) | The engine processes audio as 32-bit float throughout | Accepted | [Sample format](../architecture/engine.md#sample-format) |
| [ADR-0006](0006-each-project-has-one-fixed-sample-rate.md) | Each project has one fixed sample rate | Accepted | [Sample rate](../architecture/engine.md#sample-rate) |
| [ADR-0007](0007-the-engine-targets-a-128-frame-default-buffer.md) | The engine targets a 128-frame default buffer | Accepted | [Buffer size](../architecture/engine.md#buffer-size) |
| [ADR-0008](0008-project-audio-is-streamed-from-disk.md) | Project audio is streamed from disk | Accepted | [Audio storage](../architecture/engine.md#audio-storage) |
| [ADR-0009](0009-audio-device-i-o-uses-cpal-behind-our-own.md) | Audio device I/O uses cpal behind our own interface | Accepted | [Audio device I/O](../architecture/platform.md#audio-device-io) |
| [ADR-0010](0010-real-time-code-is-checked-by-an-allocator-guard.md) | Real-time code is checked by an allocator guard and clippy bans | Accepted | [Real-time thread](../architecture/engine.md#real-time-thread) |
| [ADR-0011](0011-editing-is-non-destructive-over-immutable-audio.md) | Editing is non-destructive over immutable audio files | Accepted | [Editing model](../architecture/project.md#editing-model) |
| [ADR-0012](0012-undo-is-unlimited-per-session-and-named-versions.md) | Undo is unlimited per session and named versions persist earlier states | Accepted | [Undo history](../architecture/project.md#undo-history) |
| [ADR-0013](0013-a-project-is-a-bundle-directory-with-versioned.md) | A project is a bundle directory with versioned JSON metadata | Accepted | [Project bundle](../architecture/project.md#project-bundle) |
| [ADR-0014](0014-project-audio-is-stored-as-mono-32-bit-float-wav.md) | Project audio is stored as mono 32-bit float WAV | Accepted | [Stored audio](../architecture/project.md#stored-audio) |
| [ADR-0015](0015-routing-is-one-general-processing-graph-per.md) | Routing is one general processing graph per project | Accepted | [Processing graph](../architecture/signal-graph.md#processing-graph) |
| [ADR-0016](0016-feedback-loops-must-pass-through-a-fixed-delay.md) | Feedback loops must pass through a fixed-delay node | Accepted | [Feedback loops](../architecture/signal-graph.md#feedback-loops) |
| [ADR-0017](0017-signals-are-mono-or-stereo-only.md) | Signals are mono or stereo only | Superseded by ADR-0030 | [Channel layouts](../architecture/signal-graph.md#channel-layouts) |
| [ADR-0018](0018-panning-uses-a-3-db-constant-power-law.md) | Panning uses a -3 dB constant-power law | Accepted | [Panning](../architecture/signal-graph.md#panning) |
| [ADR-0019](0019-only-built-in-effects-for-now-behind-a-plugin.md) | Only built-in effects for now, behind a plugin-capable node interface | Accepted | [Effect nodes](../architecture/signal-graph.md#effect-nodes) |
| [ADR-0020](0020-ports-and-track-contents-are-typed-so-midi-can-be.md) | Ports and track contents are typed so MIDI can be added later | Accepted | [Port types](../architecture/signal-graph.md#port-types) |
| [ADR-0021](0021-inputs-can-be-monitored-through-the-software-per.md) | Inputs can be monitored through the software per track | Accepted | [Input monitoring](../architecture/recording.md#input-monitoring) |
| [ADR-0022](0022-the-engine-compensates-recording-and-effect.md) | The engine compensates recording and effect latency automatically | Accepted | [Recording alignment](../architecture/recording.md#recording-alignment) |
| [ADR-0023](0023-expired-or-royalty-free-patents-may-be-implemented.md) | Expired or royalty-free patents may be implemented; others need a licensing SDK or review | Accepted | [Patent policy](../architecture/patents.md#patent-policy) |
| [ADR-0024](0024-compressed-codecs-go-through-audiotoolbox-and-mp3.md) | Compressed codecs go through AudioToolbox and MP3 export is deferred | Accepted | [Compressed audio codecs](../architecture/platform.md#compressed-audio-codecs) |
| [ADR-0025](0025-wav-is-built-in-house-and-integer-exports-are.md) | WAV is built in-house and integer exports are dithered | Accepted | [WAV files](../architecture/import-export.md#wav-files) |
| [ADR-0026](0026-the-workspace-is-split-into-seventeen-crates-in.md) | The workspace is split into seventeen crates in five dependency tiers | Accepted | [Crate map](../architecture/crates.md#crate-map) |
| [ADR-0027](0027-the-commit-guard-parses-shell-commands-with-python.md) | The commit guard parses shell commands with Python's standard library | Accepted | [Commit gate](../architecture/tooling.md#commit-gate) |
| [ADR-0028](0028-unsafe-is-allowed-in-audio-buffers-queue-module.md) | `unsafe` is allowed in audio_buffers' queue module only | Accepted | [Dependency rules](../architecture/crates.md#dependency-rules) |
| [ADR-0029](0029-the-wav-reader-accepts-only-mono-and-stereo-files.md) | The WAV reader accepts only mono and stereo files and bounds every size it reads | Accepted | [WAV files](../architecture/import-export.md#wav-files) |
| [ADR-0030](0030-signals-are-mono-or-stereo-and-the-mono-mix-is-the.md) | Signals are mono or stereo, and the mono mix is the channel sum scaled by -3 dB | Accepted | [Channel layouts](../architecture/signal-graph.md#channel-layouts) |
| [ADR-0031](0031-feedback-delays-are-at-least-1-ms.md) | Feedback delays are at least 1 ms | Accepted | [Feedback loops](../architecture/signal-graph.md#feedback-loops) |
| [ADR-0032](0032-monitored-input-is-not-delayed-for-delay.md) | Monitored input is not delayed for delay compensation | Accepted | [Input monitoring](../architecture/recording.md#input-monitoring) |

