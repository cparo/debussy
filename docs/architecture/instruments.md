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
