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
- **Audio device:** the dropdown at the bottom left picks the device used for playback and
  recording. Change it while stopped; the choice is remembered. If the device is unplugged,
  playback stops and moves to the default output (a take being recorded is kept); plug it back
  in and pick it again from the dropdown.
- **Tracks:** one row per track with its waveform and the playhead.
- **Mixer:** one strip per mixer channel, with gain, pan, **M**ute and **S**olo. Changes are heard
  while you drag; letting go makes one undo step.

## Recording

Recording needs a device with both inputs and outputs, such as an audio interface. (A Mac's
built-in speakers and microphone are separate devices; combine them in Audio MIDI Setup as an
aggregate device to record with them.)

1. While stopped, click **Arm** on each track to record. The first armed track opens the device's
   inputs; macOS asks for microphone access the first time.
2. Choose the track's input (**In 1**, or **In 1–2** for a stereo track): the meter beside it
   moves when you play or speak into that input, and turns red when it clips. Turn **Monitor** on
   to hear the input through the track's mixer channel. **Arm** turns red while armed.
3. Move the playhead where the take should start and press record (⏺). The other tracks play
   while you record. The record button turns red with a black dot while recording.
4. Press stop or pause. Each armed track gets a new clip on top of what it overlaps, lined up with
   what you heard; one **Undo** removes the whole take.

Takes are saved in the project as WAV files. Arm settings are not saved with the project.

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

Punch-in and loop recording, editing clips in the window, adding mixers and effects (the patch
board), and spectrograms.
