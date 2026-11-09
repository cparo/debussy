# ADR-0084 — Recording's count-in splits the block it ends in

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** #57
- **Architecture:** [Count-in](../architecture/recording.md#count-in)

## Context

#57 asks for a count-in before recording, so a performer hears the tempo before the take starts.
ADR-0078 begins capture on the block where the transport changes from starting to playing. A
count-in has to end at the playhead's frame exactly, but device blocks are of any size: ending the
count-in only at a block boundary would start the take up to a block early or late, and the take
would no longer start at the playhead the user chose.

## Decision

- Record takes a count-in of 0, 1 or 2 bars (session state, 1 by default), converted to frames
  with the project's tempo. Play never counts in.
- A new `TransportState::CountingIn` follows priming. The tracks are silent (armed tracks behave
  as when recording), the playhead holds, and a signed virtual position runs from the playhead
  minus the count-in to the playhead. The click sounds there, minus the plan latency, even with
  the click switched off, on the grid extended before frame 0.
- In the block where the count-in ends, the processor runs the count-in's frames, then the rest
  of the block as playing with capture starting; the device input is used from the same split.
  The graph's output does not depend on block size (ADR-0016), so this is the same as two
  smaller blocks. A take's first frame is exactly the playhead.
- This amends ADR-0078's "capture begins on the block where the transport changes from starting
  to playing": capture begins with the first frame played.
- Stop or pause during the count-in ends the takes with nothing captured; the empty take is
  discarded as ADR-0078 says.

## Alternatives considered

- **End the count-in on a block boundary** — the take would start up to a block away from the
  playhead, and differently for each device buffer size.
- **Count in from the control side, then press record** — the UI thread cannot hit a frame.
- **Start capture at the count-in and trim afterwards** — writes audio nobody wants, and the
  trim depends on the take placement arithmetic.
- **Count in only when the click is on** — the developer wanted the count-in independent of the
  click toggle.

## Consequences

- A count-in delays the take by up to two bars after pressing record; it can be switched off.
- Two graph runs in one device block cost a little more on that block only.
- Play with a count-in, and pre-roll that plays the tracks during the count-in, are not built.
