//! The mixer: mono channels, each with gain, pan and a mute/solo mode, summed to one stereo
//! output (`docs/architecture/signal-graph.md` "Mixer").

use audio_buffers::Layout;
use dsp::{db_to_gain, pan_gains};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale, Smoothed};

use crate::error::Error;

/// The most channels a mixer can have.
pub const MAX_CHANNELS: usize = 128;

/// The lowest channel gain, in dB; a channel at it is silent.
pub const SILENCE_DB: f32 = -96.0;

/// The highest channel gain, in dB.
pub const MAX_GAIN_DB: f32 = 12.0;

/// One of a channel's controls, each a parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Gain in dB, [`SILENCE_DB`] to [`MAX_GAIN_DB`], default 0.
    Gain,
    /// Pan position, −1 (left) to 1 (right), default 0.
    Pan,
    /// The [`Mode`], as its [`Mode::value`].
    Mode,
}

/// Whether a channel plays, is muted or is soloed. One parameter holds it, so a channel is never
/// both muted and soloed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Plays unless another channel of the mixer is soloed.
    Play,
    /// Silent.
    Mute,
    /// Plays; while any channel is soloed, only soloed channels play.
    Solo,
}

impl Mode {
    /// The parameter value for this mode: 0, 1 or 2.
    pub fn value(self) -> f32 {
        match self {
            Mode::Play => 0.0,
            Mode::Mute => 1.0,
            Mode::Solo => 2.0,
        }
    }

    /// The mode a parameter value means, rounded to the nearest mode.
    pub fn from_value(value: f32) -> Mode {
        if value >= 1.5 {
            Mode::Solo
        } else if value >= 0.5 {
            Mode::Mute
        } else {
            Mode::Play
        }
    }
}

/// The saved name of `control` on channel `channel` (0-based; names count from 1), such as
/// `"channel 1 gain"`. Names are part of the saved format.
pub fn parameter_name(channel: usize, control: Control) -> String {
    let control = match control {
        Control::Gain => "gain",
        Control::Pan => "pan",
        Control::Mode => "mode",
    };
    format!("channel {} {control}", channel + 1)
}

/// The parameters of a mixer of `channels` channels, in the order a running [`Mixer`] declares
/// them: each channel's gain, pan and mode. Allocates.
pub fn mixer_parameter_infos(channels: usize) -> Vec<ParameterInfo> {
    (0..channels).flat_map(channel_infos).collect()
}

/// Channel `index`'s gain, pan and mode parameters (0-based).
fn channel_infos(index: usize) -> [ParameterInfo; 3] {
    let group = format!("channel {}", index + 1);
    let parameter = |control, min, max| {
        ParameterInfo::new(parameter_name(index, control), min, max, 0.0).with_group(&group)
    };
    let modes = ["play", "mute", "solo"].map(String::from).to_vec();
    [
        parameter(Control::Gain, SILENCE_DB, MAX_GAIN_DB)
            .with_unit("dB")
            .with_step(1.0),
        parameter(Control::Pan, -1.0, 1.0).with_step(0.01),
        parameter(Control::Mode, 0.0, 2.0).with_scale(Scale::Choice(modes)),
    ]
}

/// One channel's parameters and the state that ramps them.
struct Channel {
    gain: Parameter,
    pan: Parameter,
    mode: Parameter,
    /// The linear gain, including mute and solo, as ramped. Set in `prepare`.
    level: Option<Ramp>,
    /// The pan position, as ramped. Set in `prepare`.
    pan_smoothed: Option<Smoothed>,
}

impl Channel {
    fn new(index: usize) -> Result<Self, Error> {
        let [gain, pan, mode] = channel_infos(index);
        Ok(Channel {
            gain: Parameter::new(gain)?,
            pan: Parameter::new(pan)?,
            mode: Parameter::new(mode)?,
            level: None,
            pan_smoothed: None,
        })
    }

    fn mode(&self) -> Mode {
        Mode::from_value(self.mode.get())
    }

    /// The linear gain the channel's level ramps to, with mute and solo applied.
    fn target(&self, any_solo: bool) -> f32 {
        let audible = match self.mode() {
            Mode::Solo => true,
            Mode::Play => !any_solo,
            Mode::Mute => false,
        };
        let db = self.gain.get();
        if !audible || db <= SILENCE_DB {
            0.0
        } else {
            db_to_gain(db)
        }
    }
}

/// A mixer node: `channels` mono inputs, each through its own gain, pan and [`Mode`], summed to
/// one stereo output. Input port `i` is channel `i`. Gain is applied in dB and panned with the
/// −3 dB constant-power law; every control ramps over `signal_graph::RAMP_SECONDS`.
///
/// A mixer of more channels is a new `Mixer` put in with `Graph::replace_node`, which keeps the
/// existing channels' connections and values.
pub struct Mixer {
    channels: Vec<Channel>,
}

impl Mixer {
    /// A mixer of `channels` channels, each at 0 dB, centred and playing. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::ChannelCount`] if `channels` is 0 or above [`MAX_CHANNELS`].
    pub fn new(channels: usize) -> Result<Self, Error> {
        if channels == 0 || channels > MAX_CHANNELS {
            return Err(Error::ChannelCount(channels));
        }
        let channels = (0..channels)
            .map(Channel::new)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Mixer { channels })
    }

    /// The number of channels.
    pub fn channels(&self) -> usize {
        self.channels.len()
    }
}

impl Node for Mixer {
    fn inputs(&self) -> Vec<Port> {
        (1..=self.channels.len())
            .map(|n| Port::audio(&format!("channel {n}"), Layout::Mono))
            .collect()
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("main", Layout::Stereo)]
    }

    fn parameters(&self) -> Vec<Parameter> {
        self.channels
            .iter()
            .flat_map(|c| [c.gain.clone(), c.pan.clone(), c.mode.clone()])
            .collect()
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        let any_solo = self.channels.iter().any(|c| c.mode() == Mode::Solo);
        for channel in &mut self.channels {
            channel.level = Some(Ramp::new(channel.target(any_solo), sample_rate));
            channel.pan_smoothed = Some(Smoothed::new(channel.pan.clone(), sample_rate));
        }
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        left.fill(0.0);
        right.fill(0.0);
        let any_solo = self.channels.iter().any(|c| c.mode() == Mode::Solo);
        for (index, channel) in self.channels.iter_mut().enumerate() {
            let target = channel.target(any_solo);
            let (Some(level), Some(pan), Some(input)) = (
                channel.level.as_mut(),
                channel.pan_smoothed.as_mut(),
                inputs.channel(index, 0),
            ) else {
                continue;
            };
            level.set_target(target);
            pan.update();
            if !level.is_ramping() && level.current() == 0.0 {
                continue;
            }
            let mut gains = pan_gains(pan.current());
            for ((&sample, l), r) in input.iter().zip(left.iter_mut()).zip(right.iter_mut()) {
                if pan.is_ramping() {
                    gains = pan_gains(pan.next_value());
                }
                let sample = sample * level.next_value();
                *l += sample * gains.0;
                *r += sample * gains.1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_refuses_bad_channel_counts() {
        assert_eq!(Mixer::new(0).err(), Some(Error::ChannelCount(0)));
        assert!(Mixer::new(MAX_CHANNELS + 1).is_err());
        assert!(Mixer::new(MAX_CHANNELS).is_ok());
    }

    #[test]
    fn test_new_declares_ports_and_parameters() -> Result<(), Error> {
        let mixer = Mixer::new(2)?;
        let inputs = mixer.inputs();
        assert_eq!(inputs.len(), 2);
        assert_eq!(inputs[1], Port::audio("channel 2", Layout::Mono));
        assert_eq!(mixer.outputs(), vec![Port::audio("main", Layout::Stereo)]);
        let names: Vec<String> = mixer
            .parameters()
            .iter()
            .map(|p| p.info().name.clone())
            .collect();
        assert_eq!(
            names,
            [
                "channel 1 gain",
                "channel 1 pan",
                "channel 1 mode",
                "channel 2 gain",
                "channel 2 pan",
                "channel 2 mode",
            ]
        );
        let gain = mixer.parameters()[0].info().clone();
        assert_eq!((gain.min, gain.max, gain.default), (-96.0, 12.0, 0.0));
        assert_eq!(
            (gain.unit.as_str(), gain.group.as_str()),
            ("dB", "channel 1")
        );
        let mode = mixer.parameters()[2].info().clone();
        assert!(matches!(mode.scale, Scale::Choice(ref o) if o.len() == 3));
        Ok(())
    }

    #[test]
    fn test_channel_infos_declare_whole_db_gain_and_hundredth_pan_steps() {
        let [gain, pan, mode] = channel_infos(0);
        assert_eq!(gain.step, Some(1.0));
        assert_eq!(pan.step, Some(0.01));
        assert_eq!(mode.step, None);
    }

    #[test]
    fn test_mixer_parameter_infos_match_a_running_mixer() -> Result<(), Error> {
        for channels in [1, 3, MAX_CHANNELS] {
            let running: Vec<ParameterInfo> = Mixer::new(channels)?
                .parameters()
                .iter()
                .map(|p| p.info().clone())
                .collect();
            assert_eq!(mixer_parameter_infos(channels), running);
        }
        assert!(mixer_parameter_infos(0).is_empty());
        Ok(())
    }

    #[test]
    fn test_mode_from_value_rounds() {
        assert_eq!(Mode::from_value(0.0), Mode::Play);
        assert_eq!(Mode::from_value(0.49), Mode::Play);
        assert_eq!(Mode::from_value(1.4), Mode::Mute);
        assert_eq!(Mode::from_value(1.5), Mode::Solo);
        assert_eq!(Mode::from_value(2.0), Mode::Solo);
        for mode in [Mode::Play, Mode::Mute, Mode::Solo] {
            assert_eq!(Mode::from_value(mode.value()), mode);
        }
    }

    #[test]
    fn test_channel_target_applies_mute_solo_and_silence() -> Result<(), Error> {
        let channel = Channel::new(0)?;
        assert_eq!(channel.target(false), 1.0);
        assert_eq!(channel.target(true), 0.0);
        channel.mode.set(Mode::Solo.value());
        assert_eq!(channel.target(true), 1.0);
        channel.mode.set(Mode::Mute.value());
        assert_eq!(channel.target(false), 0.0);
        channel.mode.set(Mode::Play.value());
        channel.gain.set(SILENCE_DB);
        assert_eq!(channel.target(false), 0.0);
        channel.gain.set(MAX_GAIN_DB);
        assert!((channel.target(false) - db_to_gain(12.0)).abs() < 1e-6);
        Ok(())
    }
}
