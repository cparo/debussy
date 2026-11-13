//! The Butterworth high-pass and low-pass filters (`docs/architecture/effects.md` "High-pass and
//! low-pass filters").

use audio_buffers::Layout;
use dsp::{OnePole, OnePoleCoefficients, Svf, SvfCoefficients, butterworth_q, prewarp};
use signal_graph::{Inputs, Node, Outputs, Parameter, ParameterInfo, Port, Ramp, Scale};

use crate::curve::Curve;
use crate::error::Error;
use crate::response::{FrequencyResponse, butterworth_stages};

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
struct Channel {
    sections: [Svf; SECTIONS],
    one_pole: OnePole,
}

/// A prepared stereo Butterworth cascade: the ramped cutoff, the order, the coefficients for them
/// and each channel's state. Shared by [`Butterworth`] and the parametric equaliser's high-pass
/// and low-pass stages. Never allocates.
pub(crate) struct Cascade {
    sample_rate: f32,
    /// The cutoff in log2(Hz), as ramped.
    cutoff: Ramp,
    /// The order the coefficients are for.
    order: usize,
    sections: [SvfCoefficients; SECTIONS],
    one_pole: OnePoleCoefficients,
    /// Left and right.
    channels: [Channel; 2],
}

/// The `cutoff` parameter of a filter passing `response`'s side, named `name`: 20 Hz to 20 kHz,
/// logarithmic, starting at the response's default.
pub(crate) fn cutoff_info(name: impl Into<String>, response: Response) -> ParameterInfo {
    ParameterInfo::new(name, MIN_CUTOFF, MAX_CUTOFF, response.default_cutoff())
        .with_unit("Hz")
        .with_scale(Scale::Logarithmic)
}

/// The `order` parameter of a filter, named `name`: 1 to [`MAX_ORDER`] poles, whole numbers,
/// changeable only while stopped, starting at 2.
pub(crate) fn order_info(name: impl Into<String>) -> ParameterInfo {
    #[allow(clippy::cast_precision_loss)] // a small order
    let max = MAX_ORDER as f32;
    ParameterInfo::new(name, 1.0, max, DEFAULT_ORDER)
        .with_scale(Scale::Integer)
        .stopped_only()
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
    running: Option<Cascade>,
}

impl Butterworth {
    /// A filter passing `response`'s side of its default cutoff, of order 2. Allocates.
    ///
    /// # Errors
    ///
    /// [`Error::Parameter`] if a parameter cannot be made, which does not happen for the fixed
    /// ranges here.
    pub fn new(response: Response) -> Result<Self, Error> {
        Ok(Butterworth {
            response,
            cutoff: Parameter::new(cutoff_info("cutoff", response))?,
            order: Parameter::new(order_info("order"))?,
            running: None,
        })
    }
}

/// The curve of a filter passing `response`'s side whose parameters stand at `values`, in the
/// order [`Butterworth`] declares them (cutoff, order).
fn curve(response: Response, values: &[f32], sample_rate: u32) -> Option<Curve> {
    let &[cutoff, order] = <&[f32; 2]>::try_from(values).ok()?;
    let stages = butterworth_stages(response, cutoff, order, sample_rate);
    Some(Curve::Response(FrequencyResponse::new(sample_rate, stages)))
}

/// The curve of a `high-pass` effect whose parameters stand at `values` (see [`crate::CurveFn`]).
pub(crate) fn high_pass_curve(values: &[f32], sample_rate: u32, _whole_note: f64) -> Option<Curve> {
    curve(Response::HighPass, values, sample_rate)
}

/// The curve of a `low-pass` effect whose parameters stand at `values` (see [`crate::CurveFn`]).
pub(crate) fn low_pass_curve(values: &[f32], sample_rate: u32, _whole_note: f64) -> Option<Curve> {
    curve(Response::LowPass, values, sample_rate)
}

/// The coefficients of an `order`-pole Butterworth filter passing `response`'s side of `cutoff`
/// (Hz) at `sample_rate`: one two-pole section per pole pair (the first `order / 2` are used;
/// the rest are left as the identity) and, used only for an odd order, a one-pole. Shared by
/// processing and the frequency response, so the two cannot differ.
pub(crate) fn design(
    response: Response,
    order: usize,
    cutoff: f32,
    sample_rate: f32,
) -> ([SvfCoefficients; SECTIONS], OnePoleCoefficients) {
    let g = prewarp(cutoff, sample_rate);
    let mut sections = [SvfCoefficients::identity(); SECTIONS];
    for (section, coefficients) in sections.iter_mut().enumerate() {
        let Some(q) = butterworth_q(order, section) else {
            break;
        };
        let k = 1.0 / q;
        *coefficients = match response {
            Response::HighPass => SvfCoefficients::new(g, k, 1.0, -k, -1.0),
            Response::LowPass => SvfCoefficients::new(g, k, 0.0, 0.0, 1.0),
        };
    }
    let one_pole = match response {
        Response::HighPass => OnePoleCoefficients::high_pass_prewarped(g),
        Response::LowPass => OnePoleCoefficients::low_pass_prewarped(g),
    };
    (sections, one_pole)
}

/// The order an `order` parameter value means: rounded down, from 1 to [`MAX_ORDER`].
pub(crate) fn order_from_value(value: f32) -> usize {
    // `as` saturates, and NaN becomes 0, so the clamp sees a whole number.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let order = value as usize;
    order.clamp(1, MAX_ORDER)
}

impl Cascade {
    /// A cascade passing `response`'s side of `cutoff` (Hz) with an `order` parameter value, at
    /// rest, at `sample_rate`.
    pub(crate) fn new(response: Response, cutoff: f32, order: f32, sample_rate: u32) -> Self {
        // Sample rates are exact in `f32` well past any audio rate.
        #[allow(clippy::cast_precision_loss)]
        let mut cascade = Cascade {
            sample_rate: sample_rate as f32,
            cutoff: Ramp::new(cutoff.log2(), sample_rate),
            order: order_from_value(order),
            sections: [SvfCoefficients::identity(); SECTIONS],
            one_pole: OnePoleCoefficients::low_pass_prewarped(1.0),
            channels: [Channel::default(); 2],
        };
        cascade.design(response, cutoff);
        cascade
    }

    /// Takes new parameter values, once per block: a cutoff (Hz) to ramp to and an `order`
    /// parameter value. A changed order takes effect at once, from rest.
    pub(crate) fn set(&mut self, response: Response, cutoff: f32, order: f32) {
        let order = order_from_value(order);
        if order != self.order {
            self.order = order;
            self.reset();
            self.design(response, self.cutoff.current().exp2());
        }
        self.cutoff.set_target(cutoff.log2());
    }

    /// Steps the cutoff ramp one sample, redesigning while it moves.
    pub(crate) fn tick(&mut self, response: Response) {
        if self.cutoff.is_ramping() {
            let cutoff = self.cutoff.next_value().exp2();
            self.design(response, cutoff);
        }
    }

    /// Clears both channels' state, so the cascade next runs from silence.
    pub(crate) fn reset(&mut self) {
        self.channels = [Channel::default(); 2];
    }

    /// Sets the coefficients for `cutoff` (Hz) at the current order.
    fn design(&mut self, response: Response, cutoff: f32) {
        (self.sections, self.one_pole) = design(response, self.order, cutoff, self.sample_rate);
    }

    /// Filters one sample of channel `channel`.
    pub(crate) fn process(&mut self, channel: usize, input: f32) -> f32 {
        let state = &mut self.channels[channel];
        let mut sample = input;
        for (svf, coefficients) in state
            .sections
            .iter_mut()
            .zip(&self.sections)
            .take(self.order / 2)
        {
            sample = svf.process(sample, coefficients);
        }
        if self.order % 2 == 1 {
            sample = state.one_pole.process(sample, &self.one_pole);
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
        self.running = Some(Cascade::new(
            self.response,
            self.cutoff.get(),
            self.order.get(),
            sample_rate,
        ));
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let response = self.response;
        let Some(running) = self.running.as_mut() else {
            return;
        };
        let Some((left, right)) = outputs.stereo_mut(0) else {
            return;
        };
        running.set(response, self.cutoff.get(), self.order.get());
        let (in_left, in_right) = (inputs.channel(0, 0), inputs.channel(0, 1));
        let sample = |input: Option<&[f32]>, i: usize| input.and_then(|c| c.get(i)).copied();
        for (i, (l, r)) in left.iter_mut().zip(right.iter_mut()).enumerate() {
            running.tick(response);
            *l = running.process(0, sample(in_left, i).unwrap_or(0.0));
            *r = running.process(1, sample(in_right, i).unwrap_or(0.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_curve_is_3db_down_at_the_cutoff_and_slopes_by_order()
    -> Result<(), Box<dyn std::error::Error>> {
        for (kind, cutoff, far) in [("low-pass", 200.0, 1_600.0), ("high-pass", 2_000.0, 250.0)] {
            for order in 1..=8 {
                #[allow(clippy::cast_precision_loss)] // a small order
                let values = [cutoff, order as f32];
                let Some(Curve::Response(response)) = crate::curve(kind, &values, 48_000, 96_000.0)
                else {
                    return Err("no response".into());
                };
                let at = response.magnitude_db(cutoff).ok_or("above Nyquist")?;
                assert!((at + 3.0103).abs() < 0.01, "{kind} {order}: {at}");
                // Three octaves from the cutoff, an octave's fall is close to 6 dB per pole.
                let a = response.magnitude_db(far).ok_or("above Nyquist")?;
                let b = response
                    .magnitude_db(if far > cutoff { far * 2.0 } else { far / 2.0 })
                    .ok_or("above Nyquist")?;
                #[allow(clippy::cast_precision_loss)]
                let expected = 6.02 * order as f32;
                assert!(((a - b) / expected - 1.0).abs() < 0.12, "{kind} {order}");
            }
        }
        Ok(())
    }

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
