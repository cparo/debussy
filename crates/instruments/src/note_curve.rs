//! [`NoteCurve`]: one note an instrument's parameters play, for its display
//! (`docs/architecture/effect-displays.md` "Synth note display", ADR-0172). An A4 at full velocity
//! is held and then released; the instrument's own voice code renders it offline, so the picture
//! matches what is heard. Building a curve only copies the values: [`NoteCurve::render`] runs the
//! simulation.

use dsp::{Bands, SPECTROGRAM_WINDOW_SECONDS, Stft, window_size};

/// The note a curve plays: A4.
pub const NOTE: u8 = 69;

/// The highest rate a note is simulated at, in Hz: a project at a higher rate is simulated at
/// this one, which covers the display's frequencies.
pub const NOTE_MAX_RATE: u32 = 48_000;

/// The longest span a curve shows, in seconds.
pub const NOTE_MAX_SPAN: f32 = 10.0;

/// How long the note holds after its envelopes reach their sustain, in seconds; also the
/// shortest span.
const HOLD: f32 = 0.25;

/// The latest the note is released, as a share of the span, so its release always shows.
const LATEST_GATE_OFF: f32 = 0.75;

/// The instrument a note curve simulates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Source {
    /// The synthesizer.
    Synth,
}

/// The span and release time of a note whose envelopes reach their sustain after `settle`
/// seconds and whose amplitude envelope releases over `release` seconds: the note holds for
/// [`HOLD`] past `settle`, the span covers its release, up to [`NOTE_MAX_SPAN`], and the release comes no later than [`LATEST_GATE_OFF`] of the span.
pub(crate) fn fit(settle: f32, release: f32) -> (f32, f32) {
    let held = settle.max(0.0) + HOLD;
    // `max` turns NaN into 0.
    let span = (held + release.max(0.0)).min(NOTE_MAX_SPAN);
    (span, held.min(LATEST_GATE_OFF * span))
}

/// One note of an instrument at given parameter values: an A4 at velocity 127, held, then
/// released, over a span fitted to its envelopes.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteCurve {
    source: Source,
    /// The parameter values, in declaration order, with those that do not change the note at
    /// their defaults.
    values: Vec<f32>,
    /// The simulated rate, in Hz.
    sample_rate: u32,
    span: f32,
    gate_off: f32,
}

impl NoteCurve {
    /// The curve of `source` at `values`, at `sample_rate` capped at [`NOTE_MAX_RATE`], over the
    /// span [`fit`] gives for `settle` and `release`.
    pub(crate) fn new(
        source: Source,
        values: Vec<f32>,
        sample_rate: u32,
        settle: f32,
        release: f32,
    ) -> NoteCurve {
        let (span, gate_off) = fit(settle, release);
        NoteCurve {
            source,
            values,
            sample_rate: sample_rate.clamp(1, NOTE_MAX_RATE),
            span,
            gate_off,
        }
    }

    /// The time the curve shows, in seconds, from the note-on.
    pub fn span(&self) -> f32 {
        self.span
    }

    /// When the note is released, in seconds from the note-on.
    pub fn gate_off(&self) -> f32 {
        self.gate_off
    }

    /// The rate the note is simulated at, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Renders the note over the span, with the instrument's own voice code. Allocates; it runs
    /// the whole span, so a caller keeps the result while the curve is unchanged.
    pub fn render(&self) -> Note {
        let rate = f64::from(self.sample_rate);
        // At most ten seconds at 48 kHz.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let frames = |seconds: f32| (f64::from(seconds) * rate).round().max(0.0) as usize;
        let samples = match self.source {
            Source::Synth => crate::synth::render_note(
                &self.values,
                self.sample_rate,
                frames(self.gate_off),
                frames(self.span).max(1),
            ),
        };
        Note {
            samples,
            sample_rate: self.sample_rate,
        }
    }
}

/// A rendered note: its samples, mono, from the note-on.
#[derive(Debug, Clone, PartialEq)]
pub struct Note {
    samples: Vec<f32>,
    sample_rate: u32,
}

impl Note {
    /// The samples, from the note-on to the end of the span.
    pub fn samples(&self) -> &[f32] {
        &self.samples
    }

    /// The rate of the samples, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The level in dBFS of each of `rows` log-spaced bands from `low_hz` to `high_hz`, row 0
    /// the lowest, at the middle of each of `columns` equal slices of the note, row by row
    /// (`rows × columns` values). Each column's Hann window is centred on its slice, with
    /// silence before the note-on and after the span; a band's power is the sum over the bins
    /// it covers, scaled so a sine at a bin centre reads its amplitude. NaN from the Nyquist
    /// frequency up; −∞ where the note is silent. Empty for no rows or columns. Allocates.
    pub fn levels(&self, columns: usize, rows: usize, low_hz: f32, high_hz: f32) -> Vec<f32> {
        let Some(cells) = columns.checked_mul(rows).filter(|&n| n > 0) else {
            return Vec::new();
        };
        let Ok(mut stft) = Stft::new(window_size(SPECTROGRAM_WINDOW_SECONDS, self.sample_rate))
        else {
            return vec![f32::NAN; cells];
        };
        let size = stft.size();
        let bands = Bands::new(rows, low_hz, high_hz, self.sample_rate, size);
        let mut levels = vec![f32::NAN; cells];
        let mut frame = vec![0.0_f32; size];
        let mut magnitudes = vec![0.0_f32; stft.bins()];
        let mut powers = vec![0.0_f32; rows];
        // At most a few million samples.
        #[allow(clippy::cast_precision_loss)]
        let length = self.samples.len() as f64;
        for column in 0..columns {
            // The window's first sample, which may lie before the note-on.
            #[allow(clippy::cast_precision_loss)]
            let centre = (column as f64 + 0.5) / columns as f64 * length;
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let first = centre.round() as i64 - (size / 2) as i64;
            for (k, x) in frame.iter_mut().enumerate() {
                // `k` is below the window size.
                #[allow(clippy::cast_possible_wrap)]
                let at = first + k as i64;
                *x = usize::try_from(at)
                    .ok()
                    .and_then(|i| self.samples.get(i))
                    .copied()
                    .unwrap_or(0.0);
            }
            // The buffers are the window's sizes, so this cannot fail.
            if stft.magnitudes(&frame, &mut magnitudes).is_err() {
                continue;
            }
            bands.powers(&magnitudes, &mut powers);
            for (row, &p) in powers.iter().enumerate() {
                if !bands.is_beyond(row) {
                    levels[row * columns + column] = 10.0 * p.log10();
                }
            }
        }
        levels
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fit_holds_past_the_envelopes_then_releases() {
        // Settled after 0.5 s, held 0.25 s more, then a 1 s release.
        let (span, gate_off) = fit(0.5, 1.0);
        assert!((span - 1.75).abs() < 1e-6, "{span}");
        assert!((gate_off - 0.75).abs() < 1e-6, "{gate_off}");
    }

    #[test]
    fn test_fit_spans_at_least_the_hold_and_at_most_ten_seconds() {
        // The hold sets the shortest span.
        assert_eq!(fit(0.0, 0.0), (HOLD, LATEST_GATE_OFF * HOLD));
        assert_eq!(fit(f32::NAN, f32::NAN), fit(0.0, 0.0));
        assert_eq!(
            fit(20.0, 10.0),
            (NOTE_MAX_SPAN, LATEST_GATE_OFF * NOTE_MAX_SPAN)
        );
    }

    #[test]
    fn test_fit_releases_by_three_quarters_of_the_span() {
        for (settle, release) in [(0.0, 0.0), (0.002, 0.002), (5.0, 0.01), (20.0, 10.0)] {
            let (span, gate_off) = fit(settle, release);
            assert!(gate_off <= LATEST_GATE_OFF * span, "{settle}, {release}");
            assert!(gate_off > 0.0);
        }
    }
}
