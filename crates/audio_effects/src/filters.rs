//! The Butterworth high-pass and low-pass cascade behind the equaliser's pass stages
//! (`docs/architecture/effects.md` "Equalizer") and the amplifier's DC filter.

use dsp::{OnePole, OnePoleCoefficients, Svf, SvfCoefficients, butterworth_q, prewarp};
use signal_graph::{ParameterInfo, Ramp, Reading, Scale};

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

/// Which side of the cutoff a Butterworth cascade passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Response {
    /// Passes what is above the cutoff; default cutoff 80 Hz.
    HighPass,
    /// Passes what is below the cutoff; default cutoff 12 kHz.
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
/// and each channel's state: maximally flat, falling 6 dB per octave per pole beyond the cutoff,
/// where it is 3 dB down. Shared by the equaliser's high-pass and low-pass stages and the
/// amplifier's DC filter. Never allocates.
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

    /// Takes new parameter values, once per block: a cutoff (Hz) to ramp to, by the next control
    /// point while automated, and an `order` parameter value. A changed order takes effect at
    /// once, from rest.
    pub(crate) fn set(&mut self, response: Response, cutoff: Reading, order: f32) {
        let order = order_from_value(order);
        if order != self.order {
            self.order = order;
            self.reset();
            self.design(response, self.cutoff.current().exp2());
        }
        self.cutoff.approach(cutoff.map(f32::log2));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{assert_lands, automated};

    #[test]
    fn test_set_lands_an_automated_cutoff_on_the_control_point() {
        let mut cascade = Cascade::new(Response::LowPass, 1_000.0, 2.0, 48_000);
        cascade.set(Response::LowPass, automated(4_000.0), 2.0);
        assert_lands(&mut [("cutoff", &mut cascade.cutoff)]);
    }

    #[test]
    fn test_process_uses_one_pole_only_for_odd_orders() {
        // Order 1 is the one-pole alone: its DC gain is 1 and it has no sections in use.
        let mut cascade = Cascade::new(Response::LowPass, 1_000.0, 1.0, 48_000);
        let mut sum = 0.0;
        for _ in 0..48_000 {
            sum = cascade.process(0, 1.0);
        }
        assert!((sum - 1.0).abs() < 1e-4);
    }
}
