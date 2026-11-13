//! The curve an effect's parameter values ask for, for display (`docs/architecture/effects.md`
//! "Effect curves"). Each effect kind's row in [`crate::EFFECTS`] names the function that builds
//! its curve, written beside the effect so that it reads values as processing reads them.

use crate::amplifier::ShaperCurve;
use crate::dynamics::TransferCurve;
use crate::registry::EFFECTS;
use crate::response::FrequencyResponse;

/// What an effect does to a signal at given parameter values, in the form its display plots.
///
/// The set of variants is closed: a display matches on every one, so a new kind of curve does not
/// build until it has a display.
#[derive(Debug, Clone, PartialEq)]
pub enum Curve {
    /// A magnitude response over frequency: the equalisers.
    Response(FrequencyResponse),
    /// A steady-state output level over detected input level: `dynamics`.
    Transfer(TransferCurve),
    /// A static output sample over input sample: `amplifier`.
    Shaper(ShaperCurve),
}

/// Builds one effect kind's curve from its parameter values (in the order the effect declares
/// them) at a sample rate in Hz; `None` for values of the wrong length.
pub type CurveFn = fn(&[f32], u32) -> Option<Curve>;

/// The curve of an effect of kind `kind` whose parameters stand at `values` (in the order
/// [`crate::parameter_infos`] gives them), in a project at `sample_rate` Hz. `None` for a kind
/// with no curve or one this build does not know, or for `values` of the wrong length.
pub fn curve(kind: &str, values: &[f32], sample_rate: u32) -> Option<Curve> {
    let build = EFFECTS.iter().find(|e| e.id == kind)?.curve?;
    build(values, sample_rate)
}

/// The default values of `kind`'s parameters with `changes` set by name, for tests.
#[cfg(test)]
pub(crate) fn values_with(kind: &str, changes: &[(&str, f32)]) -> Option<Vec<f32>> {
    let infos = crate::registry::parameter_infos(kind)?;
    Some(
        infos
            .iter()
            .map(|info| {
                changes
                    .iter()
                    .find(|(name, _)| *name == info.name)
                    .map_or(info.default, |&(_, value)| value)
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The variant each kind's curve is expected to be, by kind id; `None` for a kind with none.
    fn expected(kind: &str) -> Option<&'static str> {
        match kind {
            "high-pass" | "low-pass" | "parametric-eq" => Some("response"),
            "dynamics" => Some("transfer"),
            "amplifier" => Some("shaper"),
            _ => None,
        }
    }

    /// The name of `curve`'s variant.
    fn variant(curve: &Curve) -> &'static str {
        match curve {
            Curve::Response(_) => "response",
            Curve::Transfer(_) => "transfer",
            Curve::Shaper(_) => "shaper",
        }
    }

    #[test]
    fn test_curve_every_kind_gives_its_variant_at_its_defaults()
    -> Result<(), Box<dyn std::error::Error>> {
        for kind in EFFECTS {
            let values = values_with(kind.id, &[]).ok_or("no parameters")?;
            let got = curve(kind.id, &values, 48_000);
            assert_eq!(got.as_ref().map(variant), expected(kind.id), "{}", kind.id);
        }
        Ok(())
    }

    #[test]
    fn test_curve_refuses_wrong_lengths_and_unknown_kinds() -> Result<(), Box<dyn std::error::Error>>
    {
        for kind in EFFECTS.iter().filter(|k| k.curve.is_some()) {
            let mut values = values_with(kind.id, &[]).ok_or("no parameters")?;
            values.push(0.0);
            assert!(
                curve(kind.id, &values, 48_000).is_none(),
                "{} long",
                kind.id
            );
            values.truncate(values.len() - 2);
            assert!(
                curve(kind.id, &values, 48_000).is_none(),
                "{} short",
                kind.id
            );
        }
        assert!(curve("eq", &[1_000.0, 2.0], 48_000).is_none());
        assert!(curve("mixer", &[], 48_000).is_none());
        Ok(())
    }
}
