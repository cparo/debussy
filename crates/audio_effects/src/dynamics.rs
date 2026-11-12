//! The dynamics effect: a compressor, limiter, gate or saturator by its settings
//! (`docs/architecture/effects.md` "Dynamics", ADR-0099).
//!
//! The signal is amplified by the preamp, then delayed by the lookahead. Its level is a weighted
//! RMS over a window of 0.5 ms blocks, recomputed once per block, and the gain the transfer curve
//! gives for that level is interpolated across the next block. The curve works on levels in dB,
//! normalised so the noise floor (−96 dB) is 0 and full scale is 1 (ADR-0102). With attack and release both zero
//! the level is each sample's own magnitude, which makes the curve a waveshaper. Shaping with the
//! output gain, the DC blocker and the clip run oversampled.

use audio_buffers::Layout;
use dsp::{OnePole, OnePoleCoefficients, Oversampler, db_to_gain, gain_to_db};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;

/// The longest attack, in ms.
pub const MAX_ATTACK_MS: f32 = 100.0;

/// The longest release, in ms.
pub const MAX_RELEASE_MS: f32 = 5_000.0;

/// The shortest attack or release other than 0, in ms.
pub const MIN_WINDOW_MS: f32 = 0.25;

/// The smallest transfer curve exponent; the largest is its reciprocal.
pub const MIN_CURVE_EXPONENT: f32 = 1.0 / 16.0;

/// The longest lookahead, in ms.
pub const MAX_LOOKAHEAD_MS: f32 = 100.0;

/// The oversampling factors, by `oversampling` value. The order is part of the saved format.
pub const OVERSAMPLING_FACTORS: [usize; 4] = [1, 2, 4, 8];

/// The length of a detector block, in seconds.
const BLOCK_SECONDS: f64 = 0.000_5;

/// The DC blocker's cutoff, in Hz.
const DC_BLOCKER_HZ: f32 = 7.0;

/// The level the transfer curve maps to 0, in dB; full scale maps to 1.
pub const FLOOR_DB: f32 = -96.0;

/// One of the dynamics effect's controls, each a parameter, in the order they are declared and
/// shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DynamicsControl {
    /// Gain before everything else, in dB.
    Preamp,
    /// Whether left and right share one level.
    LinkChannels,
    /// The window's future side, in ms; stopped-only.
    Attack,
    /// The exponent shaping the attack side; stopped-only.
    AttackExponent,
    /// The window's past side, in ms; stopped-only.
    Release,
    /// The exponent shaping the release side; stopped-only.
    ReleaseExponent,
    /// How far ahead the window reaches, in ms: the effect's delay; stopped-only.
    Lookahead,
    /// Whether negative samples use their own curve, output gain and ceiling.
    Asymmetric,
    /// How sharp the transfer curve's shoulder is: a multiplier inside it.
    PositiveSteepness,
    /// The transfer curve's exponent.
    PositiveExponent,
    /// Gain applied with the transfer curve's, in dB.
    PositiveOutputGain,
    /// The clip level, in dB.
    PositiveCeiling,
    /// [`DynamicsControl::PositiveSteepness`] for negative samples, while asymmetric.
    NegativeSteepness,
    /// [`DynamicsControl::PositiveExponent`] for negative samples, while asymmetric.
    NegativeExponent,
    /// [`DynamicsControl::PositiveOutputGain`] for negative samples, while asymmetric.
    NegativeOutputGain,
    /// [`DynamicsControl::PositiveCeiling`] for negative samples, while asymmetric.
    NegativeCeiling,
    /// The oversampling factor, as an index into [`OVERSAMPLING_FACTORS`]; stopped-only.
    Oversampling,
}

impl DynamicsControl {
    /// Every control, in declaration order.
    pub const ALL: [DynamicsControl; 17] = [
        DynamicsControl::Preamp,
        DynamicsControl::LinkChannels,
        DynamicsControl::Attack,
        DynamicsControl::AttackExponent,
        DynamicsControl::Release,
        DynamicsControl::ReleaseExponent,
        DynamicsControl::Lookahead,
        DynamicsControl::Asymmetric,
        DynamicsControl::PositiveSteepness,
        DynamicsControl::PositiveExponent,
        DynamicsControl::PositiveOutputGain,
        DynamicsControl::PositiveCeiling,
        DynamicsControl::NegativeSteepness,
        DynamicsControl::NegativeExponent,
        DynamicsControl::NegativeOutputGain,
        DynamicsControl::NegativeCeiling,
        DynamicsControl::Oversampling,
    ];

    /// The saved parameter name, such as `"attack exponent"`. Names are part of the saved
    /// format.
    pub fn name(self) -> &'static str {
        match self {
            DynamicsControl::Preamp => "preamp",
            DynamicsControl::LinkChannels => "link channels",
            DynamicsControl::Attack => "attack",
            DynamicsControl::AttackExponent => "attack exponent",
            DynamicsControl::Release => "release",
            DynamicsControl::ReleaseExponent => "release exponent",
            DynamicsControl::Lookahead => "lookahead",
            DynamicsControl::Asymmetric => "asymmetric",
            DynamicsControl::PositiveSteepness => "positive steepness",
            DynamicsControl::PositiveExponent => "positive exponent",
            DynamicsControl::PositiveOutputGain => "positive output gain",
            DynamicsControl::PositiveCeiling => "positive ceiling",
            DynamicsControl::NegativeSteepness => "negative steepness",
            DynamicsControl::NegativeExponent => "negative exponent",
            DynamicsControl::NegativeOutputGain => "negative output gain",
            DynamicsControl::NegativeCeiling => "negative ceiling",
            DynamicsControl::Oversampling => "oversampling",
        }
    }

    /// The parameter's declaration.
    fn info(self) -> ParameterInfo {
        let name = self.name();
        let db = |min, max, group| {
            ParameterInfo::new(name, min, max, 0.0)
                .with_unit("dB")
                .with_group(group)
        };
        let ms = |min, max, default, scale| {
            ParameterInfo::new(name, min, max, default)
                .with_unit("ms")
                .with_scale(scale)
                .with_group("envelope")
                .stopped_only()
        };
        let exponent = |min, group| {
            ParameterInfo::new(name, min, 16.0, 1.0)
                .with_scale(Scale::Logarithmic)
                .with_group(group)
        };
        let steepness = |group| {
            ParameterInfo::new(name, 0.01, 10.0, 1.0)
                .with_scale(Scale::Logarithmic)
                .with_group(group)
        };
        let negative =
            |info: ParameterInfo| info.with_enabled_by(DynamicsControl::Asymmetric.name());
        let window = Scale::LogarithmicOrZero;
        match self {
            DynamicsControl::Preamp => db(-24.0, 48.0, "input"),
            DynamicsControl::LinkChannels => ParameterInfo::new(name, 0.0, 1.0, 1.0)
                .with_scale(Scale::Toggle)
                .with_state_labels("Linked", "Unlinked")
                .with_group("envelope"),
            DynamicsControl::Attack => ms(MIN_WINDOW_MS, MAX_ATTACK_MS, 10.0, window),
            DynamicsControl::Release => ms(MIN_WINDOW_MS, MAX_RELEASE_MS, 100.0, window),
            DynamicsControl::AttackExponent | DynamicsControl::ReleaseExponent => {
                exponent(0.1, "envelope").stopped_only()
            }
            DynamicsControl::Lookahead => ms(0.0, MAX_LOOKAHEAD_MS, 0.0, Scale::Linear),
            DynamicsControl::Asymmetric => ParameterInfo::new(name, 0.0, 1.0, 0.0)
                .with_scale(Scale::Toggle)
                .with_state_labels("Asymmetric", "Symmetric")
                .with_group("gain function"),
            DynamicsControl::PositiveSteepness => steepness("positive"),
            DynamicsControl::PositiveExponent => exponent(MIN_CURVE_EXPONENT, "positive"),
            DynamicsControl::PositiveOutputGain => db(-24.0, 24.0, "positive"),
            DynamicsControl::PositiveCeiling => db(-48.0, 24.0, "positive"),
            DynamicsControl::NegativeSteepness => negative(steepness("negative")),
            DynamicsControl::NegativeExponent => negative(exponent(MIN_CURVE_EXPONENT, "negative")),
            DynamicsControl::NegativeOutputGain => negative(db(-24.0, 24.0, "negative")),
            DynamicsControl::NegativeCeiling => negative(db(-48.0, 24.0, "negative")),
            DynamicsControl::Oversampling => ParameterInfo::new(name, 0.0, 3.0, 0.0)
                .with_scale(Scale::Choice(
                    ["1×", "2×", "4×", "8×"].map(String::from).to_vec(),
                ))
                .with_group("quality")
                .stopped_only(),
        }
    }
}

/// The envelope window's weight at `offset_ms` from its peak (positive towards the future):
/// 1 at the peak, `1 − (offset/attack)^attack_exponent` on the attack side and
/// `1 − (−offset/release)^release_exponent` on the release side, 0 outside. A side of zero length
/// has no weight.
pub fn window_weight(
    offset_ms: f32,
    attack_ms: f32,
    attack_exponent: f32,
    release_ms: f32,
    release_exponent: f32,
) -> f32 {
    let side = |length: f32, exponent: f32, distance: f32| {
        if length > 0.0 && distance <= length {
            1.0 - (distance / length).powf(exponent)
        } else {
            0.0
        }
    };
    if offset_ms > 0.0 {
        side(attack_ms, attack_exponent, offset_ms)
    } else if offset_ms < 0.0 {
        side(release_ms, release_exponent, -offset_ms)
    } else {
        1.0
    }
}

/// The transfer curve: the output level in dB for input level `input_db`. With
/// `u = (input_db − FLOOR_DB) / −FLOOR_DB` (0 at the floor, 1 at full scale), the output is
/// `FLOOR_DB · (1 − v)` with `v = tanh((steepness · u)^exponent) / tanh(steepness^exponent)`, so
/// the floor and full scale map to themselves. An exponent below 1 bends the curve up (quiet
/// levels raised), above 1 down (quiet levels lowered); a higher steepness sharpens the shoulder.
/// At or below the floor the level passes unchanged.
pub fn transfer_db(input_db: f32, steepness: f32, exponent: f32) -> f32 {
    let u = (input_db - FLOOR_DB) / -FLOOR_DB;
    if u.is_nan() || u <= 0.0 {
        return input_db;
    }
    let v = (steepness * u).powf(exponent).tanh() / steepness.powf(exponent).tanh();
    FLOOR_DB * (1.0 - v)
}

/// The gain the transfer curve applies at linear level `level`: unity at silence and at or below
/// the floor.
fn curve_gain(level: f32, steepness: f32, exponent: f32) -> f32 {
    if level <= 0.0 {
        return 1.0;
    }
    let input_db = gain_to_db(level);
    db_to_gain(transfer_db(input_db, steepness, exponent) - input_db)
}

/// The static curve the display plots: the output level for a steady input level `input`
/// (linear), through the preamp, transfer curve, output gain and clip. The gains are in dB; the
/// steepness and exponent are as [`transfer_db`] takes them.
pub fn static_output(
    input: f32,
    preamp_db: f32,
    steepness: f32,
    curve_exponent: f32,
    output_gain_db: f32,
    ceiling_db: f32,
) -> f32 {
    let level = input * db_to_gain(preamp_db);
    let shaped = level * curve_gain(level, steepness, curve_exponent) * db_to_gain(output_gain_db);
    shaped.min(db_to_gain(ceiling_db))
}

/// The settings fixed while running: the stopped-only parameters, read in `prepare`.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Fixed {
    attack_ms: f32,
    attack_exponent: f32,
    release_ms: f32,
    release_exponent: f32,
    lookahead_ms: f32,
    factor: usize,
}

impl Fixed {
    /// The oversampling factor an `oversampling` value means, rounded to the nearest index.
    fn factor_from_value(value: f32) -> usize {
        // Rounded and clamped to the table's indices first.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let index = value.round().clamp(0.0, 3.0) as usize;
        OVERSAMPLING_FACTORS[index]
    }

    /// Whether both sides of the window are empty, so the level is each sample's magnitude.
    fn is_instantaneous(&self) -> bool {
        self.attack_ms <= 0.0 && self.release_ms <= 0.0
    }
}

/// `ms` milliseconds in frames at `sample_rate`, rounded.
fn frames(ms: f32, sample_rate: u32) -> usize {
    // Non-negative and at most a few seconds of frames.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (f64::from(ms.max(0.0)) * f64::from(sample_rate) / 1_000.0).round() as usize;
    frames
}

/// The detector block length at `sample_rate`, in frames: 0.5 ms, at least 1.
fn block_length(sample_rate: u32) -> usize {
    // A few hundred frames at most.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let block = (f64::from(sample_rate) * BLOCK_SECONDS).round() as usize;
    block.max(1)
}

/// The window's weight for each block, newest first, for blocks of `block` frames at
/// `sample_rate`: the weight at each block's centre. Empty when the level is instantaneous. If
/// every weight is 0 the block holding the peak gets weight 1. Allocates.
fn block_weights(fixed: &Fixed, block: usize, sample_rate: u32) -> Box<[f32]> {
    if fixed.is_instantaneous() {
        return Box::new([]);
    }
    let rate = f64::from(sample_rate);
    let block_ms = block as f64 * 1_000.0 / rate;
    let span = f64::from(fixed.attack_ms + fixed.release_ms);
    // At most about ten thousand blocks.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let count = ((span / block_ms).ceil() as usize).max(1);
    // Block `k`'s centre is (k + ½) blocks before the newest input; the peak is `attack` before it.
    #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
    let mut weights: Box<[f32]> = (0..count)
        .map(|k| {
            let offset = f64::from(fixed.attack_ms) - (k as f64 + 0.5) * block_ms;
            window_weight(
                offset as f32,
                fixed.attack_ms,
                fixed.attack_exponent,
                fixed.release_ms,
                fixed.release_exponent,
            )
        })
        .collect();
    if weights.iter().all(|&w| w <= 0.0) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let peak = ((f64::from(fixed.attack_ms) / block_ms) as usize).min(count - 1);
        weights[peak] = 1.0;
    }
    weights
}

/// One channel's running state.
struct ChannelState {
    /// The lookahead delay, a ring of `lookahead` frames.
    delay: Box<[f32]>,
    delay_position: usize,
    /// Each finished block's sum of squares, newest first, stored twice so the window's blocks
    /// are always one contiguous slice from `newest_block`.
    block_sums: Box<[f32]>,
    newest_block: usize,
    /// The sum of squares of the block being filled.
    accumulator: f32,
    /// The gain for positive and for negative samples, stepping towards `target`.
    gain: [f32; 2],
    step: [f32; 2],
    target: [f32; 2],
    oversampler: Oversampler,
    dc_blocker: OnePole,
}

impl ChannelState {
    fn new(fixed: &Fixed, blocks: usize, sample_rate: u32) -> Self {
        ChannelState {
            delay: vec![0.0; frames(fixed.lookahead_ms, sample_rate)].into_boxed_slice(),
            delay_position: 0,
            block_sums: vec![0.0; 2 * blocks].into_boxed_slice(),
            newest_block: 0,
            accumulator: 0.0,
            gain: [0.0; 2],
            step: [0.0; 2],
            target: [0.0; 2],
            oversampler: Oversampler::new(fixed.factor),
            dc_blocker: OnePole::new(),
        }
    }

    /// Pushes `sample` into the lookahead delay and returns the one it delays.
    fn delay(&mut self, sample: f32) -> f32 {
        let Some(slot) = self.delay.get_mut(self.delay_position) else {
            return sample;
        };
        let delayed = std::mem::replace(slot, sample);
        self.delay_position = (self.delay_position + 1) % self.delay.len();
        delayed
    }

    /// Ends the block being filled: stores its sum and starts the next.
    fn finish_block(&mut self) {
        let blocks = self.block_sums.len() / 2;
        if blocks == 0 {
            return;
        }
        self.newest_block = if self.newest_block == 0 {
            blocks - 1
        } else {
            self.newest_block - 1
        };
        self.block_sums[self.newest_block] = self.accumulator;
        self.block_sums[self.newest_block + blocks] = self.accumulator;
        self.accumulator = 0.0;
    }

    /// The weighted sum of the stored block sums.
    fn weighted(&self, weights: &[f32]) -> f32 {
        let recent = &self.block_sums[self.newest_block..self.newest_block + weights.len()];
        weights.iter().zip(recent).map(|(w, s)| w * s).sum()
    }
}

/// The values read once per block that are not ramped.
#[derive(Debug, Clone, Copy)]
struct Live {
    linked: bool,
    asymmetric: bool,
}

/// Every ramped value: linear gains and the curve exponents.
struct Ramps {
    preamp: Ramp,
    /// For positive, then negative, samples.
    output_gain: [Ramp; 2],
    steepness: [Ramp; 2],
    exponent: [Ramp; 2],
    ceiling: [Ramp; 2],
}

/// Linear values for one sample, from [`Ramps`].
#[derive(Debug, Clone, Copy)]
struct Step {
    preamp: f32,
    output_gain: [f32; 2],
    steepness: [f32; 2],
    exponent: [f32; 2],
    ceiling: [f32; 2],
}

impl Ramps {
    fn next(&mut self) -> Step {
        let pair = |r: &mut [Ramp; 2]| [r[0].next_value(), r[1].next_value()];
        Step {
            preamp: self.preamp.next_value(),
            output_gain: pair(&mut self.output_gain),
            steepness: pair(&mut self.steepness),
            exponent: pair(&mut self.exponent),
            ceiling: pair(&mut self.ceiling),
        }
    }
}

/// The state while prepared.
struct Running {
    fixed: Fixed,
    block: usize,
    /// Frames in the block being filled.
    filled: usize,
    weights: Box<[f32]>,
    /// `1 / (block · Σ weights)`: turns a weighted sum of squares into a mean square.
    normaliser: f32,
    ramps: Ramps,
    channels: [ChannelState; 2],
    /// One base frame's oversampled values.
    high: Box<[f32]>,
    dc_coefficients: OnePoleCoefficients,
    asymmetric: bool,
    latency: usize,
}

impl Running {
    fn new(fixed: Fixed, targets: &Targets, sample_rate: u32) -> Self {
        let block = block_length(sample_rate);
        let weights = block_weights(&fixed, block, sample_rate);
        let sum: f32 = weights.iter().sum();
        #[allow(clippy::cast_precision_loss)] // a block is a few hundred frames
        let normaliser = if sum > 0.0 {
            1.0 / (block as f32 * sum)
        } else {
            0.0
        };
        let ramp = |value| Ramp::new(value, sample_rate);
        let pair = |values: [f32; 2]| [ramp(values[0]), ramp(values[1])];
        let channels = [
            ChannelState::new(&fixed, weights.len(), sample_rate),
            ChannelState::new(&fixed, weights.len(), sample_rate),
        ];
        let latency = frames(fixed.lookahead_ms, sample_rate) + channels[0].oversampler.latency();
        #[allow(clippy::cast_precision_loss)] // a rate times at most 8
        let high_rate = sample_rate as f32 * fixed.factor as f32;
        let mut running = Running {
            fixed,
            block,
            filled: 0,
            weights,
            normaliser,
            ramps: Ramps {
                preamp: ramp(targets.preamp),
                output_gain: pair(targets.output_gain),
                steepness: pair(targets.steepness),
                exponent: pair(targets.exponent),
                ceiling: pair(targets.ceiling),
            },
            channels,
            high: vec![0.0; fixed.factor].into_boxed_slice(),
            dc_coefficients: OnePoleCoefficients::high_pass(DC_BLOCKER_HZ, high_rate),
            asymmetric: targets.live.asymmetric,
            latency,
        };
        // Start from the gain silence gets, so the first block does not ramp in from 0.
        let silent = [
            curve_gain(0.0, targets.steepness[0], targets.exponent[0]),
            curve_gain(0.0, targets.steepness[1], targets.exponent[1]),
        ];
        for channel in &mut running.channels {
            channel.gain = silent;
            channel.target = silent;
        }
        running
    }

    /// Sets the ramps' targets.
    fn set(&mut self, targets: &Targets) {
        self.ramps.preamp.set_target(targets.preamp);
        for side in 0..2 {
            self.ramps.output_gain[side].set_target(targets.output_gain[side]);
            self.ramps.steepness[side].set_target(targets.steepness[side]);
            self.ramps.exponent[side].set_target(targets.exponent[side]);
            self.ramps.ceiling[side].set_target(targets.ceiling[side]);
        }
        if targets.live.asymmetric && !self.asymmetric {
            for channel in &mut self.channels {
                channel.dc_blocker.reset();
            }
        }
        self.asymmetric = targets.live.asymmetric;
    }

    /// Recomputes each channel's target gains from the window, at the end of a block.
    fn update_gains(&mut self, step: &Step, linked: bool) {
        let sums = [
            self.channels[0].weighted(&self.weights),
            self.channels[1].weighted(&self.weights),
        ];
        let shared = 0.5 * (sums[0] + sums[1]);
        #[allow(clippy::cast_precision_loss)] // a block is a few hundred frames
        let per_frame = 1.0 / self.block as f32;
        for (channel, sum) in self.channels.iter_mut().zip(sums) {
            let mean_square = if linked { shared } else { sum } * self.normaliser;
            let level = mean_square.max(0.0).sqrt();
            for side in 0..2 {
                channel.target[side] = curve_gain(level, step.steepness[side], step.exponent[side]);
                channel.step[side] = (channel.target[side] - channel.gain[side]) * per_frame;
            }
        }
    }

    /// Processes one frame in place.
    fn frame(&mut self, left: &mut f32, right: &mut f32, live: Live) {
        let step = self.ramps.next();
        // Negative samples use the positive side's values unless asymmetric.
        let side_of = |x: f32| usize::from(live.asymmetric && x < 0.0);
        let instantaneous = self.fixed.is_instantaneous();
        let inputs = [*left * step.preamp, *right * step.preamp];
        if !instantaneous {
            for (channel, x) in self.channels.iter_mut().zip(inputs) {
                channel.accumulator += x * x;
            }
            self.filled += 1;
            if self.filled == self.block {
                self.filled = 0;
                for channel in &mut self.channels {
                    channel.finish_block();
                }
                self.update_gains(&step, live.linked);
            }
        }
        let asymmetric = live.asymmetric;
        let dc = self.dc_coefficients;
        let mut outputs = [0.0; 2];
        for ((channel, x), out) in self.channels.iter_mut().zip(inputs).zip(&mut outputs) {
            let delayed = channel.delay(x);
            channel.oversampler.up(delayed, &mut self.high);
            for sample in self.high.iter_mut() {
                let side = side_of(*sample);
                let gain = if instantaneous {
                    curve_gain(sample.abs(), step.steepness[side], step.exponent[side])
                } else {
                    channel.gain[side]
                };
                // The output gain goes in before the DC blocker, which removes the offset an
                // asymmetric one adds.
                let shaped = *sample * gain * step.output_gain[side];
                let blocked = if asymmetric {
                    channel.dc_blocker.process(shaped, &dc)
                } else {
                    shaped
                };
                // Ceilings are linear gains, so positive; the negative one equals the positive
                // one unless asymmetric.
                *sample = blocked.clamp(-step.ceiling[1], step.ceiling[0]);
            }
            *out = channel.oversampler.down(&self.high);
            for side in 0..2 {
                if channel.gain[side] != channel.target[side] {
                    let next = channel.gain[side] + channel.step[side];
                    let passed = (channel.step[side] > 0.0 && next >= channel.target[side])
                        || (channel.step[side] < 0.0 && next <= channel.target[side]);
                    channel.gain[side] = if passed || channel.step[side] == 0.0 {
                        channel.target[side]
                    } else {
                        next
                    };
                }
            }
        }
        (*left, *right) = (outputs[0], outputs[1]);
    }
}

/// The parameter values a block works towards, as linear values.
struct Targets {
    preamp: f32,
    output_gain: [f32; 2],
    steepness: [f32; 2],
    exponent: [f32; 2],
    ceiling: [f32; 2],
    live: Live,
}

/// The dynamics effect: one stereo input and one stereo output. See the module documentation.
///
/// Its stopped-only parameters (attack, release, their exponents, lookahead and oversampling) are
/// read when it is prepared; a node whose stopped-only values change is replaced, so a new value
/// takes effect from rest.
pub struct Dynamics {
    parameters: Vec<Parameter>,
    running: Option<Running>,
}

impl Dynamics {
    /// A dynamics effect at its defaults. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::Parameter`] if a parameter cannot be made, which does not happen for the fixed
    /// ranges here.
    pub fn new() -> Result<Self, Error> {
        let parameters = DynamicsControl::ALL
            .iter()
            .map(|c| Parameter::new(c.info()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Dynamics {
            parameters,
            running: None,
        })
    }

    /// The value of `control`.
    fn get(&self, control: DynamicsControl) -> f32 {
        let index = DynamicsControl::ALL
            .iter()
            .position(|&c| c == control)
            .unwrap_or(0);
        self.parameters.get(index).map_or(0.0, Parameter::get)
    }

    fn fixed(&self) -> Fixed {
        Fixed {
            attack_ms: self.get(DynamicsControl::Attack),
            attack_exponent: self.get(DynamicsControl::AttackExponent),
            release_ms: self.get(DynamicsControl::Release),
            release_exponent: self.get(DynamicsControl::ReleaseExponent),
            lookahead_ms: self.get(DynamicsControl::Lookahead),
            factor: Fixed::factor_from_value(self.get(DynamicsControl::Oversampling)),
        }
    }

    fn targets(&self) -> Targets {
        use DynamicsControl as C;
        let asymmetric = self.get(C::Asymmetric) >= 0.5;
        let negative = |own: C, positive: C| self.get(if asymmetric { own } else { positive });
        Targets {
            preamp: db_to_gain(self.get(C::Preamp)),
            output_gain: [
                db_to_gain(self.get(C::PositiveOutputGain)),
                db_to_gain(negative(C::NegativeOutputGain, C::PositiveOutputGain)),
            ],
            steepness: [
                self.get(C::PositiveSteepness),
                negative(C::NegativeSteepness, C::PositiveSteepness),
            ],
            exponent: [
                self.get(C::PositiveExponent),
                negative(C::NegativeExponent, C::PositiveExponent),
            ],
            ceiling: [
                db_to_gain(self.get(C::PositiveCeiling)),
                db_to_gain(negative(C::NegativeCeiling, C::PositiveCeiling)),
            ],
            live: Live {
                linked: self.get(C::LinkChannels) >= 0.5,
                asymmetric,
            },
        }
    }

    /// Processes `left` and `right` in place. Does nothing until prepared. Real-time safe.
    pub(crate) fn process_in_place(&mut self, left: &mut [f32], right: &mut [f32]) {
        let targets = self.targets();
        let Some(running) = self.running.as_mut() else {
            return;
        };
        running.set(&targets);
        for (l, r) in left.iter_mut().zip(right.iter_mut()) {
            running.frame(l, r, targets.live);
        }
    }
}

impl Node for Dynamics {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Stereo)]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Stereo)]
    }

    fn latency(&self) -> usize {
        self.running.as_ref().map_or(0, |r| r.latency)
    }

    fn parameters(&self) -> Vec<Parameter> {
        self.parameters.clone()
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        self.running = Some(Running::new(self.fixed(), &self.targets(), sample_rate));
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        let copy = |from: Option<&[f32]>, to: &mut [f32]| match from {
            Some(from) => {
                for (t, f) in to
                    .iter_mut()
                    .zip(from.iter().chain(std::iter::repeat(&0.0)))
                {
                    *t = *f;
                }
            }
            None => to.fill(0.0),
        };
        copy(inputs.channel(0, 0), left);
        copy(inputs.channel(0, 1), right);
        self.process_in_place(left, right);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    /// A prepared effect with `values` set first.
    fn effect(values: &[(DynamicsControl, f32)]) -> Result<Dynamics, Error> {
        let mut dynamics = Dynamics::new()?;
        for &(control, value) in values {
            let index = DynamicsControl::ALL
                .iter()
                .position(|&c| c == control)
                .unwrap_or(0);
            dynamics.parameters[index].set(value);
        }
        dynamics.prepare(RATE, 128);
        Ok(dynamics)
    }

    /// Runs `left` and `right` through `dynamics` in blocks of 128.
    fn run(dynamics: &mut Dynamics, left: &mut [f32], right: &mut [f32]) {
        for (l, r) in left.chunks_mut(128).zip(right.chunks_mut(128)) {
            dynamics.process_in_place(l, r);
        }
    }

    /// A square wave of `amplitude` with a 100-frame period, `frames` long.
    fn square(amplitude: f32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|i| {
                if (i / 50) % 2 == 0 {
                    amplitude
                } else {
                    -amplitude
                }
            })
            .collect()
    }

    fn peak(samples: &[f32]) -> f32 {
        samples.iter().fold(0.0_f32, |m, x| m.max(x.abs()))
    }

    #[test]
    fn test_new_declares_ports_and_parameters() -> Result<(), Error> {
        let dynamics = Dynamics::new()?;
        assert_eq!(dynamics.inputs(), vec![Port::audio("in", Layout::Stereo)]);
        assert_eq!(dynamics.outputs(), vec![Port::audio("out", Layout::Stereo)]);
        let infos: Vec<ParameterInfo> = dynamics
            .parameters()
            .iter()
            .map(|p| p.info().clone())
            .collect();
        let names: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        let expected: Vec<&str> = DynamicsControl::ALL.iter().map(|c| c.name()).collect();
        assert_eq!(names, expected);
        let stopped: Vec<&str> = infos
            .iter()
            .filter(|i| i.stopped_only)
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(
            stopped,
            [
                "attack",
                "attack exponent",
                "release",
                "release exponent",
                "lookahead",
                "oversampling"
            ]
        );
        Ok(())
    }

    #[test]
    fn test_parameters_are_grouped_and_scaled() -> Result<(), Error> {
        let infos: Vec<ParameterInfo> = Dynamics::new()?
            .parameters()
            .iter()
            .map(|p| p.info().clone())
            .collect();
        let groups: Vec<(&str, &str)> = infos
            .iter()
            .map(|i| (i.group.as_str(), i.name.as_str()))
            .collect();
        assert_eq!(
            groups,
            [
                ("input", "preamp"),
                ("envelope", "link channels"),
                ("envelope", "attack"),
                ("envelope", "attack exponent"),
                ("envelope", "release"),
                ("envelope", "release exponent"),
                ("envelope", "lookahead"),
                ("gain function", "asymmetric"),
                ("positive", "positive steepness"),
                ("positive", "positive exponent"),
                ("positive", "positive output gain"),
                ("positive", "positive ceiling"),
                ("negative", "negative steepness"),
                ("negative", "negative exponent"),
                ("negative", "negative output gain"),
                ("negative", "negative ceiling"),
                ("quality", "oversampling"),
            ]
        );
        let enabled: Vec<&str> = infos
            .iter()
            .filter(|i| i.enabled_by.as_deref() == Some("asymmetric"))
            .map(|i| i.name.as_str())
            .collect();
        assert_eq!(
            enabled,
            [
                "negative steepness",
                "negative exponent",
                "negative output gain",
                "negative ceiling"
            ]
        );
        for info in infos.iter().filter(|i| i.name.ends_with(" exponent")) {
            assert_eq!(info.scale, Scale::Logarithmic, "{}", info.name);
        }
        for info in infos
            .iter()
            .filter(|i| i.group == "positive" || i.group == "negative")
            .filter(|i| i.name.ends_with(" exponent"))
        {
            assert_eq!((info.min, info.max, info.default), (1.0 / 16.0, 16.0, 1.0));
        }
        for info in infos
            .iter()
            .filter(|i| ["attack", "release"].contains(&i.name.as_str()))
        {
            assert_eq!(info.scale, Scale::LogarithmicOrZero);
            assert_eq!(info.min, 0.25);
        }
        let negative_gain = &infos[14];
        assert_eq!((negative_gain.min, negative_gain.max), (-24.0, 24.0));
        assert_eq!(negative_gain.unit, "dB");
        Ok(())
    }

    #[test]
    fn test_window_weight_shapes_each_side() {
        assert_eq!(window_weight(0.0, 10.0, 2.0, 100.0, 1.0), 1.0);
        assert!((window_weight(5.0, 10.0, 1.0, 100.0, 1.0) - 0.5).abs() < 1e-6);
        assert!((window_weight(5.0, 10.0, 2.0, 100.0, 1.0) - 0.75).abs() < 1e-6);
        assert!((window_weight(-50.0, 10.0, 1.0, 100.0, 1.0) - 0.5).abs() < 1e-6);
        assert_eq!(window_weight(11.0, 10.0, 1.0, 100.0, 1.0), 0.0);
        assert_eq!(window_weight(-101.0, 10.0, 1.0, 100.0, 1.0), 0.0);
        // A side of zero length contributes nothing.
        assert_eq!(window_weight(1.0, 0.0, 1.0, 100.0, 1.0), 0.0);
    }

    #[test]
    fn test_block_weights_cover_the_window() {
        let fixed = |attack_ms, release_ms| Fixed {
            attack_ms,
            attack_exponent: 1.0,
            release_ms,
            release_exponent: 1.0,
            lookahead_ms: 0.0,
            factor: 1,
        };
        assert!(block_weights(&fixed(0.0, 0.0), 24, RATE).is_empty());
        // 110 ms of 0.5 ms blocks.
        let weights = block_weights(&fixed(10.0, 100.0), 24, RATE);
        assert_eq!(weights.len(), 220);
        assert!(weights.iter().all(|&w| (0.0..=1.0).contains(&w)));
        // A window shorter than half a block still has weight.
        let weights = block_weights(&fixed(0.1, 0.0), 24, RATE);
        assert_eq!(&weights[..], &[1.0]);
    }

    #[test]
    fn test_curve_gain_is_unity_at_silence_and_below_floor() {
        assert_eq!(curve_gain(0.0, 2.0, 1.0), 1.0);
        assert!((curve_gain(db_to_gain(-120.0), 2.0, 3.0) - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_transfer_db_fixes_floor_and_full_scale() {
        for (steepness, exponent) in [(0.01, 1.0), (1.0, 0.5), (3.0, 2.0), (10.0, 16.0)] {
            assert!((transfer_db(FLOOR_DB + 1e-6, steepness, exponent) - FLOOR_DB).abs() < 0.1);
            assert!(transfer_db(0.0, steepness, exponent).abs() < 1e-4);
        }
        // Below 1 the curve bends up, above 1 down; exponent 0 sends everything above the floor
        // to full scale.
        assert!(transfer_db(-48.0, 0.01, 0.5) > -48.0);
        assert!(transfer_db(-48.0, 0.01, 2.0) < -48.0);
        assert!(transfer_db(-48.0, 1.0, 0.0).abs() < 1e-4);
        // A shallow steepness with exponent 1 is nearly the identity.
        assert!((transfer_db(-48.0, 0.01, 1.0) + 48.0).abs() < 0.01);
    }

    #[test]
    fn test_latency_is_lookahead_plus_oversampling() -> Result<(), Error> {
        assert_eq!(effect(&[])?.latency(), 0);
        assert_eq!(
            effect(&[(DynamicsControl::Lookahead, 10.0)])?.latency(),
            480
        );
        let both = effect(&[
            (DynamicsControl::Lookahead, 10.0),
            (DynamicsControl::Oversampling, 2.0),
        ])?;
        assert_eq!(both.latency(), 512);
        Ok(())
    }

    #[test]
    fn test_process_maps_square_levels_through_the_curve() -> TestResult {
        // A full-scale square wave reads 0 dB, which the curve maps to itself; half scale reads
        // −6 dB, which the default curve raises.
        let mut dynamics = effect(&[])?;
        let (mut left, mut right) = (square(1.0, 24_000), square(1.0, 24_000));
        run(&mut dynamics, &mut left, &mut right);
        let settled = peak(&left[20_000..]);
        assert!((settled - 1.0).abs() < 1e-3, "{settled}");
        let mut dynamics = effect(&[])?;
        let (mut left, mut right) = (square(0.5, 24_000), square(0.5, 24_000));
        run(&mut dynamics, &mut left, &mut right);
        let settled = peak(&left[20_000..]);
        let expected = db_to_gain(transfer_db(gain_to_db(0.5), 1.0, 1.0));
        assert!((settled - expected).abs() < 1e-3, "{settled}");
        assert!(settled > 0.5);
        Ok(())
    }

    #[test]
    fn test_process_gates_quiet_signal_with_high_exponent() -> TestResult {
        let mut dynamics = effect(&[(DynamicsControl::PositiveExponent, 4.0)])?;
        let (mut left, mut right) = (square(0.01, 24_000), square(0.01, 24_000));
        run(&mut dynamics, &mut left, &mut right);
        // −40 dB in comes out below −80 dB.
        assert!(peak(&left[20_000..]) < db_to_gain(-80.0));
        Ok(())
    }

    #[test]
    fn test_process_without_window_is_a_waveshaper() -> TestResult {
        let mut dynamics = effect(&[
            (DynamicsControl::Attack, 0.0),
            (DynamicsControl::Release, 0.0),
            (DynamicsControl::PositiveSteepness, 4.0),
            (DynamicsControl::PositiveExponent, 0.5),
        ])?;
        let input: Vec<f32> = (0..1_000).map(|i| (i as f32 * 0.01).sin()).collect();
        let (mut left, mut right) = (input.clone(), input.clone());
        run(&mut dynamics, &mut left, &mut right);
        for (out, x) in left.iter().zip(&input) {
            assert!((out - x * curve_gain(x.abs(), 4.0, 0.5)).abs() < 1e-4);
        }
        Ok(())
    }

    #[test]
    fn test_process_delays_by_lookahead() -> TestResult {
        let mut dynamics = effect(&[
            (DynamicsControl::Attack, 0.0),
            (DynamicsControl::Release, 0.0),
            (DynamicsControl::Lookahead, 1.0),
        ])?;
        let mut left = vec![0.0; 256];
        left[0] = 0.5;
        let mut right = left.clone();
        run(&mut dynamics, &mut left, &mut right);
        let at = left.iter().position(|&x| x != 0.0);
        assert_eq!(at, Some(48));
        Ok(())
    }

    #[test]
    fn test_process_clips_at_ceiling() -> TestResult {
        let mut dynamics = effect(&[
            (DynamicsControl::PositiveOutputGain, 24.0),
            (DynamicsControl::PositiveCeiling, -6.0),
        ])?;
        let (mut left, mut right) = (square(1.0, 4_800), square(1.0, 4_800));
        run(&mut dynamics, &mut left, &mut right);
        assert!((peak(&left) - db_to_gain(-6.0)).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn test_process_links_channels_by_default() -> TestResult {
        let render = |linked: f32| -> Result<f32, Box<dyn std::error::Error>> {
            let mut dynamics = effect(&[(DynamicsControl::LinkChannels, linked)])?;
            let (mut left, mut right) = (square(1.0, 24_000), square(0.01, 24_000));
            run(&mut dynamics, &mut left, &mut right);
            Ok(peak(&right[20_000..]))
        };
        // Linked: the quiet side takes the gain of the mean level. Unlinked: it keeps its own.
        let shared = ((1.0 + 0.01 * 0.01) / 2.0_f32).sqrt();
        let linked = render(1.0)?;
        assert!(
            (linked - 0.01 * curve_gain(shared, 1.0, 1.0)).abs() < 1e-5,
            "{linked}"
        );
        let unlinked = render(0.0)?;
        assert!(
            (unlinked - 0.01 * curve_gain(0.01, 1.0, 1.0)).abs() < 1e-6,
            "{unlinked}"
        );
        Ok(())
    }

    #[test]
    fn test_process_asymmetric_removes_dc() -> TestResult {
        let mut dynamics = effect(&[
            (DynamicsControl::Attack, 0.0),
            (DynamicsControl::Release, 0.0),
            (DynamicsControl::Asymmetric, 1.0),
            (DynamicsControl::PositiveExponent, 0.5),
            (DynamicsControl::NegativeExponent, 2.0),
            // Headroom, so the clip after the DC blocker does not cut the shifted peaks.
            (DynamicsControl::PositiveCeiling, 6.0),
            (DynamicsControl::NegativeCeiling, 6.0),
        ])?;
        let frames = 4 * RATE as usize;
        let input: Vec<f32> = (0..frames)
            .map(|i| (2.0 * std::f32::consts::PI * 100.0 * i as f32 / RATE as f32).sin())
            .collect();
        let (mut left, mut right) = (input.clone(), input);
        run(&mut dynamics, &mut left, &mut right);
        let tail = &left[frames - RATE as usize..];
        let mean = tail.iter().sum::<f32>() / tail.len() as f32;
        assert!(mean.abs() < 1e-3, "{mean}");
        // The halves are shaped differently: half a period on is not the negation (even
        // harmonics).
        let half = (RATE / 200) as usize;
        let asymmetry =
            (0..tail.len() - half).fold(0.0_f32, |m, i| m.max((tail[i] + tail[i + half]).abs()));
        assert!(asymmetry > 0.2, "{asymmetry}");
        Ok(())
    }

    /// The mean of the positive and of the negative samples of a settled waveshaped sine through
    /// an effect with `values` set.
    fn half_means(values: &[(DynamicsControl, f32)]) -> Result<(f32, f32), Error> {
        let mut settings = vec![
            (DynamicsControl::Attack, 0.0),
            (DynamicsControl::Release, 0.0),
            (DynamicsControl::PositiveCeiling, 24.0),
            (DynamicsControl::NegativeCeiling, 24.0),
        ];
        settings.extend_from_slice(values);
        let mut dynamics = effect(&settings)?;
        let input: Vec<f32> = (0..RATE as usize)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 100.0 * i as f32 / RATE as f32).sin())
            .collect();
        let (mut left, mut right) = (input.clone(), input);
        run(&mut dynamics, &mut left, &mut right);
        let tail = &left[RATE as usize / 2..];
        let mean = |keep: fn(f32) -> bool| {
            let kept: Vec<f32> = tail.iter().copied().filter(|&x| keep(x)).collect();
            kept.iter().map(|x| x.abs()).sum::<f32>() / kept.len().max(1) as f32
        };
        Ok((mean(|x| x > 0.0), mean(|x| x < 0.0)))
    }

    #[test]
    fn test_process_negative_output_gain_needs_asymmetric() -> TestResult {
        let (plain_positive, plain_negative) = half_means(&[])?;
        // Symmetric: the negative output gain is ignored.
        let (positive, negative) = half_means(&[(DynamicsControl::NegativeOutputGain, -12.0)])?;
        assert!((positive - plain_positive).abs() < 1e-4);
        assert!((negative - plain_negative).abs() < 1e-4);
        // Asymmetric: it lowers the negative half; the DC blocker then shifts both halves, so
        // compare their sum (the peak-to-peak size) and their difference.
        let (positive, negative) = half_means(&[
            (DynamicsControl::Asymmetric, 1.0),
            (DynamicsControl::NegativeOutputGain, -12.0),
        ])?;
        assert!(positive + negative < 0.8 * (plain_positive + plain_negative));
        assert!(positive > negative, "{positive} vs {negative}");
        Ok(())
    }

    #[test]
    fn test_process_positive_output_gain_scales_both_halves_when_symmetric() -> TestResult {
        let (plain_positive, plain_negative) = half_means(&[])?;
        let (positive, negative) = half_means(&[(DynamicsControl::PositiveOutputGain, 6.0)])?;
        assert!((positive / plain_positive - db_to_gain(6.0)).abs() < 1e-3);
        assert!((negative / plain_negative - db_to_gain(6.0)).abs() < 1e-3);
        Ok(())
    }

    #[test]
    fn test_process_oversampled_matches_delayed_waveshaper_at_low_frequency() -> TestResult {
        let mut dynamics = effect(&[
            (DynamicsControl::Attack, 0.0),
            (DynamicsControl::Release, 0.0),
            (DynamicsControl::Oversampling, 2.0),
        ])?;
        let input: Vec<f32> = (0..2_000)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 100.0 * i as f32 / RATE as f32).sin())
            .collect();
        let (mut left, mut right) = (input.clone(), input.clone());
        run(&mut dynamics, &mut left, &mut right);
        for i in 500..2_000 {
            let x = input[i - 32];
            assert!(
                (left[i] - x * curve_gain(x.abs(), 1.0, 1.0)).abs() < 2e-3,
                "at {i}"
            );
        }
        Ok(())
    }
}
