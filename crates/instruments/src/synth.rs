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
use modulation::{Adsr, EnvelopeStage, Lfo, Shape};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;

/// The most voices a synth plays at once; the `voices` parameter limits it further.
pub const MAX_VOICES: usize = 16;
/// The lowest and highest cutoff, in Hz.
const CUTOFF_RANGE: (f32, f32) = (20.0, 20_000.0);
/// The highest cutoff as a share of the sample rate, kept below the filter's limit
/// (`docs/research/synth-patents.md`, element 3).
const MAX_CUTOFF_RATIO: f32 = 0.49;
/// How long a stolen voice fades out before its new note starts, in seconds.
const STEAL_FADE_SECONDS: f32 = 0.003;
/// The corner of each voice's DC blocker, in Hz.
const DC_BLOCK_HZ: f32 = 5.0;
/// The pitch range of the LFO at full depth, in semitones each way.
const LFO_PITCH_SEMITONES: f32 = 2.0;
/// The cutoff range of the LFO at full depth, in octaves each way.
const LFO_CUTOFF_OCTAVES: f32 = 4.0;
/// The controller number of the sustain pedal.
const SUSTAIN_PEDAL: u8 = 64;
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

/// An envelope's parameters, named `prefix` and grouped as `group`.
struct EnvelopeParameters {
    attack: Parameter,
    decay: Parameter,
    sustain: Parameter,
    release: Parameter,
}

impl EnvelopeParameters {
    fn new(prefix: &str, group: &str, defaults: [f32; 4]) -> Result<Self, Error> {
        let time = |name: &str, default| {
            Parameter::new(
                ParameterInfo::new(format!("{prefix} {name}"), 0.001, 10.0, default)
                    .with_unit("s")
                    .with_scale(Scale::Logarithmic)
                    .with_group(group),
            )
        };
        Ok(EnvelopeParameters {
            attack: time("attack", defaults[0])?,
            decay: time("decay", defaults[1])?,
            sustain: Parameter::new(
                ParameterInfo::new(format!("{prefix} sustain"), 0.0, 1.0, defaults[2])
                    .with_group(group),
            )?,
            release: time("release", defaults[3])?,
        })
    }

    fn all(&self) -> [&Parameter; 4] {
        [&self.attack, &self.decay, &self.sustain, &self.release]
    }

    /// Sets `env`'s times and sustain from the parameters, at `sample_rate`.
    fn apply(&self, env: &mut Adsr, sample_rate: f32) {
        env.set(
            self.attack.get(),
            self.decay.get(),
            self.sustain.get(),
            self.release.get(),
            sample_rate,
        );
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

/// A note waiting for its stolen voice to fade out.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Pending {
    note: u8,
    velocity: f32,
    /// When the note was started, counted in notes started.
    age: u64,
    /// Whether its key is still down.
    held: bool,
}

/// One voice: its oscillators, filter, envelopes and the note it plays.
#[derive(Debug, Clone)]
struct Voice {
    osc: [Oscillator; 2],
    dc: OnePole,
    svf: Svf,
    amp: Adsr,
    filter_env: Adsr,
    note: u8,
    /// The strike velocity, 0 to 1.
    velocity: f32,
    /// When the note started, counted in notes started.
    age: u64,
    /// Whether the key is down.
    held: bool,
    /// Whether the key is up but the sustain pedal holds the note.
    sustained: bool,
    /// Samples left in a steal's fade-out; 0 when not fading.
    fade: u32,
    /// The note to start when the fade ends.
    pending: Option<Pending>,
}

impl Voice {
    fn new() -> Self {
        Voice {
            osc: [Oscillator::new(), Oscillator::new()],
            dc: OnePole::new(),
            svf: Svf::new(),
            amp: Adsr::new(),
            filter_env: Adsr::new(),
            note: 0,
            velocity: 0.0,
            age: 0,
            held: false,
            sustained: false,
            fade: 0,
            pending: None,
        }
    }

    /// Whether the voice is silent and free.
    fn is_free(&self) -> bool {
        self.amp.is_idle() && self.pending.is_none()
    }

    /// Returns to silence at once.
    fn reset(&mut self) {
        for osc in &mut self.osc {
            osc.reset();
        }
        self.dc.reset();
        self.svf.reset();
        self.amp.reset();
        self.filter_env.reset();
        self.held = false;
        self.sustained = false;
        self.fade = 0;
        self.pending = None;
    }

    /// Starts `note` at `velocity` from the envelopes' current levels, with the amplitude peak
    /// `peak`.
    fn start(&mut self, note: u8, velocity: f32, peak: f32, age: u64) {
        self.note = note;
        self.velocity = velocity;
        self.age = age;
        self.held = true;
        self.sustained = false;
        self.amp.gate_on(peak);
        self.filter_env.gate_on(1.0);
    }

    /// Releases the note.
    fn release(&mut self) {
        self.held = false;
        self.sustained = false;
        self.amp.gate_off();
        self.filter_env.gate_off();
    }

    /// The next sample.
    fn render(&mut self, f: &Frame, table: &MinBlep, dc: &OnePoleCoefficients) -> f32 {
        let pitch = f32::from(self.note) + f.pitch;
        let mut sum = 0.0;
        for (i, osc) in self.osc.iter_mut().enumerate() {
            let note = pitch + 12.0 * f.octave[i] + f.detune[i] / 100.0;
            osc.set_frequency(note_to_hz(note), f.sample_rate);
            osc.set_pulse_width(f.width[i]);
            sum += f.level[i] * osc.next(table);
        }
        let sample = self.dc.process(sum, dc);
        let filter_env = self.filter_env.next_value();
        let amp = self.amp.next_value();
        let cutoff = (f.cutoff
            + f.filter_amount * filter_env
            + f.filter_velocity * self.velocity
            + f.lfo_cutoff)
            .exp2()
            .clamp(CUTOFF_RANGE.0, f.max_cutoff);
        let coefficients = SvfCoefficients::low_pass(cutoff, f.resonance, f.sample_rate);
        let sample = self.svf.process(sample, &coefficients);
        sample * amp * f.tremolo
    }
}

/// The running state, made in `prepare`.
struct Running {
    sample_rate: f32,
    table: MinBlep,
    voices: Box<[Voice]>,
    lfo: Lfo,
    ramps: Ramps,
    dc: OnePoleCoefficients,
    /// Samples a steal's fade lasts.
    fade: u32,
    /// Notes started so far, for the voices' ages.
    started: u64,
    /// Whether the sustain pedal is down.
    pedal: bool,
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

impl Running {
    /// The voice for a new note `note`: the voice already playing it, else a free voice among
    /// the first `limit`, else the quietest released one, else the oldest held one. The second
    /// value says whether the voice must be stolen.
    fn choose(&self, note: u8, limit: usize) -> Option<(usize, bool)> {
        let playing = |v: &Voice| !v.is_free() && v.fade == 0;
        if let Some(i) = self
            .voices
            .iter()
            .position(|v| playing(v) && v.note == note)
        {
            return Some((i, false));
        }
        let candidates = || self.voices.iter().enumerate().take(limit);
        if let Some((i, _)) = candidates().find(|(_, v)| v.is_free()) {
            return Some((i, false));
        }
        let quietest_released = candidates()
            .filter(|(_, v)| v.fade == 0 && v.amp.stage() == EnvelopeStage::Release)
            .min_by(|a, b| a.1.amp.level().total_cmp(&b.1.amp.level()));
        if let Some((i, _)) = quietest_released {
            return Some((i, true));
        }
        candidates()
            .filter(|(_, v)| v.fade == 0)
            .min_by_key(|(_, v)| v.age)
            .or_else(|| candidates().min_by_key(|(_, v)| v.fade))
            .map(|(i, _)| (i, true))
    }

    /// Handles a note-on.
    fn note_on(&mut self, note: u8, velocity: f32, limit: usize, amp_velocity: f32) {
        let Some((i, steal)) = self.choose(note, limit) else {
            return;
        };
        self.started += 1;
        let voice = &mut self.voices[i];
        if steal {
            voice.fade = self.fade;
            voice.pending = Some(Pending {
                note,
                velocity,
                age: self.started,
                held: true,
            });
        } else {
            let peak = 1.0 - amp_velocity * (1.0 - velocity);
            voice.start(note, velocity, peak, self.started);
        }
    }

    /// Handles a note-off.
    fn note_off(&mut self, note: u8) {
        let pedal = self.pedal;
        for voice in self.voices.iter_mut() {
            if let Some(pending) = voice.pending.as_mut().filter(|p| p.note == note && p.held) {
                pending.held = false;
            }
            if voice.held && voice.note == note && voice.fade == 0 {
                if pedal {
                    voice.held = false;
                    voice.sustained = true;
                } else {
                    voice.release();
                }
            }
        }
    }

    /// Handles the sustain pedal going down (`true`) or up.
    fn set_pedal(&mut self, down: bool) {
        self.pedal = down;
        if !down {
            for voice in self.voices.iter_mut().filter(|v| v.sustained) {
                voice.release();
            }
        }
    }

    /// Ends each finished fade by starting its pending note from silence.
    fn finish_fade(voice: &mut Voice, amp_velocity: f32) {
        let Some(pending) = voice.pending.take() else {
            return;
        };
        voice.reset();
        let peak = 1.0 - amp_velocity * (1.0 - pending.velocity);
        voice.start(pending.note, pending.velocity, peak, pending.age);
        if !pending.held {
            voice.release();
        }
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
        // A few hundred samples at most.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let fade = (STEAL_FADE_SECONDS * rate).ceil().max(1.0) as u32;
        self.running = Some(Running {
            sample_rate: rate,
            table,
            voices: (0..MAX_VOICES).map(|_| Voice::new()).collect(),
            lfo: Lfo::new(sample_rate, LFO_SEED),
            ramps: Ramps::new(&self.parameters, sample_rate),
            dc: OnePoleCoefficients::high_pass(DC_BLOCK_HZ, rate),
            fade,
            started: 0,
            pedal: false,
            bend: 0.0,
        });
    }

    fn reset(&mut self) {
        if let Some(running) = self.running.as_mut() {
            for voice in running.voices.iter_mut() {
                voice.reset();
            }
            running.lfo.reset(0.0);
            running.pedal = false;
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
        for voice in running.voices.iter_mut() {
            p.amp_env.apply(&mut voice.amp, running.sample_rate);
            p.filter_env
                .apply(&mut voice.filter_env, running.sample_rate);
            for (i, osc) in voice.osc.iter_mut().enumerate() {
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
                        running.note_on(note, velocity, limit, running.ramps.amp_velocity.target());
                    }
                    Kind::NoteOn { note, .. } | Kind::NoteOff { note, .. } => {
                        running.note_off(note);
                    }
                    Kind::ControlChange { controller, value } if controller == SUSTAIN_PEDAL => {
                        running.set_pedal(value >= 64);
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
            for voice in running.voices.iter_mut().filter(|v| !v.is_free()) {
                let mut sample = voice.render(&frame, &running.table, &running.dc);
                if voice.fade > 0 {
                    #[allow(clippy::cast_precision_loss)] // a few hundred samples
                    let share = voice.fade as f32 / running.fade as f32;
                    sample *= share;
                    voice.fade -= 1;
                    if voice.fade == 0 {
                        Running::finish_fade(voice, amp_velocity);
                    }
                }
                sum += sample;
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

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn running() -> Result<Running, Box<dyn std::error::Error>> {
        let mut synth = Synth::new()?;
        synth.prepare(48_000, 128);
        Ok(synth.running.ok_or("not prepared")?)
    }

    #[test]
    fn test_choose_prefers_same_note_then_free_then_quietest_released_then_oldest() -> TestResult {
        let mut r = running()?;
        let amp = 0.7;
        r.note_on(60, 1.0, 3, amp);
        r.note_on(62, 1.0, 3, amp);
        r.note_on(64, 1.0, 3, amp);
        // A repeated note takes its own voice.
        assert_eq!(r.choose(62, 3), Some((1, false)));
        // All three held: the oldest (note 60) is stolen.
        assert_eq!(r.choose(65, 3), Some((0, true)));
        // Release 64 and 62; let 62 fall further: it is the quietest released.
        for v in r.voices.iter_mut() {
            v.amp.set(0.0, 0.0, 1.0, 0.5, 48_000.0);
            v.amp.next_value();
        }
        r.note_off(62);
        for _ in 0..2_000 {
            r.voices[1].amp.next_value();
        }
        r.note_off(64);
        r.voices[2].amp.next_value();
        assert_eq!(r.choose(65, 3), Some((1, true)));
        // A higher voice limit offers a free voice first.
        assert_eq!(r.choose(65, 4), Some((3, false)));
        Ok(())
    }

    #[test]
    fn test_sustain_pedal_holds_released_notes_until_lifted() -> TestResult {
        let mut r = running()?;
        r.note_on(60, 1.0, 8, 0.0);
        r.voices[0].amp.next_value();
        r.set_pedal(true);
        r.note_off(60);
        assert!(r.voices[0].sustained);
        assert_ne!(r.voices[0].amp.stage(), EnvelopeStage::Release);
        r.set_pedal(false);
        assert_eq!(r.voices[0].amp.stage(), EnvelopeStage::Release);
        Ok(())
    }
}
