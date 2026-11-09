# ADR-0080 — Record and arm are lit in a fixed recording red and the audio device is picked in the bottom bar

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** #67
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

With recording working (ADR-0078, ADR-0079), the record button and the arm toggles were drawn in
the same audio accent as every other selected button, so recording did not stand out. The audio
device, now chosen for input as well as output, was buried in the ≡ application menu.

## Decision

- Recording red is `#BB0000` in both tones. The record button's dot is red on the button's
  usual face; while recording, the button's face is red and its dot black, though the button is
  disabled then. An armed track's
  arm toggle is red with black text. Other selected buttons keep the tone's audio accent.
- The audio device is a dropdown at the bottom bar's left end, before the mono/stereo listening
  toggle, and is no longer in the application menu. Choosing another device while playing is
  refused with a notice, as before (ADR-0059).

## Alternatives considered

- **The tone's audio accent for record and arm** — it does not tell recording apart from other
  selected states.
- **A red that differs by tone** — one colour keeps "recording" recognisable across tones.
- **Keep the device in the application menu** — two clicks to reach, and it hid which device is
  in use.

## Consequences

- The bottom bar is 216 px wider at its left; the position bar shrinks to make room.
- The device list is re-read each time the dropdown opens, so a device plugged in after launch
  appears there.
