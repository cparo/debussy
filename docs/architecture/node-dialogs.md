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
control while playing ([ADR-0093](../adrs/0093-effect-controls-show-slider-values-as-hover.md)).
A parameter shown only while a toggle is in a given state is left out otherwise: not drawn,
skipped by Tab, and unchanged by keys
([ADR-0116](../adrs/0116-a-parameter-may-be-shown-only-while-a-toggle-is-on.md)).
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

## Mixer dialogs

**Mixer dialogs** have a "Channels" field with up and down buttons, clamped on commit from one
past the highest connected input (at least 1) to 128 and applied as one step, above narrow
strips in a horizontal scroll whose scrollbar has its own row below the strips. A strip shows,
8 pixels apart: its channel number; Mute above Solo; a vertical gain slider between +12dB and -96dB labels, in whole-dB steps; and a vertical
pan slider between L (top) and R (bottom) labels. Each slider has a tic mark under its handle at
0 dB or centre, and shows its value only as a hover tip. Soloing a channel unsolos the mixer's
others in the same step. A keyboard focus, drawn as an accent ring, goes to a control when it is
pressed, leaves on a press on the dialog's background, and moves with Tab and Shift-Tab from the
field down each strip's mute, solo, gain and pan, strip by strip, then "Delete", wrapping. The up and down
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
