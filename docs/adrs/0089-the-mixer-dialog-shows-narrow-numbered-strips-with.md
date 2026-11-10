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

- A strip is 52 pixels wide, its parts 8 pixels apart: its channel number, then "Mute" above
  "Solo" in small text, then a vertical gain slider labelled +12dB at the top and -96dB at the
  bottom, then a vertical pan slider labelled L at the top and R at the bottom. Each slider has a tic
  mark under its handle at 0 dB or centre.
- A slider's value is shown only as a hover tip: gain in whole dB, pan as "C", "L 35" or
  "R 20".
- The gain slider moves in whole dB, by drag and by arrow keys.
- A "Channels" field with up and down buttons replaces the add and remove buttons. A committed count is clamped from one
  past the highest connected input (at least 1) to 128 and applied as one undo step; text that is
  not a whole number is refused with a notice.
- Soloing a channel unsolos the mixer's other channels, in the same undo step. The mixer node
  still plays every soloed channel (ADR-0044); the dialog keeps there from being more than one.
- Tab and Shift-Tab move a keyboard focus through the dialog: the field, then each strip's mute,
  solo, gain and pan, strip by strip. Pressing a control focuses it; pressing the background
  clears the focus. The arrow keys step the focused control, never the hovered slider, and Space
  or Enter presses a focused button. The app keeps this focus itself, since iced 0.14 can focus
  only text fields; a small wrapper widget reports presses and keeps keys and the wheel from the
  sliders.
- The close × has no button face. A "Delete" button at the bottom right of every node dialog asks
  for confirmation, with Cancel the default.
- A track's name moves to the hover tip of its ports in the gutter, with " L" or " R" for a side
  of a stereo port.

## Alternatives considered

- **Keep track names on strips, truncated** — still uneven and duplicated the board's wiring.
- **Keep add and remove buttons beside the field** — two ways to do one thing; the field already
  reaches any count in one step.
- **Show values in fixed readouts under each slider** — widens the strip, which the narrow layout
  is meant to avoid.
- **Snap gain to whole dB only on release** — the drag and the tip showed tenths that the release
  then discarded, and arrow keys still moved in tenths.
- **A focusable slider widget of our own** — would replace iced's slider for one dialog; an
  app-level focus is small and testable without a window.
- **Let a hovered slider keep its own arrow keys** — the keys then went to whichever slider the
  cursor was over, not the focused one, and the change was never recorded, since iced sends no
  release for a key press.
- **Several solos at once** (the mixer node allows it) — the developer called it a bug; one solo
  per mixer is what the dialog offers.
- **Delete with no confirmation, relying on undo** — the developer asked for a confirmation.

## Consequences

- The tic marks assume iced's default slider handle (a 7-pixel-radius circle); a custom slider
  style in the theme must update `tic_y` in `crates/debussy_ui/src/mixer_strip.rs`.
- Gain is set in whole dB from the dialog; finer values need automation or a future typed entry.
- The keyboard focus covers only the mixer dialog; Tab elsewhere does nothing yet.
- Ctrl-scroll no longer moves a mixer slider; the wheel scrolls the strips instead.
- Delete or Backspace on a node selected on the board still deletes it at once (undoable).
- The `debussy_ui` crate enables iced's `advanced` feature for the wrapper widget; it adds no
  crates.
- Which track feeds a channel is no longer shown in the dialog; the patch board shows it.
