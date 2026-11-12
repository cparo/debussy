//! Dynamics: level ballistics, the static gain curve of a compressor, expander and gate with an
//! optional ceiling, and a fixed delay for lookahead (`docs/architecture/effects.md` "Dynamics").
//!
//! The design is feed-forward: a detected level is smoothed by [`Ballistics`], the curve turns the
//! smoothed level in dB into a gain in dB, and that gain is applied to the input, delayed by the
//! lookahead, with no further smoothing. The soft knees are the quadratic knee of D. Giannoulis,
//! M. Massberg and J. D. Reiss ("Digital Dynamic Range Compressor Design — A Tutorial and
//! Analysis", JAES 60(6), 2012), used on both sides of the threshold (ADR-0099).

/// The lowest level the curve sees, in dB; quieter levels, silence and NaN are raised to it, so
/// the curve's arithmetic stays finite.
pub const FLOOR_DB: f32 = -200.0;

/// The one-pole smoothing coefficient for time constant `seconds` at `sample_rate`:
/// `1 − exp(−1 / (τ·fs))`, the fraction of the remaining distance covered per sample, so a step
/// is 63% complete after `τ`. A time constant of 0 or less, or a rate of 0, gives 1: no
/// smoothing.
pub fn smoothing_coefficient(seconds: f32, sample_rate: f32) -> f32 {
    let samples = seconds * sample_rate;
    if samples > 0.0 && samples.is_finite() {
        1.0 - (-1.0 / samples).exp()
    } else {
        1.0
    }
}

/// The attack, release and hold of a level detector: a one-pole smoother that uses the attack
/// coefficient while its input is above the level and the release coefficient while it is below,
/// with release held off for a number of samples after the last sample that raised the level.
/// Never allocates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ballistics {
    level: f32,
    attack: f32,
    release: f32,
    /// Samples release waits after the level last rose.
    hold: u32,
    /// Samples of hold left.
    held: u32,
}

impl Default for Ballistics {
    fn default() -> Self {
        Ballistics::new()
    }
}

impl Ballistics {
    /// Ballistics at level 0 that follow their input at once, with no hold.
    pub fn new() -> Self {
        Ballistics {
            level: 0.0,
            attack: 1.0,
            release: 1.0,
            hold: 0,
            held: 0,
        }
    }

    /// Sets the attack and release time constants and the hold time, in seconds, at
    /// `sample_rate`. The level is kept; a hold in progress keeps what it has left, up to the
    /// new hold.
    pub fn set_times(&mut self, attack: f32, release: f32, hold: f32, sample_rate: f32) {
        self.attack = smoothing_coefficient(attack, sample_rate);
        self.release = smoothing_coefficient(release, sample_rate);
        let samples = (hold * sample_rate).round();
        // Saturating: a hold beyond `u32::MAX` samples is far longer than any parameter allows,
        // and NaN or a negative hold becomes 0.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let samples = samples as u32;
        self.hold = samples;
        self.held = self.held.min(samples);
    }

    /// Smooths one sample of a detected level, returning the new level.
    pub fn process(&mut self, input: f32) -> f32 {
        if input > self.level {
            self.level += self.attack * (input - self.level);
            self.held = self.hold;
        } else if self.held > 0 {
            self.held -= 1;
        } else {
            self.level += self.release * (input - self.level);
        }
        self.level
    }

    /// The current level.
    pub fn level(&self) -> f32 {
        self.level
    }

    /// Sets the level, as when the detector changes between amplitude and power, and ends any
    /// hold.
    pub fn set_level(&mut self, level: f32) {
        self.level = level;
        self.held = 0;
    }
}

/// The static curve of a compressor, expander or gate, in dB. Below the knee the output follows
/// a line through the threshold of slope `ratio_below` (expansion); above it, one of slope
/// `slope_above` (compression); between, a quadratic joins them so level and slope are
/// continuous.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Curve {
    /// The threshold, in dB.
    pub threshold: f32,
    /// Output dB per input dB above the threshold: `1 / ratio`, 0 for an infinite ratio.
    pub slope_above: f32,
    /// Output dB per input dB below the threshold: the expansion ratio, 1 for none.
    pub ratio_below: f32,
    /// The knee's width in dB, centred on the threshold; 0 for a hard knee.
    pub knee: f32,
    /// The deepest attenuation expansion gives, in dB, as a positive number.
    pub range: f32,
    /// A gain added after the curve, in dB.
    pub makeup: f32,
}

/// A ceiling on the output level: above it the output is held at `level`, with a quadratic knee
/// of width `knee` dB centred on it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ceiling {
    /// The ceiling, in dB.
    pub level: f32,
    /// The knee's width in dB; 0 for a hard knee.
    pub knee: f32,
}

/// The gain in dB that `curve` gives a smoothed level of `level_db`: expansion (clamped to the
/// range), compression and makeup. Finite for any input.
pub fn curve_gain_db(level_db: f32, curve: &Curve) -> f32 {
    let over = level_db.max(FLOOR_DB) - curve.threshold;
    let half = curve.knee / 2.0;
    let in_knee = curve.knee > 0.0 && over.abs() <= half;
    let compression = if in_knee {
        (curve.slope_above - 1.0) * (over + half).powi(2) / (2.0 * curve.knee)
    } else if over > 0.0 {
        (curve.slope_above - 1.0) * over
    } else {
        0.0
    };
    let expansion = if in_knee {
        (1.0 - curve.ratio_below) * (half - over).powi(2) / (2.0 * curve.knee)
    } else if over < 0.0 {
        (curve.ratio_below - 1.0) * over
    } else {
        0.0
    };
    compression + expansion.max(-curve.range.max(0.0)) + curve.makeup
}

/// The gain in dB, 0 or below, that `ceiling` gives an output level of `level_db`.
pub fn ceiling_gain_db(level_db: f32, ceiling: &Ceiling) -> f32 {
    let over = level_db.max(FLOOR_DB) - ceiling.level;
    let half = ceiling.knee / 2.0;
    if ceiling.knee > 0.0 && over.abs() <= half {
        -(over + half).powi(2) / (2.0 * ceiling.knee)
    } else if over > 0.0 {
        -over
    } else {
        0.0
    }
}

/// The output level in dB for a steady input level of `level_db`: [`curve_gain_db`], then
/// `ceiling` on the result if there is one. Shared by processing and any display of the curve.
pub fn transfer_db(level_db: f32, curve: &Curve, ceiling: Option<&Ceiling>) -> f32 {
    let level = level_db.max(FLOOR_DB);
    let output = level + curve_gain_db(level, curve);
    output + ceiling.map_or(0.0, |c| ceiling_gain_db(output, c))
}

/// A fixed delay of a whole number of samples. Allocates only in [`DelayLine::new`].
#[derive(Debug, Clone, PartialEq)]
pub struct DelayLine {
    buffer: Vec<f32>,
    position: usize,
}

impl DelayLine {
    /// A delay of `frames` samples, holding silence. Allocates.
    pub fn new(frames: usize) -> Self {
        DelayLine {
            buffer: vec![0.0; frames],
            position: 0,
        }
    }

    /// The delay, in samples.
    pub fn frames(&self) -> usize {
        self.buffer.len()
    }

    /// Takes one sample and returns the one taken `frames` samples ago.
    pub fn process(&mut self, input: f32) -> f32 {
        let Some(slot) = self.buffer.get_mut(self.position) else {
            return input;
        };
        let output = std::mem::replace(slot, input);
        self.position += 1;
        if self.position == self.buffer.len() {
            self.position = 0;
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compressor() -> Curve {
        Curve {
            threshold: -20.0,
            slope_above: 0.25,
            ratio_below: 2.0,
            knee: 6.0,
            range: 96.0,
            makeup: 0.0,
        }
    }

    #[test]
    fn test_curve_gain_db_is_continuous_at_knee_edges() {
        let curve = compressor();
        for edge in [-23.0_f32, -17.0] {
            let below = transfer_db(edge - 1e-3, &curve, None);
            let above = transfer_db(edge + 1e-3, &curve, None);
            assert!((below - above).abs() < 1e-2, "{edge}: {below} {above}");
            // Slope is continuous too: compare slopes either side of the edge.
            let slope = |x: f32| {
                (transfer_db(x + 1e-2, &curve, None) - transfer_db(x - 1e-2, &curve, None)) / 2e-2
            };
            assert!((slope(edge - 0.05) - slope(edge + 0.05)).abs() < 0.05);
        }
        // Outside the knee, the lines.
        assert!((transfer_db(-10.0, &curve, None) - (-17.5)).abs() < 1e-4);
        assert!((transfer_db(-30.0, &curve, None) - (-40.0)).abs() < 1e-4);
    }

    #[test]
    fn test_curve_gain_db_hard_knee_meets_at_threshold() {
        let curve = Curve {
            knee: 0.0,
            ..compressor()
        };
        assert_eq!(transfer_db(-20.0, &curve, None), -20.0);
        assert!((transfer_db(-12.0, &curve, None) - (-18.0)).abs() < 1e-4);
    }

    #[test]
    fn test_curve_gain_db_infinite_ratio_is_flat() {
        let curve = Curve {
            slope_above: 0.0,
            ..compressor()
        };
        for x in [-10.0, 0.0, 12.0] {
            assert!((transfer_db(x, &curve, None) - (-20.0)).abs() < 1e-4);
        }
    }

    #[test]
    fn test_curve_gain_db_range_clamps_expansion() {
        let curve = Curve {
            ratio_below: 100.0,
            range: 40.0,
            ..compressor()
        };
        assert!((curve_gain_db(-60.0, &curve) - (-40.0)).abs() < 1e-4);
        assert!((curve_gain_db(f32::NEG_INFINITY, &curve) - (-40.0)).abs() < 1e-4);
        assert!((curve_gain_db(f32::NAN, &curve) - (-40.0)).abs() < 1e-4);
        let none = Curve {
            range: 0.0,
            ..curve
        };
        assert_eq!(curve_gain_db(-60.0, &none), 0.0);
    }

    #[test]
    fn test_curve_gain_db_adds_makeup() {
        let curve = Curve {
            makeup: 6.0,
            ..compressor()
        };
        assert!((transfer_db(-10.0, &curve, None) - (-11.5)).abs() < 1e-4);
    }

    #[test]
    fn test_ceiling_gain_db_holds_output_at_or_below_ceiling() {
        let curve = Curve {
            slope_above: 1.0,
            ratio_below: 1.0,
            ..compressor()
        };
        let hard = Ceiling {
            level: -1.0,
            knee: 0.0,
        };
        let soft = Ceiling {
            level: -1.0,
            knee: 4.0,
        };
        for x in [-10.0_f32, -3.0, -1.0, 0.0, 6.0] {
            assert!(transfer_db(x, &curve, Some(&hard)) <= -1.0 + 1e-5);
            assert!(transfer_db(x, &curve, Some(&soft)) <= -1.0 + 1e-5);
        }
        assert!((transfer_db(-10.0, &curve, Some(&soft)) - (-10.0)).abs() < 1e-5);
        for edge in [-3.0_f32, 1.0] {
            let below = transfer_db(edge - 1e-3, &curve, Some(&soft));
            let above = transfer_db(edge + 1e-3, &curve, Some(&soft));
            assert!((below - above).abs() < 1e-2);
        }
    }

    #[test]
    fn test_ballistics_reach_63_percent_at_time_constants() {
        let rate = 48_000.0;
        let mut ballistics = Ballistics::new();
        ballistics.set_times(0.01, 0.1, 0.0, rate);
        for _ in 0..480 {
            ballistics.process(1.0);
        }
        assert!((ballistics.level() - 0.632).abs() < 0.01);
        ballistics.set_level(1.0);
        for _ in 0..4800 {
            ballistics.process(0.0);
        }
        assert!((ballistics.level() - 0.368).abs() < 0.01);
    }

    #[test]
    fn test_ballistics_hold_delays_release() {
        let mut ballistics = Ballistics::new();
        ballistics.set_times(0.0, 0.0, 0.01, 1_000.0);
        assert_eq!(ballistics.process(1.0), 1.0);
        for _ in 0..10 {
            assert_eq!(ballistics.process(0.0), 1.0);
        }
        assert_eq!(ballistics.process(0.0), 0.0);
    }

    #[test]
    fn test_smoothing_coefficient_without_time_is_instant() {
        assert_eq!(smoothing_coefficient(0.0, 48_000.0), 1.0);
        assert_eq!(smoothing_coefficient(0.01, 0.0), 1.0);
        assert_eq!(smoothing_coefficient(f32::NAN, 48_000.0), 1.0);
    }

    #[test]
    fn test_delay_line_delays_by_its_frames() {
        let mut line = DelayLine::new(3);
        assert_eq!(line.frames(), 3);
        let out: Vec<f32> = (1..=6).map(|i| line.process(i as f32)).collect();
        assert_eq!(out, vec![0.0, 0.0, 0.0, 1.0, 2.0, 3.0]);
        let mut none = DelayLine::new(0);
        assert_eq!(none.process(5.0), 5.0);
    }
}
