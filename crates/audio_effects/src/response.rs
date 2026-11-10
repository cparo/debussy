//! The magnitude response an equaliser's parameters ask for, for display
//! (`docs/architecture/effects.md` "Frequency response").

use dsp::{OnePoleCoefficients, SvfCoefficients};

use crate::filters::{self, Response};
use crate::parametric::{self, BANDS};

/// The parameters of a high-pass or low-pass filter: cutoff and order.
const FILTER_PARAMETERS: usize = 2;

/// The parameters of each parametric band: enabled, type, frequency, gain and Q.
const BAND_PARAMETERS: usize = 5;

/// One stage of an effect's filter chain.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
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

/// The response of an effect of kind `kind` whose parameters stand at `values` (in the order
/// [`crate::parameter_infos`] gives them), at `sample_rate`. `None` for a kind that has no
/// frequency response, or `values` of the wrong length.
pub fn frequency_response(
    kind: &str,
    values: &[f32],
    sample_rate: f32,
) -> Option<FrequencyResponse> {
    let stages = match kind {
        "high-pass" => filter_stages(Response::HighPass, values, sample_rate)?,
        "low-pass" => filter_stages(Response::LowPass, values, sample_rate)?,
        "parametric-eq" => parametric_stages(values, sample_rate)?,
        _ => return None,
    };
    Some(FrequencyResponse {
        sample_rate,
        stages,
    })
}

/// The stages of a Butterworth filter at cutoff and order `values`.
fn filter_stages(response: Response, values: &[f32], sample_rate: f32) -> Option<Vec<Stage>> {
    let &[cutoff, order] = <&[f32; FILTER_PARAMETERS]>::try_from(values).ok()?;
    let order = filters::order_from_value(order);
    let (sections, one_pole) = filters::design(response, order, cutoff, sample_rate);
    let mut stages: Vec<Stage> = sections[..order / 2]
        .iter()
        .map(|&c| Stage::Svf(c))
        .collect();
    if order % 2 == 1 {
        stages.push(Stage::OnePole(one_pole));
    }
    Some(stages)
}

/// The stages of a parametric equaliser at `values`: one per enabled band.
fn parametric_stages(values: &[f32], sample_rate: f32) -> Option<Vec<Stage>> {
    if values.len() != BANDS * BAND_PARAMETERS {
        return None;
    }
    let stages = values
        .as_chunks::<BAND_PARAMETERS>()
        .0
        .iter()
        .filter_map(|&[enabled, kind, frequency, gain, q]| {
            let band_type = parametric::wanted(enabled, kind)?;
            Some(Stage::Svf(band_type.design(
                frequency,
                q,
                gain,
                sample_rate,
            )))
        })
        .collect();
    Some(stages)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::parameter_infos;

    const RATE: f32 = 48_000.0;

    /// The default values of `kind`'s parameters.
    fn defaults(kind: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        Ok(parameter_infos(kind)
            .ok_or("unknown kind")?
            .iter()
            .map(|i| i.default)
            .collect())
    }

    #[test]
    fn test_frequency_response_filters_are_3db_down_and_slope_by_order()
    -> Result<(), Box<dyn std::error::Error>> {
        for (kind, cutoff, far) in [("low-pass", 200.0, 1_600.0), ("high-pass", 2_000.0, 250.0)] {
            for order in 1..=8 {
                #[allow(clippy::cast_precision_loss)] // a small order
                let values = [cutoff, order as f32];
                let response = frequency_response(kind, &values, RATE).ok_or("no response")?;
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
    fn test_frequency_response_flat_when_every_band_is_off()
    -> Result<(), Box<dyn std::error::Error>> {
        let response = frequency_response("parametric-eq", &defaults("parametric-eq")?, RATE)
            .ok_or("no response")?;
        for frequency in [20.0, 1_000.0, 20_000.0] {
            assert_eq!(response.magnitude_db(frequency), Some(0.0));
        }
        Ok(())
    }

    #[test]
    fn test_frequency_response_enabled_bells_sum() -> Result<(), Box<dyn std::error::Error>> {
        let mut values = defaults("parametric-eq")?;
        // Bands 1 and 2 as bells at 1 kHz, +6 and +3 dB.
        for (band, gain) in [(0, 6.0), (1, 3.0)] {
            let base = band * BAND_PARAMETERS;
            values[base..base + 4].copy_from_slice(&[1.0, 0.0, 1_000.0, gain]);
        }
        let response = frequency_response("parametric-eq", &values, RATE).ok_or("no response")?;
        let centre = response.magnitude_db(1_000.0).ok_or("above Nyquist")?;
        assert!((centre - 9.0).abs() < 1e-3, "{centre}");
        // A notch on band 3 drives the centre far down.
        values[10..14].copy_from_slice(&[1.0, 3.0, 1_000.0, 0.0]);
        let response = frequency_response("parametric-eq", &values, RATE).ok_or("no response")?;
        assert!(response.magnitude_db(1_000.0).is_some_and(|db| db < -60.0));
        let zeros: Vec<f32> = response.zeros().collect();
        assert!(
            zeros.len() == 1 && (zeros[0] - 1_000.0).abs() < 0.1,
            "{zeros:?}"
        );
        Ok(())
    }

    #[test]
    fn test_frequency_response_refuses_unknown_kinds_and_lengths() {
        assert!(frequency_response("eq", &[1_000.0, 2.0], RATE).is_none());
        assert!(frequency_response("low-pass", &[1_000.0], RATE).is_none());
        assert!(frequency_response("parametric-eq", &[0.0; 39], RATE).is_none());
        let response = frequency_response("low-pass", &[1_000.0, 2.0], RATE);
        assert!(response.is_some_and(|r| r.magnitude_db(30_000.0).is_none()));
    }
}
