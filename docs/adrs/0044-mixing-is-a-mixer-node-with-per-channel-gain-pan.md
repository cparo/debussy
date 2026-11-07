# ADR-0044 — Mixing is a mixer node with per-channel gain, pan and a mute/solo mode

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #48
- **Architecture:** [Mixer](../architecture/signal-graph.md#mixer)


## Context

Issue #48 planned gain, pan and mute as separate nodes, following
[ADR-0043](0043-routing-is-undoable-project-state-tracks-are-bare.md)'s "gain, pan and mute are
ordinary nodes". The brief asks for mute and solo per channel in a mixer, and solo needs a scope:
a set of channels it isolates one from. Separate nodes have no such scope. The design was put to
the developer.

## Decision

- **A mixer is one node.** The user places as many as the project needs. Each has one or more
  channels, each with one mono input and its own gain, pan and mute/solo, and one stereo output.
  This replaces ADR-0043's clause that gain, pan and mute are separate nodes; tracks stay bare
  sources and the rest of ADR-0043 stands.
- **Channels are added, never removed.** A channel's index is its port index and its parameter
  names, so a larger mixer replaces the node by [ADR-0041](0041-replacing-a-node-keeps-its-id-compatible.md)
  and keeps every connection and value. Removal is left to a later record.
- **Mute and solo are one three-state parameter**, `channel N mode`: 0 plays, 1 is muted, 2 is
  soloed. Setting one clears the other, and a saved project cannot hold both.
- **Solo is scoped to its mixer**: while any of a mixer's channels is soloed, only its soloed
  channels play. Mixers do not affect each other.
- **Gain is in dB, −96 to +12**, default 0, with −96 as silence.
- **The output is always stereo.** A mono mix is the output connected to a mono input, folded down
  by [ADR-0030](0030-signals-are-mono-or-stereo-and-the-mono-mix-is-the.md).
- **In the saved routing a mixer is its own node kind** with a channel count and parameter values
  by name, like buses and feedback delays, so the model checks its ports.
- **New tracks still connect straight to the output** (ADR-0043); a template that routes them into
  a mixer can come later.

## Alternatives considered

- **Separate gain, pan and mute nodes** (the original #48 plan) — solo has no scope to act in, and
  a typical channel would take three nodes.
- **Solo across the whole project** — would silence paths the user never meant to touch, such as
  a cue mix built in a second mixer.
- **Separate 0/1 `mute` and `solo` parameters** — parameters are independent atomics, so two
  writes could leave both set; the node would need a tie-break and the saved form an extra check.
- **Removing any channel and renumbering** — every later channel's connections and values would
  have to be remapped in the edit.
- **Linear gain** — the UI would convert for every display, and fader travel would not match dB.
- **The mixer as an effect kind id with a `channels` parameter** — a channel count is structural,
  not a live value, and the model could not check its ports.
- **New tracks routed into a mixer automatically** — deferred, to keep ADR-0043's behaviour until a
  default layout is designed.

## Consequences

- The parameter names `channel N gain`, `channel N pan` and `channel N mode`, and the mode values,
  are part of the saved format.
- Without an effect kind id, the kind-id registry and the unknown-kind placeholder move to the
  first built-in effect.
- A mixer channel cannot be deleted; an unused one stays, unconnected and silent.
