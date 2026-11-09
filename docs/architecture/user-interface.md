# User Interface

## UI toolkit

The graphical user interface must be written in Rust with the `iced` toolkit ([ADR-0003](../adrs/0003-the-user-interface-is-built-with-iced.md)), version
0.14 with its GPU (`wgpu`) and software (`tiny-skia`) renderers, `canvas`, and the `thread-pool`
executor; only `debussy_ui` depends on it ([ADR-0055](../adrs/0055-iced-0-14-is-added-the-msrv-rises-to-1-90-and-cc0.md)).
Image support is on without codecs (`image-without-codecs`), for drawing in-memory pixels
([ADR-0068](../adrs/0068-lanes-are-rasterised-in-memory-and-blurred-with-a.md)).

## Interface structure

`debussy_ui` holds the project's session, the `engine_bridge` bridge running its routing, and the
engine's control side. It reaches I/O and the platform only through its `Services` trait, which
`debussy` implements: disk-streamed track feeds, moving the streams, waveform summaries, and
saving. Its update logic runs without a window, so it is tested directly ([ADR-0056](../adrs/0056-the-interface-owns-the-session-and-reaches-i-o.md)).

The window is laid out as described under [Layout](#layout).

## Layout

The window is a top bar, a track pane beside a right pane, and a bottom bar.

- **Top bar:** a logo, the waveform/spectrogram toggle, undo and redo, cut, copy and paste
  (disabled until clip editing exists), zoom out and in, the project's name (read-only), the
  light/dark toggle, and the ≡ application menu: new and open (disabled until the interface can
  switch projects), import, export, export mono, and quit.
- **Bottom bar:** the audio device dropdown, mono/stereo listening, play/pause, stop, record (see
  [Recording](#recording)), the position bar, the playhead's time, and the status line
  ([ADR-0080](../adrs/0080-record-and-arm-are-lit-in-a-fixed-recording-red.md)).
- **Track pane:** each track is a header (its name, the arm toggle and, while armed, the monitor
  toggle, the input choice and an input meter, and a ≡ menu: rename, import audio into, delete) over one 128-pixel lane per channel, left above right. A "+" below the tracks adds a mono
  or stereo track; automation curves are listed but disabled. A red cursor marks the playhead in
  every lane. A menu item that cannot be used now is greyed out, not hidden. Adding, deleting and
  importing into a track are refused while playing.
- **Divider, gutter and board:** a draggable divider (starting at 60 % tracks, 40 % board), the
  port gutter, and the [patch board](#patch-board). The track pane, gutter and board scroll
  vertically together under one scrollbar at the window's right edge.
- **Node dialogs** open over the track pane for the selected node: a header with its name and a
  delete button, then a mixer's channel strips (gain, pan, mute, solo) with add and remove channel,
  or a feedback delay's time in seconds; a bus has only the header.
- **Lanes** show waveforms from the waveform summaries, or, at zooms finer than the summaries,
  the stored audio as an intensity-graded trace (`docs/architecture/analysis.md` "Waveform
  summaries"), or spectrograms: one spectrum per two waveform columns, computed on demand
  from the stored audio as 192 log-spaced bands, each from a frame centred on its column
  ([ADR-0062](../adrs/0062-zooms-finer-than-the-waveform-summaries-are-drawn.md),
  [ADR-0068](../adrs/0068-lanes-are-rasterised-in-memory-and-blurred-with-a.md)).
  Each lane is rasterised at the window's scale factor, blurred with a Gaussian of
  σ = 0.5 physical pixel, and drawn as an image; the playhead cursor is drawn sharp over it
  ([ADR-0068](../adrs/0068-lanes-are-rasterised-in-memory-and-blurred-with-a.md)).
- **Colours:** audio signals (and a selected button's face, and the logo) are orange in the dark
  tone and blue in the light tone
  ([ADR-0071](../adrs/0071-the-audio-colour-is-orange-on-dark-chrome-and-blue.md)); control
  signals are lavender in both. The tone follows the system's appearance at launch and
  when it changes; the toggle is not saved.
- **Button groups** (each toggle and each set of related toolbar or dialog buttons) are drawn as
  one strip, rounded only at its ends, with a hairline seam between buttons.
- **Menus** are flat buttons floated over the window; a click anywhere else closes the open menu.

([ADR-0060](../adrs/0060-the-window-is-laid-out-as-top-bar-track-pane.md))

## Icons

Application icons and interface glyphs are SVG files in `crates/debussy_ui/assets/icons/`,
compiled into the binary and drawn with `iced`'s `svg` widget. Each is a single-colour shape on a
transparent background; the interface tints it with a theme colour, so a file's own fill colour
never shows. The logo (`debussy.svg`) sits at the left of the top bar in the accent of a selected
toolbar button, with no box or background of its own.

The waveform/spectrogram toggle, undo, redo, cut, copy, paste, play/pause, stop, record and the
light/dark toggle show 16 px icons instead of text, padded to a text button's height. The
light/dark toggle shows the tone it switches to, in the logo's accent, with no button face or
outline, like the ≡ buttons. The record icon is recording red (`#BB0000`, both tones) on the
button's face, and black on a red face while recording
([ADR-0080](../adrs/0080-record-and-arm-are-lit-in-a-fixed-recording-red.md)). Any other icon takes its button's text colour; a disabled icon button
keeps its face and greys its icon out towards the face. A tooltip that follows the cursor names the
action. The ≡ buttons draw a menu icon in the chrome's text colour, centred in their button. Other
toolbar buttons keep text labels
([ADR-0072](../adrs/0072-toolbar-controls-show-icons-named-in-tooltips.md)).

## Patch board

The patch board draws the routing; it pans horizontally with its own scrollbar when wider than
its pane, is never narrower than its pane, and does not zoom.

- The gutter is drawn in the board's colour and holds one port per lane, centred vertically on
  the lane with its left edge 4 pixels from the divider.
- Every wire is mono and in the audio colour ("Layout"). A stereo port is drawn as two circles, left above right; a
  connection that names a side joins that circle, and a whole-port connection is drawn as a wire
  per side.
- Node ports are 48 pixels apart, inputs on the left and outputs on the right, and a node's height
  follows from its taller side. A mixer has a mono input per channel and a stereo output; a bus or
  feedback delay one port of its layout each way; an effect is drawn with one stereo port each way.
- A node the user has moved stays where they put it. The rest are placed automatically in
  columns by graph depth (one past the deepest node feeding them), each column ordered by the
  height of what feeds it.
- The project output's L and R sit just inside the board's right edge, to the right of every node,
  at the height the user dragged them to, or else level with what feeds them.

Editing ([ADR-0061](../adrs/0061-node-positions-and-the-outputs-height-are-saved-as.md)):

- Drag from an output to an input to connect; at a stereo port the connection names its side. An
  input takes one wire; a drop on an occupied input, an output, or empty board does nothing.
- Drag a node, or the L and R outputs (vertically), to move them; the move is one undo step.
- Click a node or wire to select it; a node's dialog opens. Delete or Backspace removes the
  selection, except while text is being typed. A click on empty board clears the selection and
  closes the dialog.
- The "+" at the board's top right adds a two-channel mixer, a stereo bus or a stereo feedback
  delay; no effects are offered yet.

([ADR-0060](../adrs/0060-the-window-is-laid-out-as-top-bar-track-pane.md))

## Transport and seeking

A click in a lane, or a click or drag on the position bar, seeks the playhead there, during
playback too, and makes that frame the return point. Stop returns to the return point; pausing
and playing again leaves it in place. Playback stops by itself at the project's end, leaving the
playhead there; Play from the end, or from past it, starts from the return point if that is before
the end, otherwise from the start
([ADR-0076](../adrs/0076-playback-stops-by-itself-at-the-project-s-end.md)). A seek past the end
is kept. A seek outside the visible window recentres the view on it.
The position bar shows the playhead as a red bar along a grey channel over the timeline's length
(the end of the last clip, or more to take in the window and the playhead), with ticks at the
visible window's edges. The playhead's time beside it reads `HH:MM:SS.000`, dropping the hours
while the later of the project's end and the playhead is under an hour, and the minutes while it
is under a minute ([ADR-0075](../adrs/0075-the-playhead-time-reads-hh-mm-ss-000-dropping.md)).

While playing, once the playhead is in view and reaches 80 % of the lane's width, the view moves
to put it at 20 %, including when it has passed the right edge since the last display frame. A
playhead that is out of view and was not followed there is not followed; panning or zooming during
playback stops following until the playhead is in view again
([ADR-0063](../adrs/0063-while-playing-the-view-pages-along-with-a-visible.md)).

Zoom steps by √2: two steps halve or double the time shown, from 2 to 2^24 frames per waveform
column. Zooming keeps the playhead at the same place on screen if it is visible, otherwise the
view's centre. Zoom out stops at a fit that shows frame 0 to the project's end and follows it as
clips change; zooming in from the fit goes to the coarsest grid step finer than it, and zoom out is
disabled while the view covers the project
([ADR-0066](../adrs/0066-zoom-out-stops-at-an-exact-fit-of-the-project.md)). A mostly sideways
two-finger scroll over a lane pans the view; there is no gesture zoom
([ADR-0060](../adrs/0060-the-window-is-laid-out-as-top-bar-track-pane.md),
[ADR-0062](../adrs/0062-zooms-finer-than-the-waveform-summaries-are-drawn.md)). Panning forward,
zooming out, playback paging and a seek's recentring all stop with the scroll bound, the later of
the project's end and the playhead, as the lane's last frame; a view already past that is not
moved back by any of them ([ADR-0077](../adrs/0077-every-view-move-stops-at-the-scroll-bound.md)).

## Gestures

While a slider is dragged its value goes straight to the running graph's parameter; releasing it
records one undo step. A mute or solo press is one step. Undo and redo are refused while the
transport is playing ([ADR-0056](../adrs/0056-the-interface-owns-the-session-and-reaches-i-o.md)).

## Range selection and shortcuts

A press in a lane seeks there, makes its track the current track, and clears the range selection.
Dragging from it selects a range ([`project.md` "Range edits"](project.md#range-edits)): the frames
between the press and the pointer, on the tracks from the pressed one to the one under the
pointer. Shift-press moves the selection's nearer edge to the press and its tracks to reach the
pressed one, keeping the far end; with no selection it selects from the playhead on the current
track to the press. Neither seeks. Escape clears the
selection. The selection is drawn as a light band over its tracks' lanes.

Cut, copy and paste work on an in-app clipboard holding one excerpt. Paste puts it at the
playhead, its first lane on the top track of the selection, or on the current track without one.
Cut, delete and paste are refused while the transport is playing; copy is not.

| Key | Action |
|---|---|
| ⌘X, ⌘C, ⌘V | Cut, copy, paste |
| Delete, Backspace | Delete the patch board's selection if it has one, otherwise the range |
| ⌘Z, ⇧⌘Z | Undo, redo |
| Escape | Clear the range selection |

⌘ is Ctrl off macOS. A key typed into a text field goes to the field
([ADR-0074](../adrs/0074-a-drag-in-the-lanes-selects-a-range-and-edit.md)).

## Application startup

`debussy` opens the project named on its command line, creating it if missing, imports any
`--import` files as new tracks, builds missing waveform summaries, and opens the window, playing
through the default output device, which must run at the project rate. With `--export` it renders
the project to a file and exits instead. Command-line import and export stand in until the
interface offers them ([ADR-0057](../adrs/0057-the-first-binary-imports-and-exports-from-the.md)).

## Recording

A track header's arm toggle arms the track and is lit recording red while armed; an armed track
shows a monitor toggle, off by default, an input choice: one device input for a mono track, a
pair (1–2, 3–4, …) for a stereo track, defaulting to input 1 or 1–2, and an input meter, one bar
per channel from −60 to 0 dBFS, red from −1 dBFS, that holds a peak and falls 1.5 dB per display
frame. Arm, monitor and input are session state, not saved in the project, and change only while
the transport is stopped or paused. The device's inputs are open only while a track is armed: arming the first track reopens
the device duplex, and disarming the last reopens it output-only.

Record is enabled while stopped or paused with a track armed, the device's inputs open and the
previous take saved. It records every armed track from the playhead while the other tracks play,
and runs on past the project's end. Stop or pause ends the pass. Once its files are written, one
clip per armed track is placed as one undo step (see `docs/architecture/recording.md` "Take
placement"); a pass that captured nothing or failed is discarded with its files. Overruns are
reported on the status line ([ADR-0079](../adrs/0079-arm-state-is-session-only-and-inputs-open-only.md)).

## Files and devices

Import and export use the system's file dialogs through `rfd`, used only in `debussy`. Each
imported file becomes a track holding one clip of the whole file at the start, as one undo step.
Import is refused while playing.

One audio device is chosen for both input and output, from a list re-read each time the device
dropdown opens. The choice is remembered in the application settings; at launch a remembered device that is gone falls back to the system default output. The
device can be changed only while stopped: the engine's real-time side is handed back from the old
stream and handed to the new one without being rebuilt, and if the new device fails to open the
old one is reopened. An audio device that stops with an error stops playback and is reported on
the status line ([ADR-0059](../adrs/0059-the-window-imports-exports-and-picks-one-audio.md)).
