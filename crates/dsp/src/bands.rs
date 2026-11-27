//! [`Bands`]: log-spaced frequency bands read from an [`crate::Stft`]'s magnitude spectrum, for
//! the spectrogram displays, and [`window_size`], the transform size nearest a window length.

use crate::fft::{MAX_SIZE, MIN_SIZE};

/// The spectrogram displays' analysis window length, in seconds; [`window_size`] gives the
/// transform size nearest it.
pub const SPECTROGRAM_WINDOW_SECONDS: f64 = 0.043;

/// The analysis window for `seconds` at `sample_rate`: the power of two nearest that many
/// frames, within the transform's sizes ([`MIN_SIZE`] to [`MAX_SIZE`]).
pub fn window_size(seconds: f64, sample_rate: u32) -> usize {
    let target = (seconds * f64::from(sample_rate)).max(1.0);
    // A power of two within the FFT's range: the exponent is small and non-negative.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let exponent = target.log2().round().max(0.0) as u32;
    1_usize
        .checked_shl(exponent)
        .unwrap_or(MAX_SIZE)
        .clamp(MIN_SIZE, MAX_SIZE)
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

/// Log-spaced bands over a magnitude spectrum, lowest first. A band's power is the sum of the
/// powers of the bins whose centres it holds, or, for a band narrower than one bin, the power
/// interpolated between the two bins either side of its centre; a band centred at or above the
/// Nyquist frequency is undefined.
#[derive(Debug, Clone, PartialEq)]
pub struct Bands(Vec<Band>);

impl Bands {
    /// `rows` log-spaced bands from `low_hz` to `high_hz`, read from spectra of a `size`-point
    /// transform at `sample_rate`. Allocates.
    pub fn new(rows: usize, low_hz: f32, high_hz: f32, sample_rate: u32, size: usize) -> Bands {
        // Transform sizes and display rows are small.
        #[allow(clippy::cast_precision_loss)]
        let (size_f, rows_f) = (size as f64, rows as f64);
        let rate = f64::from(sample_rate);
        let bin_hz = rate / size_f;
        let last = size / 2;
        let (low, high) = (f64::from(low_hz), f64::from(high_hz));
        // The frequency `r` rows up from `low`, as a fractional bin.
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

    /// The number of bands.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no bands.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Whether band `row` lies at or above the Nyquist frequency, where it has no power; `true`
    /// for a row past the last.
    pub fn is_beyond(&self, row: usize) -> bool {
        self.0.get(row).is_none_or(|b| *b == Band::Beyond)
    }

    /// The power in each band of `magnitudes` (a spectrum of the size the bands were made for),
    /// into `powers`; 0 for a band beyond the Nyquist frequency. The shorter of `powers` and the
    /// bands sets how many are written. Real-time safe.
    pub fn powers(&self, magnitudes: &[f32], powers: &mut [f32]) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_size_is_the_nearest_power_of_two() {
        assert_eq!(window_size(0.043, 44_100), 2048);
        assert_eq!(window_size(0.043, 48_000), 2048);
        assert_eq!(window_size(0.043, 96_000), 4096);
        assert_eq!(window_size(0.043, 192_000), 8192);
        assert_eq!(window_size(0.043, 0), MIN_SIZE);
        assert_eq!(window_size(1e9, 48_000), MAX_SIZE);
    }

    #[test]
    fn test_powers_beyond_nyquist_are_zero() {
        // 20 Hz to 20 kHz at 22.05 kHz: the Nyquist frequency, 11.025 kHz, lies in row 26 of 30.
        let bands = Bands::new(30, 20.0, 20_000.0, 22_050, 1024);
        assert_eq!(bands.len(), 30);
        assert!(!bands.is_beyond(25));
        assert!((27..30).all(|r| bands.is_beyond(r)));
        assert!(bands.is_beyond(30));
        let mut powers = vec![1.0; 30];
        bands.powers(&[1.0; 513], &mut powers);
        assert!(powers[27..].iter().all(|&p| p == 0.0));
        assert!(powers[..26].iter().all(|&p| p > 0.0));
    }

    #[test]
    fn test_powers_sum_wide_bands_and_interpolate_narrow_ones() {
        // Two bands at 48 kHz over a 64-point spectrum (750 Hz bins): 20 to 632 Hz holds no bin
        // centre, so it reads between bins 0 and 1; 632 Hz to 20 kHz sums bins 1 to 26.
        let bands = Bands::new(2, 20.0, 20_000.0, 48_000, 64);
        let magnitudes: Vec<f32> = (0..33).map(|k| if k == 5 { 2.0 } else { 0.0 }).collect();
        let mut powers = [0.0; 2];
        bands.powers(&magnitudes, &mut powers);
        assert_eq!(powers, [0.0, 4.0]);
    }

    #[test]
    fn test_new_with_bad_bounds_is_beyond() {
        let bands = Bands::new(4, f32::NAN, 20_000.0, 48_000, 2048);
        assert!((0..4).all(|r| bands.is_beyond(r)));
        assert!(Bands::new(0, 20.0, 20_000.0, 48_000, 2048).is_empty());
    }
}
