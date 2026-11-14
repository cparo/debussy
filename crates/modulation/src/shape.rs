//! [`Shape`]: the LFO waveforms (`docs/architecture/modulation.md` "LFO", ADR-0119).

use std::f64::consts::TAU;

/// An LFO waveform. None is band-limited: they are meant for modulation rates, not for
/// audio-rate oscillators, where their corners would alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    /// `sin(2πp)`.
    #[default]
    Sine,
    /// 0 at phase 0, rising to 1 at ¼, falling to −1 at ¾, and back to 0.
    Triangle,
    /// `2p − 1`: a ramp up, then a jump down.
    SawUp,
    /// `1 − 2p`: a ramp down, then a jump up.
    SawDown,
    /// 1 for the first half of each cycle, −1 for the second.
    Square,
    /// One random value per cycle, held for the whole cycle.
    SampleAndHold,
}

impl Shape {
    /// The value at `phase` (in cycles; its whole part is ignored, and NaN reads as 0), from −1
    /// to 1. `held` is the cycle's random value, used only by [`Shape::SampleAndHold`].
    pub fn value(self, phase: f64, held: f32) -> f32 {
        let p = wrap(phase);
        let value = match self {
            Shape::Sine => (TAU * p).sin(),
            Shape::Triangle => {
                if p < 0.25 {
                    4.0 * p
                } else if p < 0.75 {
                    2.0 - 4.0 * p
                } else {
                    4.0 * p - 4.0
                }
            }
            Shape::SawUp => 2.0 * p - 1.0,
            Shape::SawDown => 1.0 - 2.0 * p,
            Shape::Square => {
                if p < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            Shape::SampleAndHold => return held.clamp(-1.0, 1.0),
        };
        // A value from −1 to 1.
        #[allow(clippy::cast_possible_truncation)]
        let value = value as f32;
        value
    }

    /// Whether the shape draws a random value per cycle.
    pub fn is_random(self) -> bool {
        matches!(self, Shape::SampleAndHold)
    }
}

/// `phase` reduced to `0 ≤ p < 1`; NaN and infinities read as 0.
pub(crate) fn wrap(phase: f64) -> f64 {
    if !phase.is_finite() {
        return 0.0;
    }
    let p = phase - phase.floor();
    // `phase - floor` can round up to exactly 1 for a tiny negative phase.
    if p >= 1.0 { 0.0 } else { p }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERIODIC: [Shape; 5] = [
        Shape::Sine,
        Shape::Triangle,
        Shape::SawUp,
        Shape::SawDown,
        Shape::Square,
    ];

    #[test]
    fn test_value_stays_in_range() {
        for shape in PERIODIC {
            for i in 0..=1000 {
                let v = shape.value(f64::from(i) / 1000.0, 0.0);
                assert!((-1.0..=1.0).contains(&v), "{shape:?} {i} {v}");
            }
        }
        assert_eq!(Shape::SampleAndHold.value(0.3, 7.0), 1.0);
        assert_eq!(Shape::SampleAndHold.value(0.3, -0.25), -0.25);
    }

    #[test]
    fn test_value_hits_landmarks() {
        let at = |s: Shape, p| s.value(p, 0.0);
        assert!(at(Shape::Sine, 0.25) > 0.999_999);
        assert!(at(Shape::Sine, 0.0).abs() < 1e-7);
        assert_eq!(at(Shape::Triangle, 0.0), 0.0);
        assert_eq!(at(Shape::Triangle, 0.25), 1.0);
        assert_eq!(at(Shape::Triangle, 0.5), 0.0);
        assert_eq!(at(Shape::Triangle, 0.75), -1.0);
        assert_eq!(at(Shape::SawUp, 0.0), -1.0);
        assert_eq!(at(Shape::SawUp, 0.5), 0.0);
        assert_eq!(at(Shape::SawDown, 0.0), 1.0);
        assert_eq!(at(Shape::Square, 0.49), 1.0);
        assert_eq!(at(Shape::Square, 0.5), -1.0);
    }

    #[test]
    fn test_value_is_periodic() {
        for shape in PERIODIC {
            for p in [0.1, 0.37, 0.8] {
                let base = shape.value(p, 0.0);
                assert!((shape.value(p + 3.0, 0.0) - base).abs() < 1e-6, "{shape:?}");
                assert!((shape.value(p - 2.0, 0.0) - base).abs() < 1e-6, "{shape:?}");
            }
        }
    }

    #[test]
    fn test_value_is_continuous_for_sine_and_triangle() {
        for shape in [Shape::Sine, Shape::Triangle] {
            let step = 1e-4;
            let mut previous = shape.value(0.0, 0.0);
            for i in 1..=20_000 {
                let v = shape.value(f64::from(i) * step, 0.0);
                // The steepest slope is 2π per cycle for sine, 4 for triangle.
                assert!((v - previous).abs() < 7.0 * 1e-4, "{shape:?} {i}");
                previous = v;
            }
        }
    }

    #[test]
    fn test_value_reads_nan_phase_as_zero() {
        for shape in PERIODIC {
            assert_eq!(shape.value(f64::NAN, 0.0), shape.value(0.0, 0.0));
            assert_eq!(shape.value(f64::INFINITY, 0.0), shape.value(0.0, 0.0));
        }
    }

    #[test]
    fn test_wrap_keeps_tiny_negative_phase_below_one() {
        let p = wrap(-1e-20);
        assert!((0.0..1.0).contains(&p));
    }

    #[test]
    fn test_is_random_only_for_sample_and_hold() {
        assert!(Shape::SampleAndHold.is_random());
        assert!(PERIODIC.iter().all(|s| !s.is_random()));
    }
}
