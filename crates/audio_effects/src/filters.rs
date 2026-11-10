//! The Butterworth high-pass and low-pass filters (`docs/architecture/effects.md` "High-pass and
//! low-pass filters").

use audio_buffers::Layout;
use dsp::{OnePole, OnePoleCoefficients, Svf, SvfCoefficients, butterworth_q, prewarp};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::error::Error;

/// The lowest cutoff, in Hz.
pub const MIN_CUTOFF: f32 = 20.0;

/// The highest cutoff, in Hz.
pub const MAX_CUTOFF: f32 = 20_000.0;

/// The highest order: 8 poles, 48 dB per octave.
pub const MAX_ORDER: usize = 8;

/// The order a new filter starts at: 2 poles, 12 dB per octave.
const DEFAULT_ORDER: f32 = 2.0;

/// The two-pole sections of the highest order.
const SECTIONS: usize = MAX_ORDER / 2;

/// Which side of the cutoff a [`Butterworth`] passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    /// Passes what is above the cutoff. Kind id `"high-pass"`; default cutoff 80 Hz.
    HighPass,
    /// Passes what is below the cutoff. Kind id `"low-pass"`; default cutoff 12 kHz.
    LowPass,
}

impl Response {
    /// The cutoff a new filter starts at, in Hz.
    fn default_cutoff(self) -> f32 {
        match self {
            Response::HighPass => 80.0,
            Response::LowPass => 12_000.0,
        }
    }
}

/// One channel's filter state: a two-pole section per pole pair and a one-pole for an odd order.
#[derive(Debug, Clone, Copy, Default)]
struct Cascade {
    sections: [Svf; SECTIONS],
    one_pole: OnePole,
}

/// The state that exists once the filter is prepared.
struct Running {
    sample_rate: f32,
    /// The cutoff in log2(Hz), as ramped.
    cutoff: Ramp,
    /// The order the coefficients are for.
    order: usize,
    sections: [SvfCoefficients; SECTIONS],
    one_pole: OnePoleCoefficients,
    /// Left and right.
    channels: [Cascade; 2],
}

/// A Butterworth filter: maximally flat, with no ripple, falling 6 dB per octave per pole beyond
/// its cutoff, where it is 3 dB down. One stereo input and one stereo output.
///
/// Its parameters are `cutoff` (Hz, logarithmic, ramped in log frequency) and `order` (1 to
/// [`MAX_ORDER`] poles). The order can change only while playback is stopped, so a change takes
/// effect at once, from rest.
pub struct Butterworth {
    response: Response,
    cutoff: Parameter,
    order: Parameter,
    running: Option<Running>,
}

impl Butterworth {
    /// A filter passing `response`'s side of its default cutoff, of order 2. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::Parameter`] if a parameter cannot be made, which does not happen for the fixed
    /// ranges here.
    pub fn new(response: Response) -> Result<Self, Error> {
        let cutoff = Parameter::new(
            ParameterInfo::new("cutoff", MIN_CUTOFF, MAX_CUTOFF, response.default_cutoff())
                .with_unit("Hz")
                .with_scale(Scale::Logarithmic),
        )?;
        #[allow(clippy::cast_precision_loss)] // a small order
        let order = Parameter::new(
            ParameterInfo::new("order", 1.0, MAX_ORDER as f32, DEFAULT_ORDER)
                .with_scale(Scale::Integer)
                .stopped_only(),
        )?;
        Ok(Butterworth {
            response,
            cutoff,
            order,
            running: None,
        })
    }

    /// The order the parameter holds, from 1 to [`MAX_ORDER`].
    fn order(&self) -> usize {
        // The parameter holds whole numbers from 1 to 8.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let order = self.order.get() as usize;
        order.clamp(1, MAX_ORDER)
    }
}

impl Running {
    /// Sets the coefficients for `cutoff` (Hz) at the current order.
    fn design(&mut self, response: Response, cutoff: f32) {
        let g = prewarp(cutoff, self.sample_rate);
        for (section, coefficients) in self.sections.iter_mut().enumerate() {
            let Some(q) = butterworth_q(self.order, section) else {
                break;
            };
            let k = 1.0 / q;
            *coefficients = match response {
                Response::HighPass => SvfCoefficients::new(g, k, 1.0, -k, -1.0),
                Response::LowPass => SvfCoefficients::new(g, k, 0.0, 0.0, 1.0),
            };
        }
        self.one_pole = match response {
            Response::HighPass => OnePoleCoefficients::high_pass_prewarped(g),
            Response::LowPass => OnePoleCoefficients::low_pass_prewarped(g),
        };
    }

    /// Filters one sample of channel `channel`.
    fn process(&mut self, channel: usize, input: f32) -> f32 {
        let cascade = &mut self.channels[channel];
        let mut sample = input;
        for (svf, coefficients) in cascade
            .sections
            .iter_mut()
            .zip(&self.sections)
            .take(self.order / 2)
        {
            sample = svf.process(sample, coefficients);
        }
        if self.order % 2 == 1 {
            sample = cascade.one_pole.process(sample, &self.one_pole);
        }
        sample
    }
}

impl Node for Butterworth {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", Layout::Stereo)]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Stereo)]
    }

    fn parameters(&self) -> Vec<Parameter> {
        vec![self.cutoff.clone(), self.order.clone()]
    }

    fn prepare(&mut self, sample_rate: u32, _max_frames: usize) {
        let cutoff = self.cutoff.get();
        // Sample rates are exact in `f32` well past any audio rate.
        #[allow(clippy::cast_precision_loss)]
        let mut running = Running {
            sample_rate: sample_rate as f32,
            cutoff: Ramp::new(cutoff.log2(), sample_rate),
            order: self.order(),
            sections: [SvfCoefficients::identity(); SECTIONS],
            one_pole: OnePoleCoefficients::low_pass_prewarped(1.0),
            channels: [Cascade::default(); 2],
        };
        running.design(self.response, cutoff);
        self.running = Some(running);
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let order = self.order();
        let cutoff = self.cutoff.get();
        let response = self.response;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        if order != running.order {
            running.order = order;
            running.channels = [Cascade::default(); 2];
            running.design(response, running.cutoff.current().exp2());
        }
        running.cutoff.set_target(cutoff.log2());
        let (in_left, in_right) = (inputs.channel(0, 0), inputs.channel(0, 1));
        let sample = |input: Option<&[f32]>, i: usize| input.and_then(|c| c.get(i)).copied();
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            if running.cutoff.is_ramping() {
                let cutoff = running.cutoff.next_value().exp2();
                running.design(response, cutoff);
            }
            *l = running.process(0, sample(in_left, i).unwrap_or(0.0));
            *r = running.process(1, sample(in_right, i).unwrap_or(0.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_declares_ports_and_parameters() -> Result<(), Error> {
        for (response, cutoff) in [(Response::HighPass, 80.0), (Response::LowPass, 12_000.0)] {
            let filter = Butterworth::new(response)?;
            assert_eq!(filter.inputs(), vec![Port::audio("in", Layout::Stereo)]);
            assert_eq!(filter.outputs(), vec![Port::audio("out", Layout::Stereo)]);
            let infos: Vec<ParameterInfo> = filter
                .parameters()
                .iter()
                .map(|p| p.info().clone())
                .collect();
            assert_eq!(infos[0].name, "cutoff");
            assert_eq!(
                (infos[0].min, infos[0].max, infos[0].default),
                (20.0, 20_000.0, cutoff)
            );
            assert_eq!(infos[0].scale, Scale::Logarithmic);
            assert_eq!(infos[0].unit, "Hz");
            assert!(!infos[0].stopped_only);
            assert_eq!(infos[1].name, "order");
            assert_eq!(
                (infos[1].min, infos[1].max, infos[1].default),
                (1.0, 8.0, 2.0)
            );
            assert_eq!(infos[1].scale, Scale::Integer);
            assert!(infos[1].stopped_only);
        }
        Ok(())
    }

    #[test]
    fn test_design_uses_one_pole_only_for_odd_orders() -> Result<(), Box<dyn std::error::Error>> {
        let mut filter = Butterworth::new(Response::LowPass)?;
        filter.order.set(1.0);
        filter.prepare(48_000, 64);
        let running = filter.running.as_mut().ok_or("not prepared")?;
        // Order 1 is the one-pole alone: its DC gain is 1 and it has no sections in use.
        let mut sum = 0.0;
        for _ in 0..48_000 {
            sum = running.process(0, 1.0);
        }
        assert!((sum - 1.0).abs() < 1e-4);
        Ok(())
    }
}
