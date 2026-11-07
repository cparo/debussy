//! Node parameters: atomic values the UI sets, ramped by the node that reads them
//! (`docs/architecture/signal-graph.md` "Processing graph").

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use crate::error::Error;

/// How long a parameter takes to ramp to a new value, in seconds.
pub const RAMP_SECONDS: f64 = 0.01;

/// What a parameter is: its name and range.
#[derive(Debug, Clone, PartialEq)]
pub struct ParameterInfo {
    /// A name for display.
    pub name: String,
    /// The smallest value.
    pub min: f32,
    /// The largest value.
    pub max: f32,
    /// The value a new parameter starts at; clamped to the range.
    pub default: f32,
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
    /// [`Error::InvalidParameterRange`] if the range is not finite or `min > max`.
    pub fn new(info: ParameterInfo) -> Result<Self, Error> {
        if !info.min.is_finite() || !info.max.is_finite() || info.min > info.max {
            return Err(Error::InvalidParameterRange);
        }
        let default = if info.default.is_nan() {
            info.min
        } else {
            info.default.clamp(info.min, info.max)
        };
        let info = ParameterInfo { default, ..info };
        Ok(Parameter {
            shared: Arc::new(Shared {
                info,
                bits: AtomicU32::new(default.to_bits()),
            }),
        })
    }

    /// The parameter's name and range.
    pub fn info(&self) -> &ParameterInfo {
        &self.shared.info
    }

    /// The current value.
    pub fn get(&self) -> f32 {
        f32::from_bits(self.shared.bits.load(Ordering::Relaxed))
    }

    /// Sets the value, clamped to the range. NaN is ignored.
    pub fn set(&self, value: f32) {
        if value.is_nan() {
            return;
        }
        let info = &self.shared.info;
        let value = value.clamp(info.min, info.max);
        self.shared.bits.store(value.to_bits(), Ordering::Relaxed);
    }
}

/// A parameter as a node reads it: [`Smoothed::update`] reads the value once per call to
/// `process`, and [`Smoothed::next_value`] steps linearly towards it, reaching it [`RAMP_SECONDS`]
/// after it changed.
#[derive(Debug, Clone)]
pub struct Smoothed {
    parameter: Parameter,
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

impl Smoothed {
    /// A smoothed reader of `parameter` at `sample_rate`, starting at its current value.
    pub fn new(parameter: Parameter, sample_rate: u32) -> Self {
        let value = parameter.get();
        let ramp = (f64::from(sample_rate) * RAMP_SECONDS).ceil().max(1.0);
        // Within `usize` for any `u32` rate.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let ramp = ramp as usize;
        Smoothed {
            parameter,
            current: value,
            target: value,
            step: 0.0,
            remaining: 0,
            ramp,
        }
    }

    /// Reads the parameter and, if it has changed, starts a new ramp from the current value.
    pub fn update(&mut self) {
        let target = self.parameter.get();
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

    /// Whether the value is still moving towards the parameter.
    pub fn is_ramping(&self) -> bool {
        self.remaining > 0
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
        Parameter::new(ParameterInfo {
            name: "gain".into(),
            min: 0.0,
            max: 2.0,
            default: 1.0,
        })
    }

    #[test]
    fn test_new_refuses_bad_ranges() {
        let info = |min, max| ParameterInfo {
            name: String::new(),
            min,
            max,
            default: 0.0,
        };
        assert!(Parameter::new(info(1.0, 0.0)).is_err());
        assert!(Parameter::new(info(f32::NEG_INFINITY, 0.0)).is_err());
        assert!(Parameter::new(info(0.0, f32::NAN)).is_err());
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
        assert_eq!(smoothed.ramp, 1);
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
}
