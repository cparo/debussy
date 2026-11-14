//! Node parameters: atomic values the UI sets, ramped by the node that reads them
//! (`docs/architecture/signal-graph.md` "Processing graph").

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::error::Error;

/// How long a parameter takes to ramp to a new value, in seconds.
pub const RAMP_SECONDS: f64 = 0.01;

/// How a parameter's range is presented and stepped: what a generic control for it looks like.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Scale {
    /// Continuous, spaced evenly.
    #[default]
    Linear,
    /// Continuous, spaced evenly in the logarithm of the value; the minimum is above 0.
    Logarithmic,
    /// Whole numbers only.
    Integer,
    /// One of the named options: the value is the option's index, from 0.
    Choice(Vec<String>),
    /// Off (0) or on (1).
    Toggle,
}

impl Scale {
    /// Whether the value is always a whole number: [`Scale::Integer`], [`Scale::Choice`] and
    /// [`Scale::Toggle`].
    pub fn is_stepped(&self) -> bool {
        matches!(self, Scale::Integer | Scale::Choice(_) | Scale::Toggle)
    }
}

/// What a parameter is: its name, range, unit, scale and group, enough for a generic control, and
/// whether it may change while playing.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterInfo {
    /// The parameter's name, unique within its node; saved projects key values by it.
    pub name: String,
    /// The smallest value.
    pub min: f32,
    /// The largest value.
    pub max: f32,
    /// The value a new parameter starts at; clamped to the range.
    pub default: f32,
    /// The unit shown after the value, such as `"dB"` or `"Hz"`; empty for none.
    pub unit: String,
    /// How the range is presented and stepped.
    pub scale: Scale,
    /// The group a control is shown in, such as `"band 1"`; empty for none.
    pub group: String,
    /// Whether the value may change only while playback is stopped or paused: the UI offers no
    /// change while playing and no automation may drive it, so a node may apply a change at once,
    /// without a ramp.
    pub stopped_only: bool,
    /// Whether the node's [`latency`](crate::Node::latency) depends on the value: a change
    /// replaces the node with one prepared at the new value, so the graph re-plans its delay
    /// compensation. Implies [`stopped_only`](Self::stopped_only).
    pub sets_latency: bool,
    /// For a [`Scale::Toggle`], the names of its states as `(on, off)`, such as
    /// `("Enabled", "Disabled")`, shown in place of the parameter's name; `None` to show the name
    /// in both states. Display only: not saved, and ignored for other scales.
    pub state_labels: Option<(String, String)>,
    /// Text shown in place of the value when it is at the maximum, such as `"∞"` for a ratio
    /// whose top value acts as infinite; `None` to show the number. Display only: not saved.
    pub max_label: Option<String>,
    /// The grid a control snaps to and an arrow press moves by; `None` for the scale's own step.
    /// Display only: not saved.
    pub step: Option<f32>,
    /// The toggle parameter, by name, and the state it must be in (`true` for on) for this
    /// parameter's control to be shown; `None` to show it always. A hidden parameter keeps its
    /// value and still takes effect if the node reads it. Display only: not saved.
    pub shown_while: Option<(String, bool)>,
}

impl ParameterInfo {
    /// A linear parameter with no unit or group, named `name`, from `min` to `max`, starting at
    /// `default`.
    pub fn new(name: impl Into<String>, min: f32, max: f32, default: f32) -> Self {
        ParameterInfo {
            name: name.into(),
            min,
            max,
            default,
            unit: String::new(),
            scale: Scale::Linear,
            group: String::new(),
            stopped_only: false,
            sets_latency: false,
            state_labels: None,
            max_label: None,
            step: None,
            shown_while: None,
        }
    }

    /// This, changeable only while playback is stopped or paused.
    #[must_use]
    pub fn stopped_only(self) -> Self {
        ParameterInfo {
            stopped_only: true,
            ..self
        }
    }

    /// This, setting the node's latency (see [`sets_latency`](Self::sets_latency)), and so
    /// changeable only while playback is stopped or paused.
    #[must_use]
    pub fn sets_latency(self) -> Self {
        ParameterInfo {
            sets_latency: true,
            stopped_only: true,
            ..self
        }
    }

    /// This, with unit `unit`.
    #[must_use]
    pub fn with_unit(self, unit: impl Into<String>) -> Self {
        ParameterInfo {
            unit: unit.into(),
            ..self
        }
    }

    /// This, with scale `scale`.
    #[must_use]
    pub fn with_scale(self, scale: Scale) -> Self {
        ParameterInfo { scale, ..self }
    }

    /// This, with its toggle states named `on` and `off` (see
    /// [`state_labels`](Self::state_labels)).
    #[must_use]
    pub fn with_state_labels(self, on: impl Into<String>, off: impl Into<String>) -> Self {
        ParameterInfo {
            state_labels: Some((on.into(), off.into())),
            ..self
        }
    }

    /// This, showing `label` in place of its value at the maximum (see
    /// [`max_label`](Self::max_label)).
    #[must_use]
    pub fn with_max_label(self, label: impl Into<String>) -> Self {
        ParameterInfo {
            max_label: Some(label.into()),
            ..self
        }
    }

    /// This, snapping to and stepping by `step` (see [`step`](Self::step)).
    #[must_use]
    pub fn with_step(self, step: f32) -> Self {
        ParameterInfo {
            step: Some(step),
            ..self
        }
    }

    /// This, its control shown only while toggle `toggle` is on (`on`) or off (see
    /// [`shown_while`](Self::shown_while)).
    #[must_use]
    pub fn shown_while(self, toggle: impl Into<String>, on: bool) -> Self {
        ParameterInfo {
            shown_while: Some((toggle.into(), on)),
            ..self
        }
    }

    /// This, in group `group`.
    #[must_use]
    pub fn with_group(self, group: impl Into<String>) -> Self {
        ParameterInfo {
            group: group.into(),
            ..self
        }
    }

    /// `value` as the parameter holds it: clamped to the range, and rounded to a whole number if
    /// the scale is stepped. `None` for NaN.
    pub fn constrain(&self, value: f32) -> Option<f32> {
        if value.is_nan() {
            return None;
        }
        let value = value.clamp(self.min, self.max);
        Some(if self.scale.is_stepped() {
            value.round().clamp(self.min, self.max)
        } else {
            value
        })
    }

    /// Whether an automation curve may drive the parameter: it is not
    /// [`stopped_only`](Self::stopped_only) and its scale is not stepped
    /// (`docs/architecture/automation.md` "Curves").
    pub fn is_automatable(&self) -> bool {
        !self.stopped_only && !self.scale.is_stepped()
    }
}

/// The parameter value a normalised curve value `value` stands for: 0 is the minimum and 1 the
/// maximum, spaced evenly in the value for [`Scale::Linear`] and in its logarithm for
/// [`Scale::Logarithmic`]. `value` is clamped to 0 to 1 first, NaN taken as 0. A logarithmic
/// range whose minimum is not above 0 maps linearly; a stepped scale maps linearly and rounds
/// (`docs/architecture/automation.md` "Curves").
pub fn map_to_parameter(value: f32, info: &ParameterInfo) -> f32 {
    let value = if value.is_nan() {
        0.0
    } else {
        value.clamp(0.0, 1.0)
    };
    let mapped = match info.scale {
        Scale::Logarithmic if info.min > 0.0 => {
            // In `f64`, so the ends map back to the minimum and maximum exactly.
            let (low, high) = (f64::from(info.min).ln(), f64::from(info.max).ln());
            (low + f64::from(value) * (high - low)).exp() as f32
        }
        _ => info.min + value * (info.max - info.min),
    };
    // `constrain` keeps the rounding of `exp` inside the range; the value is never NaN here.
    info.constrain(mapped).unwrap_or(info.min)
}

/// The state every handle to one parameter shares.
#[derive(Debug)]
struct Shared {
    info: ParameterInfo,
    /// The value's bits, as `f32::to_bits`.
    bits: AtomicU32,
}

/// A node parameter: an `f32` that any thread can set and a node reads without locking. Cloning
/// gives another handle to the same value.
///
/// Cloning and dropping a handle touch a reference count, and dropping the last one frees the
/// parameter, so handles are made and dropped off the real-time thread. Reading and setting never
/// allocate.
#[derive(Debug, Clone)]
pub struct Parameter {
    shared: Arc<Shared>,
}

impl Parameter {
    /// A parameter set to its default. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidParameterRange`] if the range is not finite, `min > max`, a logarithmic
    /// range does not stay above 0, or a choice's range is not its options' indices.
    pub fn new(info: ParameterInfo) -> Result<Self, Error> {
        if !info.min.is_finite() || !info.max.is_finite() || info.min > info.max {
            return Err(Error::InvalidParameterRange);
        }
        let valid = match &info.scale {
            Scale::Linear | Scale::Integer => true,
            Scale::Logarithmic => info.min > 0.0,
            #[allow(clippy::cast_precision_loss)] // a handful of options
            Scale::Choice(options) => {
                !options.is_empty() && info.min == 0.0 && info.max == (options.len() - 1) as f32
            }
            Scale::Toggle => info.min == 0.0 && info.max == 1.0,
        };
        if !valid {
            return Err(Error::InvalidParameterRange);
        }
        let default = info.constrain(info.default).unwrap_or(info.min);
        let info = ParameterInfo { default, ..info };
        Ok(Parameter {
            shared: Arc::new(Shared {
                info,
                bits: AtomicU32::new(default.to_bits()),
            }),
        })
    }

    /// The parameter's name, range, unit, scale and group.
    pub fn info(&self) -> &ParameterInfo {
        &self.shared.info
    }

    /// The current value.
    pub fn get(&self) -> f32 {
        f32::from_bits(self.shared.bits.load(Ordering::Relaxed))
    }

    /// Sets the value, clamped to the range and, for a stepped scale, rounded. NaN is ignored.
    pub fn set(&self, value: f32) {
        if let Some(value) = self.shared.info.constrain(value) {
            self.shared.bits.store(value.to_bits(), Ordering::Relaxed);
        }
    }
}

/// A value that steps linearly to each new target, reaching it [`RAMP_SECONDS`] after the target
/// was set. A node uses it for values it derives from parameters, such as a gain that combines a
/// level with a mute; [`Smoothed`] is one that follows a single parameter. Never allocates.
#[derive(Debug, Clone)]
pub struct Ramp {
    /// The value given for the latest sample.
    current: f32,
    /// The value being ramped towards.
    target: f32,
    /// The change per sample while ramping.
    step: f32,
    /// Samples left in the ramp.
    remaining: usize,
    /// Samples in a whole ramp.
    ramp: usize,
}

impl Ramp {
    /// A ramp at `sample_rate`, resting at `value`.
    pub fn new(value: f32, sample_rate: u32) -> Self {
        let ramp = (f64::from(sample_rate) * RAMP_SECONDS).ceil().max(1.0);
        // Within `usize` for any `u32` rate.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ramp = ramp as usize;
        Ramp {
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
            ramp,
        }
    }

    /// Sets the target. If it has changed, starts a new ramp from the current value.
    pub fn set_target(&mut self, target: f32) {
        if target != self.target {
            self.target = target;
            self.remaining = self.ramp;
            #[allow(clippy::cast_precision_loss)] // a ramp is at most a few thousand samples
            let samples = self.ramp as f32;
            self.step = (target - self.current) / samples;
        }
    }

    /// The value for the next sample.
    pub fn next_value(&mut self) -> f32 {
        if self.remaining > 0 {
            self.remaining -= 1;
            self.current = if self.remaining == 0 {
                self.target
            } else {
                self.current + self.step
            };
        }
        self.current
    }

    /// The value given for the latest sample.
    pub fn current(&self) -> f32 {
        self.current
    }

    /// The value being ramped towards.
    pub fn target(&self) -> f32 {
        self.target
    }

    /// Whether the value is still moving towards the target.
    pub fn is_ramping(&self) -> bool {
        self.remaining > 0
    }
}

/// A parameter as a node reads it: [`Smoothed::update`] reads the value once per call to
/// `process`, and [`Smoothed::next_value`] steps linearly towards it, reaching it [`RAMP_SECONDS`]
/// after it changed.
#[derive(Debug, Clone)]
pub struct Smoothed {
    parameter: Parameter,
    ramp: Ramp,
}

impl Smoothed {
    /// A smoothed reader of `parameter` at `sample_rate`, starting at its current value.
    pub fn new(parameter: Parameter, sample_rate: u32) -> Self {
        let ramp = Ramp::new(parameter.get(), sample_rate);
        Smoothed { parameter, ramp }
    }

    /// Reads the parameter and, if it has changed, starts a new ramp from the current value.
    pub fn update(&mut self) {
        self.ramp.set_target(self.parameter.get());
    }

    /// The value for the next sample.
    pub fn next_value(&mut self) -> f32 {
        self.ramp.next_value()
    }

    /// The value given for the latest sample.
    pub fn current(&self) -> f32 {
        self.ramp.current()
    }

    /// Whether the value is still moving towards the parameter.
    pub fn is_ramping(&self) -> bool {
        self.ramp.is_ramping()
    }

    /// The parameter this reads.
    pub fn parameter(&self) -> &Parameter {
        &self.parameter
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gain() -> Result<Parameter, Error> {
        Parameter::new(ParameterInfo::new("gain", 0.0, 2.0, 1.0))
    }

    #[test]
    fn test_is_automatable_refuses_stopped_only_and_stepped() {
        let info = ParameterInfo::new("x", 0.0, 4.0, 1.0);
        assert!(info.is_automatable());
        assert!(info.clone().with_scale(Scale::Logarithmic).is_automatable());
        assert!(!info.clone().stopped_only().is_automatable());
        assert!(!info.clone().sets_latency().is_automatable());
        assert!(!info.clone().with_scale(Scale::Integer).is_automatable());
        assert!(!info.clone().with_scale(Scale::Toggle).is_automatable());
        let choice = Scale::Choice(vec!["a".into(), "b".into()]);
        assert!(!info.with_scale(choice).is_automatable());
    }

    #[test]
    fn test_map_to_parameter_linear() {
        let info = ParameterInfo::new("pan", -1.0, 1.0, 0.0);
        assert_eq!(map_to_parameter(0.0, &info), -1.0);
        assert_eq!(map_to_parameter(0.5, &info), 0.0);
        assert_eq!(map_to_parameter(1.0, &info), 1.0);
        assert_eq!(map_to_parameter(-3.0, &info), -1.0);
        assert_eq!(map_to_parameter(7.0, &info), 1.0);
        assert_eq!(map_to_parameter(f32::NAN, &info), -1.0);
    }

    #[test]
    fn test_map_to_parameter_logarithmic() {
        let info =
            ParameterInfo::new("freq", 20.0, 20_000.0, 1_000.0).with_scale(Scale::Logarithmic);
        assert_eq!(map_to_parameter(0.0, &info), 20.0);
        assert_eq!(map_to_parameter(1.0, &info), 20_000.0);
        // The geometric mean of 20 and 20 000 is 632.46.
        assert!((map_to_parameter(0.5, &info) - 632.455_5).abs() < 0.01);
        // A third of the way is a factor of 10 up.
        assert!((map_to_parameter(1.0 / 3.0, &info) - 200.0).abs() < 0.01);
    }

    #[test]
    fn test_map_to_parameter_degenerate_ranges() {
        let bad_log = ParameterInfo::new("x", 0.0, 10.0, 1.0).with_scale(Scale::Logarithmic);
        assert_eq!(map_to_parameter(0.5, &bad_log), 5.0);
        let stepped = ParameterInfo::new("n", 0.0, 3.0, 0.0).with_scale(Scale::Integer);
        assert_eq!(map_to_parameter(0.4, &stepped), 1.0);
    }

    #[test]
    fn test_new_refuses_bad_ranges() {
        let info = |min, max| ParameterInfo::new("", min, max, 0.0);
        assert!(Parameter::new(info(1.0, 0.0)).is_err());
        assert!(Parameter::new(info(f32::NEG_INFINITY, 0.0)).is_err());
        assert!(Parameter::new(info(0.0, f32::NAN)).is_err());
        assert!(Parameter::new(info(0.0, 1.0).with_scale(Scale::Logarithmic)).is_err());
        assert!(Parameter::new(info(1.0, 10.0).with_scale(Scale::Logarithmic)).is_ok());
        let choice = |n: usize| Scale::Choice((0..n).map(|i| i.to_string()).collect());
        assert!(Parameter::new(info(0.0, 2.0).with_scale(choice(3))).is_ok());
        assert!(Parameter::new(info(0.0, 2.0).with_scale(choice(2))).is_err());
        assert!(Parameter::new(info(0.0, 0.0).with_scale(choice(0))).is_err());
        assert!(Parameter::new(info(0.0, 1.0).with_scale(Scale::Toggle)).is_ok());
        assert!(Parameter::new(info(0.0, 2.0).with_scale(Scale::Toggle)).is_err());
    }

    #[test]
    fn test_set_rounds_stepped_scales() -> Result<(), Error> {
        let order =
            Parameter::new(ParameterInfo::new("order", 1.0, 8.0, 2.4).with_scale(Scale::Integer))?;
        assert_eq!(order.get(), 2.0);
        order.set(7.6);
        assert_eq!(order.get(), 8.0);
        order.set(0.2);
        assert_eq!(order.get(), 1.0);
        let level = Parameter::new(ParameterInfo::new("level", 0.0, 1.0, 0.25))?;
        assert_eq!(level.get(), 0.25);
        Ok(())
    }

    #[test]
    fn test_parameter_info_builders_set_metadata() {
        let info = ParameterInfo::new("band 1 gain", -24.0, 24.0, 0.0)
            .with_unit("dB")
            .with_group("band 1");
        assert_eq!(info.unit, "dB");
        assert_eq!(info.group, "band 1");
        assert_eq!(info.scale, Scale::Linear);
        assert!(!info.stopped_only);
        assert!(info.clone().stopped_only().stopped_only);
        assert!(!info.sets_latency);
        let latency = info.clone().sets_latency();
        assert!(latency.sets_latency && latency.stopped_only);
        assert_eq!(info.state_labels, None);
        assert_eq!(info.max_label, None);
        assert_eq!(
            info.clone().with_max_label("∞").max_label.as_deref(),
            Some("∞")
        );
        let toggle = ParameterInfo::new("enabled", 0.0, 1.0, 0.0)
            .with_scale(Scale::Toggle)
            .with_state_labels("Enabled", "Disabled");
        assert_eq!(
            toggle.state_labels,
            Some(("Enabled".to_owned(), "Disabled".to_owned()))
        );
        assert_eq!(info.constrain(30.0), Some(24.0));
        assert_eq!(info.constrain(f32::NAN), None);
    }

    #[test]
    fn test_set_clamps_and_ignores_nan() -> Result<(), Error> {
        let parameter = gain()?;
        let other = parameter.clone();
        other.set(5.0);
        assert_eq!(parameter.get(), 2.0);
        other.set(f32::NAN);
        assert_eq!(parameter.get(), 2.0);
        other.set(-1.0);
        assert_eq!(parameter.get(), 0.0);
        Ok(())
    }

    #[test]
    fn test_smoothed_ramps_over_ten_ms() -> Result<(), Error> {
        let parameter = gain()?;
        let mut smoothed = Smoothed::new(parameter.clone(), 48_000);
        parameter.set(0.0);
        smoothed.update();
        let values: Vec<f32> = (0..480).map(|_| smoothed.next_value()).collect();
        assert!((values[239] - 0.5).abs() < 1e-4);
        assert_eq!(values[479], 0.0);
        assert!(!smoothed.is_ramping());
        assert_eq!(smoothed.next_value(), 0.0);
        Ok(())
    }

    #[test]
    fn test_smoothed_restarts_from_current_value() -> Result<(), Error> {
        let parameter = gain()?;
        let mut smoothed = Smoothed::new(parameter.clone(), 100);
        assert_eq!(smoothed.ramp.ramp, 1);
        let mut smoothed_slow = Smoothed::new(parameter.clone(), 1_000);
        parameter.set(2.0);
        smoothed_slow.update();
        for _ in 0..5 {
            smoothed_slow.next_value();
        }
        parameter.set(1.0);
        smoothed_slow.update();
        let start = smoothed_slow.current();
        assert!((start - 1.5).abs() < 1e-5);
        for _ in 0..10 {
            smoothed_slow.next_value();
        }
        assert_eq!(smoothed_slow.current(), 1.0);
        smoothed.update();
        assert_eq!(smoothed.next_value(), 1.0);
        Ok(())
    }

    #[test]
    fn test_ramp_set_target_steps_linearly() {
        let mut ramp = Ramp::new(0.0, 1_000);
        assert!(!ramp.is_ramping());
        ramp.set_target(1.0);
        assert_eq!(ramp.target(), 1.0);
        let values: Vec<f32> = (0..10).map(|_| ramp.next_value()).collect();
        assert!((values[4] - 0.5).abs() < 1e-6);
        assert_eq!(values[9], 1.0);
        assert!(!ramp.is_ramping());
        ramp.set_target(1.0);
        assert!(!ramp.is_ramping());
    }
}
