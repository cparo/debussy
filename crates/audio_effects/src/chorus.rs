//! The chorus: 1 to 4 LFO-swept delay voices per channel, mixed with the input
//! (`docs/architecture/modulation.md` "Chorus", ADR-0120).

use dsp::TapDelay;
use modulation::Lfo;
use signal_graph::{ParameterInfo, Ramp, Scale};

use crate::modulated_delay::{LfoDefaults, ModulatedDelay, Sweep, ms_to_frames};

/// A chorus: each channel's input written into a line read by 1 to 4 voices, each at a delay the
/// LFO sweeps from `delay` up to `delay + depth`, the voices at evenly spaced LFO phases and the
/// right channel `spread` degrees from the left. The output mixes the input with the voices'
/// sum × 1/√n. One stereo input and one stereo output.
pub(crate) type Chorus = ModulatedDelay<Voices>;

/// The most voices per channel.
const MAX_VOICES: usize = 4;

/// The longest base delay, in milliseconds.
const MAX_DELAY_MS: f32 = 30.0;

/// The deepest sweep, in milliseconds.
const MAX_DEPTH_MS: f32 = 10.0;

/// The chorus's own parameters, in the order it declares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum Control {
    Delay,
    Depth,
    Voices,
}

/// The chorus's voices: their base delay, sweep, LFO offsets and gains.
pub(crate) struct Voices {
    sample_rate: u32,
    /// The base delay, in frames.
    delay: Ramp,
    /// The sweep, in frames.
    depth: Ramp,
    /// Each voice's LFO offset, in cycles.
    offsets: [Ramp; MAX_VOICES],
    /// Each voice's gain: 1/√n for the first n voices, 0 for the rest.
    gains: [Ramp; MAX_VOICES],
}

impl Sweep for Voices {
    const LFO_DEFAULTS: LfoDefaults = LfoDefaults {
        rate: 0.8,
        note: 4.0,
        shape: 0.0,
    };
    const MAX_MS: f32 = MAX_DELAY_MS + MAX_DEPTH_MS;
    const CONTROLS: usize = 3;

    fn infos() -> Vec<ParameterInfo> {
        #[allow(clippy::cast_precision_loss)] // a handful of voices
        let most = MAX_VOICES as f32;
        vec![
            ParameterInfo::new("delay", 5.0, MAX_DELAY_MS, 15.0)
                .with_unit("ms")
                .with_group("delay"),
            ParameterInfo::new("depth", 0.0, MAX_DEPTH_MS, 3.0)
                .with_unit("ms")
                .with_group("delay"),
            ParameterInfo::new("voices", 1.0, most, 2.0)
                .with_scale(Scale::Integer)
                .with_group("delay"),
        ]
    }

    fn new(sample_rate: u32) -> Self {
        let ramp = || Ramp::new(0.0, sample_rate);
        Voices {
            sample_rate,
            delay: ramp(),
            depth: ramp(),
            offsets: std::array::from_fn(|_| ramp()),
            gains: std::array::from_fn(|_| ramp()),
        }
    }

    fn set(&mut self, value: &dyn Fn(usize) -> f32) {
        let value = |control: Control| value(control as usize);
        let rate = self.sample_rate;
        self.delay
            .set_target(ms_to_frames(value(Control::Delay), rate));
        self.depth
            .set_target(ms_to_frames(value(Control::Depth), rate));
        #[allow(clippy::cast_precision_loss)] // a handful of voices
        let most = MAX_VOICES as f32;
        let voices = value(Control::Voices).round().clamp(1.0, most);
        // NaN would survive the clamp; it counts as one voice.
        let voices = if voices.is_nan() { 1.0 } else { voices };
        let gain = voices.sqrt().recip();
        for (i, (offset, g)) in (0_u8..).zip(self.offsets.iter_mut().zip(&mut self.gains)) {
            let i = f32::from(i);
            if i < voices {
                offset.set_target(i / voices);
                g.set_target(gain);
            } else {
                g.set_target(0.0);
            }
        }
    }

    fn ramps(&mut self) -> impl Iterator<Item = &mut Ramp> {
        [&mut self.delay, &mut self.depth]
            .into_iter()
            .chain(&mut self.offsets)
            .chain(&mut self.gains)
    }

    fn frame(
        &mut self,
        lines: &mut [TapDelay; 2],
        lfo: &Lfo,
        spread: f64,
        input: [f32; 2],
    ) -> [f32; 2] {
        let delay = self.delay.next_value();
        let depth = self.depth.next_value();
        let offsets = self.offsets.each_mut().map(|o| f64::from(o.next_value()));
        let gains = self.gains.each_mut().map(Ramp::next_value);
        let mut wet = [0.0_f32; 2];
        for ((w, line), (shift, lane)) in wet.iter_mut().zip(&*lines).zip([(0.0, 0), (spread, 1)]) {
            for (voice, (&offset, &gain)) in (0_u64..).zip(offsets.iter().zip(&gains)) {
                if gain == 0.0 {
                    continue;
                }
                let lfo = lfo.value_at_offset(offset + shift, lane * 4 + voice);
                *w += gain * line.read_cubic(delay + depth * 0.5 * (1.0 + lfo));
            }
        }
        for (line, x) in lines.iter_mut().zip(input) {
            line.write(x);
        }
        wet
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_graph::Node;

    /// Every own parameter at 10, except `voices` at `voices`.
    fn with_voices(voices: f32) -> impl Fn(usize) -> f32 {
        move |i| {
            if i == Control::Voices as usize {
                voices
            } else {
                10.0
            }
        }
    }

    #[test]
    fn test_new_declares_parameters_in_groups() -> Result<(), crate::Error> {
        let infos: Vec<ParameterInfo> = Chorus::new()?
            .parameters()
            .iter()
            .map(|p| p.info().clone())
            .collect();
        let names: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "sync", "rate", "note", "feel", "shape", "delay", "depth", "voices", "spread",
                "mix"
            ]
        );
        let groups: Vec<&str> = infos.iter().map(|i| i.group.as_str()).collect();
        assert_eq!(
            groups,
            [
                "lfo", "lfo", "lfo", "lfo", "lfo", "delay", "delay", "delay", "stereo", "output"
            ]
        );
        assert_eq!(infos[1].default, 0.8);
        assert_eq!(infos[2].default, 4.0);
        assert_eq!(infos[4].default, 0.0);
        let voices = &infos[7];
        assert_eq!(voices.scale, Scale::Integer);
        assert_eq!((voices.min, voices.max, voices.default), (1.0, 4.0, 2.0));
        assert!(infos.iter().all(|i| !i.stopped_only));
        Ok(())
    }

    #[test]
    fn test_set_voices_spaces_offsets_and_gains() {
        let mut voices = Voices::new(48_000);
        voices.set(&with_voices(4.0));
        let offsets: Vec<f32> = voices.offsets.iter().map(Ramp::target).collect();
        assert_eq!(offsets, [0.0, 0.25, 0.5, 0.75]);
        assert!(voices.gains.iter().all(|g| g.target() == 0.5));
        voices.set(&with_voices(2.0));
        let gains: Vec<f32> = voices.gains.iter().map(Ramp::target).collect();
        let half = 2.0_f32.sqrt().recip();
        assert_eq!(gains, [half, half, 0.0, 0.0]);
        assert_eq!(voices.offsets[1].target(), 0.5);
    }

    #[test]
    fn test_set_clamps_voices() {
        let mut voices = Voices::new(48_000);
        voices.set(&with_voices(9.0));
        assert!(voices.gains.iter().all(|g| g.target() == 0.5));
        voices.set(&with_voices(f32::NAN));
        assert_eq!(voices.gains[0].target(), 1.0);
        assert_eq!(voices.gains[1].target(), 0.0);
    }
}
