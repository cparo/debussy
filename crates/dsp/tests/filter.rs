//! Filter responses measured from impulse responses: Butterworth cutoff, slope and flatness,
//! bell, shelf and notch gains, and stability under per-sample coefficient changes.

use std::error::Error;
use std::f32::consts::FRAC_1_SQRT_2;
use std::f64::consts::PI;

use dsp::{
    Complex, Noise, OnePole, OnePoleCoefficients, RealFft, Svf, SvfCoefficients, butterworth_q,
    prewarp,
};

const RATE: f32 = 48_000.0;
/// Impulse-response length: long enough for the lowest frequencies tested to settle.
const LENGTH: usize = 1 << 15;

/// The impulse response of a filter run sample by sample.
fn impulse(mut filter: impl FnMut(f32) -> f32) -> Vec<f32> {
    (0..LENGTH)
        .map(|i| filter(if i == 0 { 1.0 } else { 0.0 }))
        .collect()
}

/// The gain in dB of impulse response `h` at `frequency`, by direct DFT at that frequency.
fn gain_db(h: &[f32], frequency: f32) -> f64 {
    let w = 2.0 * PI * f64::from(frequency) / f64::from(RATE);
    let (re, im) = h.iter().enumerate().fold((0.0, 0.0), |(re, im), (n, &x)| {
        let phase = w * n as f64;
        (
            re + f64::from(x) * phase.cos(),
            im - f64::from(x) * phase.sin(),
        )
    });
    10.0 * (re * re + im * im).log10()
}

/// A Butterworth cascade of `order` poles at `cutoff`, high-pass if `high`.
fn butterworth(order: usize, cutoff: f32, high: bool) -> impl FnMut(f32) -> f32 {
    let g = prewarp(cutoff, RATE);
    let sections: Vec<SvfCoefficients> = (0..order / 2)
        .filter_map(|s| butterworth_q(order, s))
        .map(|q| {
            if high {
                SvfCoefficients::high_pass(cutoff, q, RATE)
            } else {
                SvfCoefficients::low_pass(cutoff, q, RATE)
            }
        })
        .collect();
    let one = if high {
        OnePoleCoefficients::high_pass_prewarped(g)
    } else {
        OnePoleCoefficients::low_pass_prewarped(g)
    };
    let mut svfs = vec![Svf::new(); sections.len()];
    let mut pole = OnePole::new();
    move |x| {
        let mut y = x;
        for (svf, c) in svfs.iter_mut().zip(&sections) {
            y = svf.process(y, c);
        }
        if order % 2 == 1 {
            y = pole.process(y, &one);
        }
        y
    }
}

fn single(c: SvfCoefficients) -> impl FnMut(f32) -> f32 {
    let mut svf = Svf::new();
    move |x| svf.process(x, &c)
}

#[test]
fn test_butterworth_is_3db_down_at_cutoff_for_orders_1_to_8() {
    for order in 1..=8 {
        for high in [false, true] {
            let h = impulse(butterworth(order, 1_000.0, high));
            let at = gain_db(&h, 1_000.0);
            assert!(
                (at + 3.0103).abs() < 0.02,
                "order {order} high {high}: {at}"
            );
        }
    }
}

/// The stopband slope, in dB, over the octave `f..2f`, of an `order`-pole Butterworth after the
/// bilinear transform: `6.02·n` dB scaled by the prewarped octave's width.
fn expected_slope(order: usize, f: f32) -> f64 {
    let octave = f64::from(prewarp(2.0 * f, RATE)) / f64::from(prewarp(f, RATE));
    20.0 * order as f64 * octave.log10()
}

#[test]
fn test_butterworth_falls_6db_per_octave_per_order() {
    for order in 1..=8 {
        // The cutoff is set so the octave starts 40 dB down, where the slope has settled but the
        // gain is well above `f32` rounding.
        let depth = 10.0_f32.powf(40.0 / (20.0 * order as f32));
        let low_pass = impulse(butterworth(order, 1_000.0 / depth, false));
        let slope = gain_db(&low_pass, 1_000.0) - gain_db(&low_pass, 2_000.0);
        let expected = expected_slope(order, 1_000.0);
        assert!(
            (slope - expected).abs() < 0.01 * expected,
            "LP {order}: {slope}"
        );
        assert!((expected / (6.0206 * order as f64) - 1.0).abs() < 0.05);
        let high_pass = impulse(butterworth(order, 200.0 * depth, true));
        let slope = gain_db(&high_pass, 200.0) - gain_db(&high_pass, 100.0);
        let expected = expected_slope(order, 100.0);
        assert!(
            (slope - expected).abs() < 0.01 * expected,
            "HP {order}: {slope}"
        );
        assert!((slope / (6.0206 * order as f64) - 1.0).abs() < 0.01);
    }
}

#[test]
fn test_butterworth_passband_is_monotonic() -> Result<(), Box<dyn Error>> {
    let mut fft = RealFft::new(dsp::MAX_FFT_SIZE)?;
    let mut spectrum = vec![Complex::default(); fft.bins()];
    for order in 1..=8 {
        let h = impulse(butterworth(order, 2_000.0, false));
        fft.forward(&h[..dsp::MAX_FFT_SIZE], &mut spectrum)?;
        let magnitudes: Vec<f32> = spectrum.iter().map(|c| c.abs()).collect();
        // From DC to the cutoff and on to near Nyquist, the gain only falls: no ripple.
        for (bin, pair) in magnitudes.windows(2).enumerate() {
            assert!(
                pair[1] <= pair[0] * (1.0 + 1e-4) + 1e-6,
                "order {order} rises at bin {bin}: {pair:?}"
            );
        }
        assert!((magnitudes[0] - 1.0).abs() < 1e-3, "order {order} DC gain");
    }
    Ok(())
}

#[test]
fn test_bell_reaches_its_gain_at_the_centre() {
    for gain in [-18.0, -6.0, 6.0, 18.0] {
        for q in [0.5, 2.0, 8.0] {
            let h = impulse(single(SvfCoefficients::bell(1_000.0, q, gain, RATE)));
            let centre = gain_db(&h, 1_000.0);
            assert!((centre - f64::from(gain)).abs() < 0.01, "{gain} dB Q {q}");
            assert!(gain_db(&h, 20.0).abs() < 0.1);
            assert!(gain_db(&h, 20_000.0).abs() < 0.5);
        }
    }
}

#[test]
fn test_shelves_reach_their_gain_and_leave_the_far_end_flat() {
    for gain in [-12.0, 12.0] {
        let low = impulse(single(SvfCoefficients::low_shelf(
            500.0,
            FRAC_1_SQRT_2,
            gain,
            RATE,
        )));
        assert!((gain_db(&low, 10.0) - f64::from(gain)).abs() < 0.05);
        assert!((gain_db(&low, 500.0) - f64::from(gain) / 2.0).abs() < 0.05);
        assert!(gain_db(&low, 15_000.0).abs() < 0.05);
        let high = impulse(single(SvfCoefficients::high_shelf(
            2_000.0,
            FRAC_1_SQRT_2,
            gain,
            RATE,
        )));
        assert!((gain_db(&high, 23_000.0) - f64::from(gain)).abs() < 0.2);
        assert!((gain_db(&high, 2_000.0) - f64::from(gain) / 2.0).abs() < 0.05);
        assert!(gain_db(&high, 20.0).abs() < 0.05);
    }
}

#[test]
fn test_notch_is_deep_at_its_centre() {
    let h = impulse(single(SvfCoefficients::notch(1_000.0, 2.0, RATE)));
    assert!(gain_db(&h, 1_000.0) < -60.0);
    assert!(gain_db(&h, 100.0).abs() < 0.1);
    assert!(gain_db(&h, 10_000.0).abs() < 0.1);
}

#[test]
fn test_identity_and_zero_gain_are_transparent() {
    let input: Vec<f32> = {
        let mut noise = Noise::new(3);
        (0..4096).map(|_| noise.uniform()).collect()
    };
    for c in [
        SvfCoefficients::identity(),
        SvfCoefficients::bell(700.0, 1.0, 0.0, RATE),
        SvfCoefficients::low_shelf(700.0, 1.0, 0.0, RATE),
        SvfCoefficients::high_shelf(700.0, 1.0, 0.0, RATE),
    ] {
        let mut svf = Svf::new();
        for &x in &input {
            assert!((svf.process(x, &c) - x).abs() < 1e-5);
        }
    }
}

#[test]
fn test_random_per_sample_coefficient_changes_stay_bounded() {
    let mut noise = Noise::new(11);
    let mut svf = Svf::new();
    let mut peak = 0.0_f32;
    for _ in 0..200_000 {
        // Log-uniform frequency, wide Q, any type, changing every sample.
        let frequency = 20.0 * 1_000.0_f32.powf(noise.uniform() + 0.5);
        let q = 0.1 * 180.0_f32.powf(noise.uniform() + 0.5);
        let gain = 48.0 * noise.uniform();
        let c = match ((noise.uniform() + 0.5) * 6.0) as u32 {
            0 => SvfCoefficients::low_pass(frequency, q, RATE),
            1 => SvfCoefficients::high_pass(frequency, q, RATE),
            2 => SvfCoefficients::notch(frequency, q, RATE),
            3 => SvfCoefficients::low_shelf(frequency, q, gain, RATE),
            4 => SvfCoefficients::high_shelf(frequency, q, gain, RATE),
            _ => SvfCoefficients::bell(frequency, q, gain, RATE),
        };
        let y = svf.process(noise.uniform(), &c);
        assert!(y.is_finite());
        peak = peak.max(y.abs());
    }
    // Bounded by the largest gain asked for times the input, with room for resonance.
    assert!(peak < 1e4, "peak {peak}");
}
