//! [`ImpulseCurve`]: the impulse response a `delay`'s or `reverb`'s parameters ask for, for
//! display (`docs/architecture/effect-displays.md` "Impulse response display"). It is simulated
//! by running the effect's own per-frame processing offline, so it matches what is heard; the
//! simulation waits until the curve is read.

use crate::{delay, reverb};

/// How far below full scale an impulse response's span runs, in dB.
pub(crate) const SPAN_DB: f32 = 96.0;

/// The shortest span, in seconds.
const MIN_SPAN: f32 = 0.05;

/// The longest span, in seconds.
const MAX_SPAN: f32 = 20.0;

/// The effect an impulse curve simulates, at its parameter values in declaration order.
#[derive(Debug, Clone, PartialEq)]
enum Source {
    /// A `delay`, with the whole-note length it runs at, in frames.
    Delay([f32; delay::CONTROLS], f64),
    /// A `reverb`.
    Reverb([f32; reverb::CONTROLS]),
}

/// The output of an effect at given parameter values after a unit impulse into both channels at
/// frame 0, from silence: the dry impulse at `1 − mix` and the wet response scaled by `mix`.
///
/// Building one only copies the values; [`ImpulseCurve::peak_levels`] runs the simulation.
#[derive(Debug, Clone, PartialEq)]
pub struct ImpulseCurve {
    source: Source,
    sample_rate: u32,
}

impl ImpulseCurve {
    /// The curve of a delay at `values` at `sample_rate`, for a whole note of
    /// `frames_per_whole_note` frames.
    pub(crate) fn delay(
        values: [f32; delay::CONTROLS],
        sample_rate: u32,
        frames_per_whole_note: f64,
    ) -> ImpulseCurve {
        ImpulseCurve {
            source: Source::Delay(values, frames_per_whole_note),
            sample_rate,
        }
    }

    /// The curve of a reverb at `values` at `sample_rate`.
    pub(crate) fn reverb(values: [f32; reverb::CONTROLS], sample_rate: u32) -> ImpulseCurve {
        ImpulseCurve {
            source: Source::Reverb(values),
            sample_rate,
        }
    }

    /// How long the response is shown for, in seconds, from 0.05 to 20: for a reverb, the
    /// pre-delay and the decay scaled from 60 dB to 96 dB; for a delay, the echoes its feedback
    /// takes to fall 96 dB, and one more.
    pub fn span(&self) -> f32 {
        let span = match &self.source {
            Source::Delay(values, whole) => delay::impulse_span(values, self.sample_rate, *whole),
            Source::Reverb(values) => reverb::impulse_span(values),
        };
        if span.is_nan() {
            MIN_SPAN
        } else {
            span.clamp(MIN_SPAN, MAX_SPAN)
        }
    }

    /// The peak absolute output in each of `bins` equal slices of [`ImpulseCurve::span`], left
    /// channel then right; empty for no bins. Runs the effect over the whole span, a frame at a
    /// time, keeping only the peaks; it allocates the effect's lines.
    pub fn peak_levels(&self, bins: usize) -> [Vec<f32>; 2] {
        // A span of at most 20 s at an audio rate: a few million frames.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = (f64::from(self.span()) * f64::from(self.sample_rate))
            .round()
            .max(1.0) as usize;
        match &self.source {
            Source::Delay(values, whole) => peaks(
                delay::impulse_response(values, self.sample_rate, *whole),
                frames,
                bins,
            ),
            Source::Reverb(values) => peaks(
                reverb::impulse_response(values, self.sample_rate),
                frames,
                bins,
            ),
        }
    }
}

/// The peak absolute output of `step` (one frame in, one out) in each of `bins` equal slices of
/// `frames` frames, fed a unit impulse in both channels and then silence.
fn peaks(
    mut step: impl FnMut(f32, f32) -> (f32, f32),
    frames: usize,
    bins: usize,
) -> [Vec<f32>; 2] {
    let mut left = vec![0.0_f32; bins];
    let mut right = vec![0.0_f32; bins];
    if bins == 0 || frames == 0 {
        return [left, right];
    }
    let (frames_wide, bins_wide) = (frames as u128, bins as u128);
    for i in 0..frames {
        let input = if i == 0 { 1.0 } else { 0.0 };
        let (l, r) = step(input, input);
        // Below `bins`, since `i < frames`.
        #[allow(clippy::cast_possible_truncation)]
        let bin = (i as u128 * bins_wide / frames_wide) as usize;
        // `max` keeps the peak through a NaN sample.
        left[bin] = left[bin].max(l.abs());
        right[bin] = right[bin].max(r.abs());
    }
    [left, right]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Curve;
    use crate::curve::values_with;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    const RATE: u32 = 48_000;

    /// 120 BPM in 4/4 at [`RATE`].
    const WHOLE: f64 = 96_000.0;

    /// The impulse curve of `kind` at its defaults with `changes` by name.
    fn impulse(kind: &str, changes: &[(&str, f32)], whole: f64) -> Option<ImpulseCurve> {
        let values = values_with(kind, changes)?;
        match crate::curve(kind, &values, RATE, whole)? {
            Curve::Impulse(curve) => Some(curve),
            _ => None,
        }
    }

    /// `peak` in dB.
    fn db(peak: f32) -> f32 {
        20.0 * peak.log10()
    }

    /// The bin of `seconds` in `bins` bins over `curve`'s span.
    fn bin_at(curve: &ImpulseCurve, bins: usize, seconds: f32) -> usize {
        (seconds / curve.span() * bins as f32) as usize
    }

    /// Indices of the bins holding more than −120 dB.
    fn loud(peaks: &[f32]) -> Vec<usize> {
        (0..peaks.len()).filter(|&i| peaks[i] > 1e-6).collect()
    }

    /// A plain delay: `time` ms, `feedback` %, full wet, as `extra` changes.
    fn delay(time: f32, feedback: f32, extra: &[(&str, f32)]) -> Option<ImpulseCurve> {
        let mut changes = vec![("time", time), ("feedback", feedback), ("mix", 100.0)];
        changes.extend_from_slice(extra);
        impulse("delay", &changes, WHOLE)
    }

    #[test]
    fn test_peak_levels_delay_echoes_fall_by_the_feedback() -> TestResult {
        let curve = delay(100.0, 50.0, &[]).ok_or("no curve")?;
        // 16 echoes fall 96 dB at 50 %; one more.
        assert!((curve.span() - 1.7).abs() < 1e-4, "{}", curve.span());
        // Bins of 10 ms: echo k lands in bin 10k.
        let bins = 170;
        let [left, right] = curve.peak_levels(bins);
        assert_eq!(left, right);
        let echoes: Vec<usize> = (1..=16).map(|k| k * 10).collect();
        assert_eq!(loud(&left), echoes);
        for (k, &bin) in echoes.iter().enumerate().take(6) {
            let expected = -6.02 * k as f32;
            assert!(
                (db(left[bin]) - expected).abs() < 0.1,
                "{k}: {}",
                db(left[bin])
            );
        }
        Ok(())
    }

    #[test]
    fn test_peak_levels_ping_pong_alternates_channels() -> TestResult {
        let curve = delay(100.0, 50.0, &[("mode", 1.0)]).ok_or("no curve")?;
        let bins = (curve.span() * 100.0).round() as usize;
        let [left, right] = curve.peak_levels(bins);
        // Odd echoes on the left, even on the right.
        assert_eq!(&loud(&left)[..3], [10, 30, 50]);
        assert_eq!(&loud(&right)[..3], [20, 40, 60]);
        Ok(())
    }

    #[test]
    fn test_peak_levels_offset_delays_the_right_channel() -> TestResult {
        let curve = delay(100.0, 0.0, &[("offset", 30.0)]).ok_or("no curve")?;
        // One echo of the longer channel, and one more.
        assert!((curve.span() - 0.26).abs() < 1e-4, "{}", curve.span());
        let [left, right] = curve.peak_levels(26);
        assert_eq!(loud(&left), [10]);
        assert_eq!(loud(&right), [13]);
        Ok(())
    }

    #[test]
    fn test_peak_levels_synced_echo_is_at_the_note_length() -> TestResult {
        // A quarter note at 120 BPM in 4/4 is 500 ms; at 60 BPM, 1 s.
        for (whole, at) in [(WHOLE, 0.5), (2.0 * WHOLE, 1.0)] {
            let changes = [
                ("sync", 1.0),
                ("note", 3.0),
                ("feedback", 0.0),
                ("mix", 100.0),
            ];
            let curve = impulse("delay", &changes, whole).ok_or("no curve")?;
            assert!((curve.span() - 2.0 * at).abs() < 1e-4, "{}", curve.span());
            let bins = 100;
            let [left, _] = curve.peak_levels(bins);
            assert_eq!(loud(&left), [bin_at(&curve, bins, at)]);
        }
        Ok(())
    }

    #[test]
    fn test_peak_levels_mix_zero_is_only_the_dry_impulse() -> TestResult {
        for kind in ["delay", "reverb"] {
            let curve = impulse(kind, &[("mix", 0.0)], WHOLE).ok_or("no curve")?;
            let [left, right] = curve.peak_levels(64);
            assert_eq!(loud(&left), [0], "{kind}");
            assert_eq!(loud(&right), [0], "{kind}");
            assert_eq!(left[0], 1.0);
        }
        Ok(())
    }

    #[test]
    fn test_peak_levels_reverb_decays_60_db_in_the_decay_time() -> TestResult {
        for algorithm in [0.0, 1.0] {
            let changes = [
                ("algorithm", algorithm),
                ("pre-delay", 50.0),
                ("decay", 1.0),
                ("damping", 20_000.0),
                ("mix", 100.0),
            ];
            let curve = impulse("reverb", &changes, WHOLE).ok_or("no curve")?;
            assert!((curve.span() - 1.65).abs() < 1e-4);
            // Bins of 10 ms.
            let bins = 165;
            let [left, right] = curve.peak_levels(bins);
            for peaks in [&left, &right] {
                // Silent from the impulse to the pre-delay.
                assert!(peaks[1..5].iter().all(|&p| p == 0.0), "{algorithm}");
                let early = peaks[5..20].iter().copied().fold(0.0, f32::max);
                // The loudest of the bins around pre-delay + decay, against the early peak.
                let late = peaks[100..110].iter().copied().fold(0.0, f32::max);
                let fall = db(early) - db(late);
                assert!((fall - 60.0).abs() < 6.0, "{algorithm}: {fall} dB");
            }
        }
        Ok(())
    }

    #[test]
    fn test_span_is_clamped() -> TestResult {
        let short = delay(1.0, 0.0, &[]).ok_or("no curve")?;
        assert_eq!(short.span(), MIN_SPAN);
        let endless = delay(500.0, 100.0, &[]).ok_or("no curve")?;
        assert_eq!(endless.span(), MAX_SPAN);
        let long = impulse("reverb", &[("decay", 20.0)], WHOLE).ok_or("no curve")?;
        assert_eq!(long.span(), MAX_SPAN);
        let room =
            impulse("reverb", &[("decay", 2.0), ("pre-delay", 10.0)], WHOLE).ok_or("no curve")?;
        assert!((room.span() - 3.21).abs() < 1e-4);
        Ok(())
    }

    #[test]
    fn test_peak_levels_with_no_bins_is_empty() -> TestResult {
        let curve = delay(100.0, 50.0, &[]).ok_or("no curve")?;
        assert_eq!(curve.peak_levels(0), [Vec::<f32>::new(), Vec::new()]);
        Ok(())
    }

    #[test]
    #[ignore = "timing: run in release with --ignored --nocapture"]
    // It prints its timings; it runs off the real-time thread.
    #[allow(clippy::disallowed_macros)]
    fn test_peak_levels_worst_case_time() -> TestResult {
        for rate in [48_000, 96_000, 192_000] {
            for algorithm in [0.0, 1.0] {
                let values = values_with("reverb", &[("algorithm", algorithm), ("decay", 20.0)])
                    .ok_or("no values")?;
                let Some(Curve::Impulse(curve)) = crate::curve("reverb", &values, rate, WHOLE)
                else {
                    return Err("no curve".into());
                };
                let start = std::time::Instant::now();
                let peaks = curve.peak_levels(2_000);
                let elapsed = start.elapsed();
                println!(
                    "{rate} Hz, algorithm {algorithm}: {elapsed:?} for {} s",
                    curve.span()
                );
                assert_eq!(peaks[0].len(), 2_000);
            }
        }
        Ok(())
    }
}
