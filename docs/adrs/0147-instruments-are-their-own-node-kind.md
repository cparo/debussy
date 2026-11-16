# ADR-0147 — Instruments are their own node kind

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-16
- **Issue:** #173
- **Architecture:** [Instruments](../architecture/signal-graph.md#instruments)

## Context

Issue #173 adds the first instrument, a node that turns a MIDI track's events into stereo audio
(ADR-0139, ADR-0145). The model refused every connection from a MIDI track: effect ports are not
known to the model (ADR-0043), so it could not tell an effect's input from one that takes events,
and no node kind it knew had an event input. The model has to know which inputs take events to
refuse a MIDI track wired into an audio input, and the saved file has to say what a node is.

## Decision

- An instrument is a new node kind, `NodeKind::Instrument { kind, parameters }`: a stable kind id
  and its non-default parameter values by name, as an effect's are. Every instrument has event
  input 0 and stereo audio output 0, and the model knows these ports.
- Built-in instruments live in a new real-time core crate, `instruments`, with their own registry
  (`INSTRUMENTS`, `instrument`, `display_name`, `parameter_infos`) beside the effects' registry in
  `audio_effects`.
- An instrument kind id this build does not know is kept and plays as silence, through a stand-in
  with an instrument's ports.
- Saved format version 8 adds `{"kind":"instrument","instrument":…,"parameters":{…}}`. Version 7
  files migrate unchanged; a build that knows only version 7 refuses a version 8 file with its
  existing "newer version" error.
- A side of an event port is refused, as a side of a mono port is.

## Alternatives considered

- **An instrument as an `Effect` with a kind id from a shared registry.** No format change, but
  the model would have to build the node to learn whether input 0 takes events, and could not do
  so for an unknown kind id: a saved MIDI connection to an instrument this build does not know
  would fail to load. A separate kind keeps the ports in the model.
- **Instruments in `audio_effects`.** One crate fewer, but it mixes nodes that take audio with
  nodes that make it, and the voice, envelope and oscillator code instruments need is of no use to
  effects. A crate of its own keeps each registry to one meaning.
- **A generic "ports" description saved with every node.** It would let the model check any node
  kind's ports, but it duplicates in every file what the code already knows, and a saved
  description could disagree with the node the build makes.

## Consequences

- Older builds cannot open a project saved after this change, even one with no instrument, because
  every save writes version 8. That is the cost of every format bump so far.
- The interface draws an instrument's event input as one pin and allows only a MIDI track's output
  to reach it; a MIDI track still connects to nothing else.
- The first instrument, the synthesizer, is registered in a follow-up (#173); the sampler of
  ADR-0139 joins the same registry.
