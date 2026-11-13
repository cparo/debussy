//! [`Tap`]: a whole-frame tap into a [`TapDelay`] that crossfades to a new tap when it changes
//! (`docs/architecture/delay-and-reverb.md` "Delay").

use dsp::TapDelay;
use signal_graph::Ramp;

/// A number of frames as a whole-frame tap, from 1 up to `max`.
pub(crate) fn to_tap(frames: f64, max: usize) -> usize {
    // Clamped to a range that fits `usize` first.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let tap = frames.round().clamp(1.0, max as f64) as usize;
    tap
}

/// One channel's tap, crossfading linearly from the old tap to the new over the ramp time when it
/// changes. A change during a fade waits for it to finish.
pub(crate) struct Tap {
    sample_rate: u32,
    /// The tap being faded to, or held.
    pub(crate) current: usize,
    /// The tap being faded from.
    previous: usize,
    /// The tap asked for, taken up when no fade is running.
    pub(crate) wanted: usize,
    /// How much of `current` is heard: ramps from 0 to 1 after a change.
    fade: Ramp,
}

impl Tap {
    /// A tap of `tap` frames, at rest.
    pub(crate) fn new(tap: usize, sample_rate: u32) -> Self {
        Tap {
            sample_rate,
            current: tap,
            previous: tap,
            wanted: tap,
            fade: Ramp::new(1.0, sample_rate),
        }
    }

    /// The sample `line` gives for the next frame.
    pub(crate) fn read(&mut self, line: &TapDelay) -> f32 {
        if !self.fade.is_ramping() && self.wanted != self.current {
            self.previous = self.current;
            self.current = self.wanted;
            self.fade = Ramp::new(0.0, self.sample_rate);
            self.fade.set_target(1.0);
        }
        let fade = self.fade.next_value();
        if fade >= 1.0 {
            line.read(self.current)
        } else {
            line.read(self.previous) * (1.0 - fade) + line.read(self.current) * fade
        }
    }
}
