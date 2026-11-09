# User Interface

## UI toolkit

The graphical user interface must be written in Rust with the `iced` toolkit ([ADR-0003](../adrs/0003-the-user-interface-is-built-with-iced.md)), version
0.14 with its GPU (`wgpu`) and software (`tiny-skia`) renderers, `canvas`, and the `thread-pool`
executor; only `debussy_ui` depends on it ([ADR-0055](../adrs/0055-iced-0-14-is-added-the-msrv-rises-to-1-90-and-cc0.md)).

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
  switch projects), save, import, export, export mono, the audio device, and quit.
- **Bottom bar:** mono/stereo listening, play/pause, stop, record (disabled), the playhead's time,
  the status line, and the position bar.
- **Track pane:** each track is a header (its name and a ≡ menu: rename, import audio into,
  delete) over one 128-pixel lane per channel, left above right. A "+" below the tracks adds a mono
  or stereo track; automation curves are listed but disabled. A red cursor marks the playhead in
  every lane. A menu item that cannot be used now is greyed out, not hidden. Adding, deleting and
  importing into a track are refused while playing.
- **Divider, gutter and board:** a draggable divider (starting at 60 % tracks, 40 % board), the
  port gutter, and the [patch board](#patch-board). The track pane, gutter and board scroll
  vertically together under one scrollbar at the window's right edge.
- **Node dialogs** open over the track pane for the selected node: a header with its name and a
  delete button, then a mixer's channel strips (gain, pan, mute, solo) with add and remove channel,
  or a feedback delay's time in seconds; a bus has only the header.
- **Lanes** show waveforms from the waveform summaries, or spectrograms: one spectrum per four
  waveform columns, computed on demand from the stored audio as 64 log-spaced bands.
- **Colours:** audio signals are orange and control signals lavender in both tones; only the
  chrome changes between light and dark. The tone follows the system's appearance at launch and
  when it changes; the toggle is not saved.
- **Button groups** (each toggle and each set of related toolbar or dialog buttons) are drawn as
  one strip, rounded only at its ends, with a hairline seam between buttons.
- **Menus** are flat buttons floated over the window; a click anywhere else closes the open menu.

([ADR-0060](../adrs/0060-the-window-is-laid-out-as-top-bar-track-pane.md))

## Patch board

The patch board draws the routing; it pans horizontally with its own scrollbar when wider than
its pane, is never narrower than its pane, and does not zoom.

- The gutter is drawn in the board's colour and holds one port per lane, centred vertically on
  the lane with its left edge 4 pixels from the divider.
- Every wire is mono and orange. A stereo port is drawn as two circles, left above right; a
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
and playing again leaves it in place. A seek outside the visible window recentres the view on it.
The position bar shows the playhead as a red bar along a grey channel over the timeline's length
(the end of the last clip, or more to take in the window and the playhead), with ticks at the
visible window's edges.

Zoom steps by √2: two steps halve or double the time shown, from 64 to 2^24 frames per waveform
column. Zooming keeps the playhead at the same place on screen if it is visible, otherwise the
view's centre. A mostly sideways two-finger scroll over a lane pans the view; there is no gesture
zoom ([ADR-0060](../adrs/0060-the-window-is-laid-out-as-top-bar-track-pane.md)).

## Gestures

While a slider is dragged its value goes straight to the running graph's parameter; releasing it
records one undo step. A mute or solo press is one step. Undo and redo are refused while the
transport is playing ([ADR-0056](../adrs/0056-the-interface-owns-the-session-and-reaches-i-o.md)).

## Application startup

`debussy` opens the project named on its command line, creating it if missing, imports any
`--import` files as new tracks, builds missing waveform summaries, and opens the window, playing
through the default output device, which must run at the project rate. With `--export` it renders
the project to a file and exits instead. Command-line import and export stand in until the
interface offers them ([ADR-0057](../adrs/0057-the-first-binary-imports-and-exports-from-the.md)).

## Files and devices

Import and export use the system's file dialogs through `rfd`, used only in `debussy`. Each
imported file becomes a track holding one clip of the whole file at the start, as one undo step.
Import is refused while playing.

One audio device is chosen for both input and output. The choice is remembered in the application
settings; at launch a remembered device that is gone falls back to the system default output. The
device can be changed only while stopped: the engine's real-time side is handed back from the old
stream and handed to the new one without being rebuilt, and if the new device fails to open the
old one is reopened. An audio device that stops with an error stops playback and is reported on
the status line ([ADR-0059](../adrs/0059-the-window-imports-exports-and-picks-one-audio.md)).
