//! Filters: the trapezoidal (TPT) state-variable filter and one-pole filter, with designers for
//! low-pass, high-pass, bell, shelf and notch responses and the pole Qs of a Butterworth cascade
//! (`docs/architecture/effects.md` "Filters").
//!
//! The two-pole filter is the trapezoidal-integrator state-variable filter in A. Simper's form
//! ("Linear Trapezoidal Integrated State Variable Filter", Cytomic, 2013; V. Zavalishin, "The
//! Art of VA Filter Design"). Each response is a mix of its input, band-pass and low-pass
//! outputs, so changing response is changing coefficients, and the filter stays stable when its
//! coefficients change every sample. Frequencies are prewarped for the bilinear transform.

use std::f32::consts::PI;

/// The highest frequency a designer uses, as a fraction of the sample rate; higher ones are
/// lowered to it, keeping the prewarped `tan` finite.
pub const MAX_FREQUENCY_RATIO: f32 = 0.49;

/// The lowest frequency a designer uses, as a fraction of the sample rate, keeping `g` above 0.
const MIN_FREQUENCY_RATIO: f32 = 1e-6;

/// The bilinear transform's prewarped gain for `frequency` at `sample_rate`: `tan(π·f/fs)`, with
/// `f/fs` held between a tiny positive ratio and [`MAX_FREQUENCY_RATIO`].
pub fn prewarp(frequency: f32, sample_rate: f32) -> f32 {
    let ratio = (frequency / sample_rate).clamp(MIN_FREQUENCY_RATIO, MAX_FREQUENCY_RATIO);
    // NaN (from a zero or NaN rate) fails the clamp's bounds silently; fall back to the minimum.
    let ratio = if ratio.is_nan() {
        MIN_FREQUENCY_RATIO
    } else {
        ratio
    };
    (PI * ratio).tan()
}

/// The gain factor a bell or shelf of `gain_db` uses: `A = 10^(dB/40)`, the square root of the
/// linear gain.
fn shelf_amplitude(gain_db: f32) -> f32 {
    10.0_f32.powf(gain_db / 40.0)
}

/// The Q of section `section` (from 0) of an `order`-pole Butterworth cascade:
/// `1 / (2·sin((2·section + 1)·π / (2·order)))`. An order of `n` has `n / 2` two-pole sections
/// (rounded down) and, if `n` is odd, a one-pole section. Returns `None` for a section the order
/// does not have.
pub fn butterworth_q(order: usize, section: usize) -> Option<f32> {
    if section >= order / 2 {
        return None;
    }
    // Orders and sections are small.
    #[allow(clippy::cast_precision_loss)]
    let angle = (2 * section + 1) as f32 * PI / (2 * order) as f32;
    Some(1.0 / (2.0 * angle.sin()))
}

/// The coefficients of a [`Svf`]: its prewarped gain `g`, damping `k = 1/Q`, and the mix of its
/// outputs, `out = m0·input + m1·band + m2·low`. Copy them freely; they never allocate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvfCoefficients {
    g: f32,
    k: f32,
    m0: f32,
    m1: f32,
    m2: f32,
    a1: f32,
    a2: f32,
    a3: f32,
}

impl SvfCoefficients {
    /// Coefficients from the prewarped gain `g` (above 0), damping `k` (above 0) and output mix.
    pub fn new(g: f32, k: f32, m0: f32, m1: f32, m2: f32) -> Self {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        SvfCoefficients {
            g,
            k,
            m0,
            m1,
            m2,
            a1,
            a2,
            a3,
        }
    }

    /// Passes its input through unchanged, at a mid-band `g` and Q of 1.
    pub fn identity() -> Self {
        Self::new(1.0, 1.0, 1.0, 0.0, 0.0)
    }

    /// These coefficients' `g` and `k` with the identity mix: the same filter state, passing the
    /// input through. A morph to or from it changes only the mix.
    #[must_use]
    pub fn bypassed(&self) -> Self {
        Self::new(self.g, self.k, 1.0, 0.0, 0.0)
    }

    /// A two-pole low-pass at `frequency` with resonance `q`, at `sample_rate`.
    pub fn low_pass(frequency: f32, q: f32, sample_rate: f32) -> Self {
        Self::new(prewarp(frequency, sample_rate), 1.0 / q, 0.0, 0.0, 1.0)
    }

    /// A two-pole high-pass at `frequency` with resonance `q`, at `sample_rate`.
    pub fn high_pass(frequency: f32, q: f32, sample_rate: f32) -> Self {
        let k = 1.0 / q;
        Self::new(prewarp(frequency, sample_rate), k, 1.0, -k, -1.0)
    }

    /// A notch at `frequency` of width `q`, at `sample_rate`.
    pub fn notch(frequency: f32, q: f32, sample_rate: f32) -> Self {
        let k = 1.0 / q;
        Self::new(prewarp(frequency, sample_rate), k, 1.0, -k, 0.0)
    }

    /// A bell (peaking) filter of `gain_db` at `frequency` with width `q`, at `sample_rate`.
    pub fn bell(frequency: f32, q: f32, gain_db: f32, sample_rate: f32) -> Self {
        let a = shelf_amplitude(gain_db);
        let k = 1.0 / (q * a);
        Self::new(
            prewarp(frequency, sample_rate),
            k,
            1.0,
            k * (a * a - 1.0),
            0.0,
        )
    }

    /// A low shelf of `gain_db` below `frequency` (its mid-point), with slope `q`, at
    /// `sample_rate`.
    pub fn low_shelf(frequency: f32, q: f32, gain_db: f32, sample_rate: f32) -> Self {
        let a = shelf_amplitude(gain_db);
        let k = 1.0 / q;
        Self::new(
            prewarp(frequency, sample_rate) / a.sqrt(),
            k,
            1.0,
            k * (a - 1.0),
            a * a - 1.0,
        )
    }

    /// A high shelf of `gain_db` above `frequency` (its mid-point), with slope `q`, at
    /// `sample_rate`.
    pub fn high_shelf(frequency: f32, q: f32, gain_db: f32, sample_rate: f32) -> Self {
        let a = shelf_amplitude(gain_db);
        let k = 1.0 / q;
        Self::new(
            prewarp(frequency, sample_rate) * a.sqrt(),
            k,
            a * a,
            k * (1.0 - a) * a,
            1.0 - a * a,
        )
    }

    /// The coefficients `t` of the way from `self` to `to` (0 is `self`, 1 is `to`), each of `g`,
    /// `k` and the mix interpolated linearly. Between two valid sets `g` and `k` stay above 0,
    /// so the filter stays stable through a morph.
    #[must_use]
    pub fn lerp(&self, to: &Self, t: f32) -> Self {
        let mix = |a: f32, b: f32| a + (b - a) * t;
        Self::new(
            mix(self.g, to.g),
            mix(self.k, to.k),
            mix(self.m0, to.m0),
            mix(self.m1, to.m1),
            mix(self.m2, to.m2),
        )
    }
}

/// A trapezoidal state-variable filter: two state values, run one sample at a time with
/// whatever [`SvfCoefficients`] are given for that sample. Never allocates or branches on the
/// response.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Svf {
    ic1eq: f32,
    ic2eq: f32,
}

impl Svf {
    /// A filter at rest.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filters one sample.
    pub fn process(&mut self, input: f32, c: &SvfCoefficients) -> f32 {
        let v3 = input - self.ic2eq;
        let v1 = c.a1 * self.ic1eq + c.a2 * v3;
        let v2 = self.ic2eq + c.a2 * self.ic1eq + c.a3 * v3;
        self.ic1eq = 2.0 * v1 - self.ic1eq;
        self.ic2eq = 2.0 * v2 - self.ic2eq;
        c.m0 * input + c.m1 * v1 + c.m2 * v2
    }

    /// Returns the filter to rest.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

/// The coefficients of a [`OnePole`]: its integrator gain `G = g / (1 + g)` and the mix
/// `out = m0·input + m1·low`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OnePoleCoefficients {
    gain: f32,
    m0: f32,
    m1: f32,
}

impl OnePoleCoefficients {
    /// The integrator gain for prewarped gain `g`.
    fn gain(g: f32) -> f32 {
        g / (1.0 + g)
    }

    /// A one-pole low-pass at `frequency`, at `sample_rate`.
    pub fn low_pass(frequency: f32, sample_rate: f32) -> Self {
        Self::low_pass_prewarped(prewarp(frequency, sample_rate))
    }

    /// A one-pole high-pass at `frequency`, at `sample_rate`.
    pub fn high_pass(frequency: f32, sample_rate: f32) -> Self {
        Self::high_pass_prewarped(prewarp(frequency, sample_rate))
    }

    /// A one-pole low-pass of prewarped gain `g` (see [`prewarp`]).
    pub fn low_pass_prewarped(g: f32) -> Self {
        OnePoleCoefficients {
            gain: Self::gain(g),
            m0: 0.0,
            m1: 1.0,
        }
    }

    /// A one-pole high-pass of prewarped gain `g` (see [`prewarp`]).
    pub fn high_pass_prewarped(g: f32) -> Self {
        OnePoleCoefficients {
            gain: Self::gain(g),
            m0: 1.0,
            m1: -1.0,
        }
    }
}

/// A trapezoidal one-pole filter: one state value. Never allocates.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OnePole {
    state: f32,
}

impl OnePole {
    /// A filter at rest.
    pub fn new() -> Self {
        Self::default()
    }

    /// Filters one sample.
    pub fn process(&mut self, input: f32, c: &OnePoleCoefficients) -> f32 {
        let v = (input - self.state) * c.gain;
        let low = v + self.state;
        self.state = low + v;
        c.m0 * input + c.m1 * low
    }

    /// Returns the filter to rest.
    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_butterworth_q_matches_known_tables() {
        let close = |a: Option<f32>, b: f32| a.is_some_and(|a| (a - b).abs() < 1e-3);
        assert!(close(butterworth_q(2, 0), std::f32::consts::FRAC_1_SQRT_2));
        assert!(close(butterworth_q(3, 0), 1.0));
        assert!(close(butterworth_q(4, 0), 1.3066));
        assert!(close(butterworth_q(4, 1), 0.5412));
        assert!(close(butterworth_q(5, 0), 1.6180));
        assert!(close(butterworth_q(5, 1), 0.6180));
        assert_eq!(butterworth_q(1, 0), None);
        assert_eq!(butterworth_q(4, 2), None);
    }

    #[test]
    fn test_prewarp_clamps_the_ratio() {
        assert!((prewarp(12_000.0, 48_000.0) - 1.0).abs() < 1e-6);
        let top = prewarp(1e9, 48_000.0);
        assert!(top.is_finite() && (top - (PI * MAX_FREQUENCY_RATIO).tan()).abs() < 1e-3);
        assert!(prewarp(0.0, 48_000.0) > 0.0);
        assert!(prewarp(100.0, 0.0) > 0.0);
    }

    #[test]
    fn test_svf_identity_passes_through() {
        let mut svf = Svf::new();
        let c = SvfCoefficients::identity();
        for x in [1.0, -0.5, 0.25, 0.0] {
            assert_eq!(svf.process(x, &c), x);
        }
        svf.reset();
        assert_eq!(svf, Svf::new());
    }

    #[test]
    fn test_lerp_ends_at_both_sets() {
        let a = SvfCoefficients::bell(1_000.0, 1.0, 6.0, 48_000.0);
        let b = a.bypassed();
        assert_eq!(a.lerp(&b, 0.0), a);
        assert_eq!(a.lerp(&b, 1.0), b);
    }

    #[test]
    fn test_one_pole_low_and_high_sum_to_input() {
        let (mut low, mut high) = (OnePole::new(), OnePole::new());
        let lc = OnePoleCoefficients::low_pass(500.0, 48_000.0);
        let hc = OnePoleCoefficients::high_pass(500.0, 48_000.0);
        for i in 0..64 {
            let x = if i % 7 == 0 { 1.0 } else { -0.1 };
            let sum = low.process(x, &lc) + high.process(x, &hc);
            assert!((sum - x).abs() < 1e-6);
        }
        low.reset();
        assert_eq!(low, OnePole::new());
    }
}
