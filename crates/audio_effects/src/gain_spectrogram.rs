//! [`SpectrogramCurve`]: the gain a `chorus`'s or `flanger`'s parameters give over time and
//! frequency, for display (`docs/architecture/effect-displays.md` "Modulation display"). It is
//! measured by running seeded pink noise through the effect's own per-frame processing offline,
//! so it matches what is heard; the simulation waits until the curve is read.

use dsp::{Pink, Stft};

use crate::chorus::{self, Voices};
use crate::flanger::{self, Comb};
use crate::modulated_delay::response;

/// How long the measured stretch is, in seconds.
pub const SPAN: f32 = 10.0;

/// The most columns worth measuring: one every half analysis window across [`SPAN`]; more
/// would overlap their windows further and show nothing new.
pub const MAX_COLUMNS: usize = 480;

/// The highest rate the effect is simulated at, in Hz: a project at a higher rate is measured
/// at this one, which covers the display's frequencies.
pub const MAX_RATE: u32 = 48_000;

/// The pink noise's seed: any fixed value, so the same values always give the same picture.
const SEED: u64 = 0x7069_6e6b;

/// How long the effect runs before the measured stretch, in seconds, so its lines have filled
/// and the flanger's feedback has settled.
const SETTLE: f64 = 0.5;

/// The analysis window's target length, in seconds; the window is the power of two nearest it.
const WINDOW_SECONDS: f64 = 0.043;

/// The effect a spectrogram curve simulates, at its parameter values in declaration order, with
/// the whole-note length it runs at, in frames.
#[derive(Debug, Clone, PartialEq)]
enum Source {
    /// A `chorus`.
    Chorus([f32; chorus::CONTROLS], f64),
    /// A `flanger`.
    Flanger([f32; flanger::CONTROLS], f64),
}

/// The gain of an effect at given parameter values over [`SPAN`] seconds of pink noise, the same
/// noise into both channels: per channel, the output's level against the noise's, by time and
/// frequency.
///
/// Building one only copies the values; [`SpectrogramCurve::gains`] runs the simulation.
#[derive(Debug, Clone, PartialEq)]
pub struct SpectrogramCurve {
    source: Source,
    sample_rate: u32,
}

impl SpectrogramCurve {
    /// The curve of a chorus at `values` at `sample_rate`, for a whole note of
    /// `frames_per_whole_note` frames.
    pub(crate) fn chorus(
        values: [f32; chorus::CONTROLS],
        sample_rate: u32,
        frames_per_whole_note: f64,
    ) -> SpectrogramCurve {
        SpectrogramCurve {
            source: Source::Chorus(values, frames_per_whole_note),
            sample_rate,
        }
    }

    /// The curve of a flanger at `values` at `sample_rate`, for a whole note of
    /// `frames_per_whole_note` frames.
    pub(crate) fn flanger(
        values: [f32; flanger::CONTROLS],
        sample_rate: u32,
        frames_per_whole_note: f64,
    ) -> SpectrogramCurve {
        SpectrogramCurve {
            source: Source::Flanger(values, frames_per_whole_note),
            sample_rate,
        }
    }

    /// The gain in dB of each of `rows` log-spaced bands from `low_hz` to `high_hz`, row 0 the
    /// lowest, at the middle of each of `columns` equal slices of [`SPAN`]: left channel then
    /// right, each `rows × columns` values, row by row. A band's gain is the output's power over
    /// the noise's in the bins it covers, or at the nearest bins for a band narrower than one;
    /// NaN where the noise has no power there (from the Nyquist frequency up). Empty for no rows
    /// or columns. Runs the effect over the whole span, a frame at a time, at the project's rate or
    /// [`MAX_RATE`], whichever is lower; it allocates the effect's lines and one analysis window
    /// per signal.
    pub fn gains(&self, columns: usize, rows: usize, low_hz: f32, high_hz: f32) -> [Vec<f32>; 2] {
        let Some(cells) = columns.checked_mul(rows).filter(|&n| n > 0) else {
            return [Vec::new(), Vec::new()];
        };
        let rate = self.sample_rate.min(MAX_RATE);
        let Ok(stft) = Stft::new(window_size(rate)) else {
            return [vec![f32::NAN; cells], vec![f32::NAN; cells]];
        };
        let bands = Bands::new(rows, low_hz, high_hz, rate, stft.size());
        let measure = Measure {
            stft,
            bands,
            columns,
            sample_rate: rate,
        };
        // The whole note in frames at the simulated rate.
        let at_rate = |whole: f64| whole * f64::from(rate) / f64::from(self.sample_rate.max(1));
        match &self.source {
            Source::Chorus(values, whole) => {
                measure.run(response::<Voices>(values, rate, at_rate(*whole)))
            }
            Source::Flanger(values, whole) => {
                measure.run(response::<Comb>(values, rate, at_rate(*whole)))
            }
        }
    }
}

/// The analysis window for `sample_rate`: the power of two nearest [`WINDOW_SECONDS`], within
/// the transform's sizes.
fn window_size(sample_rate: u32) -> usize {
    let target = (WINDOW_SECONDS * f64::from(sample_rate)).max(1.0);
    // A power of two within the FFT's range: the exponent is small and non-negative.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let exponent = target.log2().round().max(0.0) as u32;
    1_usize
        .checked_shl(exponent)
        .unwrap_or(dsp::MAX_FFT_SIZE)
        .clamp(dsp::MIN_FFT_SIZE, dsp::MAX_FFT_SIZE)
}

/// Where a band reads its power from a spectrum.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Band {
    /// The sum over bins `start..end`.
    Bins(usize, usize),
    /// Between bin `.0` and the next, `.1` of the way.
    Between(usize, f32),
    /// Nothing: the band lies at or above the Nyquist frequency.
    Beyond,
}

/// The bands of a spectrogram, lowest first.
struct Bands(Vec<Band>);

impl Bands {
    /// `rows` log-spaced bands from `low_hz` to `high_hz`, read from spectra of a `size`-point
    /// transform at `sample_rate`.
    fn new(rows: usize, low_hz: f32, high_hz: f32, sample_rate: u32, size: usize) -> Bands {
        // Transform sizes and display rows are small.
        #[allow(clippy::cast_precision_loss)]
        let (size_f, rows_f) = (size as f64, rows as f64);
        let rate = f64::from(sample_rate);
        let bin_hz = rate / size_f;
        let last = size / 2;
        let (low, high) = (f64::from(low_hz), f64::from(high_hz));
        // The frequency `r` rows up from `low`, as a fractional bin.
        #[allow(clippy::cast_precision_loss)]
        let bin_at = |r: f64| low * (high / low).powf(r / rows_f) / bin_hz;
        let bands = (0..rows)
            .map(|row| {
                #[allow(clippy::cast_precision_loss)]
                let r = row as f64;
                let centre = bin_at(r + 0.5);
                // NaN (from bad bounds) counts as beyond, too.
                #[allow(clippy::cast_precision_loss)]
                if centre.is_nan() || centre >= last as f64 {
                    return Band::Beyond;
                }
                // Bins whose centres lie in the band: `ceil` of each edge.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let edge = |b: f64| (b.ceil().max(0.0) as usize).min(last + 1);
                let (start, end) = (edge(bin_at(r)), edge(bin_at(r + 1.0)));
                if end > start {
                    Band::Bins(start, end)
                } else {
                    let below = centre.floor().max(0.0);
                    // Below `last`, so in range.
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let bin = below as usize;
                    #[allow(clippy::cast_possible_truncation)]
                    let frac = (centre - below) as f32;
                    Band::Between(bin, frac)
                }
            })
            .collect();
        Bands(bands)
    }

    /// The power in each band of `magnitudes`, into `powers`.
    fn powers(&self, magnitudes: &[f32], powers: &mut [f32]) {
        let power = |k: usize| magnitudes.get(k).map_or(0.0, |m| m * m);
        for (p, band) in powers.iter_mut().zip(&self.0) {
            *p = match *band {
                Band::Bins(start, end) => (start..end).map(power).sum(),
                Band::Between(bin, frac) => power(bin) + (power(bin + 1) - power(bin)) * frac,
                Band::Beyond => 0.0,
            };
        }
    }
}

/// One measurement: the window, the bands and the columns.
struct Measure {
    stft: Stft,
    bands: Bands,
    columns: usize,
    sample_rate: u32,
}

impl Measure {
    /// The gains of `step` (one frame in, one out) fed pink noise, as
    /// [`SpectrogramCurve::gains`] gives them.
    fn run(mut self, mut step: impl FnMut(f32, f32) -> (f32, f32)) -> [Vec<f32>; 2] {
        let (size, rows, columns) = (self.stft.size(), self.bands.0.len(), self.columns);
        let rate = f64::from(self.sample_rate);
        // Half a second at an audio rate.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let settle = (SETTLE * rate).round() as usize;
        let span = f64::from(SPAN) * rate;
        // The first frame of column `c`'s window, which is centred on its slice's middle.
        let start = |c: usize| {
            #[allow(clippy::cast_precision_loss)]
            let t = (c as f64 + 0.5) / columns as f64;
            // At most the span, a few million frames.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let offset = (t * span).round() as usize;
            settle + offset
        };
        let mut gains = [
            vec![f32::NAN; rows * columns],
            vec![f32::NAN; rows * columns],
        ];
        // The last `size` frames of the noise and of each output channel, oldest at `written %
        // size`.
        let mut rings = [vec![0.0_f32; size], vec![0.0; size], vec![0.0; size]];
        let mut frame = vec![0.0_f32; size];
        let mut magnitudes = vec![0.0_f32; self.stft.bins()];
        let mut dry = vec![0.0_f32; rows];
        let mut wet = vec![0.0_f32; rows];
        let mut noise = Pink::new(SEED, self.sample_rate);
        let mut column = 0;
        let mut written = 0;
        while column < columns {
            let x = noise.sample();
            let (l, r) = step(x, x);
            let at = written % size;
            for (ring, v) in rings.iter_mut().zip([x, l, r]) {
                ring[at] = v;
            }
            written += 1;
            while column < columns && start(column) + size <= written {
                let mut spectrum = |ring: &[f32], powers: &mut [f32]| {
                    let oldest = written % size;
                    let (newer, older) = ring.split_at(oldest);
                    frame[..older.len()].copy_from_slice(older);
                    frame[older.len()..].copy_from_slice(newer);
                    // The buffers are the window's sizes, so this cannot fail.
                    if self.stft.magnitudes(&frame, &mut magnitudes).is_ok() {
                        self.bands.powers(&magnitudes, powers);
                    } else {
                        powers.fill(0.0);
                    }
                };
                let [ring_dry, ring_left, ring_right] = &rings;
                spectrum(ring_dry, &mut dry);
                for (ring, out) in [ring_left, ring_right].into_iter().zip(&mut gains) {
                    spectrum(ring, &mut wet);
                    for (row, (&d, &w)) in dry.iter().zip(&wet).enumerate() {
                        out[row * columns + column] = if d > 0.0 {
                            10.0 * (w / d).log10()
                        } else {
                            f32::NAN
                        };
                    }
                }
                column += 1;
            }
        }
        gains
    }
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

    /// The spectrogram curve of `kind` at its defaults with `changes` by name, at `rate`.
    fn spectrogram(kind: &str, changes: &[(&str, f32)], rate: u32) -> Option<SpectrogramCurve> {
        let values = values_with(kind, changes)?;
        match crate::curve(kind, &values, rate, WHOLE)? {
            Curve::Spectrogram(curve) => Some(curve),
            _ => None,
        }
    }

    /// The row whose band holds `hz`, of `rows` from 20 Hz to 20 kHz.
    fn row_of(hz: f32, rows: usize) -> usize {
        ((hz / 20.0).log10() / 3.0 * rows as f32) as usize
    }

    #[test]
    fn test_gains_above_max_rate_match_max_rate() -> TestResult {
        // A synced LFO, so the whole note must be rescaled to the simulated rate.
        let changes = [("sync", 1.0), ("note", 6.0)];
        let low = spectrogram("flanger", &changes, MAX_RATE).ok_or("no curve")?;
        let values = values_with("flanger", &changes).ok_or("no values")?;
        let Some(Curve::Spectrogram(high)) = crate::curve("flanger", &values, 192_000, 4.0 * WHOLE)
        else {
            return Err("no curve".into());
        };
        assert!(low.gains(8, 40, 20.0, 20_000.0) == high.gains(8, 40, 20.0, 20_000.0));
        Ok(())
    }

    #[test]
    fn test_window_size_is_the_nearest_power_of_two() {
        assert_eq!(window_size(44_100), 2048);
        assert_eq!(window_size(48_000), 2048);
        assert_eq!(window_size(96_000), 4096);
        assert_eq!(window_size(192_000), 8192);
        assert_eq!(window_size(0), dsp::MIN_FFT_SIZE);
    }

    #[test]
    fn test_gains_mix_zero_is_unity() -> TestResult {
        for kind in ["chorus", "flanger"] {
            let curve = spectrogram(kind, &[("mix", 0.0)], RATE).ok_or("no curve")?;
            let [left, right] = curve.gains(8, 60, 20.0, 20_000.0);
            for gains in [&left, &right] {
                assert_eq!(gains.len(), 480);
                assert!(gains.iter().all(|g| g.abs() < 1e-3), "{kind}: {gains:?}");
            }
        }
        Ok(())
    }

    #[test]
    fn test_gains_static_flanger_notches_the_odd_harmonics_of_half_the_delay() -> TestResult {
        // y = (x + x delayed 1 ms) / 2: nulls at 500 Hz, 1.5 kHz, 2.5 kHz; unity at 1 and 2 kHz.
        let changes = [
            ("delay", 1.0),
            ("depth", 0.0),
            ("feedback", 0.0),
            ("mix", 50.0),
            ("spread", 0.0),
        ];
        let curve = spectrogram("flanger", &changes, RATE).ok_or("no curve")?;
        let (columns, rows) = (4, 300);
        let [left, right] = curve.gains(columns, rows, 20.0, 20_000.0);
        assert_eq!(left, right);
        let at = |hz: f32| left[row_of(hz, rows) * columns + 1];
        for hz in [500.0, 1_500.0, 2_500.0] {
            assert!(at(hz) < -15.0, "{hz}: {}", at(hz));
        }
        for hz in [1_000.0, 2_000.0] {
            assert!(at(hz).abs() < 1.0, "{hz}: {}", at(hz));
        }
        Ok(())
    }

    #[test]
    fn test_gains_spread_separates_the_channels() -> TestResult {
        for kind in ["chorus", "flanger"] {
            // Not 180°: the chorus's two default voices sit half a cycle apart, so half a cycle
            // of spread swaps them and the sum is the same.
            let apart = spectrogram(kind, &[("spread", 90.0)], RATE).ok_or("no curve")?;
            let [left, right] = apart.gains(16, 40, 20.0, 20_000.0);
            let differ = left
                .iter()
                .zip(&right)
                .filter(|(l, r)| (*l - *r).abs() > 1.0)
                .count();
            assert!(
                differ > left.len() / 4,
                "{kind}: {differ} of {}",
                left.len()
            );
            let together = spectrogram(kind, &[("spread", 0.0)], RATE).ok_or("no curve")?;
            let [left, right] = together.gains(16, 40, 20.0, 20_000.0);
            assert!(left == right, "{kind}");
        }
        Ok(())
    }

    #[test]
    fn test_gains_above_nyquist_are_nan() -> TestResult {
        let curve = spectrogram("chorus", &[], 22_050).ok_or("no curve")?;
        let (columns, rows) = (2, 30);
        let [left, _] = curve.gains(columns, rows, 20.0, 20_000.0);
        // 11.025 kHz lies in row 26 of 30.
        let nyquist = row_of(11_025.0, rows);
        assert!(left[(nyquist + 1) * columns..].iter().all(|g| g.is_nan()));
        assert!(left[..nyquist * columns].iter().all(|g| g.is_finite()));
        Ok(())
    }

    #[test]
    fn test_gains_with_no_cells_is_empty() -> TestResult {
        let curve = spectrogram("flanger", &[], RATE).ok_or("no curve")?;
        assert_eq!(curve.gains(0, 10, 20.0, 20_000.0), [Vec::new(), Vec::new()]);
        assert_eq!(curve.gains(10, 0, 20.0, 20_000.0), [Vec::new(), Vec::new()]);
        Ok(())
    }

    #[test]
    fn test_curve_refuses_values_of_the_wrong_length() {
        for (build, len) in [
            (chorus::curve as crate::CurveFn, chorus::CONTROLS),
            (flanger::curve, flanger::CONTROLS),
        ] {
            assert!(build(&vec![1.0; len - 1], RATE, WHOLE).is_none());
            assert!(build(&vec![1.0; len + 1], RATE, WHOLE).is_none());
            assert!(build(&vec![1.0; len], RATE, WHOLE).is_some());
        }
    }

    #[test]
    #[ignore = "timing: run in release with --ignored --nocapture"]
    // It prints its timings; it runs off the real-time thread.
    #[allow(clippy::disallowed_macros)]
    fn test_gains_worst_case_time() -> TestResult {
        // A display at scale 2: the most columns, and two lanes of 270 rows.
        for rate in [48_000, 96_000, 192_000] {
            for kind in ["chorus", "flanger"] {
                let curve = spectrogram(kind, &[("voices", 4.0)], rate).ok_or("no curve")?;
                let start = std::time::Instant::now();
                let gains = curve.gains(MAX_COLUMNS, 270, 20.0, 20_000.0);
                println!("{rate} Hz, {kind}: {:?}", start.elapsed());
                assert_eq!(gains[0].len(), MAX_COLUMNS * 270);
            }
        }
        Ok(())
    }
}
