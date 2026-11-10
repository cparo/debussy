# ADR-0089 — The mixer dialog shows narrow numbered strips with vertical gain and pan and a channel-count field

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

The mixer dialog was wide and cluttered: each 90-pixel strip showed the connected track's name, a
dB readout, a horizontal pan slider and side-by-side mute and solo buttons, and the dialog ended
with "Add channel" and "Remove channel" buttons. Track names made strips uneven and duplicated
what the patch board already shows, and adding or removing channels one at a time was slow for a
mixer of many inputs. The developer asked for narrow, numbered strips with values shown only on
hover.

## Decision

- A strip is 52 pixels wide: its channel number, then a vertical gain slider labelled +12dB at the
  top and -96dB at the bottom, then a vertical pan slider labelled L at the top and R at the
  bottom, then mute above solo. Each slider has a tic mark under its handle at 0 dB or centre.
- A slider's value is shown only as a hover tip: gain in dB to one decimal, pan as "C", "L 35" or
  "R 20".
- Releasing the gain slider rounds the value to the nearest whole dB, within the range, before
  recording it.
- A "Channels" field replaces the add and remove buttons. A committed count is clamped from one
  past the highest connected input (at least 1) to 128 and applied as one undo step; text that is
  not a whole number is refused with a notice.
- A track's name moves to the hover tip of its ports in the gutter, with " L" or " R" for a side
  of a stereo port.

## Alternatives considered

- **Keep track names on strips, truncated** — still uneven and duplicated the board's wiring.
- **Keep add and remove buttons beside the field** — two ways to do one thing; the field already
  reaches any count in one step.
- **Show values in fixed readouts under each slider** — widens the strip, which the narrow layout
  is meant to avoid.
- **Gain slider stepped in whole dB** — fine drags while listening would jump; snapping on release
  keeps the drag smooth and the stored value round.

## Consequences

- The tic marks assume iced's default slider handle (a 7-pixel-radius circle); a custom slider
  style in the theme must update `tic_y` in `crates/debussy_ui/src/mixer_strip.rs`.
- Gain is stored in whole dB after a drag; finer values need automation or a future typed entry.
- Which track feeds a channel is no longer shown in the dialog; the patch board shows it.
