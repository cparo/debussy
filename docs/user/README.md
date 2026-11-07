# Using Debussy

Debussy is in early development. This guide covers what works today.

## Starting

Build once with `cargo build --release`, then run:

```sh
target/release/debussy MySong.debussy
```

A project that does not exist yet is created at 48 kHz; add `--rate 44100` (or 88200, 96000) to
choose another rate. The project's sample rate is fixed once it is created.

Playback uses the system's default output device, which must support the project's rate.

## Importing audio

The window has no import button yet. Import from the command line; each file becomes a new track
starting at the beginning of the timeline:

```sh
target/release/debussy MySong.debussy --import guitar.wav --import vocal.flac
```

WAV, FLAC, AAC (`.m4a`) and MP3 files are accepted, mono or stereo, at any sample rate; they are
converted to the project's rate. New tracks play straight to the output.

## The window

- **Transport:** rewind (⏮), play or pause (▶ / ⏸), stop (⏹, returns to where playback
  started), the playhead time, and **Mono** to listen to the mono mix.
- **View:** **−** and **+** zoom the track lanes out and in; ◀ and ▶▶ scroll.
- **Undo / Redo / Save.** Undo and redo are unavailable while playing.
- **Tracks:** one row per track with its waveform and the playhead.
- **Mixer:** one strip per mixer channel, with gain, pan, **M**ute and **S**olo. Changes are heard
  while you drag; letting go makes one undo step. New projects have no mixer yet, so this area is
  empty until routing can be edited from the window.

## Exporting

```sh
target/release/debussy MySong.debussy --export mix.wav     # 24-bit WAV
target/release/debussy MySong.debussy --export mix.flac    # 24-bit FLAC
target/release/debussy MySong.debussy --export mix.m4a --mono   # AAC, mono mix
```

Export renders the whole project and exits without opening the window.

## Not yet available

Recording, editing clips in the window, adding mixers and effects (the patch board), and
spectrograms.
