//! The dynamics processor: one effect that is a compressor, expander, gate or limiter by its
//! values (`docs/architecture/effects.md` "Dynamics").

use audio_buffers::Layout;
use dsp::{Ballistics, Ceiling, Curve, DelayLine, ceiling_gain_db, curve_gain_db, db_to_gain};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;

/// The largest ratio; at it, `ratio above` is infinite (a flat line) and `ratio below` acts as a
/// gate.
pub const MAX_RATIO: f32 = 100.0;

/// The longest lookahead, in milliseconds.
pub const MAX_LOOKAHEAD_MS: f32 = 10.0;

/// The parameters, in the order the effect declares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum Control {
    Threshold,
    RatioAbove,
    RatioBelow,
    Knee,
    Range,
    CeilingEnabled,
    Ceiling,
    CeilingKnee,
    Attack,
    Release,
    Hold,
    Detector,
    Lookahead,
    Makeup,
}

/// How many parameters there are.
const CONTROLS: usize = 14;

/// The detector's options, by index.
const DETECTORS: [&str; 2] = ["peak", "RMS"];

/// What parameter `control` is: its saved name, range, default, unit, scale and group.
fn info(control: Control) -> ParameterInfo {
    let db =
        |name: &str, min, max, default| ParameterInfo::new(name, min, max, default).with_unit("dB");
    let ms =
        |name: &str, min, max, default| ParameterInfo::new(name, min, max, default).with_unit("ms");
    let ratio = |name: &str, default| {
        ParameterInfo::new(name, 1.0, MAX_RATIO, default)
            .with_scale(Scale::Logarithmic)
            .with_max_label("∞")
    };
    match control {
        Control::Threshold => db("threshold", -80.0, 0.0, -18.0).with_group("curve"),
        Control::RatioAbove => ratio("ratio above", 4.0).with_group("curve"),
        Control::RatioBelow => ratio("ratio below", 1.0).with_group("curve"),
        Control::Knee => db("knee", 0.0, 24.0, 6.0).with_group("curve"),
        Control::Range => db("range", 0.0, 96.0, 96.0).with_group("curve"),
        Control::CeilingEnabled => ParameterInfo::new("ceiling enabled", 0.0, 1.0, 0.0)
            .with_scale(Scale::Toggle)
            .with_group("ceiling"),
        Control::Ceiling => db("ceiling", -24.0, 0.0, -1.0).with_group("ceiling"),
        Control::CeilingKnee => db("ceiling knee", 0.0, 12.0, 0.0).with_group("ceiling"),
        Control::Attack => ms("attack", 0.01, 200.0, 10.0)
            .with_scale(Scale::Logarithmic)
            .with_group("timing"),
        Control::Release => ms("release", 1.0, 3000.0, 150.0)
            .with_scale(Scale::Logarithmic)
            .with_group("timing"),
        Control::Hold => ms("hold", 0.0, 500.0, 0.0).with_group("timing"),
        #[allow(clippy::cast_precision_loss)] // two options
        Control::Detector => ParameterInfo::new("detector", 0.0, (DETECTORS.len() - 1) as f32, 0.0)
            .with_scale(Scale::Choice(
                DETECTORS.iter().map(|&d| d.to_owned()).collect(),
            ))
            .with_group("timing"),
        Control::Lookahead => ms("lookahead", 0.0, MAX_LOOKAHEAD_MS, 0.0)
            .sets_latency()
            .with_group("timing"),
        Control::Makeup => db("makeup", -24.0, 24.0, 0.0).with_group("output"),
    }
}

/// Every control, in declaration order.
const ALL: [Control; CONTROLS] = [
    Control::Threshold,
    Control::RatioAbove,
    Control::RatioBelow,
    Control::Knee,
    Control::Range,
    Control::CeilingEnabled,
    Control::Ceiling,
    Control::CeilingKnee,
    Control::Attack,
    Control::Release,
    Control::Hold,
    Control::Detector,
    Control::Lookahead,
    Control::Makeup,
];

/// The output dB per input dB above the threshold for `ratio`: `1 / ratio`, and 0 at
/// [`MAX_RATIO`].
fn slope_above(ratio: f32) -> f32 {
    if ratio >= MAX_RATIO {
        0.0
    } else {
        1.0 / ratio.max(1.0)
    }
}

/// The steady-state curve of a `dynamics` node: its static curve, makeup and ceiling.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransferCurve {
    curve: Curve,
    ceiling: Option<Ceiling>,
}

impl TransferCurve {
    /// The output level in dB for a steady detected level of `input_db`, as processing gives
    /// it once settled ([`dsp::transfer_db`]).
    pub fn output_db(&self, input_db: f32) -> f32 {
        dsp::transfer_db(input_db, &self.curve, self.ceiling.as_ref())
    }
}

/// The steady-state curve of a `dynamics` effect whose parameters stand at `values`, in
/// declaration order (see [`crate::CurveFn`]). Read through [`Control`] and built by the same
/// mapping as processing, so a display of it cannot differ.
pub(crate) fn curve(values: &[f32], _sample_rate: u32) -> Option<crate::Curve> {
    let values = <&[f32; CONTROLS]>::try_from(values).ok()?;
    let value = |control: Control| values[control as usize];
    let curve = Curve {
        threshold: value(Control::Threshold),
        slope_above: slope_above(value(Control::RatioAbove)),
        ratio_below: value(Control::RatioBelow),
        knee: value(Control::Knee),
        range: value(Control::Range),
        makeup: value(Control::Makeup),
    };
    let ceiling = (value(Control::CeilingEnabled) >= 0.5).then(|| Ceiling {
        level: value(Control::Ceiling),
        knee: value(Control::CeilingKnee),
    });
    Some(crate::Curve::Transfer(TransferCurve { curve, ceiling }))
}

/// The lookahead of `milliseconds` at `sample_rate`, in whole frames.
fn lookahead_frames(milliseconds: f32, sample_rate: u32) -> usize {
    let frames = (f64::from(milliseconds.clamp(0.0, MAX_LOOKAHEAD_MS)) / 1000.0
        * f64::from(sample_rate))
    .round();
    // At most 10 ms of any `u32` rate.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = frames as usize;
    frames
}

/// The ramped values the curve is drawn from.
struct Ramps {
    threshold: Ramp,
    slope_above: Ramp,
    ratio_below: Ramp,
    knee: Ramp,
    range: Ramp,
    ceiling: Ramp,
    ceiling_knee: Ramp,
    /// How much of the ceiling's gain applies: 0 off, 1 on, ramped between.
    ceiling_mix: Ramp,
    makeup: Ramp,
}

/// A prepared processor: ramps, ballistics, and the lookahead delay of each channel.
struct Running {
    sample_rate: f32,
    ramps: Ramps,
    ballistics: Ballistics,
    /// Whether the detector smooths power (RMS) rather than amplitude (peak).
    rms: bool,
    /// The attack, release and hold the ballistics were last set to, in ms.
    times: [f32; 3],
    delays: [DelayLine; 2],
}

/// A feed-forward dynamics processor: a compressor, expander, gate or limiter, by its values. One
/// stereo input and one stereo output; both channels share one gain, from the louder channel.
///
/// The detected level (peak, or power for RMS) is smoothed by attack, release and hold; the static
/// curve turns the smoothed level into a gain in dB, then makeup and an optional ceiling apply;
/// the gain is applied to the input delayed by the lookahead. The lookahead is fixed when the
/// node is prepared and is its latency; changing it replaces the node.
pub struct Dynamics {
    parameters: [Parameter; CONTROLS],
    running: Option<Running>,
}

impl Dynamics {
    /// A dynamics processor at its defaults: a 4:1 compressor at −18 dB. Allocates.
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
        Ok(Dynamics {
            parameters,
            running: None,
        })
    }

    /// The current value of `control`.
    fn get(&self, control: Control) -> f32 {
        self.parameters[control as usize].get()
    }
}

impl Running {
    /// Takes the parameter values, once per block: new ramp targets, ballistics times and
    /// detector.
    fn set(&mut self, value: impl Fn(Control) -> f32) {
        let ramps = &mut self.ramps;
        ramps.threshold.set_target(value(Control::Threshold));
        ramps
            .slope_above
            .set_target(slope_above(value(Control::RatioAbove)));
        ramps.ratio_below.set_target(value(Control::RatioBelow));
        ramps.knee.set_target(value(Control::Knee));
        ramps.range.set_target(value(Control::Range));
        ramps.ceiling.set_target(value(Control::Ceiling));
        ramps.ceiling_knee.set_target(value(Control::CeilingKnee));
        ramps.ceiling_mix.set_target(value(Control::CeilingEnabled));
        ramps.makeup.set_target(value(Control::Makeup));
        let times = [
            value(Control::Attack),
            value(Control::Release),
            value(Control::Hold),
        ];
        if times != self.times {
            self.times = times;
            let [attack, release, hold] = times.map(|ms| ms / 1000.0);
            self.ballistics
                .set_times(attack, release, hold, self.sample_rate);
        }
        let rms = value(Control::Detector) >= 0.5;
        if rms != self.rms {
            // Carry the level over between amplitude and power.
            let level = self.ballistics.level();
            self.ballistics
                .set_level(if rms { level * level } else { level.sqrt() });
            self.rms = rms;
        }
    }

    /// The gain for one frame of input `left`, `right`.
    fn gain(&mut self, left: f32, right: f32) -> f32 {
        let ramps = &mut self.ramps;
        let curve = Curve {
            threshold: ramps.threshold.next_value(),
            slope_above: ramps.slope_above.next_value(),
            ratio_below: ramps.ratio_below.next_value(),
            knee: ramps.knee.next_value(),
            range: ramps.range.next_value(),
            makeup: ramps.makeup.next_value(),
        };
        let ceiling = Ceiling {
            level: ramps.ceiling.next_value(),
            knee: ramps.ceiling_knee.next_value(),
        };
        let mix = ramps.ceiling_mix.next_value();
        let detected = if self.rms {
            (left * left).max(right * right)
        } else {
            left.abs().max(right.abs())
        };
        let level = self.ballistics.process(detected);
        // Power to amplitude for RMS; `gain_to_db` of 0 is −∞, which the curve raises to its
        // floor.
        let amplitude = if self.rms { level.sqrt() } else { level };
        let level_db = dsp::gain_to_db(amplitude);
        let gain_db = curve_gain_db(level_db, &curve);
        let ceiling_db = if mix > 0.0 {
            mix * ceiling_gain_db(level_db.max(dsp::FLOOR_DB) + gain_db, &ceiling)
        } else {
            0.0
        };
        db_to_gain(gain_db + ceiling_db)
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
        self.running.as_ref().map_or(0, |r| r.delays[0].frames())
    }

    fn parameters(&self) -> Vec<Parameter> {
        self.parameters.to_vec()
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        let ramp = |control| Ramp::new(self.get(control), sample_rate);
        let frames = lookahead_frames(self.get(Control::Lookahead), sample_rate);
        // Sample rates are exact in `f32` well past any audio rate.
        #[allow(clippy::cast_precision_loss)]
        let mut running = Running {
            sample_rate: sample_rate as f32,
            ramps: Ramps {
                threshold: ramp(Control::Threshold),
                slope_above: Ramp::new(slope_above(self.get(Control::RatioAbove)), sample_rate),
                ratio_below: ramp(Control::RatioBelow),
                knee: ramp(Control::Knee),
                range: ramp(Control::Range),
                ceiling: ramp(Control::Ceiling),
                ceiling_knee: ramp(Control::CeilingKnee),
                ceiling_mix: ramp(Control::CeilingEnabled),
                makeup: ramp(Control::Makeup),
            },
            ballistics: Ballistics::new(),
            rms: false,
            times: [f32::NAN; 3],
            delays: [DelayLine::new(frames), DelayLine::new(frames)],
        };
        running.set(|control| self.get(control));
        self.running = Some(running);
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let parameters = &self.parameters;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        running.set(|control| parameters[control as usize].get());
        let (in_left, in_right) = (inputs.channel(0, 0), inputs.channel(0, 1));
        let sample =
            |input: Option<&[f32]>, i: usize| input.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            let (x_left, x_right) = (sample(in_left, i), sample(in_right, i));
            let gain = running.gain(x_left, x_right);
            *l = running.delays[0].process(x_left) * gain;
            *r = running.delays[1].process(x_right) * gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_declares_parameters_in_groups() -> Result<(), Error> {
        let dynamics = Dynamics::new()?;
        let infos: Vec<ParameterInfo> = dynamics
            .parameters()
            .iter()
            .map(|p| p.info().clone())
            .collect();
        let names: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "threshold",
                "ratio above",
                "ratio below",
                "knee",
                "range",
                "ceiling enabled",
                "ceiling",
                "ceiling knee",
                "attack",
                "release",
                "hold",
                "detector",
                "lookahead",
                "makeup"
            ]
        );
        let groups: Vec<&str> = infos.iter().map(|i| i.group.as_str()).collect();
        assert_eq!(groups[0..5], ["curve"; 5]);
        assert_eq!(groups[5..8], ["ceiling"; 3]);
        assert_eq!(groups[8..13], ["timing"; 5]);
        assert_eq!(groups[13], "output");
        let lookahead = &infos[Control::Lookahead as usize];
        assert!(lookahead.sets_latency && lookahead.stopped_only);
        assert!(infos.iter().filter(|i| i.stopped_only).count() == 1);
        assert_eq!(
            infos[Control::RatioAbove as usize].max_label.as_deref(),
            Some("∞")
        );
        Ok(())
    }

    #[test]
    fn test_latency_is_the_prepared_lookahead() -> Result<(), Error> {
        let mut dynamics = Dynamics::new()?;
        assert_eq!(dynamics.latency(), 0);
        dynamics.prepare(48_000, 64);
        assert_eq!(dynamics.latency(), 0);
        dynamics.parameters[Control::Lookahead as usize].set(5.0);
        // Fixed until prepared again.
        assert_eq!(dynamics.latency(), 0);
        dynamics.prepare(48_000, 64);
        assert_eq!(dynamics.latency(), 240);
        dynamics.prepare(44_100, 64);
        assert_eq!(dynamics.latency(), 221);
        Ok(())
    }

    /// The transfer curve at the defaults with `changes` by name.
    fn transfer(changes: &[(&str, f32)]) -> Result<TransferCurve, Box<dyn std::error::Error>> {
        let values = crate::curve::values_with("dynamics", changes).ok_or("unknown kind")?;
        match curve(&values, 48_000) {
            Some(crate::Curve::Transfer(transfer)) => Ok(transfer),
            other => Err(format!("not a transfer curve: {other:?}").into()),
        }
    }

    #[test]
    fn test_curve_reads_parameters_in_declaration_order() -> Result<(), Box<dyn std::error::Error>>
    {
        let TransferCurve { curve, ceiling } = transfer(&[])?;
        assert_eq!(
            (curve.threshold, curve.slope_above, curve.knee),
            (-18.0, 0.25, 6.0)
        );
        assert_eq!(ceiling, None);
        // Above the knee at 4:1: −18 + 18/4.
        assert!((transfer(&[])?.output_db(0.0) - (-13.5)).abs() < 1e-4);
        let limiter = [("ceiling enabled", 1.0), ("ratio above", MAX_RATIO)];
        let TransferCurve { curve, ceiling } = transfer(&limiter)?;
        assert_eq!(curve.slope_above, 0.0);
        assert_eq!(
            ceiling,
            Some(Ceiling {
                level: -1.0,
                knee: 0.0
            })
        );
        Ok(())
    }

    #[test]
    fn test_slope_above_is_flat_at_the_largest_ratio() {
        assert_eq!(slope_above(MAX_RATIO), 0.0);
        assert_eq!(slope_above(4.0), 0.25);
        assert_eq!(slope_above(0.5), 1.0);
    }
}
