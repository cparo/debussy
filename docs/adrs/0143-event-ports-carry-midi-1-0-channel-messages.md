# ADR-0143 — Event ports carry MIDI 1.0 channel messages stamped with frame offsets in fixed-capacity per-port buffers

- **Status:** Accepted
- **Decided by:** agent, ratified by Chris Paro
- **Date:** 2026-11-15
- **Issue:** #169
- **Architecture:** [Port types](../architecture/signal-graph.md#port-types)

## Context

Roadmap step 36 (ADR-0139) adds MIDI: clips, CoreMIDI input and output, a synthesizer, a sampler and
recording all move notes through the processing graph. ADR-0020 typed ports so that this could be
added, but the graph had only audio and control ports. Notes must reach instruments on the real-time
thread, which may not allocate (`docs/architecture/engine.md` "Real-time thread"), with sample
accuracy inside a block, and must stay aligned with audio when delay compensation delays a parallel
path (ADR-0022, ADR-0136).

## Decision

- An event port carries MIDI 1.0 channel messages (status `0x80`–`0xEF` with one or two data bytes
  of at most `0x7F`), each stamped with a `u32` frame offset into the block. System messages and
  running status are not represented; a message is validated when made and is three bytes.
- Message and buffer types live in a new real-time core crate, `midi`, with no dependencies.
- Every event port has a buffer of `EVENT_CAPACITY` = 512 events per block, allocated with the plan
  and kept in frame order; it is emptied at the start of each block.
- Several connections into one event input are merged in frame order; events at the same frame keep
  the order of the connections' ids, and within one connection the order they were sent in.
- An event connection is compensated like an audio one, by adding its compensation to each event's
  frame. Events still in flight wait in a queue of 2048 events (4 × `EVENT_CAPACITY`) on that
  connection; an uncompensated connection has no queue. Queues start empty in each new plan, as
  compensation delays do (ADR-0040), and a reset empties them.
- An event a full buffer or queue cannot take is dropped (the incoming one, not an older one) and counted in one counter
  per graph (`Graph::event_overflows`). Nothing allocates to make room.
- Feedback delays stay audio-only, so a cycle of event connections alone is refused.
- A plan's buffers are indexed by port index; a control port has an empty slot.

## Alternatives considered

- **MIDI 2.0 / UMP packets** — wider values and per-note controllers, but no CoreMIDI device or
  clip in scope needs them (ADR-0139), and every consumer would pay to translate them. A later
  widening can add a new message type behind the same buffer.
- **Raw byte streams per port** — would carry SysEx and running status, but every node would have
  to parse, and no first-release feature needs system messages.
- **Growable buffers** — no fixed limit, but growth allocates on the real-time thread.
- **A smaller capacity (for example 128)** — DIN MIDI carries about 31 messages in 10 ms, so 512 is
  far above any hardware stream at the 1024-frame largest block; the cost is 4 KB per port.
- **Unbounded or latency-sized compensation queues** — never drop, but the size would scale with the
  reported latency and the smallest block size; a fixed 2048 covers four full blocks in flight.
- **Event types inside `signal_graph`** — fewer crates, but clip storage, CoreMIDI and the synth need
  the message type without the graph.
- **Building the MIDI types on a crate** — no crate is needed for a three-byte validated message;
  the real-time core defaults to build.

## Consequences

- `Cargo.lock` gains only the in-workspace `midi` crate; no third-party dependency changes.
- System exclusive, clock and other system messages cannot pass through the graph; MIDI clock or
  SysEx support would need a new decision.
- A burst over 512 events per port per block is lost, and visible only through the counter.
- The per-port index fixes plans for nodes that mix control ports with audio ports.
- The rest of step 36 (#170–#177) builds on these ports: clips, CoreMIDI, playback, synth, sampler,
  recording and MIDI out.
