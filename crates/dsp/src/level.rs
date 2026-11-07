//! Gain, panning and the mono fold-down (`docs/architecture/signal-graph.md` "Panning" and
//! "Channel layouts").

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_4};

/// The mono fold-down gain, √½ (−3 dB): the mono mix is `(L + R) × FOLD_DOWN_GAIN` (ADR-0030).
pub const FOLD_DOWN_GAIN: f32 = FRAC_1_SQRT_2;

/// Converts decibels to a linear gain. `f32::NEG_INFINITY` gives 0.
pub fn db_to_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

/// Converts a linear gain to decibels. 0 gives `f32::NEG_INFINITY`; the sign is ignored.
pub fn gain_to_db(gain: f32) -> f32 {
    20.0 * gain.abs().log10()
}

/// Multiplies every sample by `gain`, in place.
pub fn apply_gain(samples: &mut [f32], gain: f32) {
    for sample in samples {
        *sample *= gain;
    }
}

/// The left and right gains for a pan position under the −3 dB constant-power law (ADR-0018).
/// `pan` runs from −1 (hard left) through 0 (centre, 0.7071 each) to 1 (hard right); values
/// outside that are clamped, and NaN is treated as centre. `left² + right²` is always 1.
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let pan = if pan.is_nan() {
        0.0
    } else {
        pan.clamp(-1.0, 1.0)
    };
    let angle = (pan + 1.0) * FRAC_PI_4;
    (angle.cos(), angle.sin())
}

/// Pans `mono` into `left` and `right`, overwriting them.
pub fn pan_mono(mono: &[f32], pan: f32, left: &mut [f32], right: &mut [f32]) {
    let (gain_left, gain_right) = pan_gains(pan);
    for ((&m, l), r) in mono.iter().zip(left.iter_mut()).zip(right.iter_mut()) {
        *l = m * gain_left;
        *r = m * gain_right;
    }
}

/// Folds a stereo signal to mono, `(L + R) × FOLD_DOWN_GAIN`, overwriting `mono` (ADR-0030).
pub fn fold_to_mono(left: &[f32], right: &[f32], mono: &mut [f32]) {
    for ((&l, &r), m) in left.iter().zip(right).zip(mono.iter_mut()) {
        *m = (l + r) * FOLD_DOWN_GAIN;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f32 = 1e-6;

    #[test]
    fn test_db_to_gain_and_back() {
        assert_eq!(db_to_gain(0.0), 1.0);
        assert!((db_to_gain(-6.0206) - 0.5).abs() < 1e-4);
        assert!((db_to_gain(-3.0103) - FOLD_DOWN_GAIN).abs() < 1e-4);
        assert_eq!(db_to_gain(f32::NEG_INFINITY), 0.0);
        assert_eq!(gain_to_db(0.0), f32::NEG_INFINITY);
        for db in [-60.0, -12.5, 0.0, 6.0] {
            assert!((gain_to_db(db_to_gain(db)) - db).abs() < 1e-3, "{db}");
        }
    }

    #[test]
    fn test_pan_gains_constant_power() {
        assert!((pan_gains(0.0).0 - FRAC_1_SQRT_2).abs() < EPS);
        assert!((pan_gains(0.0).1 - FRAC_1_SQRT_2).abs() < EPS);
        let (l, r) = pan_gains(-1.0);
        assert!((l - 1.0).abs() < EPS && r.abs() < EPS);
        let (l, r) = pan_gains(1.0);
        assert!(l.abs() < EPS && (r - 1.0).abs() < EPS);
        for i in -100..=100 {
            let (l, r) = pan_gains(i as f32 / 100.0);
            assert!((l * l + r * r - 1.0).abs() < 1e-5, "{i}");
        }
        assert_eq!(pan_gains(5.0), pan_gains(1.0));
        assert_eq!(pan_gains(f32::NAN), pan_gains(0.0));
    }

    #[test]
    fn test_fold_to_mono_centred_source_returns_to_unity() {
        let source = [0.5_f32, -0.25, 1.0];
        let (mut left, mut right, mut mono) = ([0.0; 3], [0.0; 3], [0.0; 3]);
        pan_mono(&source, 0.0, &mut left, &mut right);
        fold_to_mono(&left, &right, &mut mono);
        for (m, s) in mono.iter().zip(source) {
            assert!((m - s).abs() < EPS, "{m} vs {s}");
        }
    }

    #[test]
    fn test_fold_to_mono_hard_panned_is_minus_3_db() {
        let (mut left, mut right, mut mono) = ([0.0; 1], [0.0; 1], [0.0; 1]);
        pan_mono(&[1.0], -1.0, &mut left, &mut right);
        fold_to_mono(&left, &right, &mut mono);
        assert!((mono[0] - FOLD_DOWN_GAIN).abs() < EPS);
    }

    #[test]
    fn test_shorter_slice_sets_length() {
        let mut mono = [9.0_f32; 4];
        fold_to_mono(&[1.0, 1.0], &[1.0, 1.0, 1.0], &mut mono);
        assert!((mono[1] - 2.0 * FOLD_DOWN_GAIN).abs() < EPS);
        assert_eq!(mono[2..], [9.0, 9.0]);
        let mut samples = [1.0_f32, -2.0];
        apply_gain(&mut samples, 0.5);
        assert_eq!(samples, [0.5, -1.0]);
    }
}
