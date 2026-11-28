//! The synth's noise chain (`docs/architecture/instruments.md` "Synthesizer", ADR-0174): per
//! voice, white, pink or brown noise through a bell filter that tracks the note, then an
//! amplitude envelope of its own, summed with the oscillator chain. A filter envelope and an LFO
//! of its own move the bell.

use dsp::{Brown, Noise, Pink, Svf, SvfCoefficients, note_to_hz};
use modulation::{Adsr, Lfo};
use signal_graph::{Parameter, ParameterInfo, Scale};

use super::{CUTOFF_RANGE, LFO_CUTOFF_OCTAVES, LFO_SHAPES, choice, names};
use crate::error::Error;
use crate::voices::{EnvelopeParameters, peak};

/// The first voice's noise seed; voice *i* uses `NOISE_SEED + i`, fixed so a render repeats.
pub(super) const NOISE_SEED: u64 = 0x0015_E5ED_0000;
/// The noise LFO's random seed, fixed so a render repeats.
pub(super) const NOISE_LFO_SEED: u64 = 0x5EED_5718;
/// The bell's gain limit, in dB each way; also the noise LFO's gain range at full depth.
const BELL_GAIN_DB: f32 = 24.0;

/// The noise colours, in the order of the `noise type` parameter's choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Colour {
    White,
    Pink,
    Brown,
}

const COLOURS: [(&str, Colour); 3] = [
    ("white", Colour::White),
    ("pink", Colour::Pink),
    ("brown", Colour::Brown),
];

/// What the noise LFO moves, in the order of the `noise lfo destination` parameter's choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NoiseDestination {
    BellFrequency,
    BellGain,
    Amplitude,
}

const NOISE_DESTINATIONS: [(&str, NoiseDestination); 3] = [
    ("bell frequency", NoiseDestination::BellFrequency),
    ("bell gain", NoiseDestination::BellGain),
    ("amplitude", NoiseDestination::Amplitude),
];

/// The noise chain's parameters.
pub(super) struct NoiseParameters {
    pub kind: Parameter,
    pub level: Parameter,
    pub octave: Parameter,
    pub gain: Parameter,
    pub q: Parameter,
    pub env_amount: Parameter,
    pub filter_env: EnvelopeParameters,
    pub amp_env: EnvelopeParameters,
    pub amp_velocity: Parameter,
    pub lfo_rate: Parameter,
    pub lfo_shape: Parameter,
    pub lfo_destination: Parameter,
    pub lfo_depth: Parameter,
}

impl NoiseParameters {
    pub fn new() -> Result<Self, Error> {
        let grouped = |group: &'static str| {
            move |name: &str, min, max, default| {
                ParameterInfo::new(name, min, max, default).with_group(group)
            }
        };
        let (noise, filter, lfo) = (
            grouped("noise"),
            grouped("noise filter"),
            grouped("noise lfo"),
        );
        Ok(NoiseParameters {
            kind: Parameter::new(
                noise("noise type", 0.0, 2.0, 0.0).with_scale(Scale::Choice(names(&COLOURS))),
            )?,
            // 0 by default, so a project saved before the noise chain sounds as it did.
            level: Parameter::new(noise("noise level", 0.0, 1.0, 0.0))?,
            octave: Parameter::new(filter("noise filter octave", -4.0, 4.0, 0.0).with_unit("oct"))?,
            gain: Parameter::new(
                filter("noise filter gain", -BELL_GAIN_DB, BELL_GAIN_DB, 12.0).with_unit("dB"),
            )?,
            q: Parameter::new(filter("noise filter q", 0.5, 12.0, 1.0).with_label("Q"))?,
            env_amount: Parameter::new(
                filter("noise filter envelope amount", -5.0, 5.0, 0.0).with_unit("oct"),
            )?,
            filter_env: EnvelopeParameters::new(
                "noise filter",
                "noise filter envelope",
                [0.005, 0.3, 0.3, 0.3],
            )?,
            amp_env: EnvelopeParameters::new(
                "noise amp",
                "noise amp envelope",
                [0.005, 0.2, 0.8, 0.25],
            )?,
            amp_velocity: Parameter::new(
                ParameterInfo::new("noise amp velocity", 0.0, 1.0, 0.7)
                    .with_group("noise amp envelope"),
            )?,
            lfo_rate: Parameter::new(
                lfo("noise lfo rate", 0.01, 20.0, 5.0)
                    .with_unit("Hz")
                    .with_scale(Scale::Logarithmic),
            )?,
            lfo_shape: Parameter::new(
                lfo("noise lfo shape", 0.0, 5.0, 0.0).with_scale(Scale::Choice(names(&LFO_SHAPES))),
            )?,
            lfo_destination: Parameter::new(
                lfo("noise lfo destination", 0.0, 2.0, 0.0)
                    .with_scale(Scale::Choice(names(&NOISE_DESTINATIONS))),
            )?,
            lfo_depth: Parameter::new(lfo("noise lfo depth", 0.0, 1.0, 0.0))?,
        })
    }

    /// Every parameter, in declaration order.
    pub fn all(&self) -> Vec<&Parameter> {
        let mut all = vec![
            &self.kind,
            &self.level,
            &self.octave,
            &self.gain,
            &self.q,
            &self.env_amount,
        ];
        all.extend(self.filter_env.all());
        all.extend(self.amp_env.all());
        all.extend([
            &self.amp_velocity,
            &self.lfo_rate,
            &self.lfo_shape,
            &self.lfo_destination,
            &self.lfo_depth,
        ]);
        all
    }

    /// The colour the `noise type` parameter names.
    pub fn colour(&self) -> Colour {
        COLOURS[choice(self.kind.get(), COLOURS.len())].1
    }

    /// The destination the `noise lfo destination` parameter names.
    pub fn destination(&self) -> NoiseDestination {
        NOISE_DESTINATIONS[choice(self.lfo_destination.get(), NOISE_DESTINATIONS.len())].1
    }

    /// Sets `lfo`'s rate and shape from the parameters.
    pub fn configure_lfo(&self, lfo: &mut Lfo) {
        lfo.set_frequency(f64::from(self.lfo_rate.get()));
        lfo.set_shape(LFO_SHAPES[choice(self.lfo_shape.get(), LFO_SHAPES.len())].1);
    }
}

/// The noise chain's values for one sample, shared by every voice.
#[derive(Debug, Clone, Copy)]
pub(super) struct NoiseFrame {
    pub colour: Colour,
    pub level: f32,
    /// Octaves the bell sits from the note.
    pub octave: f32,
    pub gain_db: f32,
    pub q: f32,
    /// Octaves the filter envelope moves the bell at its peak.
    pub env_amount: f32,
    /// Octaves the LFO moves the bell.
    pub lfo_octaves: f32,
    /// Decibels the LFO adds to the bell's gain.
    pub lfo_gain_db: f32,
    /// The amplitude the noise is multiplied by for the LFO.
    pub tremolo: f32,
}

impl NoiseFrame {
    /// The frame for `colour`, `destination` and the values, with the noise LFO at `lfo` (−1
    /// to 1) and `depth`.
    #[allow(clippy::too_many_arguments)] // each is one parameter's value for the sample
    pub fn new(
        colour: Colour,
        destination: NoiseDestination,
        level: f32,
        octave: f32,
        gain_db: f32,
        q: f32,
        env_amount: f32,
        lfo: f32,
        depth: f32,
    ) -> Self {
        let moved = |d| if destination == d { lfo * depth } else { 0.0 };
        NoiseFrame {
            colour,
            level,
            octave,
            gain_db,
            q,
            env_amount,
            lfo_octaves: moved(NoiseDestination::BellFrequency) * LFO_CUTOFF_OCTAVES,
            lfo_gain_db: moved(NoiseDestination::BellGain) * BELL_GAIN_DB,
            tremolo: if destination == NoiseDestination::Amplitude {
                1.0 - depth * 0.5 * (1.0 - lfo)
            } else {
                1.0
            },
        }
    }
}

/// The bell's centre in Hz for a note at `pitch` semitones (note and bend), `octaves` from it in
/// all (the offset, the envelope and the LFO), clamped from 20 Hz to `max`.
pub(super) fn bell_centre(pitch: f32, octaves: f32, max: f32) -> f32 {
    (note_to_hz(pitch).log2() + octaves)
        .exp2()
        .clamp(CUTOFF_RANGE.0, max)
}

/// The bell's gain in dB: `gain_db` and the LFO's `lfo_db`, clamped to the gain range.
pub(super) fn bell_gain(gain_db: f32, lfo_db: f32) -> f32 {
    (gain_db + lfo_db).clamp(-BELL_GAIN_DB, BELL_GAIN_DB)
}

/// One voice's noise chain: its generators, bell and two envelopes.
#[derive(Debug, Clone)]
pub(super) struct NoiseVoice {
    white: Noise,
    pink: Pink,
    brown: Brown,
    bell: Svf,
    pub filter_env: Adsr,
    pub amp: Adsr,
    /// Whether the noise level is above 0 this block; while it is not, the chain is silent and
    /// does not keep its voice busy.
    pub on: bool,
}

impl NoiseVoice {
    /// A silent chain whose generators start from `seed`, at `sample_rate`. Allocates nothing.
    pub fn new(seed: u64, sample_rate: u32) -> Self {
        NoiseVoice {
            white: Noise::new(seed),
            pink: Pink::new(seed, sample_rate),
            brown: Brown::new(seed, sample_rate),
            bell: Svf::new(),
            filter_env: Adsr::new(),
            amp: Adsr::new(),
            on: false,
        }
    }

    /// The next noise sample of `colour`, about −1 to 1; only that colour's generator advances.
    /// All three share the uniform values' level at 1 kHz.
    pub fn sample(&mut self, colour: Colour) -> f32 {
        2.0 * match colour {
            Colour::White => self.white.uniform(),
            Colour::Pink => self.pink.sample(),
            Colour::Brown => self.brown.sample(),
        }
    }

    /// Gates both envelopes on; the amplitude envelope peaks for `velocity` with `amount` of
    /// sensitivity.
    pub fn start(&mut self, velocity: f32, amount: f32) {
        self.filter_env.gate_on(1.0);
        self.amp.gate_on(peak(velocity, amount));
    }

    pub fn release(&mut self) {
        self.filter_env.gate_off();
        self.amp.gate_off();
    }

    pub fn reset(&mut self) {
        self.bell.reset();
        self.filter_env.reset();
        self.amp.reset();
    }

    /// Whether the chain is on and its envelope still sounds.
    pub fn sounding(&self) -> bool {
        self.on && !self.amp.is_idle()
    }

    /// The amplitude envelope's level while the chain is on, else 0.
    pub fn level(&self) -> f32 {
        if self.on { self.amp.level() } else { 0.0 }
    }

    /// The next sample for a note at `pitch` semitones (note and bend). The envelopes advance
    /// whether or not the chain is on; while it is off the result is 0 and nothing else runs.
    pub fn render(&mut self, pitch: f32, f: &NoiseFrame, sample_rate: f32, max: f32) -> f32 {
        let env = self.filter_env.next_value();
        let amp = self.amp.next_value();
        if !self.on {
            return 0.0;
        }
        let centre = bell_centre(pitch, f.octave + f.env_amount * env + f.lfo_octaves, max);
        let gain = bell_gain(f.gain_db, f.lfo_gain_db);
        let c = SvfCoefficients::bell(centre, f.q, gain, sample_rate);
        let x = self.sample(f.colour);
        self.bell.process(x, &c) * amp * f.level * f.tremolo
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bell_centre_tracks_note_octave_and_envelope_and_clamps() {
        let max = 20_000.0;
        assert!((bell_centre(69.0, 0.0, max) - 440.0).abs() < 1e-2);
        assert!((bell_centre(81.0, 0.0, max) - 880.0).abs() < 1e-2);
        // The octave offset and the envelope both count in octaves.
        assert!((bell_centre(69.0, 1.0, max) - 880.0).abs() < 1e-2);
        assert!((bell_centre(69.0, -2.0, max) - 110.0).abs() < 1e-2);
        assert_eq!(bell_centre(0.0, -4.0, max), CUTOFF_RANGE.0);
        assert_eq!(bell_centre(127.0, 4.0, 10_000.0), 10_000.0);
    }

    #[test]
    fn test_bell_gain_clamps() {
        assert_eq!(bell_gain(12.0, 6.0), 18.0);
        assert_eq!(bell_gain(20.0, 24.0), BELL_GAIN_DB);
        assert_eq!(bell_gain(-20.0, -24.0), -BELL_GAIN_DB);
    }

    #[test]
    fn test_each_noise_lfo_destination_moves_only_its_target() {
        let frame = |d| NoiseFrame::new(Colour::White, d, 1.0, 0.0, 0.0, 1.0, 0.0, -1.0, 1.0);
        let f = frame(NoiseDestination::BellFrequency);
        assert_eq!(
            (f.lfo_octaves, f.lfo_gain_db, f.tremolo),
            (-LFO_CUTOFF_OCTAVES, 0.0, 1.0)
        );
        let f = frame(NoiseDestination::BellGain);
        assert_eq!(
            (f.lfo_octaves, f.lfo_gain_db, f.tremolo),
            (0.0, -BELL_GAIN_DB, 1.0)
        );
        let f = frame(NoiseDestination::Amplitude);
        assert_eq!((f.lfo_octaves, f.lfo_gain_db, f.tremolo), (0.0, 0.0, 0.0));
    }

    #[test]
    fn test_noise_types_differ_and_are_deterministic() {
        let run = |colour| {
            let mut voice = NoiseVoice::new(NOISE_SEED, 48_000);
            (0..256).map(|_| voice.sample(colour)).collect::<Vec<f32>>()
        };
        let [white, pink, brown] = [Colour::White, Colour::Pink, Colour::Brown].map(run);
        assert_eq!(white, run(Colour::White));
        assert_eq!(brown, run(Colour::Brown));
        assert_ne!(white, pink);
        assert_ne!(pink, brown);
        assert!(white.iter().all(|s| (-1.0..1.0).contains(s)));
        assert!(white.iter().any(|s| s.abs() > 0.5));
    }

    #[test]
    fn test_off_chain_is_silent_and_never_sounding() {
        let mut voice = NoiseVoice::new(NOISE_SEED, 48_000);
        voice.amp.set(0.0, 0.0, 1.0, 0.1, 48_000.0);
        voice.start(1.0, 0.0);
        let f = NoiseFrame::new(
            Colour::White,
            NoiseDestination::Amplitude,
            1.0,
            0.0,
            12.0,
            1.0,
            0.0,
            0.0,
            0.0,
        );
        assert_eq!(voice.render(69.0, &f, 48_000.0, 20_000.0), 0.0);
        assert!(!voice.sounding());
        assert_eq!(voice.level(), 0.0);
        voice.on = true;
        assert!(voice.sounding());
        assert!(voice.level() > 0.0);
        assert_ne!(voice.render(69.0, &f, 48_000.0, 20_000.0), 0.0);
    }
}
