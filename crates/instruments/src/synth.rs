//! [`Synth`]: the built-in polyphonic subtractive synthesizer (`docs/architecture/instruments.md`
//! "Synthesizer", ADR-0148).
//!
//! Each voice sums two MinBLEP oscillators, removes their DC, and runs them through a resonant
//! state-variable low-pass and an amplitude envelope; a second envelope, velocity and the shared
//! LFO move the cutoff. Voices are summed to mono and written to both output channels.

use audio_buffers::Layout;
use dsp::{
    MinBlep, OnePole, OnePoleCoefficients, Oscillator, Svf, SvfCoefficients, Wave, db_to_gain,
    note_to_hz,
};
use midi::Kind;
use modulation::{Adsr, Lfo, Shape};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;
use crate::voices::{EnvelopeParameters, Pool, SUSTAIN_PEDAL, Sound, Voice};

/// The most voices a synth plays at once; the `voices` parameter limits it further.
pub const MAX_VOICES: usize = 16;
/// The lowest and highest cutoff, in Hz.
const CUTOFF_RANGE: (f32, f32) = (20.0, 20_000.0);
/// The highest cutoff as a share of the sample rate, kept below the filter's limit
/// (`docs/research/synth-patents.md`, element 3).
const MAX_CUTOFF_RATIO: f32 = 0.49;
/// The corner of each voice's DC blocker, in Hz.
const DC_BLOCK_HZ: f32 = 5.0;
/// The pitch range of the LFO at full depth, in semitones each way.
const LFO_PITCH_SEMITONES: f32 = 2.0;
/// The cutoff range of the LFO at full depth, in octaves each way.
const LFO_CUTOFF_OCTAVES: f32 = 4.0;
/// The pitch-bend value at rest, and its half range.
const BEND_CENTRE: f32 = 8192.0;
/// The LFO's random seed, fixed so a render repeats.
const LFO_SEED: u64 = 0x5EED_5717;

/// The oscillator waveforms, in the order of the `wave` parameters' choices.
const WAVES: [(&str, Wave); 3] = [
    ("saw", Wave::Saw),
    ("pulse", Wave::Pulse),
    ("triangle", Wave::Triangle),
];
/// The LFO shapes, in the order of the `lfo shape` parameter's choices.
const LFO_SHAPES: [(&str, Shape); 6] = [
    ("sine", Shape::Sine),
    ("triangle", Shape::Triangle),
    ("saw up", Shape::SawUp),
    ("saw down", Shape::SawDown),
    ("square", Shape::Square),
    ("sample and hold", Shape::SampleAndHold),
];
/// What the LFO moves, in the order of the `lfo destination` parameter's choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destination {
    Pitch,
    Cutoff,
    Amplitude,
}
const DESTINATIONS: [(&str, Destination); 3] = [
    ("pitch", Destination::Pitch),
    ("cutoff", Destination::Cutoff),
    ("amplitude", Destination::Amplitude),
];

/// The index a choice parameter value names, from 0 to `len − 1`.
fn choice(value: f32, len: usize) -> usize {
    // Rounded, non-negative and small; `as` saturates and NaN becomes 0.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let index = value.round().max(0.0) as usize;
    index.min(len.saturating_sub(1))
}

/// The names of a list of choices.
fn names<T>(choices: &[(&str, T)]) -> Vec<String> {
    choices.iter().map(|(name, _)| (*name).to_owned()).collect()
}

/// One oscillator's parameters.
struct OscillatorParameters {
    wave: Parameter,
    octave: Parameter,
    detune: Parameter,
    width: Parameter,
    level: Parameter,
}

impl OscillatorParameters {
    /// Oscillator `n`'s parameters, starting at `detune` cents and `level`.
    fn new(n: usize, detune: f32, level: f32) -> Result<Self, Error> {
        let group = format!("oscillator {n}");
        let info = |name: &str, min, max, default| {
            ParameterInfo::new(format!("osc {n} {name}"), min, max, default).with_group(&group)
        };
        Ok(OscillatorParameters {
            wave: Parameter::new(
                info("wave", 0.0, 2.0, 0.0).with_scale(Scale::Choice(names(&WAVES))),
            )?,
            octave: Parameter::new(info("octave", -3.0, 3.0, 0.0).with_scale(Scale::Integer))?,
            detune: Parameter::new(info("detune", -100.0, 100.0, detune).with_unit("cents"))?,
            width: Parameter::new(info("pulse width", 5.0, 95.0, 50.0).with_unit("%"))?,
            level: Parameter::new(info("level", 0.0, 1.0, level))?,
        })
    }

    fn all(&self) -> [&Parameter; 5] {
        [
            &self.wave,
            &self.octave,
            &self.detune,
            &self.width,
            &self.level,
        ]
    }
}

/// Every parameter, in the order they are declared.
struct Parameters {
    osc: [OscillatorParameters; 2],
    cutoff: Parameter,
    resonance: Parameter,
    filter_amount: Parameter,
    filter_velocity: Parameter,
    filter_env: EnvelopeParameters,
    amp_env: EnvelopeParameters,
    amp_velocity: Parameter,
    lfo_rate: Parameter,
    lfo_shape: Parameter,
    lfo_destination: Parameter,
    lfo_depth: Parameter,
    bend_range: Parameter,
    voices: Parameter,
    gain: Parameter,
}

impl Parameters {
    fn new() -> Result<Self, Error> {
        let filter = |name: &str, min, max, default| {
            ParameterInfo::new(name, min, max, default).with_group("filter")
        };
        let lfo = |name: &str, min, max, default| {
            ParameterInfo::new(name, min, max, default).with_group("lfo")
        };
        Ok(Parameters {
            osc: [
                OscillatorParameters::new(1, 0.0, 1.0)?,
                OscillatorParameters::new(2, 7.0, 0.5)?,
            ],
            cutoff: Parameter::new(
                filter("cutoff", CUTOFF_RANGE.0, CUTOFF_RANGE.1, 2_000.0)
                    .with_unit("Hz")
                    .with_scale(Scale::Logarithmic),
            )?,
            resonance: Parameter::new(filter("resonance", 0.5, 12.0, 0.707))?,
            filter_amount: Parameter::new(
                filter("envelope amount", -5.0, 5.0, 2.0).with_unit("oct"),
            )?,
            filter_velocity: Parameter::new(
                filter("velocity amount", 0.0, 4.0, 0.0).with_unit("oct"),
            )?,
            filter_env: EnvelopeParameters::new(
                "filter",
                "filter envelope",
                [0.005, 0.3, 0.3, 0.3],
            )?,
            amp_env: EnvelopeParameters::new("amp", "amp envelope", [0.005, 0.2, 0.8, 0.25])?,
            amp_velocity: Parameter::new(
                ParameterInfo::new("amp velocity", 0.0, 1.0, 0.7).with_group("amp envelope"),
            )?,
            lfo_rate: Parameter::new(
                lfo("lfo rate", 0.01, 20.0, 5.0)
                    .with_unit("Hz")
                    .with_scale(Scale::Logarithmic),
            )?,
            lfo_shape: Parameter::new(
                lfo("lfo shape", 0.0, 5.0, 0.0).with_scale(Scale::Choice(names(&LFO_SHAPES))),
            )?,
            lfo_destination: Parameter::new(
                lfo("lfo destination", 0.0, 2.0, 0.0)
                    .with_scale(Scale::Choice(names(&DESTINATIONS))),
            )?,
            lfo_depth: Parameter::new(lfo("lfo depth", 0.0, 1.0, 0.0))?,
            bend_range: Parameter::new(
                ParameterInfo::new("bend range", 0.0, 24.0, 2.0)
                    .with_unit("st")
                    .with_step(1.0),
            )?,
            voices: Parameter::new(
                ParameterInfo::new("voices", 1.0, 16.0, 8.0).with_scale(Scale::Integer),
            )?,
            gain: Parameter::new(ParameterInfo::new("gain", -48.0, 12.0, -12.0).with_unit("dB"))?,
        })
    }

    fn all(&self) -> Vec<Parameter> {
        let mut all: Vec<Parameter> = Vec::new();
        for osc in &self.osc {
            all.extend(osc.all().into_iter().cloned());
        }
        all.extend([
            self.cutoff.clone(),
            self.resonance.clone(),
            self.filter_amount.clone(),
            self.filter_velocity.clone(),
        ]);
        all.extend(self.filter_env.all().into_iter().cloned());
        all.extend(self.amp_env.all().into_iter().cloned());
        all.extend([
            self.amp_velocity.clone(),
            self.lfo_rate.clone(),
            self.lfo_shape.clone(),
            self.lfo_destination.clone(),
            self.lfo_depth.clone(),
            self.bend_range.clone(),
            self.voices.clone(),
            self.gain.clone(),
        ]);
        all
    }
}

/// The ramped values read once per block and stepped every sample.
struct Ramps {
    /// Per oscillator: detune (cents), pulse width (share), level.
    detune: [Ramp; 2],
    width: [Ramp; 2],
    level: [Ramp; 2],
    /// The cutoff in log2(Hz).
    cutoff: Ramp,
    resonance: Ramp,
    filter_amount: Ramp,
    filter_velocity: Ramp,
    amp_velocity: Ramp,
    lfo_depth: Ramp,
    /// The output gain, linear.
    gain: Ramp,
    /// The pitch bend, in semitones.
    bend: Ramp,
}

impl Ramps {
    fn new(p: &Parameters, sample_rate: u32) -> Self {
        let ramp = |value: f32| Ramp::new(value, sample_rate);
        Ramps {
            detune: [ramp(p.osc[0].detune.get()), ramp(p.osc[1].detune.get())],
            width: [
                ramp(p.osc[0].width.get() / 100.0),
                ramp(p.osc[1].width.get() / 100.0),
            ],
            level: [ramp(p.osc[0].level.get()), ramp(p.osc[1].level.get())],
            cutoff: ramp(p.cutoff.get().log2()),
            resonance: ramp(p.resonance.get()),
            filter_amount: ramp(p.filter_amount.get()),
            filter_velocity: ramp(p.filter_velocity.get()),
            amp_velocity: ramp(p.amp_velocity.get()),
            lfo_depth: ramp(p.lfo_depth.get()),
            gain: ramp(db_to_gain(p.gain.get())),
            bend: ramp(0.0),
        }
    }

    /// Takes each parameter's reading, once per block.
    fn read(&mut self, p: &Parameters) {
        for (i, osc) in p.osc.iter().enumerate() {
            self.detune[i].approach(osc.detune.read());
            self.width[i].approach(osc.width.read().map(|w| w / 100.0));
            self.level[i].approach(osc.level.read());
        }
        self.cutoff.approach(p.cutoff.read().map(f32::log2));
        self.resonance.approach(p.resonance.read());
        self.filter_amount.approach(p.filter_amount.read());
        self.filter_velocity.approach(p.filter_velocity.read());
        self.amp_velocity.approach(p.amp_velocity.read());
        self.lfo_depth.approach(p.lfo_depth.read());
        self.gain.approach(p.gain.read().map(db_to_gain));
    }
}

/// What every voice needs for one sample.
#[derive(Debug, Clone, Copy)]
struct Frame {
    sample_rate: f32,
    max_cutoff: f32,
    /// Semitones added to every note: pitch bend and the LFO.
    pitch: f32,
    octave: [f32; 2],
    detune: [f32; 2],
    width: [f32; 2],
    level: [f32; 2],
    cutoff: f32,
    resonance: f32,
    filter_amount: f32,
    filter_velocity: f32,
    /// Octaves added to every cutoff by the LFO.
    lfo_cutoff: f32,
    /// The amplitude every voice is multiplied by for the LFO.
    tremolo: f32,
}

/// The synth's part of a voice: its oscillators, filter and filter envelope.
#[derive(Debug, Clone)]
struct Tone {
    osc: [Oscillator; 2],
    dc: OnePole,
    svf: Svf,
    filter_env: Adsr,
}

impl Tone {
    fn new() -> Self {
        Tone {
            osc: [Oscillator::new(), Oscillator::new()],
            dc: OnePole::new(),
            svf: Svf::new(),
            filter_env: Adsr::new(),
        }
    }
}

impl Sound for Tone {
    type Start = ();

    fn start(&mut self, _note: u8, _velocity: f32, _start: ()) {
        self.filter_env.gate_on(1.0);
    }

    fn release(&mut self) {
        self.filter_env.gate_off();
    }

    fn reset(&mut self) {
        for osc in &mut self.osc {
            osc.reset();
        }
        self.dc.reset();
        self.svf.reset();
        self.filter_env.reset();
    }
}

/// The next sample of `voice`.
fn render(voice: &mut Voice<Tone>, f: &Frame, table: &MinBlep, dc: &OnePoleCoefficients) -> f32 {
    let pitch = f32::from(voice.note) + f.pitch;
    let tone = &mut voice.sound;
    let mut sum = 0.0;
    for (i, osc) in tone.osc.iter_mut().enumerate() {
        let note = pitch + 12.0 * f.octave[i] + f.detune[i] / 100.0;
        osc.set_frequency(note_to_hz(note), f.sample_rate);
        osc.set_pulse_width(f.width[i]);
        sum += f.level[i] * osc.next(table);
    }
    let sample = tone.dc.process(sum, dc);
    let filter_env = tone.filter_env.next_value();
    let amp = voice.amp.next_value();
    let cutoff = (f.cutoff
        + f.filter_amount * filter_env
        + f.filter_velocity * voice.velocity
        + f.lfo_cutoff)
        .exp2()
        .clamp(CUTOFF_RANGE.0, f.max_cutoff);
    let coefficients = SvfCoefficients::low_pass(cutoff, f.resonance, f.sample_rate);
    let sample = tone.svf.process(sample, &coefficients);
    sample * amp * f.tremolo
}

/// The running state, made in `prepare`.
struct Running {
    sample_rate: f32,
    table: MinBlep,
    pool: Pool<Tone>,
    lfo: Lfo,
    ramps: Ramps,
    dc: OnePoleCoefficients,
    /// The pitch-bend position, −1 to 1.
    bend: f32,
}

/// The built-in polyphonic subtractive synthesizer, kind id `"synth"`: event input 0 (notes, on
/// any channel) and stereo audio output 0. See `docs/architecture/instruments.md`
/// "Synthesizer" for its signal flow and parameters.
pub struct Synth {
    parameters: Parameters,
    running: Option<Running>,
}

impl Synth {
    /// A synth at its defaults. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::Parameter`] if a parameter cannot be made, which does not happen for the fixed
    /// ranges here.
    pub fn new() -> Result<Self, Error> {
        Ok(Synth {
            parameters: Parameters::new()?,
            running: None,
        })
    }
}

impl Node for Synth {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::event("notes")]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Stereo)]
    }

    fn parameters(&self) -> Vec<Parameter> {
        self.parameters.all()
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        let Ok(table) = MinBlep::new() else {
            self.running = None;
            return;
        };
        // Sample rates are exact in `f32` well past any audio rate.
        #[allow(clippy::cast_precision_loss)]
        let rate = sample_rate.max(1) as f32;
        self.running = Some(Running {
            sample_rate: rate,
            table,
            pool: Pool::new(MAX_VOICES, rate, Tone::new),
            lfo: Lfo::new(sample_rate, LFO_SEED),
            ramps: Ramps::new(&self.parameters, sample_rate),
            dc: OnePoleCoefficients::high_pass(DC_BLOCK_HZ, rate),
            bend: 0.0,
        });
    }

    fn reset(&mut self) {
        if let Some(running) = self.running.as_mut() {
            running.pool.reset();
            running.lfo.reset(0.0);
            running.bend = 0.0;
            running.ramps.bend.set_target_over(0.0, 1);
            running.ramps.bend.next_value();
        }
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let p = &self.parameters;
        let Some(running) = self.running.as_mut() else {
            if let Some((left, right)) = outputs.stereo_mut(0) {
                left.fill(0.0);
                right.fill(0.0);
            }
            return;
        };
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        // Read once per block (ADR-0128).
        running.ramps.read(p);
        for voice in running.pool.voices.iter_mut() {
            p.amp_env.apply(&mut voice.amp, running.sample_rate);
            p.filter_env
                .apply(&mut voice.sound.filter_env, running.sample_rate);
            for (i, osc) in voice.sound.osc.iter_mut().enumerate() {
                osc.set_wave(WAVES[choice(p.osc[i].wave.get(), WAVES.len())].1);
            }
        }
        running.lfo.set_frequency(f64::from(p.lfo_rate.get()));
        running
            .lfo
            .set_shape(LFO_SHAPES[choice(p.lfo_shape.get(), LFO_SHAPES.len())].1);
        let destination = DESTINATIONS[choice(p.lfo_destination.get(), DESTINATIONS.len())].1;
        let octave = [p.osc[0].octave.get().round(), p.osc[1].octave.get().round()];
        let bend_range = p.bend_range.get();
        let limit = choice(p.voices.get(), MAX_VOICES + 1).max(1);

        let frames = left.len();
        let mut events = inputs.events(0).peekable();
        for n in 0..frames {
            // Every event due at this frame, in order, before its sample.
            while let Some(event) = events.next_if(|e| e.frame as usize <= n) {
                match event.message.kind() {
                    Kind::NoteOn { note, velocity } if velocity > 0 => {
                        let velocity = f32::from(velocity) / 127.0;
                        let amount = running.ramps.amp_velocity.target();
                        running
                            .pool
                            .note_on(note, velocity, limit, amount, false, ());
                    }
                    Kind::NoteOn { note, .. } | Kind::NoteOff { note, .. } => {
                        running.pool.note_off(note);
                    }
                    Kind::ControlChange { controller, value } if controller == SUSTAIN_PEDAL => {
                        running.pool.set_pedal(value >= 64);
                    }
                    Kind::PitchBend { value } => {
                        running.bend =
                            ((f32::from(value) - BEND_CENTRE) / BEND_CENTRE).clamp(-1.0, 1.0);
                    }
                    _ => {}
                }
            }
            let r = &mut running.ramps;
            r.bend.set_target(running.bend * bend_range);
            let lfo = running.lfo.next_value();
            let depth = r.lfo_depth.next_value();
            let frame = Frame {
                sample_rate: running.sample_rate,
                max_cutoff: (MAX_CUTOFF_RATIO * running.sample_rate).min(CUTOFF_RANGE.1),
                pitch: r.bend.next_value()
                    + match destination {
                        Destination::Pitch => lfo * depth * LFO_PITCH_SEMITONES,
                        _ => 0.0,
                    },
                octave,
                detune: [r.detune[0].next_value(), r.detune[1].next_value()],
                width: [r.width[0].next_value(), r.width[1].next_value()],
                level: [r.level[0].next_value(), r.level[1].next_value()],
                cutoff: r.cutoff.next_value(),
                resonance: r.resonance.next_value(),
                filter_amount: r.filter_amount.next_value(),
                filter_velocity: r.filter_velocity.next_value(),
                lfo_cutoff: match destination {
                    Destination::Cutoff => lfo * depth * LFO_CUTOFF_OCTAVES,
                    _ => 0.0,
                },
                tremolo: match destination {
                    Destination::Amplitude => 1.0 - depth * 0.5 * (1.0 - lfo),
                    _ => 1.0,
                },
            };
            let amp_velocity = r.amp_velocity.next_value();
            let gain = r.gain.next_value();
            let mut sum = 0.0;
            for i in 0..running.pool.voices.len() {
                let voice = &mut running.pool.voices[i];
                if voice.is_free() {
                    continue;
                }
                let sample = render(voice, &frame, &running.table, &running.dc);
                sum += sample * running.pool.step_fade(i, amp_velocity);
            }
            let out = sum * gain;
            left[n] = out;
            right[n] = out;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parameter_names_are_unique_and_only_choices_and_counts_are_stepped() -> Result<(), Error>
    {
        let synth = Synth::new()?;
        let infos: Vec<ParameterInfo> = synth
            .parameters()
            .iter()
            .map(|p| p.info().clone())
            .collect();
        for (n, info) in infos.iter().enumerate() {
            assert!(
                infos[..n].iter().all(|i| i.name != info.name),
                "{}",
                info.name
            );
            let stepped = matches!(info.scale, Scale::Choice(_) | Scale::Integer);
            assert_eq!(info.is_automatable(), !stepped, "{}", info.name);
        }
        assert_eq!(infos.len(), 30);
        Ok(())
    }

    #[test]
    fn test_choice_rounds_and_clamps() {
        assert_eq!(choice(1.4, 3), 1);
        assert_eq!(choice(1.6, 3), 2);
        assert_eq!(choice(9.0, 3), 2);
        assert_eq!(choice(-1.0, 3), 0);
        assert_eq!(choice(f32::NAN, 3), 0);
    }
}
