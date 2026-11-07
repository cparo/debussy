# ADR-0058 — Stereo ports are connected by side, and new tracks join a project mixer

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-07
- **Issue:** #64
- **Architecture:** [Routing](../architecture/project.md#routing)
  ([Saved format](../architecture/project.md#saved-format))

## Context

#64's acceptance needs mixing with mute and solo, but new tracks went straight to the output and
the patch board (#73) is not built. ADR-0043 left a template that routes tracks into a mixer for
later. A stereo track in a mixer's mono channel (ADR-0044) would be folded to mono. The developer
asked for stereo tracks, and stereo ports of processing nodes, to appear to the user as left and
right mono connectors, each connectable on its own, with the existing pan and fold-down rules
applied underneath. The representation, a saved-format change, was confirmed with the developer.

## Decision

- **Sides:** a connection end may name the `Left` or `Right` side of a stereo port
  (`from_side`, `to_side`), carrying that channel as a mono signal; `None` is the whole port, as
  before. The model refuses a side of a port it knows to be mono: a mono track, a mixer input, a
  mono bus or delay. Duplicates are judged including sides.
- **In the graph:** `engine_bridge` realises a sided output with an `audio_effects::Pick` node
  (stereo in, one channel out) and a sided input with a `Place` node (mono in, that side of a
  stereo out, the other silent). Several connections into one side sum as usual.
- **Project mixer:** a new project (`Session::new_project`) has one mixer connected to the
  output; the first mixer feeding the output is the project mixer. A new track joins it in the
  same undo step: mono on the lowest free channel, stereo as left and right on two channels
  panned hard left and right, adding channels as needed, with the channels' other values reset.
  Projects without one keep connecting tracks to the output.
- **Saved format version 2** adds `from_side` and `to_side`; version 1 files migrate unchanged.
- **Mixer UI:** a channel fed by one side of a track is labelled with the track's name and `L`
  or `R`.

## Alternatives considered

- **Stereo mixer channels with a balance control:** keeps one strip per stereo track, but
  supersedes ADR-0044's mono channels and gives no way to treat one side separately.
- **Folding stereo tracks to mono in the mixer, or bypassing it:** loses the stereo image, or
  loses mixing for stereo tracks.
- **Making every model port mono:** every existing stereo connection becomes two, and every
  stereo node needs joins and splits.
- **An "Add mixer" button instead of a template:** mixing one click away rather than built in.

## Consequences

- A stereo track takes two mixer strips. The patch board (#73) will show stereo ports as
  left/right pairs.
- Older builds refuse version 2 projects.
- New parameter values set on a node already in a running graph ramp in over 10 ms; values for a
  node being added are set before it is prepared, so render and playback start without a ramp.
