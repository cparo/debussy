//! The delay: a whole-frame tap per channel, free or tempo-synced, in stereo or ping-pong, with an
//! optional low-pass and saturation in the feedback path (`docs/architecture/delay-and-reverb.md`
//! "Delay").

use std::f32::consts::FRAC_1_SQRT_2;

use audio_buffers::Layout;
use dsp::{FOLD_DOWN_GAIN, Svf, SvfCoefficients, TapDelay};

use crate::tap::{Tap, to_tap};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;

/// The longest delay time, in milliseconds.
pub const MAX_DELAY_MS: f32 = 4000.0;

/// The shortest free delay time, in milliseconds.
const MIN_DELAY_MS: f32 = 1.0;

/// The largest offset of the right channel's time, either way, in milliseconds.
pub const MAX_OFFSET_MS: f32 = 50.0;

/// The low-pass's resonance: Butterworth, with no peak.
const LOW_PASS_Q: f32 = FRAC_1_SQRT_2;

/// The parameters, in the order the effect declares them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
enum Control {
    Sync,
    Time,
    Note,
    Feel,
    Mode,
    Offset,
    Feedback,
    LowPassEnabled,
    LowPass,
    Saturation,
    Mix,
}

/// How many parameters there are.
const CONTROLS: usize = 11;

/// Every control, in declaration order.
const ALL: [Control; CONTROLS] = [
    Control::Sync,
    Control::Time,
    Control::Note,
    Control::Feel,
    Control::Mode,
    Control::Offset,
    Control::Feedback,
    Control::LowPassEnabled,
    Control::LowPass,
    Control::Saturation,
    Control::Mix,
];

/// The note values' names, by index: each is twice the one before, from a 32nd note.
const NOTES: [&str; 6] = ["1/32", "1/16", "1/8", "1/4", "1/2", "1/1"];

/// The feels' names, by index.
const FEELS: [&str; 3] = ["straight", "dotted", "triplet"];

/// The modes' names, by index.
const MODES: [&str; 2] = ["stereo", "ping-pong"];

/// What parameter `control` is: its saved name, range, default, unit, scale and group.
fn info(control: Control) -> ParameterInfo {
    let choice = |name: &str, options: &[&str], default: f32| {
        #[allow(clippy::cast_precision_loss)] // a handful of options
        let max = (options.len() - 1) as f32;
        ParameterInfo::new(name, 0.0, max, default).with_scale(Scale::Choice(
            options.iter().map(|&o| o.to_owned()).collect(),
        ))
    };
    let toggle = |name: &str| ParameterInfo::new(name, 0.0, 1.0, 0.0).with_scale(Scale::Toggle);
    let percent =
        |name: &str, default| ParameterInfo::new(name, 0.0, 100.0, default).with_unit("%");
    match control {
        Control::Sync => toggle("sync").with_group("time"),
        Control::Time => ParameterInfo::new("time", MIN_DELAY_MS, MAX_DELAY_MS, 375.0)
            .with_unit("ms")
            .with_scale(Scale::Logarithmic)
            .with_group("time"),
        Control::Note => choice("note", &NOTES, 2.0).with_group("time"),
        Control::Feel => choice("feel", &FEELS, 0.0).with_group("time"),
        Control::Mode => choice("mode", &MODES, 0.0).with_group("stereo"),
        Control::Offset => ParameterInfo::new("offset", -MAX_OFFSET_MS, MAX_OFFSET_MS, 0.0)
            .with_unit("ms")
            .with_group("stereo"),
        Control::Feedback => percent("feedback", 35.0).with_group("feedback"),
        Control::LowPassEnabled => toggle("low-pass enabled").with_group("feedback"),
        Control::LowPass => ParameterInfo::new("low-pass", 200.0, 20_000.0, 6_000.0)
            .with_unit("Hz")
            .with_scale(Scale::Logarithmic)
            .with_group("feedback"),
        Control::Saturation => toggle("saturation").with_group("feedback"),
        Control::Mix => percent("mix", 30.0).with_group("output"),
    }
}

/// The length of a synced delay, in frames: note value `note` (an index into [`NOTES`], rounded
/// and clamped) times feel `feel` (an index into [`FEELS`]: ×1, ×1.5 or ×2/3), for a whole note
/// of `frames_per_whole_note` frames. Not clamped to the buffer.
pub(crate) fn synced_frames(frames_per_whole_note: f64, note: f32, feel: f32) -> f64 {
    // Both are held in small ranges first, so the casts are exact.
    #[allow(clippy::cast_possible_truncation)]
    let note = note.round().clamp(0.0, 5.0) as i32;
    let fraction = f64::from(2_i32.pow(note.unsigned_abs())) / 32.0;
    let feel = if feel >= 1.5 {
        2.0 / 3.0
    } else if feel >= 0.5 {
        1.5
    } else {
        1.0
    };
    frames_per_whole_note * fraction * feel
}

/// The whole-note length when none is given: 120 BPM in 4/4, two seconds.
fn default_whole_note(sample_rate: u32) -> f64 {
    2.0 * f64::from(sample_rate)
}

/// One channel's feedback-path filter state.
#[derive(Default)]
struct Channel {
    low_pass: Svf,
}

/// A prepared delay: the lines, taps, filters and ramps.
struct Running {
    sample_rate: f32,
    /// The longest delay time, in frames.
    max_time: f64,
    /// The whole-note length, in frames.
    frames_per_whole_note: f64,
    lines: [TapDelay; 2],
    taps: [Tap; 2],
    channels: [Channel; 2],
    /// The share of ping-pong routing: 0 is stereo, 1 ping-pong.
    ping_pong: Ramp,
    /// The feedback gain, 0 to 1.
    feedback: Ramp,
    /// The share of the low-pass in the feedback path.
    low_pass_share: Ramp,
    /// The low-pass cutoff, as log2 of hertz.
    low_pass_log: Ramp,
    coefficients: SvfCoefficients,
    /// The share of saturation in the feedback path.
    saturation: Ramp,
    /// The wet share of the output, 0 to 1.
    mix: Ramp,
}

impl Running {
    /// The delay time `value` asks for, before the offset, in frames, clamped to the longest.
    fn base_frames(&self, value: &impl Fn(Control) -> f32) -> f64 {
        let frames = if value(Control::Sync) >= 0.5 {
            synced_frames(
                self.frames_per_whole_note,
                value(Control::Note),
                value(Control::Feel),
            )
        } else {
            f64::from(value(Control::Time)) * f64::from(self.sample_rate) / 1000.0
        };
        frames.clamp(1.0, self.max_time)
    }

    /// Takes the parameter values, once per block.
    fn set(&mut self, value: impl Fn(Control) -> f32) {
        let base = self.base_frames(&value);
        let offset = f64::from(value(Control::Offset)) * f64::from(self.sample_rate) / 1000.0;
        let max_tap = self.lines[1].max_tap();
        self.taps[0].wanted = to_tap(base, max_tap);
        self.taps[1].wanted = to_tap(base + offset, max_tap);
        let on = |control| if value(control) >= 0.5 { 1.0 } else { 0.0 };
        self.ping_pong.set_target(on(Control::Mode));
        self.feedback.set_target(value(Control::Feedback) / 100.0);
        self.low_pass_share.set_target(on(Control::LowPassEnabled));
        self.low_pass_log.set_target(value(Control::LowPass).log2());
        self.saturation.set_target(on(Control::Saturation));
        self.mix.set_target(value(Control::Mix) / 100.0);
    }

    /// One frame of `left`, `right`.
    fn frame(&mut self, left: f32, right: f32) -> (f32, f32) {
        if self.low_pass_log.is_ramping() {
            let hz = self.low_pass_log.next_value().exp2();
            self.coefficients = SvfCoefficients::low_pass(hz, LOW_PASS_Q, self.sample_rate);
        }
        let low_pass = self.low_pass_share.next_value();
        let saturation = self.saturation.next_value();
        let feedback = self.feedback.next_value();
        let ping_pong = self.ping_pong.next_value();
        let mix = self.mix.next_value();
        let coefficients = self.coefficients;
        let [line_l, line_r] = &self.lines;
        let [tap_l, tap_r] = &mut self.taps;
        let wet = [tap_l.read(line_l), tap_r.read(line_r)];
        let mut fed = [0.0; 2];
        for ((f, &y), channel) in fed.iter_mut().zip(&wet).zip(&mut self.channels) {
            let filtered = channel.low_pass.process(y, &coefficients);
            let x = y + low_pass * (filtered - y);
            let x = x + saturation * (x.tanh() - x);
            *f = x * feedback;
        }
        let mono = (left + right) * FOLD_DOWN_GAIN;
        let stereo = 1.0 - ping_pong;
        self.lines[0].write(stereo * (left + fed[0]) + ping_pong * (mono + fed[1]));
        self.lines[1].write(stereo * (right + fed[1]) + ping_pong * fed[0]);
        let dry = 1.0 - mix;
        (left * dry + wet[0] * mix, right * dry + wet[1] * mix)
    }
}

/// A delay: each channel's input, plus its feedback, written into a line read at a whole-frame
/// tap; the output mixes the input with what the line gives. One stereo input and one stereo
/// output.
///
/// The time is free in milliseconds or a note value of the project tempo, and the right channel's
/// is offset. A change of time crossfades between the old and new taps over the ramp time. In
/// stereo mode each line feeds back into itself; in ping-pong the input, folded to mono, enters
/// the left line and each line feeds the other. The feedback passes an optional low-pass, then
/// optional `tanh` saturation; the mode and both options crossfade over the ramp time.
pub struct Delay {
    parameters: [Parameter; CONTROLS],
    /// The whole-note length the graph gave, in frames.
    frames_per_whole_note: Option<f64>,
    running: Option<Running>,
}

impl Delay {
    /// A delay at its defaults. Allocates.
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
        Ok(Delay {
            parameters,
            frames_per_whole_note: None,
            running: None,
        })
    }

    /// The current value of `control`.
    fn get(&self, control: Control) -> f32 {
        self.parameters[control as usize].get()
    }
}

impl Node for Delay {
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
        let rate = f64::from(sample_rate);
        let max_time = (rate * f64::from(MAX_DELAY_MS) / 1000.0).round().max(1.0);
        let max_offset = (rate * f64::from(MAX_OFFSET_MS) / 1000.0).ceil();
        let max_tap = to_tap(max_time + max_offset, usize::MAX);
        let frames_per_whole_note = self
            .frames_per_whole_note
            .filter(|f| f.is_finite() && *f > 0.0)
            .unwrap_or_else(|| default_whole_note(sample_rate));
        #[allow(clippy::cast_precision_loss)] // audio rates are exact in f32
        let sample_rate_f32 = sample_rate as f32;
        let ramp = |value| Ramp::new(value, sample_rate);
        let mut running = Running {
            sample_rate: sample_rate_f32,
            max_time,
            frames_per_whole_note,
            lines: [TapDelay::new(max_tap), TapDelay::new(max_tap)],
            taps: [Tap::new(1, sample_rate), Tap::new(1, sample_rate)],
            channels: [Channel::default(), Channel::default()],
            ping_pong: ramp(0.0),
            feedback: ramp(0.0),
            low_pass_share: ramp(0.0),
            low_pass_log: ramp(0.0),
            coefficients: SvfCoefficients::identity(),
            saturation: ramp(0.0),
            mix: ramp(0.0),
        };
        let parameters = &self.parameters;
        running.set(|control| parameters[control as usize].get());
        // Start at the values, with no ramp or crossfade in.
        running.taps = running.taps.map(|t| Tap::new(t.wanted, sample_rate));
        for ramp in [
            &mut running.ping_pong,
            &mut running.feedback,
            &mut running.low_pass_share,
            &mut running.low_pass_log,
            &mut running.saturation,
            &mut running.mix,
        ] {
            *ramp = Ramp::new(ramp.target(), sample_rate);
        }
        let hz = self.get(Control::LowPass);
        running.coefficients = SvfCoefficients::low_pass(hz, LOW_PASS_Q, sample_rate_f32);
        self.running = Some(running);
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let parameters = &self.parameters;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        running.set(|control| parameters[control as usize].get());
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
        Ok(Delay::new()?
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
                "sync",
                "time",
                "note",
                "feel",
                "mode",
                "offset",
                "feedback",
                "low-pass enabled",
                "low-pass",
                "saturation",
                "mix"
            ]
        );
        let groups: Vec<&str> = infos.iter().map(|i| i.group.as_str()).collect();
        assert_eq!(
            groups,
            [
                "time", "time", "time", "time", "stereo", "stereo", "feedback", "feedback",
                "feedback", "feedback", "output"
            ]
        );
        let time = &infos[Control::Time as usize];
        assert_eq!((time.min, time.max, time.default), (1.0, 4000.0, 375.0));
        assert_eq!(time.scale, Scale::Logarithmic);
        assert_eq!(infos[Control::Note as usize].default, 2.0);
        assert_eq!(infos[Control::Feedback as usize].default, 35.0);
        assert_eq!(infos[Control::Mix as usize].default, 30.0);
        assert!(infos.iter().all(|i| !i.stopped_only));
        Ok(())
    }

    #[test]
    fn test_synced_frames_follows_note_and_feel() {
        // 120 BPM in 4/4 at 48 kHz: a whole note is 96 000 frames.
        let whole = 96_000.0;
        assert_eq!(synced_frames(whole, 2.0, 0.0), 12_000.0);
        assert_eq!(synced_frames(whole, 0.0, 0.0), 3_000.0);
        assert_eq!(synced_frames(whole, 5.0, 0.0), 96_000.0);
        assert_eq!(synced_frames(whole, 2.0, 1.0), 18_000.0);
        assert_eq!(synced_frames(whole, 3.0, 2.0), 16_000.0);
        // Out-of-range indices are clamped.
        assert_eq!(synced_frames(whole, 9.0, 7.0), 64_000.0);
    }

    #[test]
    fn test_synced_frames_in_six_eight() {
        // 6/8 at 120 eighth notes per minute, 48 kHz: 24 000 frames a beat, beat unit 8.
        let whole = 24_000.0 * 8.0;
        assert_eq!(synced_frames(whole, 2.0, 0.0), 24_000.0);
        assert_eq!(synced_frames(whole, 2.0, 1.0), 36_000.0);
    }

    #[test]
    fn test_prepare_clamps_a_synced_time_to_four_seconds() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut delay = Delay::new()?;
        delay.parameters[Control::Sync as usize].set(1.0);
        delay.parameters[Control::Note as usize].set(5.0);
        delay.parameters[Control::Feel as usize].set(1.0);
        // 40 BPM in 4/4 at 48 kHz: a whole note is 288 000 frames, dotted 432 000.
        delay.set_tempo(288_000.0);
        delay.prepare(48_000, 64);
        let running = delay.running.as_ref().ok_or("not prepared")?;
        assert_eq!(running.taps[0].current, 192_000);
        assert_eq!(running.taps[1].current, 192_000);
        assert_eq!(running.lines[0].max_tap(), 194_400);
        Ok(())
    }

    #[test]
    fn test_prepare_without_tempo_uses_120_in_four_four() -> Result<(), Box<dyn std::error::Error>>
    {
        let mut delay = Delay::new()?;
        delay.parameters[Control::Sync as usize].set(1.0);
        delay.parameters[Control::Offset as usize].set(-10.0);
        delay.prepare(48_000, 64);
        let running = delay.running.as_ref().ok_or("not prepared")?;
        assert_eq!(running.taps[0].current, 12_000);
        assert_eq!(running.taps[1].current, 11_520);
        Ok(())
    }
}
