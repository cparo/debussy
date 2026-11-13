//! The reverb: a pre-delay, then one of two engines, a feedback delay network ("room") or
//! Dattorro's plate ("plate") (`docs/architecture/delay-and-reverb.md` "Reverb").

use std::f32::consts::TAU;

use audio_buffers::Layout;
use dsp::{FOLD_DOWN_GAIN, OnePole, OnePoleCoefficients, TapDelay};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;
use crate::impulse::{ImpulseCurve, SPAN_DB};
use crate::tap::{Tap, to_tap};

/// The longest pre-delay, in milliseconds.
pub const MAX_PRE_DELAY_MS: f32 = 200.0;

/// The parameters, in the order the effect declares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum Control {
    Algorithm,
    PreDelay,
    Decay,
    Damping,
    Mix,
}

/// How many parameters there are.
pub(crate) const CONTROLS: usize = 5;

/// Every control, in declaration order.
const ALL: [Control; CONTROLS] = [
    Control::Algorithm,
    Control::PreDelay,
    Control::Decay,
    Control::Damping,
    Control::Mix,
];

/// The algorithms' names, by index.
const ALGORITHMS: [&str; 2] = ["room", "plate"];

/// What parameter `control` is: its saved name, range, default, unit, scale and group.
fn info(control: Control) -> ParameterInfo {
    match control {
        #[allow(clippy::cast_precision_loss)] // two options
        Control::Algorithm => {
            ParameterInfo::new("algorithm", 0.0, (ALGORITHMS.len() - 1) as f32, 0.0)
                .with_scale(Scale::Choice(
                    ALGORITHMS.iter().map(|&a| a.to_owned()).collect(),
                ))
                .stopped_only()
                .with_group("type")
        }
        Control::PreDelay => ParameterInfo::new("pre-delay", 0.0, MAX_PRE_DELAY_MS, 10.0)
            .with_unit("ms")
            .with_group("time"),
        Control::Decay => ParameterInfo::new("decay", 0.2, 20.0, 2.0)
            .with_unit("s")
            .with_scale(Scale::Logarithmic)
            .with_group("time"),
        Control::Damping => ParameterInfo::new("damping", 1_000.0, 20_000.0, 8_000.0)
            .with_unit("Hz")
            .with_scale(Scale::Logarithmic)
            .with_group("tone"),
        Control::Mix => ParameterInfo::new("mix", 0.0, 100.0, 25.0)
            .with_unit("%")
            .with_group("output"),
    }
}

/// Which engine a reverb runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Algorithm {
    Room,
    Plate,
}

impl Algorithm {
    /// The algorithm an `algorithm` parameter value means.
    fn from_value(value: f32) -> Algorithm {
        if value >= 0.5 {
            Algorithm::Plate
        } else {
            Algorithm::Room
        }
    }
}

/// The gain per pass through a loop of `frames` frames that decays by 60 dB in `seconds` at
/// `sample_rate`: `10^(−3·frames / (seconds·rate))`.
fn loop_gain(frames: f32, seconds: f32, sample_rate: f32) -> f32 {
    10.0_f32.powf(-3.0 * frames / (seconds * sample_rate))
}

/// Whether `n` is prime.
fn is_prime(n: usize) -> bool {
    n >= 2
        && (2..)
            .take_while(|d| d * d <= n)
            .all(|d| !n.is_multiple_of(d))
}

/// `frames` at 48 kHz scaled to `sample_rate`, rounded, at least 1.
fn scaled(frames: usize, from_rate: f64, sample_rate: u32) -> usize {
    #[allow(clippy::cast_precision_loss)] // lengths are a few thousand frames
    let frames = frames as f64 * f64::from(sample_rate) / from_rate;
    to_tap(frames, usize::MAX)
}

/// A Schroeder allpass: `w = x + g·d`, `y = d − g·w`, where `d` is `w` from `delay` frames ago.
struct Allpass {
    line: TapDelay,
    delay: usize,
    gain: f32,
}

impl Allpass {
    /// An allpass of `delay` frames and gain `gain`, with room for `extra` frames more of
    /// modulation. Allocates.
    fn new(delay: usize, extra: usize, gain: f32) -> Self {
        Allpass {
            line: TapDelay::new(delay + extra + 1),
            delay,
            gain,
        }
    }

    /// One sample through the allpass, its delay read from the line `d` frames back.
    fn step(&mut self, x: f32, d: f32) -> f32 {
        let w = x + self.gain * d;
        self.line.write(w);
        d - self.gain * w
    }

    /// One sample through the allpass at its fixed delay.
    fn process(&mut self, x: f32) -> f32 {
        let d = self.line.read(self.delay);
        self.step(x, d)
    }

    /// One sample through the allpass at a delay of `frames`, read by linear interpolation.
    fn process_fractional(&mut self, x: f32, frames: f32) -> f32 {
        let d = self.line.read_fractional(frames);
        self.step(x, d)
    }

    fn clear(&mut self) {
        self.line.clear();
    }

    /// The samples it holds.
    #[cfg(test)]
    fn samples(&self) -> usize {
        self.line.max_tap() + 1
    }
}

/// A fixed delay of `frames`, read at its end or at taps along it.
struct Fixed {
    line: TapDelay,
    frames: usize,
}

impl Fixed {
    fn new(frames: usize) -> Self {
        Fixed {
            line: TapDelay::new(frames),
            frames,
        }
    }

    /// The sample leaving the end of the line.
    fn end(&self) -> f32 {
        self.line.read(self.frames)
    }
}

/// The room's eight line lengths at 48 kHz: distinct primes from 21 to 60 ms.
const ROOM_LINES: [usize; 8] = [1031, 1327, 1523, 1801, 2063, 2357, 2621, 2903];

/// The room's four input diffusers' lengths at 48 kHz.
const ROOM_DIFFUSERS: [usize; 4] = [113, 163, 241, 337];

/// The room's input diffusers' gain.
const ROOM_DIFFUSION: f32 = 0.6;

/// The rate the room's lengths are given at.
const ROOM_RATE: f64 = 48_000.0;

/// The room's output sign patterns over the lines, left and right; orthogonal.
const ROOM_LEFT: [f32; 8] = [1.0, 1.0, -1.0, -1.0, 1.0, 1.0, -1.0, -1.0];
const ROOM_RIGHT: [f32; 8] = [1.0, -1.0, 1.0, -1.0, -1.0, 1.0, -1.0, 1.0];

/// The room's output gain: `1/√8`, so the sum of eight lines keeps their level.
const ROOM_OUTPUT: f32 = 0.353_553_4;

/// The fast Walsh–Hadamard transform of 8 values, scaled by `1/√8` so it is orthogonal.
fn hadamard(v: &mut [f32; 8]) {
    let mut half = 1;
    while half < 8 {
        for start in (0..8).step_by(half * 2) {
            for i in start..start + half {
                let (a, b) = (v[i], v[i + half]);
                v[i] = a + b;
                v[i + half] = a - b;
            }
        }
        half *= 2;
    }
    for x in v.iter_mut() {
        *x *= ROOM_OUTPUT;
    }
}

/// The feedback delay network: four allpasses per input channel, then eight lines fed back
/// through an 8×8 Hadamard matrix, each with a decay gain and a damping low-pass. The outputs are
/// taken from the lines' ends, before the matrix.
struct Room {
    sample_rate: f32,
    diffusers: [[Allpass; 4]; 2],
    lines: [Fixed; 8],
    gains: [f32; 8],
    damping: [OnePole; 8],
}

impl Room {
    /// A silent room at `sample_rate`. Allocates.
    fn new(sample_rate: u32) -> Self {
        let mut lengths = [0; 8];
        let mut last = 0;
        for (length, base) in lengths.iter_mut().zip(ROOM_LINES) {
            // The next prime after the scaled length, and after the line before: distinct primes
            // are mutually prime.
            let mut n = scaled(base, ROOM_RATE, sample_rate).max(last + 1);
            while !is_prime(n) {
                n += 1;
            }
            *length = n;
            last = n;
        }
        let diffusers = || {
            ROOM_DIFFUSERS
                .map(|base| Allpass::new(scaled(base, ROOM_RATE, sample_rate), 0, ROOM_DIFFUSION))
        };
        #[allow(clippy::cast_precision_loss)] // audio rates are exact in f32
        let rate = sample_rate as f32;
        Room {
            sample_rate: rate,
            diffusers: [diffusers(), diffusers()],
            lines: lengths.map(Fixed::new),
            gains: [0.0; 8],
            damping: Default::default(),
        }
    }

    /// Sets each line's gain for a decay of `seconds`.
    fn set_decay(&mut self, seconds: f32) {
        for (gain, line) in self.gains.iter_mut().zip(&self.lines) {
            #[allow(clippy::cast_precision_loss)] // lengths are a few thousand frames
            let frames = line.frames as f32;
            *gain = loop_gain(frames, seconds, self.sample_rate);
        }
    }

    /// One frame of `left`, `right`, with the damping low-pass `damping`.
    fn frame(&mut self, left: f32, right: f32, damping: &OnePoleCoefficients) -> (f32, f32) {
        let mut input = [left, right];
        for (x, chain) in input.iter_mut().zip(&mut self.diffusers) {
            for allpass in chain {
                *x = allpass.process(*x);
            }
        }
        let ends: [f32; 8] = std::array::from_fn(|i| self.lines[i].end());
        let mut fed: [f32; 8] =
            std::array::from_fn(|i| self.gains[i] * self.damping[i].process(ends[i], damping));
        hadamard(&mut fed);
        for (i, (line, f)) in self.lines.iter_mut().zip(fed).enumerate() {
            line.line.write(f + input[i % 2]);
        }
        let out = |signs: &[f32; 8]| {
            ROOM_OUTPUT * signs.iter().zip(&ends).map(|(s, e)| s * e).sum::<f32>()
        };
        (out(&ROOM_LEFT), out(&ROOM_RIGHT))
    }

    fn clear(&mut self) {
        self.diffusers.iter_mut().flatten().for_each(Allpass::clear);
        self.lines.iter_mut().for_each(|l| l.line.clear());
        self.damping.iter_mut().for_each(OnePole::reset);
    }

    /// The samples it holds.
    #[cfg(test)]
    fn samples(&self) -> usize {
        let diffusers: usize = self.diffusers.iter().flatten().map(Allpass::samples).sum();
        let lines: usize = self.lines.iter().map(|l| l.line.max_tap() + 1).sum();
        diffusers + lines
    }
}

/// The rate Dattorro gives the plate's lengths at, in hertz.
const PLATE_RATE: f64 = 29_761.0;

/// The input band-limit: `y = b·x + (1 − b)·y`.
const PLATE_BANDWIDTH: f32 = 0.9995;

/// The input diffusers' lengths and gains at [`PLATE_RATE`].
const PLATE_DIFFUSERS: [(usize, f32); 4] = [(142, 0.75), (107, 0.75), (379, 0.625), (277, 0.625)];

/// The modulated tank allpasses' gain, negated as in Dattorro's figure.
const PLATE_DECAY_DIFFUSION_1: f32 = -0.7;

/// The tank allpasses' modulation depth either way, in frames at [`PLATE_RATE`].
const PLATE_EXCURSION: f64 = 16.0;

/// The tank modulation's rate, in hertz.
const PLATE_LFO_HZ: f32 = 1.0;

/// The output taps' gain.
const PLATE_TAP: f32 = 0.6;

/// One half of the plate's figure-eight tank, lengths at [`PLATE_RATE`]: a modulated allpass, a
/// delay, the damping low-pass and decay, an allpass, and a delay.
struct Half {
    modulated: Allpass,
    first: Fixed,
    damping: OnePole,
    diffuser: Allpass,
    second: Fixed,
}

impl Half {
    /// A half at `sample_rate` with lengths `[modulated, first, diffuser, second]` at
    /// [`PLATE_RATE`] and a modulation depth of `excursion` frames. Allocates.
    fn new(lengths: [usize; 4], excursion: usize, sample_rate: u32) -> Self {
        let at = |frames| scaled(frames, PLATE_RATE, sample_rate);
        Half {
            modulated: Allpass::new(at(lengths[0]), excursion + 1, PLATE_DECAY_DIFFUSION_1),
            first: Fixed::new(at(lengths[1])),
            damping: OnePole::new(),
            diffuser: Allpass::new(at(lengths[2]), 0, 0.5),
            second: Fixed::new(at(lengths[3])),
        }
    }

    /// One sample of `input` through the half, its allpass delayed by `modulation` frames more
    /// or less, with `decay` and the `damping` low-pass.
    fn process(&mut self, input: f32, modulation: f32, decay: f32, damping: &OnePoleCoefficients) {
        #[allow(clippy::cast_precision_loss)] // lengths are a few thousand frames
        let delay = self.modulated.delay as f32 + modulation;
        let a = self.modulated.process_fractional(input, delay);
        let first = self.first.end();
        self.first.line.write(a);
        let damped = self.damping.process(first, damping) * decay;
        let b = self.diffuser.process(damped);
        self.second.line.write(b);
    }

    fn clear(&mut self) {
        self.modulated.clear();
        self.first.line.clear();
        self.damping.reset();
        self.diffuser.clear();
        self.second.line.clear();
    }

    #[cfg(test)]
    fn samples(&self) -> usize {
        self.modulated.samples()
            + self.first.line.max_tap()
            + 1
            + self.diffuser.samples()
            + self.second.line.max_tap()
            + 1
    }
}

/// An output tap: which part of which half, at how many frames, with which sign.
#[derive(Clone, Copy)]
enum Part {
    First,
    Diffuser,
    Second,
}

/// Dattorro's output taps for the left output, at [`PLATE_RATE`]: (right half?, part, frames,
/// sign).
const PLATE_LEFT_TAPS: [(bool, Part, usize, f32); 7] = [
    (true, Part::First, 266, 1.0),
    (true, Part::First, 2974, 1.0),
    (true, Part::Diffuser, 1913, -1.0),
    (true, Part::Second, 1996, 1.0),
    (false, Part::First, 1990, -1.0),
    (false, Part::Diffuser, 187, -1.0),
    (false, Part::Second, 1066, -1.0),
];

/// Dattorro's output taps for the right output, as [`PLATE_LEFT_TAPS`].
const PLATE_RIGHT_TAPS: [(bool, Part, usize, f32); 7] = [
    (false, Part::First, 353, 1.0),
    (false, Part::First, 3627, 1.0),
    (false, Part::Diffuser, 1228, -1.0),
    (false, Part::Second, 2673, 1.0),
    (true, Part::First, 2111, -1.0),
    (true, Part::Diffuser, 335, -1.0),
    (true, Part::Second, 121, -1.0),
];

/// The length of a full pass round the tank, both halves, in seconds: the sum of its eight
/// lengths at [`PLATE_RATE`].
const PLATE_LOOP_SECONDS: f32 =
    (672 + 4453 + 1800 + 3720 + 908 + 4217 + 2656 + 3163) as f32 / PLATE_RATE as f32;

/// Dattorro's plate (J. Dattorro, "Effect Design Part 1", JAES 45(9), 1997): a band-limit and
/// four input diffusers, then a figure-eight tank of two halves, each feeding the other, read at
/// fixed taps.
struct Plate {
    band: f32,
    diffusers: [Allpass; 4],
    halves: [Half; 2],
    /// The tank's gain, applied twice in each half.
    decay: f32,
    /// The modulation depth, in frames.
    excursion: f32,
    /// The LFO's phasor, (cos, sin), and its rotation per frame.
    lfo: (f32, f32),
    rotation: (f32, f32),
    /// The output taps, scaled to the rate.
    left_taps: [(bool, Part, usize, f32); 7],
    right_taps: [(bool, Part, usize, f32); 7],
}

impl Plate {
    /// A silent plate at `sample_rate`. Allocates.
    fn new(sample_rate: u32) -> Self {
        let ratio = f64::from(sample_rate) / PLATE_RATE;
        let excursion = PLATE_EXCURSION * ratio;
        let extra = to_tap(excursion.ceil(), usize::MAX);
        let tap = |taps: [(bool, Part, usize, f32); 7]| {
            taps.map(|(right, part, frames, sign)| {
                (right, part, scaled(frames, PLATE_RATE, sample_rate), sign)
            })
        };
        #[allow(clippy::cast_precision_loss)] // audio rates are exact in f32
        let step = TAU * PLATE_LFO_HZ / sample_rate as f32;
        #[allow(clippy::cast_possible_truncation)] // a few frames
        let excursion = excursion as f32;
        Plate {
            band: 0.0,
            diffusers: PLATE_DIFFUSERS.map(|(frames, gain)| {
                Allpass::new(scaled(frames, PLATE_RATE, sample_rate), 0, gain)
            }),
            halves: [
                Half::new([672, 4453, 1800, 3720], extra, sample_rate),
                Half::new([908, 4217, 2656, 3163], extra, sample_rate),
            ],
            decay: 0.0,
            excursion,
            lfo: (1.0, 0.0),
            rotation: (step.cos(), step.sin()),
            left_taps: tap(PLATE_LEFT_TAPS),
            right_taps: tap(PLATE_RIGHT_TAPS),
        }
    }

    /// Sets the tank's gain for a decay of `seconds`: four passes of `decay` per trip round the
    /// tank, and the second allpasses' gain from it, as Dattorro gives it.
    fn set_decay(&mut self, seconds: f32) {
        self.decay = 10.0_f32.powf(-3.0 * PLATE_LOOP_SECONDS / (4.0 * seconds));
        let diffusion = (self.decay + 0.15).clamp(0.25, 0.5);
        for half in &mut self.halves {
            half.diffuser.gain = diffusion;
        }
    }

    /// Brings the LFO's phasor back to unit length; called once per block.
    fn renormalise(&mut self) {
        let (c, s) = self.lfo;
        let length = (c * c + s * s).sqrt();
        self.lfo = if length > 0.0 {
            (c / length, s / length)
        } else {
            (1.0, 0.0)
        };
    }

    /// The sum of `taps` read from the halves.
    fn read(&self, taps: &[(bool, Part, usize, f32); 7]) -> f32 {
        PLATE_TAP
            * taps
                .iter()
                .map(|&(right, part, frames, sign)| {
                    let half = &self.halves[usize::from(right)];
                    let line = match part {
                        Part::First => &half.first.line,
                        Part::Diffuser => &half.diffuser.line,
                        Part::Second => &half.second.line,
                    };
                    sign * line.read(frames)
                })
                .sum::<f32>()
    }

    /// One frame of mono input `x`, with the damping low-pass `damping`.
    fn frame(&mut self, x: f32, damping: &OnePoleCoefficients) -> (f32, f32) {
        let out = (self.read(&self.left_taps), self.read(&self.right_taps));
        self.band = PLATE_BANDWIDTH * x + (1.0 - PLATE_BANDWIDTH) * self.band;
        let mut d = self.band;
        for allpass in &mut self.diffusers {
            d = allpass.process(d);
        }
        let (c, s) = self.lfo;
        let (rc, rs) = self.rotation;
        self.lfo = (c * rc - s * rs, c * rs + s * rc);
        let into_left = d + self.halves[1].second.end() * self.decay;
        let into_right = d + self.halves[0].second.end() * self.decay;
        let decay = self.decay;
        let [left, right] = &mut self.halves;
        left.process(into_left, self.excursion * c, decay, damping);
        right.process(into_right, self.excursion * s, decay, damping);
        out
    }

    fn clear(&mut self) {
        self.band = 0.0;
        self.diffusers.iter_mut().for_each(Allpass::clear);
        self.halves.iter_mut().for_each(Half::clear);
        self.lfo = (1.0, 0.0);
    }

    #[cfg(test)]
    fn samples(&self) -> usize {
        let diffusers: usize = self.diffusers.iter().map(Allpass::samples).sum();
        diffusers + self.halves.iter().map(Half::samples).sum::<usize>()
    }
}

/// A prepared reverb: the pre-delay, both engines and the ramps.
struct Running {
    sample_rate: f32,
    pre_delay: [TapDelay; 2],
    taps: [Tap; 2],
    algorithm: Algorithm,
    room: Room,
    plate: Plate,
    /// The decay, as log2 of seconds.
    decay_log: Ramp,
    /// The damping cutoff, as log2 of hertz.
    damping_log: Ramp,
    damping: OnePoleCoefficients,
    /// The wet share of the output, 0 to 1.
    mix: Ramp,
}

impl Running {
    /// A silent reverb at `sample_rate`, its parameters at `value`'s values, with no ramp or
    /// crossfade in. Allocates.
    fn new(sample_rate: u32, value: impl Fn(Control) -> f32) -> Running {
        #[allow(clippy::cast_precision_loss)] // audio rates are exact in f32
        let rate = sample_rate as f32;
        let max_pre = to_tap(
            (f64::from(sample_rate) * f64::from(MAX_PRE_DELAY_MS) / 1000.0).ceil(),
            usize::MAX,
        );
        let ramp = |value: f32| Ramp::new(value, sample_rate);
        let decay = value(Control::Decay);
        let damping = value(Control::Damping);
        let mut running = Running {
            sample_rate: rate,
            pre_delay: [TapDelay::new(max_pre), TapDelay::new(max_pre)],
            taps: [Tap::new(1, sample_rate), Tap::new(1, sample_rate)],
            algorithm: Algorithm::from_value(value(Control::Algorithm)),
            room: Room::new(sample_rate),
            plate: Plate::new(sample_rate),
            decay_log: ramp(decay.log2()),
            damping_log: ramp(damping.log2()),
            damping: OnePoleCoefficients::low_pass(damping, rate),
            mix: ramp(value(Control::Mix) / 100.0),
        };
        running.set_decay(decay);
        running.set(&value);
        // Start at the pre-delay, with no crossfade in.
        running.taps = running.taps.map(|t| Tap::new(t.wanted, sample_rate));
        running
    }

    /// Takes the parameter values, once per block.
    fn set(&mut self, value: &impl Fn(Control) -> f32) {
        let frames = f64::from(value(Control::PreDelay)) * f64::from(self.sample_rate) / 1000.0;
        let max = self.pre_delay[0].max_tap();
        for tap in &mut self.taps {
            tap.wanted = to_tap(frames, max);
        }
        let algorithm = Algorithm::from_value(value(Control::Algorithm));
        if algorithm != self.algorithm {
            // Stopped-only: switch at once, the new engine starting silent.
            self.algorithm = algorithm;
            self.room.clear();
            self.plate.clear();
        }
        self.decay_log.set_target(value(Control::Decay).log2());
        self.damping_log.set_target(value(Control::Damping).log2());
        self.mix.set_target(value(Control::Mix) / 100.0);
        self.plate.renormalise();
    }

    /// Sets both engines' decay to `seconds`.
    fn set_decay(&mut self, seconds: f32) {
        self.room.set_decay(seconds);
        self.plate.set_decay(seconds);
    }

    /// One frame of `left`, `right`.
    fn frame(&mut self, left: f32, right: f32) -> (f32, f32) {
        if self.decay_log.is_ramping() {
            let seconds = self.decay_log.next_value().exp2();
            self.set_decay(seconds);
        }
        if self.damping_log.is_ramping() {
            let hz = self.damping_log.next_value().exp2();
            self.damping = OnePoleCoefficients::low_pass(hz, self.sample_rate);
        }
        let mix = self.mix.next_value();
        let [line_l, line_r] = &self.pre_delay;
        let [tap_l, tap_r] = &mut self.taps;
        let (pl, pr) = (tap_l.read(line_l), tap_r.read(line_r));
        self.pre_delay[0].write(left);
        self.pre_delay[1].write(right);
        let (wl, wr) = match self.algorithm {
            Algorithm::Room => self.room.frame(pl, pr, &self.damping),
            Algorithm::Plate => self.plate.frame((pl + pr) * FOLD_DOWN_GAIN, &self.damping),
        };
        let dry = 1.0 - mix;
        (left * dry + wl * mix, right * dry + wr * mix)
    }

    /// The samples it holds, across every line.
    #[cfg(test)]
    fn samples(&self) -> usize {
        let pre: usize = self.pre_delay.iter().map(|l| l.max_tap() + 1).sum();
        pre + self.room.samples() + self.plate.samples()
    }
}

/// How many frames the impulse simulation runs between renormalisations of the plate's LFO, as
/// a block of processing would.
const IMPULSE_BLOCK: usize = 256;

/// The curve of a `reverb` effect whose parameters stand at `values`, in declaration order (see
/// [`crate::CurveFn`]): its impulse response, simulated when it is read.
pub(crate) fn impulse_curve(
    values: &[f32],
    sample_rate: u32,
    _frames_per_whole_note: f64,
) -> Option<crate::Curve> {
    let values = <&[f32; CONTROLS]>::try_from(values).ok()?;
    Some(crate::Curve::Impulse(ImpulseCurve::reverb(
        *values,
        sample_rate,
    )))
}

/// The time for a reverb at `values` to fall [`SPAN_DB`] after an impulse, in seconds,
/// unclamped: the pre-delay, then the decay (the time to fall 60 dB) scaled to that depth.
pub(crate) fn impulse_span(values: &[f32; CONTROLS]) -> f32 {
    let value = |control: Control| values[control as usize];
    value(Control::PreDelay) / 1000.0 + value(Control::Decay) * SPAN_DB / 60.0
}

/// A reverb at `values` from silence, at `sample_rate`: each call runs one frame of input
/// through it. Allocates.
pub(crate) fn impulse_response(
    values: &[f32; CONTROLS],
    sample_rate: u32,
) -> impl FnMut(f32, f32) -> (f32, f32) {
    let mut running = Running::new(sample_rate, |control| values[control as usize]);
    let mut frame = 0_usize;
    move |left, right| {
        if frame.is_multiple_of(IMPULSE_BLOCK) {
            running.plate.renormalise();
        }
        frame = frame.wrapping_add(1);
        running.frame(left, right)
    }
}

/// A reverb: a pre-delay, then a feedback delay network ("room") or Dattorro's plate ("plate").
/// One stereo input and one stereo output.
///
/// `decay` is the time to fall 60 dB; `damping` is a low-pass in every loop, so higher
/// frequencies die sooner. A change of pre-delay crossfades between taps over the ramp time.
/// `algorithm` is stopped-only: a change switches engines at once, the new one starting silent,
/// and the two never run together.
pub struct Reverb {
    parameters: [Parameter; CONTROLS],
    running: Option<Running>,
}

impl Reverb {
    /// A reverb at its defaults. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::Parameter`] if a parameter cannot be made, which does not happen for the fixed
    /// ranges here.
    pub fn new() -> Result<Self, Error> {
        let mut parameters = Vec::with_capacity(CONTROLS);
        for control in ALL {
            parameters.push(Parameter::new(info(control))?);
        }
        let parameters = parameters
            .try_into()
            .map_err(|_| Error::Parameter(signal_graph::Error::InvalidParameterRange))?;
        Ok(Reverb {
            parameters,
            running: None,
        })
    }
}

impl Node for Reverb {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Stereo)]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Stereo)]
    }

    fn parameters(&self) -> Vec<Parameter> {
        self.parameters.to_vec()
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        let parameters = &self.parameters;
        self.running = Some(Running::new(sample_rate, |control| {
            parameters[control as usize].get()
        }));
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let parameters = &self.parameters;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        running.set(&|control: Control| parameters[control as usize].get());
        let (in_left, in_right) = (inputs.channel(0, 0), inputs.channel(0, 1));
        let sample =
            |input: Option<&[f32]>, i: usize| input.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            (*l, *r) = running.frame(sample(in_left, i), sample(in_right, i));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_declares_parameters_in_groups() -> Result<(), Error> {
        let infos: Vec<ParameterInfo> = Reverb::new()?
            .parameters()
            .iter()
            .map(|p| p.info().clone())
            .collect();
        let names: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["algorithm", "pre-delay", "decay", "damping", "mix"]);
        let groups: Vec<&str> = infos.iter().map(|i| i.group.as_str()).collect();
        assert_eq!(groups, ["type", "time", "time", "tone", "output"]);
        let algorithm = &infos[Control::Algorithm as usize];
        assert!(algorithm.stopped_only && !algorithm.sets_latency);
        assert_eq!(infos.iter().filter(|i| i.stopped_only).count(), 1);
        let decay = &infos[Control::Decay as usize];
        assert_eq!(
            (decay.min, decay.max, decay.scale.clone()),
            (0.2, 20.0, Scale::Logarithmic)
        );
        Ok(())
    }

    #[test]
    fn test_room_lines_are_distinct_primes_at_common_rates() {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let room = Room::new(rate);
            let lengths: Vec<usize> = room.lines.iter().map(|l| l.frames).collect();
            assert!(lengths.iter().all(|&n| is_prime(n)), "{rate}: {lengths:?}");
            assert!(
                lengths.windows(2).all(|w| w[0] < w[1]),
                "{rate}: {lengths:?}"
            );
        }
        let room = Room::new(48_000);
        assert_eq!(room.lines.map(|l| l.frames), ROOM_LINES);
    }

    #[test]
    fn test_hadamard_is_orthogonal() {
        let mut v = [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        hadamard(&mut v);
        assert!(v.iter().all(|&x| (x - ROOM_OUTPUT).abs() < 1e-6));
        let mut w = [0.3, -1.2, 0.5, 2.0, -0.7, 0.1, 0.9, -0.4];
        let energy: f32 = w.iter().map(|x| x * x).sum();
        hadamard(&mut w);
        let after: f32 = w.iter().map(|x| x * x).sum();
        assert!((energy - after).abs() < 1e-5);
        // Applied twice, it is the identity.
        hadamard(&mut w);
        assert!((w[3] - 2.0).abs() < 1e-5 && (w[1] + 1.2).abs() < 1e-5);
    }

    #[test]
    fn test_output_signs_are_orthogonal() {
        let dot: f32 = ROOM_LEFT.iter().zip(&ROOM_RIGHT).map(|(a, b)| a * b).sum();
        assert_eq!(dot, 0.0);
    }

    #[test]
    fn test_loop_gain_falls_60_db_in_the_decay_time() {
        // A loop of 480 frames at 48 kHz passes 100 times a second; over 2 s, 200 passes.
        let gain = loop_gain(480.0, 2.0, 48_000.0);
        let db = 20.0 * (gain.powi(200)).log10();
        assert!((db + 60.0).abs() < 1e-2, "{db}");
    }

    #[test]
    fn test_memory_is_under_4_mb_at_192_khz() -> Result<(), Box<dyn std::error::Error>> {
        let mut reverb = Reverb::new()?;
        reverb.prepare(192_000, 64);
        let running = reverb.running.as_ref().ok_or("not prepared")?;
        let bytes = running.samples() * std::mem::size_of::<f32>();
        assert!(bytes < 4 << 20, "{bytes}");
        Ok(())
    }
}
