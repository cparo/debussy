//! The built-in effects by stable kind id: what each is called and its parameters
//! (`docs/architecture/effects.md` "Effect kinds").

use signal_graph::{Node, ParameterInfo};

use crate::amplifier::{self, Amplifier};
use crate::chorus::Chorus;
use crate::curve::CurveFn;
use crate::delay::{self, Delay};
use crate::dynamics::{self, Dynamics};
use crate::filters::{self, Butterworth, Response};
use crate::flanger::Flanger;
use crate::parametric::{self, ParametricEq};
use crate::reverb::{self, Reverb};

/// A built-in effect kind.
#[derive(Debug, Clone, Copy)]
pub struct EffectKind {
    /// The stable id saved projects name it by, such as `"high-pass"`.
    pub id: &'static str,
    /// The name shown for it, such as `"High-pass filter"`.
    pub name: &'static str,
    /// Builds a new effect of this kind at its defaults, ready to add to a graph; `None` if it
    /// cannot be made. Allocates.
    pub build: fn() -> Option<Box<dyn Node>>,
    /// What builds the curve its parameter values ask for, for its display; `None` for a kind
    /// with no display (`docs/architecture/effects.md` "Effect curves").
    pub curve: Option<CurveFn>,
}

/// Every built-in effect kind, in the order a menu lists them.
pub const EFFECTS: &[EffectKind] = &[
    EffectKind {
        id: "high-pass",
        name: "High-pass filter",
        build: build_high_pass,
        curve: Some(filters::high_pass_curve),
    },
    EffectKind {
        id: "low-pass",
        name: "Low-pass filter",
        build: build_low_pass,
        curve: Some(filters::low_pass_curve),
    },
    EffectKind {
        id: "parametric-eq",
        name: "Parametric EQ",
        build: build_parametric_eq,
        curve: Some(parametric::curve),
    },
    EffectKind {
        id: "dynamics",
        name: "Dynamics",
        build: build_dynamics,
        curve: Some(dynamics::curve),
    },
    EffectKind {
        id: "amplifier",
        name: "Amplifier",
        build: build_amplifier,
        curve: Some(amplifier::curve),
    },
    EffectKind {
        id: "delay",
        name: "Delay",
        build: build_delay,
        curve: Some(delay::impulse_curve),
    },
    EffectKind {
        id: "reverb",
        name: "Reverb",
        build: build_reverb,
        curve: Some(reverb::impulse_curve),
    },
    EffectKind {
        id: "chorus",
        name: "Chorus",
        build: build_chorus,
        curve: None,
    },
    EffectKind {
        id: "flanger",
        name: "Flanger",
        build: build_flanger,
        curve: None,
    },
];

/// A named set of values for an effect kind, chosen in its dialog: the values it names override
/// the kind's defaults, and the rest keep them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    /// The name shown for it, such as `"Compressor"`.
    pub name: &'static str,
    /// The stable id of the effect kind it starts.
    pub kind: &'static str,
    /// Parameter names and the values they start at.
    pub values: &'static [(&'static str, f32)],
}

/// Every preset, in the order a dialog lists them (`docs/architecture/effects.md`
/// "Effect presets").
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "Compressor",
        kind: "dynamics",
        values: &[],
    },
    Preset {
        name: "Expander",
        kind: "dynamics",
        values: &[
            ("threshold", -40.0),
            ("ratio below", 2.0),
            ("ratio above", 1.0),
        ],
    },
    Preset {
        name: "Gate",
        kind: "dynamics",
        values: &[
            ("threshold", -50.0),
            ("ratio below", 100.0),
            ("ratio above", 1.0),
            ("range", 80.0),
            ("attack", 0.1),
            ("hold", 50.0),
            ("release", 100.0),
        ],
    },
    Preset {
        name: "Limiter",
        kind: "dynamics",
        values: &[
            ("ratio above", 1.0),
            ("ceiling enabled", 1.0),
            ("ceiling", -1.0),
            ("lookahead", 5.0),
            ("attack", 0.2),
            ("release", 100.0),
        ],
    },
    Preset {
        name: "Slapback",
        kind: "delay",
        values: &[("time", 110.0), ("feedback", 10.0), ("mix", 35.0)],
    },
    Preset {
        name: "Ping-pong",
        kind: "delay",
        values: &[
            ("sync", 1.0),
            ("note", 2.0),
            ("feel", 1.0),
            ("mode", 1.0),
            ("feedback", 45.0),
        ],
    },
    Preset {
        name: "Dub echo",
        kind: "delay",
        values: &[
            ("sync", 1.0),
            ("note", 3.0),
            ("feedback", 70.0),
            ("low-pass enabled", 1.0),
            ("low-pass", 2_000.0),
            ("saturation", 1.0),
            ("mix", 40.0),
        ],
    },
    Preset {
        name: "Subtle",
        kind: "chorus",
        values: &[("depth", 2.0), ("mix", 35.0)],
    },
    Preset {
        name: "Wide",
        kind: "chorus",
        values: &[("voices", 3.0), ("depth", 4.0), ("spread", 180.0)],
    },
    Preset {
        name: "Jet",
        kind: "flanger",
        values: &[
            ("rate", 0.1),
            ("delay", 2.0),
            ("depth", 90.0),
            ("feedback", 70.0),
        ],
    },
    Preset {
        name: "Metallic",
        kind: "flanger",
        values: &[
            ("rate", 0.5),
            ("delay", 0.5),
            ("depth", 50.0),
            ("feedback", -85.0),
        ],
    },
];

impl Preset {
    /// The value this preset names for parameter `name`; `None` if it leaves it at its default.
    pub fn get(&self, name: &str) -> Option<f32> {
        self.values
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, value)| value)
    }

    /// The value parameter `info` takes under this preset: the preset's, else the default.
    pub fn value(&self, info: &ParameterInfo) -> f32 {
        self.get(&info.name).unwrap_or(info.default)
    }
}

/// The preset of effect kind `kind` whose values, over the defaults of `infos`, are exactly the
/// values `value` gives by parameter name; `None` if no preset matches, as after any change.
pub fn matching_preset(
    kind: &str,
    infos: &[ParameterInfo],
    value: impl Fn(&str) -> f32,
) -> Option<&'static Preset> {
    presets(kind).find(|preset| {
        infos
            .iter()
            .all(|info| info.constrain(preset.value(info)) == Some(value(&info.name)))
    })
}

/// The presets of effect kind `kind`, in dialog order; empty for a kind with none.
pub fn presets(kind: &str) -> impl Iterator<Item = &'static Preset> + '_ {
    PRESETS.iter().filter(move |p| p.kind == kind)
}

/// The preset named `name`, or `None`.
pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

/// A new high-pass filter.
fn build_high_pass() -> Option<Box<dyn Node>> {
    Some(Box::new(Butterworth::new(Response::HighPass).ok()?))
}

/// A new low-pass filter.
fn build_low_pass() -> Option<Box<dyn Node>> {
    Some(Box::new(Butterworth::new(Response::LowPass).ok()?))
}

/// A new parametric equaliser.
fn build_parametric_eq() -> Option<Box<dyn Node>> {
    Some(Box::new(ParametricEq::new().ok()?))
}

/// A new dynamics processor.
fn build_dynamics() -> Option<Box<dyn Node>> {
    Some(Box::new(Dynamics::new().ok()?))
}

/// A new amplifier.
fn build_amplifier() -> Option<Box<dyn Node>> {
    Some(Box::new(Amplifier::new().ok()?))
}

/// A new delay.
fn build_delay() -> Option<Box<dyn Node>> {
    Some(Box::new(Delay::new().ok()?))
}

/// A new chorus.
fn build_chorus() -> Option<Box<dyn Node>> {
    Some(Box::new(Chorus::new().ok()?))
}

/// A new flanger.
fn build_flanger() -> Option<Box<dyn Node>> {
    Some(Box::new(Flanger::new().ok()?))
}

/// A new reverb.
fn build_reverb() -> Option<Box<dyn Node>> {
    Some(Box::new(Reverb::new().ok()?))
}

/// The built-in effect of stable kind id `kind`, ready to add to a graph, or `None` for an id
/// this build does not know. Allocates.
pub fn effect(kind: &str) -> Option<Box<dyn Node>> {
    (EFFECTS.iter().find(|e| e.id == kind)?.build)()
}

/// The name shown for effect kind `kind`, or `None` for an id this build does not know.
pub fn display_name(kind: &str) -> Option<&'static str> {
    EFFECTS.iter().find(|e| e.id == kind).map(|e| e.name)
}

/// The parameters of effect kind `kind`, in the order the effect declares them, or `None` for
/// an id this build does not know. Allocates: builds the effect to ask it.
pub fn parameter_infos(kind: &str) -> Option<Vec<ParameterInfo>> {
    effect(kind).map(|node| node.parameters().iter().map(|p| p.info().clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_every_listed_kind_is_built_and_named() {
        for kind in EFFECTS {
            assert!(effect(kind.id).is_some(), "{}", kind.id);
            assert_eq!(display_name(kind.id), Some(kind.name));
            assert!(parameter_infos(kind.id).is_some_and(|p| !p.is_empty()));
        }
        assert!(effect("eq").is_none());
        assert_eq!(display_name("eq"), None);
        assert_eq!(parameter_infos("eq"), None);
    }

    #[test]
    fn test_every_preset_names_real_parameters_in_range() -> Result<(), String> {
        for entry in PRESETS {
            let infos = parameter_infos(entry.kind).ok_or(entry.name)?;
            assert_eq!(preset(entry.name), Some(entry));
            for &(name, value) in entry.values {
                let info = infos
                    .iter()
                    .find(|i| i.name == name)
                    .ok_or(format!("{}: no {name}", entry.name))?;
                assert_eq!(info.constrain(value), Some(value), "{}: {name}", entry.name);
            }
        }
        assert_eq!(presets("dynamics").count(), 4);
        let infos = parameter_infos("dynamics").ok_or("no dynamics")?;
        for entry in presets("dynamics") {
            let values: Vec<(String, f32)> = infos
                .iter()
                .map(|i| (i.name.clone(), entry.value(i)))
                .collect();
            let value = |name: &str| {
                values
                    .iter()
                    .find(|(n, _)| n == name)
                    .map_or(f32::NAN, |&(_, v)| v)
            };
            assert_eq!(matching_preset("dynamics", &infos, value), Some(entry));
            let changed = |name: &str| if name == "knee" { 1.5 } else { value(name) };
            assert_eq!(matching_preset("dynamics", &infos, changed), None);
        }
        assert_eq!(presets("delay").count(), 3);
        assert_eq!(presets("chorus").count(), 2);
        assert_eq!(presets("flanger").count(), 2);
        assert_eq!(presets("low-pass").count(), 0);
        let gate = preset("Gate").ok_or("no gate")?;
        assert_eq!((gate.get("range"), gate.get("knee")), (Some(80.0), None));
        Ok(())
    }
}
