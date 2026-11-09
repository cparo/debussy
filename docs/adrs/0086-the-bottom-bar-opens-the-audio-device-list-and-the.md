# ADR-0086 — The bottom bar opens the audio device list and the click dialog from icon buttons

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** —
- **Architecture:** [Layout](../architecture/user-interface.md#layout)

## Context

After #57 the bottom bar held a 200-pixel device dropdown, the click toggle and its ▾ menu, a
tempo field and two meter pickers beside the transport, crowding the position bar. The developer
asked for the device picker and the click controls to move behind icon buttons.

## Decision

- The audio device dropdown becomes an icon button (a microphone wearing over-ear headphones,
  drawn in-house) that opens a dropdown menu of the devices present, re-read on opening, the one
  in use ticked. Choosing a device is refused while playing, as before (ADR-0080).
- The click toggle, its ▾ menu, the tempo field and the meter pickers become one click button.
  It is lit while the click is on and opens a click dialog above the bottom bar holding every
  click control: on/off, tempo, meter, sound, level and count-in.
- The click dialog is floated like a menu, but its own controls leave it open; a click outside
  it, or on the click button again, closes it.

## Alternatives considered

- **Keep the toggle on the click button and put the rest in the dialog** — one click fewer to
  switch the click, but the developer wanted the button only to open the dialog.
- **A node-style dialog over the track pane** — far from the button that opens it.
- **Move the click controls to the top bar** — the top bar holds view and edit controls, not
  playback ones.

## Consequences

- Switching the click on or off, or changing the tempo, takes one more click.
- The device in use is no longer named in the bar; the menu shows it ticked.
- The dialog's position is fixed near the click button rather than measured from it.
