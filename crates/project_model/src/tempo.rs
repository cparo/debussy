//! [`Tempo`]: the project's one tempo and meter, and the bars and beats grid it lays over the
//! timeline (`docs/architecture/project.md` "Musical time").
//!
//! Beat `n` falls on frame `round(n × frames_per_beat)`, where `frames_per_beat` is
//! `60 × rate / bpm`. It is computed from `n` each time, never accumulated, so the grid does not
//! drift however long the project. Bar 1 beat 1 is frame 0; the grid extends before frame 0 with
//! negative beats, for the count-in.

use crate::error::Error;

/// The slowest tempo, in beats per minute.
pub const MIN_BPM: f64 = 20.0;
/// The fastest tempo, in beats per minute.
pub const MAX_BPM: f64 = 300.0;
/// The most beats in a bar.
pub const MAX_BEATS_PER_BAR: u8 = 16;
/// The note values a beat may be: half, quarter, eighth and sixteenth notes.
pub const BEAT_UNITS: [u8; 4] = [2, 4, 8, 16];

/// The project's tempo and meter: `beats_per_bar` / `beat_unit` at `bpm` beats of `beat_unit`
/// notes per minute.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tempo {
    /// Beats per minute, where a beat is a `beat_unit` note; 20 to 300, to 0.01.
    pub bpm: f64,
    /// Beats in a bar: the meter's upper number, 1 to 16.
    pub beats_per_bar: u8,
    /// The note value of a beat: the meter's lower number, one of [`BEAT_UNITS`].
    pub beat_unit: u8,
}

impl Default for Tempo {
    /// 120 BPM in 4/4.
    fn default() -> Self {
        Tempo {
            bpm: 120.0,
            beats_per_bar: 4,
            beat_unit: 4,
        }
    }
}

/// A beat on the grid, counted from 0: bar 0 beat 0 is the first beat of the timeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Beat {
    /// The beat's index from frame 0; negative before it.
    pub index: i64,
    /// Its bar, from 0; negative before frame 0.
    pub bar: i64,
    /// Its beat within the bar, from 0; 0 is the downbeat.
    pub beat: u8,
    /// The frame it falls on.
    pub frame: i64,
}

impl Tempo {
    /// Checks the tempo is within the limits.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidTempo`].
    pub fn check(&self) -> Result<(), Error> {
        let bpm_ok = self.bpm.is_finite() && (MIN_BPM..=MAX_BPM).contains(&self.bpm);
        let hundredths = self.bpm * 100.0;
        let precise = (hundredths - hundredths.round()).abs() < 1e-6;
        if bpm_ok
            && precise
            && (1..=MAX_BEATS_PER_BAR).contains(&self.beats_per_bar)
            && BEAT_UNITS.contains(&self.beat_unit)
        {
            Ok(())
        } else {
            Err(Error::InvalidTempo)
        }
    }

    /// Frames per beat at `rate` Hz. Every frame position on the grid is a whole multiple of this,
    /// rounded; the click uses the same value so it falls on the same frames.
    pub fn frames_per_beat(&self, rate: u32) -> f64 {
        60.0 * f64::from(rate) / self.bpm
    }

    /// The length of a whole note at `rate` Hz, in frames: what the graph gives nodes that use
    /// the tempo, and what a synced delay's note values are fractions of.
    pub fn frames_per_whole_note(&self, rate: u32) -> f64 {
        self.frames_per_beat(rate) * f64::from(self.beat_unit)
    }

    /// The frame beat `index` falls on at `rate` Hz.
    pub fn beat_frame(&self, index: i64, rate: u32) -> i64 {
        (index as f64 * self.frames_per_beat(rate)).round() as i64
    }

    /// The frame bar `bar` (from 0) starts on at `rate` Hz.
    pub fn bar_frame(&self, bar: i64, rate: u32) -> i64 {
        self.beat_frame(bar.saturating_mul(i64::from(self.beats_per_bar)), rate)
    }

    /// Beat `index` on the grid at `rate` Hz.
    pub fn beat(&self, index: i64, rate: u32) -> Beat {
        let per_bar = i64::from(self.beats_per_bar.max(1));
        Beat {
            index,
            bar: index.div_euclid(per_bar),
            // Below `beats_per_bar`, so it fits.
            beat: index.rem_euclid(per_bar) as u8,
            frame: self.beat_frame(index, rate),
        }
    }

    /// The last beat at or before `frame` at `rate` Hz.
    pub fn beat_at(&self, frame: i64, rate: u32) -> Beat {
        let mut index = (frame as f64 / self.frames_per_beat(rate)).floor() as i64;
        // The estimate can be one off either way where rounding moved a beat across `frame`.
        while self.beat_frame(index, rate) > frame {
            index -= 1;
        }
        while self.beat_frame(index + 1, rate) <= frame {
            index += 1;
        }
        self.beat(index, rate)
    }

    /// The beats falling on frames `from..to` at `rate` Hz, earliest first; either end may be
    /// negative.
    pub fn beats_in(&self, from: i64, to: i64, rate: u32) -> impl Iterator<Item = Beat> + '_ {
        let first = self.beat_at(from.saturating_sub(1), rate).index + 1;
        (first..)
            .map(move |index| self.beat(index, rate))
            .take_while(move |beat| beat.frame < to)
    }

    /// The length of `bars` bars at `rate` Hz, in frames: from frame 0 to the start of bar
    /// `bars`.
    pub fn bars_length(&self, bars: u32, rate: u32) -> u64 {
        self.bar_frame(i64::from(bars), rate).unsigned_abs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;

    #[test]
    fn test_frames_per_whole_note_scales_by_the_beat_unit() {
        assert_eq!(Tempo::default().frames_per_whole_note(RATE), 96_000.0);
        let six_eight = Tempo {
            bpm: 120.0,
            beats_per_bar: 6,
            beat_unit: 8,
        };
        assert_eq!(six_eight.frames_per_whole_note(RATE), 192_000.0);
    }

    #[test]
    fn test_check_accepts_limits_and_refuses_outside() {
        assert_eq!(Tempo::default().check(), Ok(()));
        for bpm in [20.0, 300.0, 123.45] {
            let tempo = Tempo {
                bpm,
                ..Tempo::default()
            };
            assert_eq!(tempo.check(), Ok(()), "{bpm}");
        }
        for bpm in [19.99, 300.01, 120.001, f64::NAN, f64::INFINITY] {
            let tempo = Tempo {
                bpm,
                ..Tempo::default()
            };
            assert_eq!(tempo.check(), Err(Error::InvalidTempo), "{bpm}");
        }
        for (beats_per_bar, beat_unit) in [(0, 4), (17, 4), (4, 3), (4, 1), (4, 32)] {
            let tempo = Tempo {
                beats_per_bar,
                beat_unit,
                ..Tempo::default()
            };
            assert_eq!(tempo.check(), Err(Error::InvalidTempo));
        }
    }

    #[test]
    fn test_beat_frame_does_not_drift_over_three_hours() {
        // 7/8 at an odd tempo: frames per beat is not a whole number.
        let tempo = Tempo {
            bpm: 133.33,
            beats_per_bar: 7,
            beat_unit: 8,
        };
        let per_beat = tempo.frames_per_beat(RATE);
        let three_hours = 3 * 3600 * i64::from(RATE);
        let last = (three_hours as f64 / per_beat) as i64;
        let exact = last as f64 * 60.0 * f64::from(RATE) / 133.33;
        assert!((tempo.beat_frame(last, RATE) as f64 - exact).abs() <= 0.5 + 1e-6);
        // Consecutive beats are never more than a frame from the ideal spacing.
        for n in [0, 1, 1000, last - 1] {
            let gap = tempo.beat_frame(n + 1, RATE) - tempo.beat_frame(n, RATE);
            assert!((gap as f64 - per_beat).abs() <= 1.0);
        }
    }

    #[test]
    fn test_beat_at_finds_the_beat_on_or_before() {
        let tempo = Tempo::default(); // 24 000 frames per beat at 48 kHz.
        assert_eq!(tempo.beat_at(0, RATE).index, 0);
        assert_eq!(tempo.beat_at(23_999, RATE).index, 0);
        assert_eq!(tempo.beat_at(24_000, RATE).index, 1);
        let before = tempo.beat_at(-1, RATE);
        assert_eq!((before.index, before.bar, before.beat), (-1, -1, 3));
        let odd = Tempo {
            bpm: 97.13,
            beats_per_bar: 3,
            beat_unit: 4,
        };
        for n in [-50, -1, 0, 1, 7, 12_345] {
            let frame = odd.beat_frame(n, 44_100);
            assert_eq!(odd.beat_at(frame, 44_100).index, n);
            assert_eq!(odd.beat_at(frame - 1, 44_100).index, n - 1);
        }
    }

    #[test]
    fn test_beats_in_spans_negative_frames_with_bars_and_downbeats() {
        let tempo = Tempo {
            bpm: 120.0,
            beats_per_bar: 3,
            beat_unit: 4,
        };
        // One bar before frame 0 to just past it.
        let beats: Vec<Beat> = tempo.beats_in(-72_000, 1, RATE).collect();
        let summary: Vec<(i64, u8, i64)> = beats.iter().map(|b| (b.bar, b.beat, b.frame)).collect();
        assert_eq!(
            summary,
            vec![
                (-1, 0, -72_000),
                (-1, 1, -48_000),
                (-1, 2, -24_000),
                (0, 0, 0)
            ]
        );
        assert_eq!(tempo.beats_in(1, 24_000, RATE).count(), 0);
        assert_eq!(tempo.bars_length(2, RATE), 144_000);
    }

    #[test]
    fn test_bar_frame_counts_beats_per_bar() {
        let tempo = Tempo {
            bpm: 90.0,
            beats_per_bar: 6,
            beat_unit: 8,
        };
        assert_eq!(tempo.bar_frame(1, RATE), tempo.beat_frame(6, RATE));
        assert_eq!(tempo.bar_frame(-2, RATE), tempo.beat_frame(-12, RATE));
    }
}
