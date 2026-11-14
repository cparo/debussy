//! The chorus: 1 to 4 LFO-swept delay voices per channel, mixed with the input
//! (`docs/architecture/modulation.md` "Chorus", ADR-0120).

use audio_buffers::Layout;
use dsp::TapDelay;
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;
use crate::modulated_delay::{Clock, LfoControl, LfoValues, lfo_info, lines, ms_to_frames};

/// The most voices per channel.
pub const MAX_VOICES: usize = 4;

/// The longest base delay, in milliseconds.
const MAX_DELAY_MS: f32 = 30.0;

/// The deepest sweep, in milliseconds.
const MAX_DEPTH_MS: f32 = 10.0;

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
    Voices,
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
    Control::Voices,
    Control::Spread,
    Control::Mix,
];

/// What parameter `control` is: its saved name, range, default, unit, scale and group.
fn info(control: Control) -> ParameterInfo {
    let lfo = |c| lfo_info(c, 0.8, 4.0, 0.0);
    match control {
        Control::Sync => lfo(LfoControl::Sync),
        Control::Rate => lfo(LfoControl::Rate),
        Control::Note => lfo(LfoControl::Note),
        Control::Feel => lfo(LfoControl::Feel),
        Control::Shape => lfo(LfoControl::Shape),
        Control::Delay => ParameterInfo::new("delay", 5.0, MAX_DELAY_MS, 15.0)
            .with_unit("ms")
            .with_group("delay"),
        Control::Depth => ParameterInfo::new("depth", 0.0, MAX_DEPTH_MS, 3.0)
            .with_unit("ms")
            .with_group("delay"),
        Control::Voices => {
            #[allow(clippy::cast_precision_loss)] // a handful of voices
            let max = MAX_VOICES as f32;
            ParameterInfo::new("voices", 1.0, max, 2.0)
                .with_scale(Scale::Integer)
                .with_group("delay")
        }
        Control::Spread => ParameterInfo::new("spread", 0.0, 180.0, 90.0)
            .with_unit("°")
            .with_group("stereo"),
        Control::Mix => ParameterInfo::new("mix", 0.0, 100.0, 50.0)
            .with_unit("%")
            .with_group("output"),
    }
}

/// A prepared chorus: the lines, the LFO and the ramps.
struct Running {
    sample_rate: u32,
    clock: Clock,
    lines: [TapDelay; 2],
    /// The base delay, in frames.
    delay: Ramp,
    /// The sweep, in frames.
    depth: Ramp,
    /// The right channel's LFO offset, in cycles.
    spread: Ramp,
    /// Each voice's LFO offset, in cycles.
    offsets: [Ramp; MAX_VOICES],
    /// Each voice's gain: 1/√n for the first n voices, 0 for the rest.
    gains: [Ramp; MAX_VOICES],
    /// The wet share of the output, 0 to 1.
    mix: Ramp,
}

impl Running {
    /// A chorus at `sample_rate` for a whole note of `frames_per_whole_note` frames, its
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
            lines: lines(MAX_DELAY_MS + MAX_DEPTH_MS, sample_rate),
            delay: ramp(0.0),
            depth: ramp(0.0),
            spread: ramp(0.0),
            offsets: std::array::from_fn(|_| ramp(0.0)),
            gains: std::array::from_fn(|_| ramp(0.0)),
            mix: ramp(0.0),
        };
        running.set(&value);
        // Start at the values, with no ramp in.
        let snap = |r: &mut Ramp| *r = Ramp::new(r.target(), sample_rate);
        for r in [
            &mut running.delay,
            &mut running.depth,
            &mut running.spread,
            &mut running.mix,
        ] {
            snap(r);
        }
        running.offsets.iter_mut().for_each(snap);
        running.gains.iter_mut().for_each(snap);
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
        let rate = self.sample_rate;
        self.delay
            .set_target(ms_to_frames(value(Control::Delay), rate));
        self.depth
            .set_target(ms_to_frames(value(Control::Depth), rate));
        self.spread.set_target(value(Control::Spread) / 360.0);
        self.mix.set_target(value(Control::Mix) / 100.0);
        #[allow(clippy::cast_precision_loss)] // a handful of voices
        let most = MAX_VOICES as f32;
        let voices = value(Control::Voices).round().clamp(1.0, most);
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

    /// One frame of `left`, `right`.
    fn frame(&mut self, left: f32, right: f32) -> (f32, f32) {
        let delay = self.delay.next_value();
        let depth = self.depth.next_value();
        let spread = f64::from(self.spread.next_value());
        let mix = self.mix.next_value();
        let offsets = self.offsets.each_mut().map(|o| f64::from(o.next_value()));
        let gains = self.gains.each_mut().map(Ramp::next_value);
        let mut wet = [0.0_f32; 2];
        for (channel, ((w, line), lane)) in wet.iter_mut().zip(&self.lines).zip(0_u64..).enumerate()
        {
            let shift = if channel == 1 { spread } else { 0.0 };
            for (voice, (&offset, &gain)) in (0_u64..).zip(offsets.iter().zip(&gains)) {
                if gain == 0.0 {
                    continue;
                }
                let lfo = self
                    .clock
                    .lfo
                    .value_at_offset(offset + shift, lane * 4 + voice);
                *w += gain * line.read_cubic(delay + depth * 0.5 * (1.0 + lfo));
            }
        }
        self.lines[0].write(left);
        self.lines[1].write(right);
        self.clock.lfo.advance();
        let dry = 1.0 - mix;
        (left * dry + wet[0] * mix, right * dry + wet[1] * mix)
    }
}

/// A chorus: each channel's input written into a line read by 1 to 4 voices, each at a delay one
/// LFO sweeps from `delay` up to `delay + depth`, the voices at evenly spaced LFO phases and the
/// right channel `spread` degrees from the left. The output mixes the input with the voices'
/// sum × 1/√n. One stereo input and one stereo output.
pub struct Chorus {
    parameters: [Parameter; CONTROLS],
    /// The whole-note length the graph gave, in frames.
    frames_per_whole_note: Option<f64>,
    running: Option<Running>,
}

impl Chorus {
    /// A chorus at its defaults. Allocates.
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
        Ok(Chorus {
            parameters,
            frames_per_whole_note: None,
            running: None,
        })
    }
}

impl Node for Chorus {
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
        Ok(Chorus::new()?
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
        let rate = &infos[Control::Rate as usize];
        assert_eq!((rate.min, rate.max, rate.default), (0.05, 5.0, 0.8));
        assert_eq!(rate.scale, Scale::Logarithmic);
        assert_eq!(infos[Control::Voices as usize].scale, Scale::Integer);
        assert_eq!(infos[Control::Voices as usize].default, 2.0);
        assert_eq!(infos[Control::Note as usize].default, 4.0);
        assert!(infos.iter().all(|i| !i.stopped_only));
        Ok(())
    }

    #[test]
    fn test_prepare_with_sync_follows_tempo() -> Result<(), Box<dyn std::error::Error>> {
        let mut chorus = Chorus::new()?;
        chorus.parameters[Control::Sync as usize].set(1.0);
        chorus.parameters[Control::Note as usize].set(5.0);
        // 60 BPM in 4/4 at 48 kHz: a whole note is 192 000 frames; 2/1 is eight seconds.
        chorus.set_tempo(192_000.0);
        chorus.prepare(48_000, 64);
        let running = chorus.running.as_ref().ok_or("not prepared")?;
        assert_eq!(running.clock.lfo.frequency(), 0.125);
        Ok(())
    }

    #[test]
    fn test_set_voices_spaces_offsets_and_gains() -> Result<(), Box<dyn std::error::Error>> {
        let mut chorus = Chorus::new()?;
        chorus.parameters[Control::Voices as usize].set(4.0);
        chorus.prepare(48_000, 64);
        let running = chorus.running.as_ref().ok_or("not prepared")?;
        let offsets: Vec<f32> = running.offsets.iter().map(Ramp::current).collect();
        assert_eq!(offsets, [0.0, 0.25, 0.5, 0.75]);
        assert!(running.gains.iter().all(|g| g.current() == 0.5));
        Ok(())
    }
}
