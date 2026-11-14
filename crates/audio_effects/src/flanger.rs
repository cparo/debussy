//! The flanger: one LFO-swept short delay per channel with feedback, mixed with the input
//! (`docs/architecture/modulation.md` "Flanger", ADR-0120).

use audio_buffers::Layout;
use dsp::TapDelay;
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;
use crate::modulated_delay::{Clock, LfoControl, LfoValues, lfo_info, lines, ms_to_frames};

/// The longest base delay, in milliseconds.
const MAX_DELAY_MS: f32 = 10.0;

/// The largest feedback, either way, in percent.
pub const MAX_FEEDBACK_PERCENT: f32 = 95.0;

/// The parameters, in the order the effect declares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum Control {
    Sync,
    Rate,
    Note,
    Feel,
    Shape,
    Delay,
    Depth,
    Feedback,
    Spread,
    Mix,
}

/// How many parameters there are.
const CONTROLS: usize = 10;

/// Every control, in declaration order.
const ALL: [Control; CONTROLS] = [
    Control::Sync,
    Control::Rate,
    Control::Note,
    Control::Feel,
    Control::Shape,
    Control::Delay,
    Control::Depth,
    Control::Feedback,
    Control::Spread,
    Control::Mix,
];

/// What parameter `control` is: its saved name, range, default, unit, scale and group.
fn info(control: Control) -> ParameterInfo {
    let lfo = |c| lfo_info(c, 0.25, 5.0, 1.0);
    let percent =
        |name: &str, min, default| ParameterInfo::new(name, min, 100.0, default).with_unit("%");
    match control {
        Control::Sync => lfo(LfoControl::Sync),
        Control::Rate => lfo(LfoControl::Rate),
        Control::Note => lfo(LfoControl::Note),
        Control::Feel => lfo(LfoControl::Feel),
        Control::Shape => lfo(LfoControl::Shape),
        Control::Delay => ParameterInfo::new("delay", 0.1, MAX_DELAY_MS, 1.0)
            .with_unit("ms")
            .with_scale(Scale::Logarithmic)
            .with_group("delay"),
        Control::Depth => percent("depth", 0.0, 70.0).with_group("delay"),
        Control::Feedback => ParameterInfo::new(
            "feedback",
            -MAX_FEEDBACK_PERCENT,
            MAX_FEEDBACK_PERCENT,
            50.0,
        )
        .with_unit("%")
        .with_group("delay"),
        Control::Spread => ParameterInfo::new("spread", 0.0, 180.0, 90.0)
            .with_unit("°")
            .with_group("stereo"),
        Control::Mix => percent("mix", 0.0, 50.0).with_group("output"),
    }
}

/// A prepared flanger: the lines, the LFO and the ramps.
struct Running {
    sample_rate: u32,
    clock: Clock,
    lines: [TapDelay; 2],
    /// The base delay, in frames.
    delay: Ramp,
    /// The sweep, as a share of the base delay, 0 to 1.
    depth: Ramp,
    /// The feedback gain, held below 1 in magnitude.
    feedback: Ramp,
    /// The right channel's LFO offset, in cycles.
    spread: Ramp,
    /// The wet share of the output, 0 to 1.
    mix: Ramp,
}

impl Running {
    /// A flanger at `sample_rate` for a whole note of `frames_per_whole_note` frames, its
    /// parameters at `value`'s values, with no ramp in. Allocates.
    fn new(
        sample_rate: u32,
        frames_per_whole_note: Option<f64>,
        value: impl Fn(Control) -> f32,
    ) -> Running {
        let ramp = |v| Ramp::new(v, sample_rate);
        let mut running = Running {
            sample_rate,
            clock: Clock::new(sample_rate, frames_per_whole_note),
            // At full depth the sweep reaches twice the base delay.
            lines: lines(2.0 * MAX_DELAY_MS, sample_rate),
            delay: ramp(0.0),
            depth: ramp(0.0),
            feedback: ramp(0.0),
            spread: ramp(0.0),
            mix: ramp(0.0),
        };
        running.set(&value);
        // Start at the values, with no ramp in.
        for r in [
            &mut running.delay,
            &mut running.depth,
            &mut running.feedback,
            &mut running.spread,
            &mut running.mix,
        ] {
            *r = Ramp::new(r.target(), sample_rate);
        }
        running
    }

    /// Takes the parameter values, once per block.
    fn set(&mut self, value: &impl Fn(Control) -> f32) {
        self.clock.set(LfoValues {
            sync: value(Control::Sync),
            rate: value(Control::Rate),
            note: value(Control::Note),
            feel: value(Control::Feel),
            shape: value(Control::Shape),
        });
        self.delay
            .set_target(ms_to_frames(value(Control::Delay), self.sample_rate));
        self.depth.set_target(value(Control::Depth) / 100.0);
        let limit = MAX_FEEDBACK_PERCENT / 100.0;
        self.feedback
            .set_target((value(Control::Feedback) / 100.0).clamp(-limit, limit));
        self.spread.set_target(value(Control::Spread) / 360.0);
        self.mix.set_target(value(Control::Mix) / 100.0);
    }

    /// One frame of `left`, `right`.
    fn frame(&mut self, left: f32, right: f32) -> (f32, f32) {
        let delay = self.delay.next_value();
        let depth = self.depth.next_value();
        let feedback = self.feedback.next_value();
        let spread = f64::from(self.spread.next_value());
        let mix = self.mix.next_value();
        let input = [left, right];
        let mut wet = [0.0_f32; 2];
        for (((w, line), &x), (shift, stream)) in wet
            .iter_mut()
            .zip(&mut self.lines)
            .zip(&input)
            .zip([(0.0, 0), (spread, 1)])
        {
            let lfo = self.clock.lfo.value_at_offset(shift, stream);
            let y = line.read_cubic(delay * (1.0 + depth * lfo));
            let fed = x + feedback * y;
            // A loop that has gone non-finite is restarted from the input alone.
            line.write(if fed.is_finite() { fed } else { x });
            *w = if y.is_finite() { y } else { 0.0 };
        }
        self.clock.lfo.advance();
        let dry = 1.0 - mix;
        (left * dry + wet[0] * mix, right * dry + wet[1] * mix)
    }
}

/// A flanger: each channel's input, plus its feedback, written into a short line read at a delay
/// one LFO sweeps around `delay` by `depth`, the right channel `spread` degrees from the left.
/// The output mixes the input with what the line gives. One stereo input and one stereo output.
pub struct Flanger {
    parameters: [Parameter; CONTROLS],
    /// The whole-note length the graph gave, in frames.
    frames_per_whole_note: Option<f64>,
    running: Option<Running>,
}

impl Flanger {
    /// A flanger at its defaults. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::Parameter`] if a parameter cannot be made, which does not happen for the fixed
    /// ranges here.
    pub fn new() -> Result<Self, Error> {
        let mut parameters = Vec::with_capacity(CONTROLS);
        for control in ALL {
            parameters.push(Parameter::new(info(control))?);
        }
        let parameters = parameters
            .try_into()
            .map_err(|_| Error::Parameter(signal_graph::Error::InvalidParameterRange))?;
        Ok(Flanger {
            parameters,
            frames_per_whole_note: None,
            running: None,
        })
    }
}

impl Node for Flanger {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Stereo)]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Stereo)]
    }

    fn parameters(&self) -> Vec<Parameter> {
        self.parameters.to_vec()
    }

    fn uses_tempo(&self) -> bool {
        true
    }

    fn set_tempo(&mut self, frames_per_whole_note: f64) {
        self.frames_per_whole_note = Some(frames_per_whole_note);
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        let parameters = &self.parameters;
        self.running = Some(Running::new(
            sample_rate,
            self.frames_per_whole_note,
            |control| parameters[control as usize].get(),
        ));
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let parameters = &self.parameters;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        running.set(&|control: Control| parameters[control as usize].get());
        let (in_left, in_right) = (inputs.channel(0, 0), inputs.channel(0, 1));
        let sample =
            |input: Option<&[f32]>, i: usize| input.and_then(|c| c.get(i)).copied().unwrap_or(0.0);
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            (*l, *r) = running.frame(sample(in_left, i), sample(in_right, i));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn infos() -> Result<Vec<ParameterInfo>, Error> {
        Ok(Flanger::new()?
            .parameters()
            .iter()
            .map(|p| p.info().clone())
            .collect())
    }

    #[test]
    fn test_new_declares_parameters_in_groups() -> Result<(), Error> {
        let infos = infos()?;
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
        let delay = &infos[Control::Delay as usize];
        assert_eq!((delay.min, delay.max, delay.default), (0.1, 10.0, 1.0));
        assert_eq!(delay.scale, Scale::Logarithmic);
        let feedback = &infos[Control::Feedback as usize];
        assert_eq!(
            (feedback.min, feedback.max, feedback.default),
            (-95.0, 95.0, 50.0)
        );
        assert_eq!(infos[Control::Shape as usize].default, 1.0);
        assert_eq!(infos[Control::Note as usize].default, 5.0);
        assert!(infos.iter().all(|i| !i.stopped_only));
        Ok(())
    }

    #[test]
    fn test_prepare_with_sync_follows_tempo() -> Result<(), Box<dyn std::error::Error>> {
        let mut flanger = Flanger::new()?;
        flanger.parameters[Control::Sync as usize].set(1.0);
        // 120 BPM in 4/4 at 48 kHz: a whole note is 96 000 frames; 2/1 is four seconds.
        flanger.set_tempo(96_000.0);
        flanger.prepare(48_000, 64);
        let running = flanger.running.as_ref().ok_or("not prepared")?;
        assert_eq!(running.clock.lfo.frequency(), 0.25);
        Ok(())
    }

    #[test]
    fn test_frame_writes_input_alone_when_the_loop_is_not_finite()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut flanger = Flanger::new()?;
        flanger.prepare(48_000, 64);
        let running = flanger.running.as_mut().ok_or("not prepared")?;
        running.frame(f32::INFINITY, 0.0);
        for _ in 0..2_000 {
            running.frame(0.0, 0.0);
        }
        let (left, right) = running.frame(0.0, 0.0);
        assert!(left.is_finite() && right.is_finite());
        Ok(())
    }
}
