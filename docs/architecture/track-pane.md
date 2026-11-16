# Track Pane

The tracks in the window's left pane, and the edits made from them. The window around it is in
`docs/architecture/user-interface.md` "Layout"; range selection and its shortcuts are in
`docs/architecture/user-interface.md` "Range selection and shortcuts".

Each track is a header over one 128-pixel lane per channel, left above right; a MIDI track has
one lane. The header holds the track's name; for a MIDI track, the name of the instrument it feeds,
or "No instrument"; the arm toggle; while armed, the monitor toggle, the input choice and an input
meter (an activity light for a MIDI track); and a ≡ menu: rename, import audio into (greyed out on
a MIDI track), delete, and on a MIDI track the instrument entries ("Instrument choice" below). A
"+" below the tracks adds a mono or stereo track, a MIDI track, or an automation curve ("Curve N",
the lowest N not in use, holding one point at frame 0 halfway up). A red cursor marks the playhead
in every lane. A menu item that cannot be used now is greyed out, not hidden. Adding, deleting and
importing into a track, quantizing, and choosing an instrument are refused while playing.

## MIDI clip display

A MIDI lane is read-only. It draws each clip's visible stretch as a faint band in the audio colour,
and each note that plays as a bar about 3 pixels tall in the audio colour, from its start to its
end, both cut off at the lane's edges. The notes drawn are exactly those the engine plays
(`docs/architecture/project.md` "Editing model"): a note under a higher clip is not drawn. The
lane's height spans the lowest to the highest pitch in view, widened evenly to at least 12
semitones within 0–127, highest at the top. It is drawn on the lane's raster, as waveforms are,
and redrawn with the other lanes ([ADR-0152](../adrs/0152-midi-clips-are-edited-through-range-selection.md)).

Over the lane, in the layer above its image, each labelled pitch in view gets a hairline in the
text colour across the lane and a small label at its left edge: under a tonal key each pitch
that is the key's root ("C3", "C4"), under an indefinite key each named pitch ("kick"). A label
that would crowd the one below it is left out; its line stays. Bars, lines and hit tests share
one placement of each pitch's row. While stopped, a pointer within 3 pixels of a note bar, or
within half its row where rows are taller, shows a tooltip beside the note with its label
(`project_model::notation::note_label`), kept inside the lane; there is none while playing or
recording, during a drag, or after the lanes are redrawn
([ADR-0153](../adrs/0153-midi-tracks-carry-a-key-and-pitch-names-lanes.md)).

## Key and pitch names

Every MIDI track has a key (`project_model::Notation`): a root pitch class and major or minor, or
indefinite. A new MIDI track is in C major with no names. Pitches are named with middle C, MIDI
60, as "C4" (octave = pitch / 12 − 1, so 0 is "C-1" and 127 is "G9"), in ASCII: flats ("Bb") in
F, B♭, E♭, A♭, D♭ and G♭ major and D, G, C, F, B♭ and E♭ minor, sharps ("C#") in the other keys
and under an indefinite key. A track may name pitches 0–127; a name is 1 to 32 characters, not
blank, with no control characters. Names are kept under a tonal key but shown only under an
indefinite one. The key and names change only the display, never what plays.

A MIDI track's ≡ menu has "Key: <key>…" above its instrument entries, available while playing too.
It opens the key dialog beside the track header: Major, Minor and Indefinite, then for a tonal key
twelve root buttons spelled for the mode, or for an indefinite key a scrolling list of the pitches
the track's clips play and those already named, each with a name field, and "Reset to GM drums".
Every choice applies at once through `project_model::Session::set_notation`, one undo step each,
and leaves the dialog open; Done or a click outside closes it. Choosing Indefinite on a track with
no names names the General MIDI drum pitches 35–81 (`project_model::notation::GM_DRUMS`). A typed
name is recorded, trimmed, on Return or on anything else the user does; an empty one clears the
name ([ADR-0153](../adrs/0153-midi-tracks-carry-a-key-and-pitch-names-lanes.md)).

## Quantize

The quantize button, beside cut, copy and paste in the top bar, and the Q key open the quantize
dialog. Both are available only while stopped with a range selected over at least one MIDI track.
The dialog offers a grid (bar, beat, 1/8, 1/16) and a strength slider from 0 to 100 % in steps of
5 %, starting at 1/16 and 100 % and remembered for the session, not saved; changing either leaves
the dialog open. Cancel, or a click outside it, closes it. Quantize applies
`project_model::Session::quantize` (`docs/architecture/project.md` "Editing model") to the
selected tracks and range as one undo step, closes the dialog, and sets the status line to
"Quantized N notes." or "No notes in the selection."; one that moves nothing adds no step
([ADR-0152](../adrs/0152-midi-clips-are-edited-through-range-selection.md)).

## Instrument choice

A MIDI track's ≡ menu lists every instrument in the `instruments` registry as
"Instrument: <name>", the one the track feeds ticked. Choosing one applies
`project_model::Session::set_instrument` as one undo step:

- the track's event output is disconnected, and an instrument it fed that nothing else feeds is
  removed with its connections;
- a new instrument of the chosen kind, at its defaults and laid out automatically on the patch
  board, takes the track on its event input 0;
- its stereo output 0 joins the project as a new stereo track's does
  (`docs/architecture/project.md` "Routing"), reusing the mixer channels a removed instrument
  held.

A new MIDI track feeds nothing until an instrument is chosen or wired on the patch board
([ADR-0152](../adrs/0152-midi-clips-are-edited-through-range-selection.md)).
