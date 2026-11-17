# Instruments

The built-in instrument nodes, in the `instruments` crate. Their ports and how they are saved are
in `docs/architecture/signal-graph.md` "Instruments".

## Synthesizer

`instruments::Synth`, kind id `synth`, shown as **Synthesizer**: a polyphonic subtractive
synthesizer. Event input 0 takes notes on any channel; stereo output 0 carries the voices summed to
mono, the same signal on both channels.

- **Voices.** A fixed pool of 16 voices, made in `prepare`; `process` allocates nothing. The
  `voices` parameter (1 to 16) limits which voices a new note may take; voices above it finish
  their notes.
- **Signal flow per voice.** Two oscillators (`dsp::Oscillator`, saw, pulse or triangle,
  band-limited by `dsp::MinBlep`), each at the note plus its octave and detune and scaled by its
  level, are summed; a 5 Hz one-pole high-pass removes their DC; a resonant state-variable
  low-pass filters them; the amplitude envelope (`modulation::Adsr`) scales the result.
- **Pitch.** 12-tone equal temperament from A4 = 440 Hz (`dsp::note_to_hz`). Pitch bend moves
  every voice by the bend position (−1 to 1, from the 14-bit value centred on 8192) times
  `bend range` semitones, ramped.
- **Cutoff.** In log2 Hz per sample: the `cutoff` parameter, plus `envelope amount` octaves times
  the filter envelope (0 to 1), plus `velocity amount` octaves times the note's velocity (0 to 1),
  plus the LFO; clamped to 20 Hz and to the lower of 20 kHz and 0.49 of the sample rate.
- **Envelopes.** Exponential one-pole segments; each gate-on attacks from the current level. The
  amplitude envelope peaks at `1 − amp velocity · (1 − velocity)`; the filter envelope at 1.
- **LFO.** One `modulation::Lfo` shared by all voices, its phase reset by `reset`. At full
  `lfo depth` it moves pitch ±2 semitones, cutoff ±4 octaves, or amplitude down to 0 at the
  bottom of its cycle (`1 − depth · (1 − lfo) / 2`).
- **Notes.** A note-on with velocity 0 is a note-off. A note-on takes, in order: the voice already
  playing that note, retriggered; a free voice; the quietest voice in its release; the voice whose
  note started first. Taking a sounding voice steals it: it fades out over 3 ms, then the new note
  starts from silence. A note-off releases its voice, unless the sustain pedal (controller 64,
  down at 64 and above) is down, in which case the voice releases when the pedal lifts. Other
  messages are ignored.
- **Parameters.** Read once per block; continuous ones are ramped (the cutoff in log2 Hz). The
  choices (`osc N wave`, `lfo shape`, `lfo destination`) and whole numbers (`osc N octave`,
  `voices`) are stepped and take effect at the next block; every other parameter is automatable.

  | Group | Parameters |
  |---|---|
  | oscillator 1, 2 | wave, octave (±3), detune (±100 cents), pulse width (5–95%), level (0–1) |
  | filter | cutoff (20 Hz–20 kHz), resonance (Q 0.5–12), envelope amount (±5 oct), velocity amount (0–4 oct) |
  | filter envelope, amp envelope | attack, decay, release (1 ms–10 s), sustain (0–1); amp velocity (0–1) |
  | lfo | rate (0.01–20 Hz), shape, destination (pitch, cutoff, amplitude), depth (0–1) |
  | (none) | bend range (0–24 st), voices (1–16), gain (−48 to +12 dB) |
- **Reset.** Every voice falls silent at once, the pedal lifts, the bend returns to centre and the
  LFO returns to phase 0.

The design follows [ADR-0148](../adrs/0148-the-synth-s-oscillators-use-minblep-with-polyblep.md)
and [ADR-0149](../adrs/0149-the-synthesizer-is-a-fixed-pool-of-two-oscillator.md).

## Sampler

`instruments::Sampler`, kind id `sampler`, shown as **Sampler**: plays the project's own audio
files, mapped to notes by zones. Event input 0 takes notes on any channel; stereo output 0 carries
the voices summed, a stereo zone's channels on their own sides and a mono zone's on both.

- **Zones.** A sampler node carries a list of zones, each a file in the bundle's `audio/`
  directory (mono or stereo), a key range (`low` to `high`, 0–127), a root key, a mode (one-shot or
  gated), an optional choke group (1–16) and a tune in whole semitones (−24 to 24, default 0;
[ADR-0158](../adrs/0158-each-sampler-zone-has-its-own-semitone-tune-saved.md)), at most 128 zones. Only a `sampler` instrument has
  zones; choosing a different instrument removes them. Zone files count as used for cleanup
  (`docs/architecture/project.md` "Cleanup").
- **Loading.** Zone samples are imported like any other audio, at the project rate, and are at
  most 60 s long (`project_model::MAX_ZONE_SECONDS`, enforced by `import_export::import_sample`).
  The engine bridge reads them whole into memory off the audio thread (`engine_bridge::Samples`,
  cached by file name) and builds a new sampler node when the zones change; a missing or
  unreadable file leaves its zone silent and is reported as a warning
  ([ADR-0154](../adrs/0154-sampler-zones-are-saved-on-the-instrument-node-and.md)).
- **Notes.** A note-on plays the first zone whose range holds the note; a note in no zone is
  ignored. Voices come from the synthesizer's pool (16 voices, the same choice of voice, steal
  fade and sustain pedal, `instruments::voices`), except that a repeated note fades its voice out
  over 3 ms and restarts it rather than retriggering in place. A one-shot zone ignores note-off and
  the pedal; a gated zone releases through the amplitude envelope.
- **Playback.** Each voice steps a fractional playhead from the sample's first frame by
  `2^((note − root + zone tune + tune + bend · bend range)/12)` frames per sample, `tune` being the
sampler's parameter and reads it by 4-point
  Hermite interpolation (`dsp::hermite`); frames outside the sample read as zero. A voice frees
  itself when its playhead passes the end.
- **Choke.** A note-on in choke group g fades out over 3 ms every other voice sounding, or waiting
  to start, in group g.
- **Velocity.** The amplitude envelope peaks at `1 − amp velocity · (1 − velocity)`.
- **Parameters.** Read once per block; continuous ones are ramped, `voices` is stepped.

  | Group | Parameters |
  |---|---|
  | amp envelope | attack, decay, release (1 ms–10 s), sustain (0–1); amp velocity (0–1) |
  | (none) | tune (±12 st), bend range (0–24 st), voices (1–16), gain (−48 to +12 dB) |
- **Reset.** Every voice falls silent at once, the pedal lifts and the bend returns to centre.

The design follows [ADR-0155](../adrs/0155-the-sampler-s-varispeed-playback-zones-one-shot.md).
