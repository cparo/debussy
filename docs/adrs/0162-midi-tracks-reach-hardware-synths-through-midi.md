# ADR-0162 — MIDI tracks reach hardware synths through MIDI output nodes, sent ahead by a per-port latency

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-17
- **Issue:** #176
- **Architecture:** [MIDI output](../architecture/engine.md#midi-output)
  ([Routing](../architecture/project.md#routing),
  [Application settings](../architecture/settings.md#application-settings),
  [Instrument choice](../architecture/track-pane.md#instrument-choice))

## Context

ADR-0139 decided that notes may be voiced by external hardware through MIDI out: notes and
controllers, a per-port latency offset, no clock. #171 added the platform side
(`MidiDevices::open_output`, `MidiOutput::send` at a host time) and `signal_graph` already had an
unused event sink. #176 asks for the rest: how a track reaches a port, when each message leaves,
where the latency lives, and what happens on stop and when a port goes away.

Two of the answers change persisted formats: the project gains a node kind and the app settings a
field. Both were put to the developer during planning and approved (`flag-risk`, tag `product`).

A hardware synth's audio is usually recorded back. Takes are placed by subtracting the device's
input and output latency and the plan latency; for the recorded synth to line up with the note's
timeline position, the synth must sound when the block holding the note is heard.

## Decision

- **A MIDI output is a node kind**, `MidiOut { port, name }`: one event input, no outputs, the
  platform endpoint id and the name shown while the endpoint is missing. Project format 13; a
  version 12 file needs no change. In the graph it is an event sink; changing its port does not
  touch the graph. A MIDI track chooses it from its ≡ menu as it chooses an instrument; it joins
  no mixer.
- **Send time.** The audio thread stamps each event leaving a routed sink with the host time its
  frame is due at the device: the callback's host time plus the frame's offset. The sender thread
  sends it at `stamp + output delay − port latency`, never earlier than now, where the output
  delay is the device's output latency plus one block (as for live MIDI input) and the port
  latency comes from the app settings. No settings reach the real-time thread.
- **Per-port latency lives in the app settings**, keyed by endpoint id: it describes this
  machine's cable and synth, not the song. Settings version 4, bounded and validated as untrusted
  input; older files read with none.
- **Silence by a late All Notes Off.** Stop and pause queue a silence into every MIDI output; the
  sender sends sustain off and All Notes Off on all 16 channels just after the latest time it has
  scheduled. Removing a route, changing its port and quitting send that silence both now and after
  the latest scheduled time, then close the output.
- **Port loss.** A send refused because the endpoint went away marks the port lost (shown greyed
  in the interface) and drops what reaches it; the first successful send after that, or a retry
  every half second, sends All Notes Off first.
- **Offline render ignores MIDI outputs.**

## Alternatives considered

- **Flush scheduled events on stop with CoreMIDI's `MIDIFlushOutput`** — the immediate way to
  cancel notes already scheduled ahead, but it needs new `unsafe` FFI in `macos_platform` and a
  new trait method for one platform's feature; a late All Notes Off reaches the same silence with
  the existing API, at the cost of scheduled notes sounding briefly before it.
- **Latency saved in the project, on the node** — would travel with the song to machines with
  other cables and synths, and change the project for a property of the studio.
- **Adding the offsets on the audio thread** — would need the settings and device latency on the
  real-time thread; the sender can add them as well, off it.
- **One MIDI output per endpoint shared by every node** — fewer open ports, but each node's
  silence and loss would need sharing rules; one output per node is simpler and CoreMIDI allows
  it.
- **Sending MIDI clock** — ADR-0139 ruled it out.

## Consequences

- A synth recorded back lines up only if the port latency is set to the synth's real latency;
  the user measures it.
- After stop, notes already scheduled ahead (at most the output delay) still sound until the
  silence reaches them.
- Bounce and export never include hardware synths; the user records them back to audio first.
- Opening a project whose endpoint is absent keeps the saved name and drops the notes until the
  endpoint appears.
