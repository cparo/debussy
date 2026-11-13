//! The magnitude response an equaliser's parameters ask for, for display
//! (`docs/architecture/effects.md` "Frequency response"), and the stages it is built from.

use dsp::{OnePoleCoefficients, SvfCoefficients};

use crate::filters::{self, Response};

/// One stage of an effect's filter chain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Stage {
    /// A two-pole section.
    Svf(SvfCoefficients),
    /// A one-pole section.
    OnePole(OnePoleCoefficients),
}

impl Stage {
    /// The stage's linear gain at `frequency`, or `None` from the Nyquist frequency up.
    fn magnitude(&self, frequency: f32, sample_rate: f32) -> Option<f32> {
        match self {
            Stage::Svf(c) => c.magnitude(frequency, sample_rate),
            Stage::OnePole(c) => c.magnitude(frequency, sample_rate),
        }
    }
}

/// The steady-state magnitude response of an effect at given parameter values: the filters it
/// runs in series, designed as processing designs them. Ramps and morphs are not included; this
/// is the response they settle on.
#[derive(Debug, Clone, PartialEq)]
pub struct FrequencyResponse {
    sample_rate: f32,
    stages: Vec<Stage>,
}

impl FrequencyResponse {
    /// The gain in dB at `frequency` (Hz): the sum of every stage's. `None` from the Nyquist
    /// frequency up. A stage with no gain at all (a notch's centre) gives negative infinity.
    pub fn magnitude_db(&self, frequency: f32) -> Option<f32> {
        self.stages.iter().try_fold(0.0, |sum, stage| {
            let gain = stage.magnitude(frequency, self.sample_rate)?;
            Some(sum + 20.0 * gain.log10())
        })
    }

    /// The frequencies (Hz), below the Nyquist frequency, at which the gain is zero: the
    /// centres of notches. A plot sampling the response includes them, or it misses their
    /// depth.
    pub fn zeros(&self) -> impl Iterator<Item = f32> + '_ {
        self.stages.iter().filter_map(|stage| match stage {
            Stage::Svf(c) => c.zero_frequency(self.sample_rate),
            Stage::OnePole(_) => None,
        })
    }
}

impl FrequencyResponse {
    /// The response of `stages` in series at `sample_rate` Hz.
    pub(crate) fn new(sample_rate: u32, stages: Vec<Stage>) -> FrequencyResponse {
        // Sample rates are exact in `f32` well past any audio rate.
        #[allow(clippy::cast_precision_loss)]
        let sample_rate = sample_rate as f32;
        FrequencyResponse {
            sample_rate,
            stages,
        }
    }
}

/// The stages of a Butterworth filter passing `response`'s side of `cutoff` (Hz), with an
/// `order` parameter value, designed as processing designs them.
pub(crate) fn butterworth_stages(
    response: Response,
    cutoff: f32,
    order: f32,
    sample_rate: u32,
) -> Vec<Stage> {
    // Sample rates are exact in `f32` well past any audio rate.
    #[allow(clippy::cast_precision_loss)]
    let rate = sample_rate as f32;
    let order = filters::order_from_value(order);
    let (sections, one_pole) = filters::design(response, order, cutoff, rate);
    let mut stages: Vec<Stage> = sections[..order / 2]
        .iter()
        .map(|&c| Stage::Svf(c))
        .collect();
    if order % 2 == 1 {
        stages.push(Stage::OnePole(one_pole));
    }
    stages
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_magnitude_db_sums_stages_and_stops_at_nyquist() {
        let one = FrequencyResponse::new(
            48_000,
            butterworth_stages(Response::LowPass, 1_000.0, 2.0, 48_000),
        );
        let mut stages = butterworth_stages(Response::LowPass, 1_000.0, 2.0, 48_000);
        stages.extend(butterworth_stages(Response::LowPass, 1_000.0, 2.0, 48_000));
        let two = FrequencyResponse::new(48_000, stages);
        let (a, b) = (one.magnitude_db(1_000.0), two.magnitude_db(1_000.0));
        assert!(a.zip(b).is_some_and(|(a, b)| (2.0 * a - b).abs() < 1e-4));
        assert!(one.magnitude_db(30_000.0).is_none());
        assert!(FrequencyResponse::new(48_000, Vec::new()).magnitude_db(100.0) == Some(0.0));
    }
}
