//! The modulated-delay core the chorus and flanger share: their LFO and its parameters, and the
//! fractional read of a moving tap (`docs/architecture/modulation.md` "Modulated delay").

use dsp::TapDelay;
use modulation::{Lfo, Shape, sync};
use signal_graph::{ParameterInfo, Scale};

/// The LFO shapes a modulated delay offers, by index. The order is saved format.
pub(crate) const SHAPES: [&str; 2] = ["sine", "triangle"];

/// The seed of every modulated delay's LFO: its shapes are not random, so any fixed value does.
const SEED: u64 = 0x6368_6f72_7573;

/// The LFO parameters a modulated delay declares, in their order: sync, rate, note, feel, shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LfoControl {
    Sync,
    Rate,
    Note,
    Feel,
    Shape,
}

/// A choice parameter `name` over `options`, at index `default`.
pub(crate) fn choice(name: &str, options: &[&str], default: f32) -> ParameterInfo {
    #[allow(clippy::cast_precision_loss)] // a handful of options
    let max = (options.len() - 1) as f32;
    ParameterInfo::new(name, 0.0, max, default).with_scale(Scale::Choice(
        options.iter().map(|&o| o.to_owned()).collect(),
    ))
}

/// LFO parameter `control`, in group `lfo`, with `rate` defaulting to `rate` Hz, `note` to index
/// `note` of [`sync::LFO_NOTES`] and `shape` to index `shape` of [`SHAPES`].
pub(crate) fn lfo_info(control: LfoControl, rate: f32, note: f32, shape: f32) -> ParameterInfo {
    let info = match control {
        LfoControl::Sync => ParameterInfo::new("sync", 0.0, 1.0, 0.0).with_scale(Scale::Toggle),
        LfoControl::Rate => ParameterInfo::new("rate", 0.05, 5.0, rate)
            .with_unit("Hz")
            .with_scale(Scale::Logarithmic)
            .shown_while("sync", false),
        LfoControl::Note => choice("note", &sync::LFO_NOTES, note).shown_while("sync", true),
        LfoControl::Feel => choice("feel", &sync::FEELS, 0.0).shown_while("sync", true),
        LfoControl::Shape => choice("shape", &SHAPES, shape),
    };
    info.with_group("lfo")
}

/// The LFO parameters' values, as read once per block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LfoValues {
    pub(crate) sync: f32,
    pub(crate) rate: f32,
    pub(crate) note: f32,
    pub(crate) feel: f32,
    pub(crate) shape: f32,
}

/// A modulated delay's LFO, at the whole-note length it runs at.
pub(crate) struct Clock {
    pub(crate) lfo: Lfo,
    /// The whole-note length, in frames.
    frames_per_whole_note: f64,
}

impl Clock {
    /// A clock at `sample_rate` for a whole note of `frames_per_whole_note` frames (two seconds
    /// if none, or not a positive length), at phase 0.
    pub(crate) fn new(sample_rate: u32, frames_per_whole_note: Option<f64>) -> Clock {
        Clock {
            lfo: Lfo::new(sample_rate, SEED),
            frames_per_whole_note: sync::whole_note(frames_per_whole_note, sample_rate),
        }
    }

    /// Takes the LFO parameters' values: the frequency, free or synced, and the shape. The
    /// phase continues.
    pub(crate) fn set(&mut self, values: LfoValues) {
        if values.sync >= 0.5 {
            let period = sync::period_frames(self.frames_per_whole_note, values.note, values.feel);
            self.lfo.set_period_frames(period);
        } else {
            self.lfo.set_frequency(f64::from(values.rate));
        }
        self.lfo.set_shape(if values.shape >= 0.5 {
            Shape::Triangle
        } else {
            Shape::Sine
        });
    }
}

/// `ms` milliseconds at `sample_rate`, in frames.
pub(crate) fn ms_to_frames(ms: f32, sample_rate: u32) -> f32 {
    #[allow(clippy::cast_precision_loss)] // audio rates are exact in f32
    let rate = sample_rate as f32;
    ms * rate / 1000.0
}

/// Two delay lines, each long enough for `max_ms` milliseconds at `sample_rate` and the four
/// taps of a cubic read around it. Allocates.
pub(crate) fn lines(max_ms: f32, sample_rate: u32) -> [TapDelay; 2] {
    let frames = ms_to_frames(max_ms, sample_rate).ceil().max(0.0);
    // A few hundred milliseconds of frames at most.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let max_tap = frames as usize + 4;
    [TapDelay::new(max_tap), TapDelay::new(max_tap)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(sync: f32, rate: f32, note: f32, feel: f32, shape: f32) -> LfoValues {
        LfoValues {
            sync,
            rate,
            note,
            feel,
            shape,
        }
    }

    #[test]
    fn test_clock_set_free_takes_rate() {
        let mut clock = Clock::new(48_000, None);
        clock.set(values(0.0, 0.8, 4.0, 0.0, 1.0));
        assert!((clock.lfo.frequency() - 0.8).abs() < 1e-6);
        assert_eq!(clock.lfo.shape(), Shape::Triangle);
    }

    #[test]
    fn test_clock_set_synced_follows_tempo() {
        // 60 BPM in 4/4 at 48 kHz: a whole note is 192 000 frames, four seconds.
        let mut clock = Clock::new(48_000, Some(192_000.0));
        clock.set(values(1.0, 0.8, 4.0, 0.0, 0.0));
        assert_eq!(clock.lfo.frequency(), 0.25);
        clock.set(values(1.0, 0.8, 3.0, 1.0, 0.0));
        // A dotted half note: three seconds.
        assert!((clock.lfo.frequency() - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(clock.lfo.shape(), Shape::Sine);
    }

    #[test]
    fn test_clock_without_tempo_uses_two_second_whole_notes() {
        let mut clock = Clock::new(48_000, None);
        clock.set(values(1.0, 0.8, 4.0, 0.0, 0.0));
        assert_eq!(clock.lfo.frequency(), 0.5);
    }

    #[test]
    fn test_lines_hold_the_longest_delay_and_four_taps() {
        let [left, right] = lines(40.0, 48_000);
        assert_eq!(left.max_tap(), 1924);
        assert_eq!(right.max_tap(), 1924);
    }
}
