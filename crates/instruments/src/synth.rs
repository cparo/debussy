//! [`Synth`]: the built-in polyphonic subtractive synthesizer (`docs/architecture/instruments.md`
//! "Synthesizer", ADR-0148).
//!
//! Each voice sums two MinBLEP oscillators, removes their DC, and runs them through a low-pass of
//! 6 to 24 dB per octave (ADR-0168) and an amplitude envelope; a second envelope, velocity and the shared
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

mod note;

pub(crate) use note::{note_curve, render_note};

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
/// The low-pass's slope; the `filter order` parameter counts its poles, 1 to 4 (ADR-0168).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Order {
    /// One one-pole low-pass, 6 dB per octave; no resonance.
    Six,
    /// One state-variable low-pass, 12 dB per octave.
    Twelve,
    /// A state-variable low-pass, then a one-pole low-pass: 18 dB per octave.
    Eighteen,
    /// Two state-variable low-passes in series, at one cutoff and Q: 24 dB per octave.
    TwentyFour,
}
/// The orders by pole count less one.
const ORDERS: [Order; 4] = [
    Order::Six,
    Order::Twelve,
    Order::Eighteen,
    Order::TwentyFour,
];
/// The `filter order` parameter's default: 2 poles, 12 dB per octave, the one SVF the synth had
/// before it had a choice, so older projects sound unchanged.
const DEFAULT_ORDER: f32 = 2.0;

/// The order a `filter order` value (poles, 1 to 4) names.
fn order(poles: f32) -> Order {
    ORDERS[choice(poles - 1.0, ORDERS.len())]
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
    order: Parameter,
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
            // The one-pole of the 6 dB order has no resonance.
            resonance: Parameter::new(
                filter("resonance", 0.5, 12.0, 0.707).enabled_while("filter order", &[2, 3, 4]),
            )?,
            // Shown as the Equalizer's filter orders are: a rolloff in dB per octave.
            order: Parameter::new(
                filter("filter order", 1.0, 4.0, DEFAULT_ORDER)
                    .with_scale(Scale::Integer)
                    .stopped_only()
                    .with_label("Rolloff")
                    .with_unit("dB/octave")
                    .with_display_factor(dsp::DB_PER_POLE),
            )?,
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
            self.order.clone(),
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
    order: Order,
    filter_amount: f32,
    filter_velocity: f32,
    /// Octaves added to every cutoff by the LFO.
    lfo_cutoff: f32,
    /// The amplitude every voice is multiplied by for the LFO.
    tremolo: f32,
}

/// The values one sample's [`Frame`] is built from: the ramps' values while running, or the
/// parameters' own for a note preview.
#[derive(Debug, Clone, Copy)]
struct Values {
    /// The pitch bend, in semitones.
    bend: f32,
    /// Per oscillator: detune (cents), pulse width (share), level.
    detune: [f32; 2],
    width: [f32; 2],
    level: [f32; 2],
    /// The cutoff in log2(Hz).
    cutoff: f32,
    resonance: f32,
    filter_amount: f32,
    filter_velocity: f32,
    lfo_depth: f32,
}

impl Values {
    /// The parameters' values as they stand, with no pitch bend.
    fn of(p: &Parameters) -> Values {
        Values {
            bend: 0.0,
            detune: [p.osc[0].detune.get(), p.osc[1].detune.get()],
            width: [p.osc[0].width.get() / 100.0, p.osc[1].width.get() / 100.0],
            level: [p.osc[0].level.get(), p.osc[1].level.get()],
            cutoff: p.cutoff.get().log2(),
            resonance: p.resonance.get(),
            filter_amount: p.filter_amount.get(),
            filter_velocity: p.filter_velocity.get(),
            lfo_depth: p.lfo_depth.get(),
        }
    }
}

/// What stays fixed for a block: the rate and the stepped parameters a frame needs.
#[derive(Debug, Clone, Copy)]
struct Block {
    sample_rate: f32,
    octave: [f32; 2],
    order: Order,
    destination: Destination,
}

impl Block {
    /// The block's constants from the parameters, at `sample_rate`.
    fn new(p: &Parameters, sample_rate: f32) -> Block {
        Block {
            sample_rate,
            octave: [p.osc[0].octave.get().round(), p.osc[1].octave.get().round()],
            order: order(p.order.get()),
            destination: DESTINATIONS[choice(p.lfo_destination.get(), DESTINATIONS.len())].1,
        }
    }

    /// The frame for one sample at `values`, with the LFO at `lfo` (−1 to 1) sent to its
    /// destination.
    fn frame(&self, v: &Values, lfo: f32) -> Frame {
        let depth = v.lfo_depth;
        Frame {
            sample_rate: self.sample_rate,
            max_cutoff: (MAX_CUTOFF_RATIO * self.sample_rate).min(CUTOFF_RANGE.1),
            pitch: v.bend
                + match self.destination {
                    Destination::Pitch => lfo * depth * LFO_PITCH_SEMITONES,
                    _ => 0.0,
                },
            octave: self.octave,
            detune: v.detune,
            width: v.width,
            level: v.level,
            cutoff: v.cutoff,
            resonance: v.resonance,
            order: self.order,
            filter_amount: v.filter_amount,
            filter_velocity: v.filter_velocity,
            lfo_cutoff: match self.destination {
                Destination::Cutoff => lfo * depth * LFO_CUTOFF_OCTAVES,
                _ => 0.0,
            },
            tremolo: match self.destination {
                Destination::Amplitude => 1.0 - depth * 0.5 * (1.0 - lfo),
                _ => 1.0,
            },
        }
    }
}

/// Sets `voice`'s envelopes and waves from the parameters, at `sample_rate`.
fn configure(voice: &mut Voice<Tone>, p: &Parameters, sample_rate: f32) {
    p.amp_env.apply(&mut voice.amp, sample_rate);
    p.filter_env.apply(&mut voice.sound.filter_env, sample_rate);
    for (i, osc) in voice.sound.osc.iter_mut().enumerate() {
        osc.set_wave(WAVES[choice(p.osc[i].wave.get(), WAVES.len())].1);
    }
}

/// Sets the LFO's rate and shape from the parameters.
fn configure_lfo(lfo: &mut Lfo, p: &Parameters) {
    lfo.set_frequency(f64::from(p.lfo_rate.get()));
    lfo.set_shape(LFO_SHAPES[choice(p.lfo_shape.get(), LFO_SHAPES.len())].1);
}

/// The synth's part of a voice: its oscillators, filter stages and filter envelope.
#[derive(Debug, Clone)]
struct Tone {
    osc: [Oscillator; 2],
    dc: OnePole,
    /// The low-pass stages; which run depends on the [`Order`].
    svf: [Svf; 2],
    one_pole: OnePole,
    filter_env: Adsr,
}

impl Tone {
    fn new() -> Self {
        Tone {
            osc: [Oscillator::new(), Oscillator::new()],
            dc: OnePole::new(),
            svf: [Svf::new(), Svf::new()],
            one_pole: OnePole::new(),
            filter_env: Adsr::new(),
        }
    }

    /// Zeroes every filter stage, for a change of order (ADR-0168).
    fn reset_filter(&mut self) {
        for svf in &mut self.svf {
            svf.reset();
        }
        self.one_pole.reset();
    }

    /// `input` through the low-pass of order `order` at `cutoff` Hz and Q `resonance`.
    fn filter(&mut self, input: f32, order: Order, cutoff: f32, resonance: f32, rate: f32) -> f32 {
        let svf = || SvfCoefficients::low_pass(cutoff, resonance, rate);
        let one_pole = || OnePoleCoefficients::low_pass(cutoff, rate);
        match order {
            Order::Six => self.one_pole.process(input, &one_pole()),
            Order::Twelve => self.svf[0].process(input, &svf()),
            Order::Eighteen => {
                let x = self.svf[0].process(input, &svf());
                self.one_pole.process(x, &one_pole())
            }
            Order::TwentyFour => {
                let c = svf();
                let x = self.svf[0].process(input, &c);
                self.svf[1].process(x, &c)
            }
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
        self.reset_filter();
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
    let sample = tone.filter(sample, f.order, cutoff, f.resonance, f.sample_rate);
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
    /// The filter order the voices' stages were last run at.
    order: Order,
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
            order: order(self.parameters.order.get()),
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
            configure(voice, p, running.sample_rate);
        }
        configure_lfo(&mut running.lfo, p);
        let bend_range = p.bend_range.get();
        let limit = choice(p.voices.get(), MAX_VOICES + 1).max(1);
        let block = Block::new(p, running.sample_rate);
        // Stopped-only: a new order takes effect at once, from rest (ADR-0168).
        if block.order != running.order {
            running.order = block.order;
            for voice in running.pool.voices.iter_mut() {
                voice.sound.reset_filter();
            }
        }

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
            let values = Values {
                bend: r.bend.next_value(),
                detune: [r.detune[0].next_value(), r.detune[1].next_value()],
                width: [r.width[0].next_value(), r.width[1].next_value()],
                level: [r.level[0].next_value(), r.level[1].next_value()],
                cutoff: r.cutoff.next_value(),
                resonance: r.resonance.next_value(),
                filter_amount: r.filter_amount.next_value(),
                filter_velocity: r.filter_velocity.next_value(),
                lfo_depth: r.lfo_depth.next_value(),
            };
            let frame = block.frame(&values, lfo);
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
        assert_eq!(infos.len(), 31);
        Ok(())
    }

    /// The gain of `tone`'s low-pass of order `order` (cutoff 200 Hz, Q 0.707, 48 kHz) at
    /// `hz`, from the RMS of a settled sine.
    fn gain_at(order: Order, hz: f32) -> f32 {
        let mut tone = Tone::new();
        let rate = 48_000.0;
        let sine = |n: usize| {
            #[allow(clippy::cast_precision_loss)] // n < 2^24
            let t = n as f32 / rate;
            (std::f32::consts::TAU * hz * t).sin()
        };
        let (mut input, mut output) = (0.0, 0.0);
        for n in 0..48_000 {
            let y = tone.filter(sine(n), order, 200.0, 0.707, rate);
            if n >= 24_000 {
                input += sine(n) * sine(n);
                output += y * y;
            }
        }
        (output / input).sqrt()
    }

    #[test]
    fn test_filter_each_order_falls_at_its_slope() {
        for (n, order) in ORDERS.iter().enumerate() {
            // An octave well above the cutoff, below where `f32` rounding sets a floor near
            // −90 dB, and far below Nyquist.
            let fall = 20.0 * (gain_at(*order, 800.0) / gain_at(*order, 1_600.0)).log10();
            #[allow(clippy::cast_precision_loss)] // n < 4
            let slope = 6.0 * (n + 1) as f32;
            assert!(
                (fall - slope).abs() < 0.6,
                "{order:?}: {fall} dB per octave"
            );
        }
    }

    #[test]
    fn test_filter_twelve_is_the_single_svf() {
        let (mut tone, mut svf) = (Tone::new(), Svf::new());
        let c = SvfCoefficients::low_pass(500.0, 4.0, 48_000.0);
        for n in 0..256 {
            let x = if n % 37 == 0 { 1.0 } else { -0.25 };
            let y = tone.filter(x, Order::Twelve, 500.0, 4.0, 48_000.0);
            assert_eq!(y.to_bits(), svf.process(x, &c).to_bits());
        }
    }

    #[test]
    fn test_reset_filter_zeroes_every_stage() {
        let mut tone = Tone::new();
        for order in [Order::TwentyFour, Order::Eighteen] {
            for _ in 0..64 {
                tone.filter(1.0, order, 1_000.0, 2.0, 48_000.0);
            }
        }
        tone.reset_filter();
        assert_eq!(
            tone.filter(0.0, Order::TwentyFour, 1_000.0, 2.0, 48_000.0),
            0.0
        );
        assert_eq!(
            tone.filter(0.0, Order::Eighteen, 1_000.0, 2.0, 48_000.0),
            0.0
        );
    }

    #[test]
    fn test_filter_order_is_poles_shown_as_rolloff_and_greys_resonance() -> Result<(), Error> {
        let p = Parameters::new()?;
        let info = p.order.info();
        assert_eq!(order(info.default), Order::Twelve);
        assert_eq!(
            (info.min, info.max, &info.scale),
            (1.0, 4.0, &Scale::Integer)
        );
        assert!(info.stopped_only);
        assert_eq!(info.label.as_deref(), Some("Rolloff"));
        assert_eq!(info.unit, "dB/octave");
        assert_eq!(info.display_factor, Some(dsp::DB_PER_POLE));
        assert_eq!(
            [1.0, 2.0, 3.0, 4.0].map(order),
            [
                Order::Six,
                Order::Twelve,
                Order::Eighteen,
                Order::TwentyFour
            ]
        );
        let resonance = p.resonance.info();
        assert!(!resonance.is_enabled(|_| 1.0));
        assert!((2..=4).all(|i| resonance.is_enabled(|_| i as f32)));
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
