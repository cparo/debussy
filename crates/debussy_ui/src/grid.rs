//! The bars and beats grid over the lanes (`docs/architecture/user-interface.md` "Layout",
//! ADR-0085): where its lines fall in a lane, thinned so they stay at least [`MIN_GAP`] logical
//! pixels apart.

use project_model::Tempo;

/// The closest two grid lines may be drawn, in logical pixels.
pub(crate) const MIN_GAP: f64 = 6.0;

/// One line of the grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GridLine {
    /// Where it falls, in logical pixels from the lane's left edge.
    pub x: f32,
    /// Whether it starts a bar; otherwise it is a beat within one.
    pub bar: bool,
}

/// Which lines are drawn at a zoom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Thinning {
    /// Whether beat lines are drawn.
    pub beats: bool,
    /// Bar lines are drawn on every this many bars, a power of two.
    pub every_bars: u64,
}

/// The thinning for beats `beat_px` logical pixels apart in bars of `beats_per_bar`: beat
/// lines while they are at least [`MIN_GAP`] apart, and bar lines on every 2^k bars for the
/// least k that keeps them that far apart.
pub(crate) fn thinning(beat_px: f64, beats_per_bar: u8) -> Thinning {
    let bar_px = beat_px * f64::from(beats_per_bar.max(1));
    let mut every_bars: u64 = 1;
    while (every_bars as f64) * bar_px < MIN_GAP && every_bars < 1 << 40 {
        every_bars *= 2;
    }
    Thinning {
        beats: beat_px >= MIN_GAP,
        every_bars,
    }
}

/// The grid lines in a lane `width` logical pixels wide showing `span` frames from `start`, at
/// `rate` Hz, earliest first.
pub(crate) fn grid_lines(
    tempo: &Tempo,
    rate: u32,
    start: u64,
    span: u64,
    width: f32,
) -> Vec<GridLine> {
    if span == 0 || width <= 0.0 {
        return Vec::new();
    }
    let px_per_frame = f64::from(width) / span as f64;
    let thin = thinning(
        tempo.frames_per_beat(rate) * px_per_frame,
        tempo.beats_per_bar,
    );
    let (from, to) = (start as i64, start.saturating_add(span) as i64);
    let x = |frame: i64| ((frame - from) as f64 * px_per_frame) as f32;
    if thin.beats {
        return tempo
            .beats_in(from, to, rate)
            .map(|beat| GridLine {
                x: x(beat.frame),
                bar: beat.beat == 0,
            })
            .collect();
    }
    let every = thin.every_bars as i64;
    let first_bar = tempo.beat_at(from, rate).bar;
    let mut bar = first_bar.div_euclid(every) * every;
    let mut lines = Vec::new();
    loop {
        let frame = tempo.bar_frame(bar, rate);
        if frame >= to {
            break;
        }
        if frame >= from {
            lines.push(GridLine {
                x: x(frame),
                bar: true,
            });
        }
        bar += every;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thinning_hides_beats_then_halves_bars() {
        assert_eq!(
            thinning(6.0, 4),
            Thinning {
                beats: true,
                every_bars: 1
            }
        );
        // Beats too close; bars of 4 × 5 = 20 px are far enough.
        assert_eq!(
            thinning(5.0, 4),
            Thinning {
                beats: false,
                every_bars: 1
            }
        );
        // Bars 1 px apart: every 8th bar.
        assert_eq!(thinning(0.25, 4).every_bars, 8);
        // Bars 5 px apart: every other bar.
        assert_eq!(thinning(5.0, 1).every_bars, 2);
        assert_eq!(thinning(0.0, 4).every_bars, 1 << 40);
    }

    #[test]
    fn test_grid_lines_mark_bars_and_beats_in_view() {
        // 120 BPM in 3/4 at 48 kHz: a beat every 24 000 frames.
        let tempo = Tempo {
            bpm: 120.0,
            beats_per_bar: 3,
            beat_unit: 4,
        };
        // Frames 12 000 to 108 000 over 960 px: 100 frames a pixel, beats 240 px apart.
        let lines = grid_lines(&tempo, 48_000, 12_000, 96_000, 960.0);
        let expected = [
            (120.0, false),
            (360.0, false),
            (600.0, true),
            (840.0, false),
        ];
        let got: Vec<(f32, bool)> = lines.iter().map(|l| (l.x, l.bar)).collect();
        assert_eq!(got, expected);
    }

    #[test]
    fn test_grid_lines_thin_to_every_power_of_two_bars_when_zoomed_out() {
        let tempo = Tempo::default(); // A bar every 96 000 frames at 48 kHz.
        // An hour over 1000 px: a bar is 0.53 px, so every 16th bar (8.5 px).
        let hour = 3600 * 48_000;
        let lines = grid_lines(&tempo, 48_000, 0, hour, 1000.0);
        assert!(lines.iter().all(|l| l.bar));
        assert_eq!(lines.first().map(|l| l.x), Some(0.0));
        let gaps: Vec<f32> = lines.windows(2).map(|w| w[1].x - w[0].x).collect();
        assert!(
            gaps.iter()
                .all(|&g| g >= MIN_GAP as f32 && g < 2.0 * MIN_GAP as f32)
        );
        // Bars are aligned to multiples of 16 wherever the view starts.
        let later = grid_lines(&tempo, 48_000, 96_000 * 5, hour, 1000.0);
        let first = later.first().map(|l| l.x).unwrap_or(-1.0);
        assert!((first - (11.0 * 96_000.0 * 1000.0 / hour as f32)).abs() < 0.01);
    }

    #[test]
    fn test_grid_lines_empty_for_no_span_or_width() {
        let tempo = Tempo::default();
        assert!(grid_lines(&tempo, 48_000, 0, 0, 100.0).is_empty());
        assert!(grid_lines(&tempo, 48_000, 0, 1000, 0.0).is_empty());
    }
}
