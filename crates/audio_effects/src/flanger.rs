//! The flanger: one LFO-swept short delay per channel with feedback, mixed with the input
//! (`docs/architecture/modulation.md` "Flanger", ADR-0120).

use dsp::TapDelay;
use modulation::Lfo;
use signal_graph::{ParameterInfo, Ramp, Reading, Scale};

use crate::gain_spectrogram::SpectrogramCurve;
use crate::modulated_delay::{LfoDefaults, ModulatedDelay, Sweep, controls, ms_to_frames};

/// A flanger: each channel's input, plus its feedback, written into a short line read at a delay
/// the LFO sweeps around `delay` by `depth`, the right channel `spread` degrees from the left.
/// The output mixes the input with what the line gives. One stereo input and one stereo output.
pub(crate) type Flanger = ModulatedDelay<Comb>;

/// The longest base delay, in milliseconds.
const MAX_DELAY_MS: f32 = 10.0;

/// The largest feedback, either way, in percent.
const MAX_FEEDBACK_PERCENT: f32 = 95.0;

/// The flanger's own parameters, in the order it declares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum Control {
    Delay,
    Depth,
    Feedback,
}

/// The flanger's comb: its base delay, sweep and feedback.
pub(crate) struct Comb {
    sample_rate: u32,
    /// The base delay, in frames.
    delay: Ramp,
    /// The sweep, as a share of the base delay, 0 to 1.
    depth: Ramp,
    /// The feedback gain, held below 1 in magnitude.
    feedback: Ramp,
}

impl Sweep for Comb {
    const LFO_DEFAULTS: LfoDefaults = LfoDefaults {
        rate: 0.25,
        note: 5.0,
        shape: 1.0,
    };
    // At full depth the sweep reaches twice the base delay.
    const MAX_MS: f32 = 2.0 * MAX_DELAY_MS;
    const CONTROLS: usize = 3;

    fn infos() -> Vec<ParameterInfo> {
        vec![
            ParameterInfo::new("delay", 0.1, MAX_DELAY_MS, 1.0)
                .with_unit("ms")
                .with_scale(Scale::Logarithmic)
                .with_group("delay"),
            ParameterInfo::new("depth", 0.0, 100.0, 70.0)
                .with_unit("%")
                .with_group("delay"),
            ParameterInfo::new(
                "feedback",
                -MAX_FEEDBACK_PERCENT,
                MAX_FEEDBACK_PERCENT,
                50.0,
            )
            .with_unit("%")
            .with_group("delay"),
        ]
    }

    fn new(sample_rate: u32) -> Self {
        let ramp = || Ramp::new(0.0, sample_rate);
        Comb {
            sample_rate,
            delay: ramp(),
            depth: ramp(),
            feedback: ramp(),
        }
    }

    fn set(&mut self, read: &dyn Fn(usize) -> Reading) {
        let read = |control: Control| read(control as usize);
        let rate = self.sample_rate;
        self.delay
            .approach(read(Control::Delay).map(|ms| ms_to_frames(ms, rate)));
        self.depth.approach(read(Control::Depth).map(|v| v / 100.0));
        let limit = MAX_FEEDBACK_PERCENT / 100.0;
        self.feedback.approach(read(Control::Feedback).map(|v| {
            let feedback = (v / 100.0).clamp(-limit, limit);
            if feedback.is_nan() { 0.0 } else { feedback }
        }));
    }

    fn ramps(&mut self) -> impl Iterator<Item = &mut Ramp> {
        [&mut self.delay, &mut self.depth, &mut self.feedback].into_iter()
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
        let feedback = self.feedback.next_value();
        let mut wet = [0.0_f32; 2];
        for (((w, line), x), (shift, stream)) in wet
            .iter_mut()
            .zip(lines)
            .zip(input)
            .zip([(0.0, 0), (spread, 1)])
        {
            let lfo = lfo.value_at_offset(shift, stream);
            let y = line.read_cubic(delay * (1.0 + depth * lfo));
            let fed = x + feedback * y;
            // A loop that has gone non-finite is restarted from the input alone.
            line.write(if fed.is_finite() { fed } else { x });
            *w = if y.is_finite() { y } else { 0.0 };
        }
        wet
    }
}

/// How many parameters a flanger has.
pub(crate) const CONTROLS: usize = controls(<Comb as Sweep>::CONTROLS);

/// The curve of a `flanger` effect whose parameters stand at `values`, in declaration order (see
/// [`crate::CurveFn`]): its gain over [`crate::gain_spectrogram::SPAN`] of pink noise, simulated
/// when it is read.
pub(crate) fn curve(
    values: &[f32],
    sample_rate: u32,
    frames_per_whole_note: f64,
) -> Option<crate::Curve> {
    let values = <&[f32; CONTROLS]>::try_from(values).ok()?;
    let whole = modulation::sync::whole_note(Some(frames_per_whole_note), sample_rate);
    Some(crate::Curve::Spectrogram(SpectrogramCurve::flanger(
        *values,
        sample_rate,
        whole,
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{assert_lands, automated, off_default};

    #[test]
    fn test_set_lands_automated_values_on_the_control_point() {
        let infos = Comb::infos();
        let mut comb = Comb::new(48_000);
        comb.set(&|i| automated(off_default(&infos[i])));
        assert_lands(&mut [
            ("delay", &mut comb.delay),
            ("depth", &mut comb.depth),
            ("feedback", &mut comb.feedback),
        ]);
    }
    use signal_graph::Node;

    #[test]
    fn test_new_declares_parameters_in_groups() -> Result<(), crate::Error> {
        let infos: Vec<ParameterInfo> = Flanger::new()?
            .parameters()
            .iter()
            .map(|p| p.info().clone())
            .collect();
        let names: Vec<&str> = infos.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "sync", "rate", "note", "feel", "shape", "delay", "depth", "feedback", "spread",
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
        assert_eq!(infos[1].default, 0.25);
        assert_eq!(infos[2].default, 5.0);
        assert_eq!(infos[4].default, 1.0);
        let delay = &infos[5];
        assert_eq!((delay.min, delay.max, delay.default), (0.1, 10.0, 1.0));
        assert_eq!(delay.scale, Scale::Logarithmic);
        let feedback = &infos[7];
        assert_eq!(
            (feedback.min, feedback.max, feedback.default),
            (-95.0, 95.0, 50.0)
        );
        assert!(infos.iter().all(|i| !i.stopped_only));
        Ok(())
    }

    #[test]
    fn test_set_holds_feedback_below_one() {
        let mut comb = Comb::new(48_000);
        comb.set(&|i| {
            Reading::fixed(if i == Control::Feedback as usize {
                300.0
            } else {
                1.0
            })
        });
        assert_eq!(comb.feedback.target(), 0.95);
        comb.set(&|i| {
            Reading::fixed(if i == Control::Feedback as usize {
                -300.0
            } else {
                1.0
            })
        });
        assert_eq!(comb.feedback.target(), -0.95);
        comb.set(&|_| Reading::fixed(f32::NAN));
        assert_eq!(comb.feedback.target(), 0.0);
    }

    #[test]
    fn test_frame_writes_input_alone_when_the_loop_is_not_finite() {
        let mut comb = Comb::new(48_000);
        comb.set(&|i| {
            Reading::fixed(if i == Control::Feedback as usize {
                90.0
            } else {
                1.0
            })
        });
        let mut lines = [TapDelay::new(64), TapDelay::new(64)];
        let lfo = Lfo::new(48_000, 0);
        comb.frame(&mut lines, &lfo, 0.0, [f32::INFINITY, 0.0]);
        for _ in 0..200 {
            comb.frame(&mut lines, &lfo, 0.0, [0.0, 0.0]);
        }
        let wet = comb.frame(&mut lines, &lfo, 0.0, [0.0, 0.0]);
        assert!(wet.iter().all(|w| w.is_finite()));
        assert!((1..=64).all(|tap| lines[0].read(tap).is_finite()));
    }
}
