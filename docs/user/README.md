# Using Debussy

Debussy is in early development. This guide covers what works today.

## Starting

Build once with `cargo build --release`, then run:

```sh
target/release/debussy
```

The window opens on a start screen. Pick a sample rate (44.1, 48, 88.2 or 96 kHz; 48 kHz unless
you choose another) and click **New project…** to choose where to save it, or **Open project…**
to open an existing `.debussy` project. The project's sample rate is fixed once it is created.

With a project open, the ≡ menu at the top right has **New project…** (⌘N) and **Open…** (⌘O);
the open project is saved before the other one opens. Both need playback stopped and no track
armed. New project will not overwrite something that already exists.

A project can also be named on the command line:

```sh
target/release/debussy MySong.debussy
```

A project that does not exist yet is created at 48 kHz; add `--rate 44100` (or 88200, 96000) to
choose another rate.

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
- **Audio device:** the headphones-and-microphone button at the bottom left lists the devices;
  pick the one used for playback and
  recording. Change it while stopped or paused; the choice is remembered. If the device is
  unplugged, playback pauses and moves to the default output (a take being recorded is kept);
  plug it back in and pick it again from that list.
- **Tracks:** one row per track with its waveform and the playhead.
- **Mixer:** one strip per mixer channel, with gain, pan, **M**ute and **S**olo. Changes are heard
  while you drag; letting go makes one undo step.

## Recording

Recording needs a device with both inputs and outputs, such as an audio interface. (A Mac's
built-in speakers and microphone are separate devices; combine them in
[Audio MIDI Setup](https://support.apple.com/en-ca/guide/audio-midi-setup/ams59f301fda/mac) as an
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

### The click

The metronome button after record opens the click dialog, and is lit while the click is on. In
the dialog, turn the click **On** or **Off**; it sounds while playing or recording, never in an
export. Type the tempo in BPM and press Enter, and pick the meter (beats per bar / beat unit). The
BPM counts the beat unit, so 6/8 at 120 clicks eighth notes. The tempo and meter are saved with
the project and can be undone; change them while stopped. The sound (Block, Click or Stick) and
level are remembered on this Mac. The first beat of each bar is louder. Click outside the dialog
to close it.

The grid button beside the waveform and spectrogram buttons draws bar and beat lines over the
tracks; zoomed out, it shows fewer bars.

Recording counts in one bar first, with the click, even when the click is off; the status line
shows **Count-in**. Pick **Off** or **2 bars** in the click dialog. The take starts where the playhead
is, after the count-in.

## Routing and effects

The board on the right shows how tracks reach the output. Click **+** at its top right to add a
mixer, bus, feedback delay or effect, drag from an output to an input to connect, and click a node
to open its controls. Changes are heard while you drag; letting go makes one undo step.

- **High-pass filter** removes what is below its **Cutoff** (default 80 Hz), such as rumble;
  **Low-pass filter** removes what is above it (default 12 kHz), such as hiss. **Order** sets how
  steeply: each step adds 6 dB per octave, from 1 (gentle) to 8 (48 dB per octave). Order can only
  be changed while stopped or paused.
- **Parametric EQ** has four bands, each off until you turn it **On**. Pick its **Type**: **bell**
  boosts or cuts around its **Frequency**, **low shelf** and **high shelf** everything below or
  above it, and **notch** removes a narrow range. **Gain** is the boost or cut in dB (±24), and
  **Q** sets the width: higher is narrower. An optional **High-pass** before the bands and
  **Low-pass** after them work like the filters above; each is off until you turn it on.

Effect settings are saved with the project.

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

Punch-in and loop recording, editing clips in the window, automation, and effects beyond the
filters and equaliser.
