//! The amplifier: a pre-amp, a static waveshaper, makeup gain and a DC-removing high-pass, with
//! optional oversampling around the shaper (`docs/architecture/effects.md` "Amplifier").

use std::f32::consts::{FRAC_2_PI, FRAC_PI_2};

use audio_buffers::Layout;
use dsp::{Factor, Oversampler, db_to_gain};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Reading, Scale};

use crate::error::Error;
use crate::filters::{Cascade, Response};

/// The DC filter's cutoff, in Hz.
const DC_CUTOFF: f32 = 20.0;

/// The DC filter's order: 24 dB per octave.
const DC_ORDER: f32 = 4.0;

/// The parameters, in the order the effect declares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum Control {
    Drive,
    Bias,
    Shape,
    Makeup,
    Oversampling,
}

/// How many parameters there are.
const CONTROLS: usize = 5;

/// Every control, in declaration order.
const ALL: [Control; CONTROLS] = [
    Control::Drive,
    Control::Bias,
    Control::Shape,
    Control::Makeup,
    Control::Oversampling,
];

/// The shapes' names, by index.
const SHAPES: [&str; 3] = ["warm", "crunched", "harsh"];

/// The oversampling options' names, by index.
const FACTORS: [&str; 3] = ["off", "2×", "4×"];

/// A static transfer function of the shaper. Each has slope 1 at 0 and tends to ±1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    /// `(2/π)·atan((π/2)·u)`: the softest knee.
    Warm,
    /// `tanh(u)`.
    Crunched,
    /// `u` clamped to ±1: a hard clip.
    Harsh,
}

impl Shape {
    /// The shape a `shape` parameter value means: rounded, out-of-range values clamped.
    pub fn from_value(value: f32) -> Shape {
        if value >= 1.5 {
            Shape::Harsh
        } else if value >= 0.5 {
            Shape::Crunched
        } else {
            Shape::Warm
        }
    }

    /// The shaper's output for an input `u`.
    pub fn apply(self, u: f32) -> f32 {
        match self {
            Shape::Warm => FRAC_2_PI * (FRAC_PI_2 * u).atan(),
            Shape::Crunched => u.tanh(),
            Shape::Harsh => u.clamp(-1.0, 1.0),
        }
    }
}

/// The oversampling factor an `oversampling` parameter value means.
fn factor_from_value(value: f32) -> Factor {
    if value >= 1.5 {
        Factor::Four
    } else if value >= 0.5 {
        Factor::Two
    } else {
        Factor::One
    }
}

/// What parameter `control` is: its saved name, range, default, unit, scale and group.
fn info(control: Control) -> ParameterInfo {
    let choice = |name: &str, options: &[&str]| {
        #[allow(clippy::cast_precision_loss)] // three options
        let max = (options.len() - 1) as f32;
        ParameterInfo::new(name, 0.0, max, 0.0).with_scale(Scale::Choice(
            options.iter().map(|&o| o.to_owned()).collect(),
        ))
    };
    match control {
        Control::Drive => ParameterInfo::new("drive", -12.0, 36.0, 0.0)
            .with_unit("dB")
            .with_group("pre-amp"),
        Control::Bias => ParameterInfo::new("bias", -1.0, 1.0, 0.0).with_group("pre-amp"),
        Control::Shape => choice("shape", &SHAPES).with_group("shaper"),
        Control::Makeup => ParameterInfo::new("makeup", -36.0, 12.0, 0.0)
            .with_unit("dB")
            .with_group("output"),
        Control::Oversampling => choice("oversampling", &FACTORS)
            .sets_latency()
            .with_group("output"),
    }
}

/// The static transfer curve of an `amplifier` node, as drawn: the shaper's output for a steady
/// input, recentred by the offset the DC filter removes at silence.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShaperCurve {
    drive: f32,
    bias: f32,
    shape: Shape,
    makeup: f32,
}

impl ShaperCurve {
    /// The output for an input sample `x`: `makeup·(f(drive·x + bias) − f(bias))`. Passes through
    /// the origin.
    pub fn output(&self, x: f32) -> f32 {
        self.makeup * (self.shape.apply(self.drive * x + self.bias) - self.shape.apply(self.bias))
    }
}

/// The curve of an `amplifier` effect whose parameters stand at `values`, in declaration order
/// (see [`crate::CurveFn`]). Read through [`Control`] and built by the same mapping as
/// processing, so a display of it cannot differ.
pub(crate) fn curve(values: &[f32], _sample_rate: u32, _whole_note: f64) -> Option<crate::Curve> {
    let values = <&[f32; CONTROLS]>::try_from(values).ok()?;
    let value = |control: Control| values[control as usize];
    Some(crate::Curve::Shaper(ShaperCurve {
        drive: db_to_gain(value(Control::Drive)),
        bias: value(Control::Bias),
        shape: Shape::from_value(value(Control::Shape)),
        makeup: db_to_gain(value(Control::Makeup)),
    }))
}

/// Every shape, in `shape` parameter order.
const SHAPES_IN_ORDER: [Shape; 3] = [Shape::Warm, Shape::Crunched, Shape::Harsh];

impl Shape {
    /// The shape's position in [`SHAPES_IN_ORDER`].
    fn index(self) -> usize {
        self as usize
    }
}

/// The share of each shape in what is heard, by [`SHAPES_IN_ORDER`] index; the shares sum to 1.
type Mix = [f32; 3];

/// A prepared amplifier: ramps, the shape crossfade, and each channel's oversampler and DC
/// filter.
struct Running {
    sample_rate: u32,
    /// Drive, in dB.
    drive: Ramp,
    /// Bias, as a sample value.
    bias: Ramp,
    /// Makeup, in dB.
    makeup: Ramp,
    /// The shape being faded to.
    shape: Shape,
    /// The mix heard when the fade began.
    fading_from: Mix,
    /// How much of `shape` has replaced `fading_from`: ramps from 0 to 1 after a change.
    fade_in: Ramp,
    oversamplers: [Oversampler; 2],
    dc: Cascade,
}

impl Running {
    /// The mix of shapes for a fade `fade_in` of the way from `fading_from` to `shape`.
    fn mix(fading_from: &Mix, shape: Shape, fade_in: f32) -> Mix {
        let mut mix = fading_from.map(|share| share * (1.0 - fade_in));
        mix[shape.index()] += fade_in;
        mix
    }

    /// Takes the parameter readings, once per block; each ramp reaches its target by the next
    /// control point while a curve drives its parameter.
    fn set(&mut self, read: impl Fn(Control) -> Reading) {
        self.drive.approach(read(Control::Drive));
        self.bias.approach(read(Control::Bias));
        self.makeup.approach(read(Control::Makeup));
        let shape = Shape::from_value(read(Control::Shape).value);
        if shape != self.shape {
            // A change mid-fade starts from the mix heard now, so the output does not jump.
            self.fading_from = Self::mix(&self.fading_from, self.shape, self.fade_in.current());
            self.shape = shape;
            self.fade_in = Ramp::new(0.0, self.sample_rate);
            self.fade_in.set_target(1.0);
        }
    }

    /// One frame of `left`, `right`.
    fn frame(&mut self, left: f32, right: f32) -> (f32, f32) {
        let drive = db_to_gain(self.drive.next_value());
        let bias = self.bias.next_value();
        let makeup = db_to_gain(self.makeup.next_value());
        let fading = self.fade_in.is_ramping();
        let fade_in = self.fade_in.next_value();
        let shape = self.shape;
        let mix = Self::mix(&self.fading_from, shape, fade_in);
        let shaper = |x: f32| {
            let u = drive * x + bias;
            if fading {
                SHAPES_IN_ORDER
                    .iter()
                    .zip(mix)
                    .filter(|&(_, share)| share > 0.0)
                    .map(|(s, share)| share * s.apply(u))
                    .sum()
            } else {
                shape.apply(u)
            }
        };
        let [left_os, right_os] = &mut self.oversamplers;
        let l = left_os.process(left, shaper) * makeup;
        let r = right_os.process(right, shaper) * makeup;
        (self.dc.process(0, l), self.dc.process(1, r))
    }
}

/// An amplifier: `makeup·f(drive·x + bias)`, high-passed at 20 Hz to remove the bias's DC, per
/// channel. One stereo input and one stereo output.
///
/// `drive` and `makeup` ramp in dB and `bias` ramps linearly; a change of `shape` crossfades
/// from the mix of shapes heard to the new one over the ramp time. With `oversampling` on, the shaper runs
/// at 2 or 4 times the rate, the ramps holding over each frame's sub-samples; the factor is fixed
/// when the node is prepared, and the oversampler's delay is the node's latency.
pub struct Amplifier {
    parameters: [Parameter; CONTROLS],
    running: Option<Running>,
}

impl Amplifier {
    /// An amplifier at its defaults: unity drive, no bias, the warm shape, no oversampling.
    /// Allocates.
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
        Ok(Amplifier {
            parameters,
            running: None,
        })
    }

    /// The current value of `control`.
    fn get(&self, control: Control) -> f32 {
        self.parameters[control as usize].get()
    }
}

impl Node for Amplifier {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Stereo)]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Stereo)]
    }

    fn latency(&self) -> usize {
        self.running
            .as_ref()
            .map_or(0, |r| r.oversamplers[0].latency())
    }

    fn parameters(&self) -> Vec<Parameter> {
        self.parameters.to_vec()
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        let factor = factor_from_value(self.get(Control::Oversampling));
        let oversampler = || {
            Oversampler::new(factor, sample_rate)
                .or_else(|_| Oversampler::new(Factor::One, sample_rate))
        };
        let (Ok(left), Ok(right)) = (oversampler(), oversampler()) else {
            self.running = None;
            return;
        };
        let ramp = |control| Ramp::new(self.get(control), sample_rate);
        let shape = Shape::from_value(self.get(Control::Shape));
        self.running = Some(Running {
            sample_rate,
            drive: ramp(Control::Drive),
            bias: ramp(Control::Bias),
            makeup: ramp(Control::Makeup),
            shape,
            fading_from: Running::mix(&[0.0; 3], shape, 1.0),
            fade_in: Ramp::new(1.0, sample_rate),
            oversamplers: [left, right],
            dc: Cascade::new(Response::HighPass, DC_CUTOFF, DC_ORDER, sample_rate),
        });
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let parameters = &self.parameters;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        running.set(|control| parameters[control as usize].read());
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
    use crate::testing::{assert_lands, automated, off_default};

    #[test]
    fn test_set_lands_automated_values_on_the_control_point()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut amplifier = Amplifier::new()?;
        amplifier.prepare(48_000, 64);
        let infos: Vec<ParameterInfo> = amplifier
            .parameters
            .iter()
            .map(|p| p.info().clone())
            .collect();
        let running = amplifier.running.as_mut().ok_or("not prepared")?;
        running.set(|control| automated(off_default(&infos[control as usize])));
        assert_lands(&mut [
            ("drive", &mut running.drive),
            ("bias", &mut running.bias),
            ("makeup", &mut running.makeup),
        ]);
        Ok(())
    }

    fn infos() -> Result<Vec<ParameterInfo>, Error> {
        Ok(Amplifier::new()?
            .parameters()
            .iter()
            .map(|p| p.info().clone())
            .collect())
    }

    #[test]
    fn test_new_declares_parameters_in_groups() -> Result<(), Error> {
        let infos = infos()?;
        let names: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["drive", "bias", "shape", "makeup", "oversampling"]);
        let groups: Vec<&str> = infos.iter().map(|i| i.group.as_str()).collect();
        assert_eq!(groups, ["pre-amp", "pre-amp", "shaper", "output", "output"]);
        let oversampling = &infos[Control::Oversampling as usize];
        assert!(oversampling.sets_latency && oversampling.stopped_only);
        assert_eq!(infos.iter().filter(|i| i.stopped_only).count(), 1);
        Ok(())
    }

    #[test]
    fn test_shape_apply_formulas_and_limits() {
        let u = 0.7_f32;
        assert!((Shape::Warm.apply(u) - FRAC_2_PI * (FRAC_PI_2 * u).atan()).abs() < 1e-7);
        assert!((Shape::Crunched.apply(u) - u.tanh()).abs() < 1e-7);
        assert_eq!(Shape::Harsh.apply(u), u);
        for shape in [Shape::Warm, Shape::Crunched, Shape::Harsh] {
            assert_eq!(shape.apply(0.0), 0.0);
            // Unity slope at the origin.
            assert!((shape.apply(1e-3) - 1e-3).abs() < 1e-8, "{shape:?}");
            // Odd, and bounded by ±1.
            assert_eq!(shape.apply(-u), -shape.apply(u));
            assert!(shape.apply(1e6) <= 1.0 && shape.apply(1e6) > 0.999);
            assert!(shape.apply(-1e6) >= -1.0 && shape.apply(-1e6) < -0.999);
        }
        assert_eq!(Shape::Harsh.apply(3.0), 1.0);
    }

    #[test]
    fn test_shape_from_value_rounds() {
        assert_eq!(Shape::from_value(0.0), Shape::Warm);
        assert_eq!(Shape::from_value(1.0), Shape::Crunched);
        assert_eq!(Shape::from_value(2.0), Shape::Harsh);
        assert_eq!(Shape::from_value(f32::NAN), Shape::Warm);
    }

    #[test]
    fn test_latency_follows_the_prepared_factor() -> Result<(), Error> {
        let mut amplifier = Amplifier::new()?;
        amplifier.prepare(48_000, 64);
        assert_eq!(amplifier.latency(), 0);
        for (value, factor) in [(1.0, Factor::Two), (2.0, Factor::Four)] {
            amplifier.parameters[Control::Oversampling as usize].set(value);
            amplifier.prepare(48_000, 64);
            let expected = Oversampler::new(factor, 48_000)
                .map_err(|_| Error::Parameter(signal_graph::Error::InvalidParameterRange))?
                .latency();
            assert!(expected > 0);
            assert_eq!(amplifier.latency(), expected);
        }
        Ok(())
    }

    #[test]
    fn test_set_shape_change_mid_fade_does_not_jump() -> Result<(), Box<dyn std::error::Error>> {
        let mut amplifier = Amplifier::new()?;
        amplifier.parameters[Control::Drive as usize].set(12.0);
        amplifier.prepare(48_000, 64);
        let parameters = amplifier.parameters.clone();
        let running = amplifier.running.as_mut().ok_or("not prepared")?;
        let set = |running: &mut Running| running.set(|c| parameters[c as usize].read());
        // A steady input deep in the shapes' knees, where they differ most; let the DC filter
        // settle so only the shaper moves the output.
        let x = 0.8;
        let mut last = 0.0;
        for _ in 0..48_000 {
            last = running.frame(x, x).0;
        }
        let mut largest_step = 0.0_f32;
        // Warm to harsh, then crunched 300 frames into the 480-frame fade, then back to warm
        // 100 frames later.
        for (shape, frames) in [(2.0, 300), (1.0, 100), (0.0, 1000)] {
            parameters[Control::Shape as usize].set(shape);
            set(running);
            for _ in 0..frames {
                let y = running.frame(x, x).0;
                largest_step = largest_step.max((y - last).abs());
                last = y;
            }
        }
        // The shapes differ by about 0.2 here; a fade moves at most that over 480 frames.
        assert!(largest_step < 2e-3, "{largest_step}");
        Ok(())
    }

    #[test]
    fn test_mix_shares_sum_to_one() {
        let from = Running::mix(&[0.0; 3], Shape::Warm, 1.0);
        assert_eq!(from, [1.0, 0.0, 0.0]);
        let mid = Running::mix(&from, Shape::Harsh, 0.25);
        assert_eq!(mid, [0.75, 0.0, 0.25]);
        let again = Running::mix(&mid, Shape::Crunched, 0.5);
        assert_eq!(again, [0.375, 0.5, 0.125]);
        assert!((again.iter().sum::<f32>() - 1.0).abs() < 1e-6);
    }

    /// The shaper curve at the defaults with `changes` by name.
    fn shaper(changes: &[(&str, f32)]) -> Result<ShaperCurve, Box<dyn std::error::Error>> {
        let values = crate::curve::values_with("amplifier", changes).ok_or("unknown kind")?;
        match curve(&values, 48_000, 96_000.0) {
            Some(crate::Curve::Shaper(shaper)) => Ok(shaper),
            other => Err(format!("not a shaper curve: {other:?}").into()),
        }
    }

    #[test]
    fn test_curve_passes_through_the_origin() -> Result<(), Box<dyn std::error::Error>> {
        for shape in [0.0, 1.0, 2.0] {
            for bias in [-0.6, 0.0, 0.4] {
                let curve = shaper(&[("shape", shape), ("bias", bias), ("drive", 12.0)])?;
                assert!(curve.output(0.0).abs() < 1e-7);
                assert!(curve.output(0.5) > 0.0 && curve.output(-0.5) < 0.0);
            }
        }
        // Defaults: warm at unity, so small signals pass unchanged.
        let curve = shaper(&[])?;
        assert!((curve.output(1e-3) - 1e-3).abs() < 1e-7);
        Ok(())
    }
}
