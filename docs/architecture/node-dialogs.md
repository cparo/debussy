# Dialogs

## Node dialogs

**Node dialogs** cover the whole track pane for the selected node: a header with its name and a
close × with no button face, then a mixer's controls, a feedback delay's time in seconds, or a
built-in effect's controls; a bus or an effect this build does not know has only the header.
A "Delete" button at the bottom right, last in the dialog's Tab order (alone in a dialog with
no other controls), asks first: a confirmation with Cancel, the default, and Delete. Tab and
Shift-Tab move between the two, the focused one lit; Enter or Space presses it; Escape or a
press outside cancels
([ADR-0108](../adrs/0108-a-node-dialog-s-delete-button-is-in-its-tab-cycle.md)). An effect's controls are drawn from its parameters'
metadata (`docs/architecture/signal-graph.md` "Effect nodes"), under its
[display](effect-displays.md#effect-displays): ungrouped parameters in a row,
after a "Preset" pick list if the kind has presets (`docs/architecture/effects.md`
"Effect presets"), then one strip per group in a horizontal scroll whose scrollbar has its own row below the
strips. A linear parameter is a slider, a logarithmic one a slider over its logarithm, an
integer one a slider in whole steps, and a choice a pick list,
each under its name. A slider shows its value and unit only as a tooltip while hovered, hertz
from 1000 up as kilohertz. A toggle is one button, lit while on and unlit while off, with no
separate label; it shows the name of its current state if the parameter names its states, else
the parameter's name; in a group's strip, its unlit background is the
pick lists' background. A stopped-only parameter shows "Stop to change" in place of its
control while playing, at the height of a pick list for a choice and of a slider otherwise; a
toggle shows its name and that text within its button's height
([ADR-0093](../adrs/0093-effect-controls-show-slider-values-as-hover.md)).
A parameter shown only while a toggle is in a given state is left out otherwise: not drawn,
skipped by Tab, and unchanged by keys
([ADR-0116](../adrs/0116-a-parameter-may-be-shown-only-while-a-toggle-is-on.md)).
A parameter enabled only while a stepped parameter of its node holds given values
is disabled otherwise: greyed out (a slider in the curve-driven style, a pick list or toggle as
greyed text of its value in the control's room, as for "Stop to change"), still drawn and in Tab order, and unchanged by drags, presses and keys;
`signal_graph::check_conditions` refuses any other kind of deciding parameter
([ADR-0169](../adrs/0169-a-parameter-s-control-can-be-disabled-by-the-value.md),
[ADR-0173](../adrs/0173-a-control-can-be-disabled-by-any-stepped-parameter.md)).
A parameter may declare a label, shown in place of the one made from its name, and a display
factor, by which its shown value is multiplied; neither changes the value held or saved. Every
filter order is labelled "Rolloff" and shown in dB/octave, 6 per pole
([ADR-0170](../adrs/0170-a-filter-order-is-shown-as-a-rolloff-in-db-per.md)).
Group strips are all as tall as the tallest, their content at the top. The mixer dialogs'
keyboard focus applies, over the controls in drawn order: up and down step a linear slider by
1 % of its range, a logarithmic one by 1 % of its logarithm's range, an integer by one and a
choice by one option, each press one step; right and left step a focused slider the same way;
Space or Enter flips a focused toggle
([ADR-0096](../adrs/0096-effect-dialogs-share-the-mixer-s-keyboard-focus.md)).
A parameter that declares a step replaces those steps with its own: an arrow press snaps the
value to that grid and moves it one step, a linear slider drags in that step, and its value
text has the step's decimal places
([ADR-0110](../adrs/0110-a-parameter-may-declare-its-step-the-mixer-strip.md)).

Beside each automatable parameter's control (not stopped-only, not stepped; see
`docs/architecture/automation.md` "Curves") is a 20-pixel curve port button: the
curve icon while the parameter has no port, else the port's number on a lavender face. A press,
or Space or Enter while it has the keyboard focus, gives the parameter the lowest free port or
takes its port and wire away, as one step, also while playing; arrows do nothing on it. A
parameter that is not automatable has a space as wide in its place. In Tab order each port button
comes just before its control. While a curve is wired to its port, the parameter is shown at the
curve's value at the playhead, mapped into its range: its slider is greyed out and follows the
curve, and drags and arrow keys leave it unchanged
([ADR-0126](../adrs/0126-curve-lanes-are-edited-by-refusing-invalid-shapes.md)).

## Mixer dialogs

**Mixer dialogs** have a "Channels" field with up and down buttons, clamped on commit from one
past the highest connected input (at least 1) to 128 and applied as one step, above narrow
strips in a horizontal scroll whose scrollbar has its own row below the strips. A strip shows,
8 pixels apart: its channel number; Mute above Solo; a vertical gain slider between +12dB and -96dB labels, in whole-dB steps; and a vertical
pan slider between L (top) and R (bottom) labels. Each slider has a tic mark under its handle at
0 dB or centre, and shows its value only as a hover tip. Soloing a channel unsolos the mixer's
others in the same step. A keyboard focus, drawn as an accent ring, goes to a control when it is
pressed, leaves on a press on the dialog's background, and moves with Tab and Shift-Tab from the
field down each strip's mute, solo, gain and pan, strip by strip, then "Delete", wrapping. The gain
and pan each have a curve port button above their top label, as in any node dialog ("Node
dialogs"), before them in Tab order. The up and down
arrows step the focused field or slider (1 channel, 1 dB, 1 % pan with up to the left), each
press one step, never the hovered slider; Space or Enter presses a focused button
([ADR-0089](../adrs/0089-the-mixer-dialog-shows-narrow-numbered-strips-with.md)).
The gain and pan sliders take their range, step, tic position (the default) and the gain's
end labels and value text from their parameters' metadata, and arrow presses step them as any
parameter with a declared step; gain declares 1 dB and pan 0.01
([ADR-0110](../adrs/0110-a-parameter-may-declare-its-step-the-mixer-strip.md)).
In any node dialog, Tab or Shift-Tab scrolls the newly focused control into view, by the
least that shows it whole with a 12-pixel margin; the body keeps room on its right for its
vertical scrollbar, which never covers a control ([ADR-0109](../adrs/0109-moving-a-node-dialog-s-focus-with-tab-scrolls-the.md)).

## Sampler zones

A sampler's dialog has a "Zones" section above its parameters: one row per zone, in the
sampler's order, with its file's name; pick lists for its low key, high key and root key, named by
the first MIDI track feeding the sampler (`docs/architecture/track-pane.md` "Key and pitch names"):
in a gated zone, by pitch name in the track's key (sharps if no track); in a one-shot zone, by
drum: the track's own name for the key if its key is Indefinite, else the General MIDI drum name
(`project_model::notation::GM_DRUMS`), else the pitch name; a tune pick list from −24 to +24
semitones ("0 st" untuned); a button naming its mode, One-shot or
Gated, that switches it; a choke pick list, "No choke" or groups 1 to 16; and Remove. A low key
chosen above the high key raises the high key with it, and the reverse. Each change is one step.
Below the rows, "Add sample…" asks for one file and adds a gated zone on C3 (48) with root C3.
"Add kit…" asks for a folder and imports every audio file in it and its subfolders, sorted by path,
except hidden ones and without following links; a folder holding more than 128 is refused. Each
file becomes a one-shot single-key zone placed by the words of its path within the folder
(`debussy_ui::kit_map`): one naming a drum takes its General MIDI key, a second crash or ride the
second crash or ride key, a tom not named floor, low, mid or high the next free tom key; any other
file, or one whose drum is taken, takes the next free key from 82, and files past 127 are left out.
When the kit has more than one hi-hat (42, 44, 46), they share choke group 1. A file that cannot be
imported (too long, unreadable) is skipped; the status line counts the samples a kit added, and an
alert lists the files skipped (`docs/architecture/user-interface.md` "Files and devices"). A kit
that would take the sampler past 128 zones, an empty folder, or a folder of more than 128 files is
refused with an alert saying so. Each adds all its zones as one step after the existing ones, and both
are disabled while playing. Choosing a different instrument for the track removes the zones with the sampler
([ADR-0156](../adrs/0156-sampler-zones-are-edited-in-rows-in-the-sampler-s.md)).

