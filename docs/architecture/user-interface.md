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

The first screens are the transport bar (play, pause, stop, rewind, mono listening, zoom,
scroll, undo, redo, save, import, export, export mono, and the audio device), one waveform lane per track drawn from the stored files' summaries
(the top clip shown where clips overlap), and a strip per mixer channel with gain, pan, mute and
solo ([ADR-0056](../adrs/0056-the-interface-owns-the-session-and-reaches-i-o.md)).

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
