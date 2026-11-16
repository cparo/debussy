# ADR-0153 — MIDI tracks carry a key and pitch names; lanes label roots and named pitches

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-16
- **Issue:** #192
- **Architecture:** [Key and pitch names](../architecture/track-pane.md#key-and-pitch-names)

## Context

A MIDI lane (ADR-0152) draws notes as bars scaled to the pitch range in view, with no reference
to pitch: nothing says which bar is which note. Pitched parts want a reference tied to their key;
drum and sample tracks have no key, and their pitches stand for sounds ("kick", "snare") rather
than notes. Issue #192 asked for both, and for a way to read one note's name without guessing.

## Decision

- **A key per MIDI track.** Every MIDI track carries a `Notation`: a key (a tonic spelled as a
  letter and an accidental — none, sharp or flat — with major or minor, or **Indefinite**) and a
  map of pitch names. Enharmonic tonics such as C# and Db are distinct keys. It is a field of `Track`
  (`None` on audio tracks), set by its own edit, `Edit::SetNotation`, one undo step.
- **Standard pitch names.** Middle C, MIDI 60, is C4. Keys whose signature has flats (from the
  tonic's place on the circle of fifths) spell with flats, others and Indefinite with sharps, in ASCII (`#`, `b`), which every interface font has.
- **Lanes label roots, or named pitches.** A tonal lane draws a line and label on each root in
  view; an Indefinite lane on each named pitch instead. They are vectors in the cursor layer, so
  they draw over the lane image and need no raster redraw.
- **Indefinite starts from General MIDI.** Choosing Indefinite on a track with no names names
  pitches 35–81 after the GM drum map; every name can be edited or cleared, and reset. Names
  survive a switch to a tonal key but show only while Indefinite.
- **A dialog, not a submenu.** The key and names are edited in a "Key…" dialog opened from the
  MIDI track's ≡ menu, which has no submenus. The tonic is a row of letters A–G and a strip of
  `#` and `b`, mutually exclusive toggles. Each choice applies at once. It is allowed while
  playing, as it changes only the display.
- **A tooltip while stopped.** Hovering a note while stopped shows its label; the lane reports
  only changes of the hovered note, not every move. Nothing shows while playing or recording.
- **Saved, format 9.** The key and names are saved in the MIDI track's kind; version 8 MIDI
  tracks migrate to C major with no names. Names in a file are untrusted input, a new place for
  it: they are checked against the model's limits (1–32 characters, not blank, no control
  characters, pitches 0–127, strictly ascending) and a bad one refuses the bundle.

## Alternatives considered

- **Key as data on `TrackKind::Midi`.** Rejected: `TrackKind` is `Copy` and compared by value in
  many places (`kind == TrackKind::Midi`); a field leaves them untouched.
- **Keys and names as ≡ menu entries.** Rejected: the menu has no submenus, and 24 keys plus a
  name per pitch would swamp it; names need text fields anyway.
- **An empty name map for Indefinite.** Rejected: nearly every Indefinite track is drums, and GM
  is what MIDI drum sources send; starting empty makes every user type the same map.
- **Keep the key for the session only.** Rejected: a drum map typed by hand would be lost on
  reopening; the format change was approved for this.
- **Unicode ♯ and ♭.** Rejected for now: not every face the interface may draw with has them.
- **A root pitch class (0–11) with spelling derived from it.** Rejected: it cannot tell C# major
  from Db major, and twelve root buttons did not fit the dialog.
- **Double sharps and flats, or enharmonic letters in pitch names.** Not done: pitch names use
  only the key's sharp-or-flat preference, so B# major labels its roots "C".

## Consequences

- Format versions 9 and 10; older builds refuse projects saved by this one. Version 10 exists
  because development builds of this change saved version 9 keys as a pitch-class `root`; those
  files migrate to the spelling they were shown with.
- Names are display only: nothing routes, plays or exports by them.
- No scale highlighting, other modes, or key changes over time; the key is one per track. A
  per-pitch drum lane view or note entry by name would build on the names.
