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
| [ADR-0004](0004-mp3-and-aac-go-through-apple-audiotoolbox.md) | MP3 and AAC go through Apple AudioToolbox | Accepted | [Patent-encumbered codecs](../architecture/platform.md#patent-encumbered-codecs) |
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

