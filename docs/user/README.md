# Using Debussy

Debussy is in early development. This guide covers what works today.

## Starting

Build once with `cargo build --release`, then run:

```sh
target/release/debussy MySong.debussy
```

A project that does not exist yet is created at 48 kHz; add `--rate 44100` (or 88200, 96000) to
choose another rate. The project's sample rate is fixed once it is created.

Playback uses the audio device chosen in the window (at first, the system's default output),
which must support the project's rate.

## Importing audio

Click **Import…** and choose one or more files (while stopped). Each becomes a new track starting
at the beginning of the timeline. WAV, FLAC, AAC (`.m4a`) and MP3 files are accepted, mono or
stereo, at any sample rate; they are converted to the project's rate. The command line can do the
same before the window opens:

```sh
target/release/debussy MySong.debussy --import guitar.wav --import vocal.flac
```

New tracks join the project's mixer: a mono track on one channel, a stereo track as two channels,
its left (**L**) panned hard left and its right (**R**) hard right. (Projects made before this
mixer existed send tracks straight to the output.)

## The window

- **Transport:** rewind (⏮), play or pause (▶ / ⏸), stop (⏹, returns to where playback
  started), the playhead time, and **Mono** to listen to the mono mix.
- **View:** **−** and **+** zoom the track lanes out and in; ◀ and ▶▶ scroll.
- **Undo / Redo / Save.** Undo and redo are unavailable while playing.
- **Import… / Export… / Export mono…** open file dialogs.
- **Audio device:** the device used for playback (and, later, recording). Change it while
  stopped; the choice is remembered.
- **Tracks:** one row per track with its waveform and the playhead.
- **Mixer:** one strip per mixer channel, with gain, pan, **M**ute and **S**olo. Changes are heard
  while you drag; letting go makes one undo step.

## Exporting

Click **Export…** or **Export mono…** and choose where; the file name's extension picks the
format. Or, from the command line:

```sh
target/release/debussy MySong.debussy --export mix.wav     # 24-bit WAV
target/release/debussy MySong.debussy --export mix.flac    # 24-bit FLAC
target/release/debussy MySong.debussy --export mix.m4a --mono   # AAC, mono mix
```

Export renders the whole project and exits without opening the window.

## Not yet available

Recording, editing clips in the window, adding mixers and effects (the patch board), and
spectrograms.
