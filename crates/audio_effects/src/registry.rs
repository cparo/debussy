//! The built-in effects by stable kind id: what each is called and its parameters
//! (`docs/architecture/effects.md` "Effect kinds").

use signal_graph::{Node, ParameterInfo};

use crate::dynamics::Dynamics;
use crate::filters::{Butterworth, Response};
use crate::parametric::ParametricEq;

/// A built-in effect kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectKind {
    /// The stable id saved projects name it by, such as `"high-pass"`.
    pub id: &'static str,
    /// The name shown for it, such as `"High-pass filter"`.
    pub name: &'static str,
}

/// Every built-in effect kind, in the order a menu lists them.
pub const EFFECTS: &[EffectKind] = &[
    EffectKind {
        id: "high-pass",
        name: "High-pass filter",
    },
    EffectKind {
        id: "low-pass",
        name: "Low-pass filter",
    },
    EffectKind {
        id: "parametric-eq",
        name: "Parametric EQ",
    },
    EffectKind {
        id: "dynamics",
        name: "Dynamics",
    },
];

/// A named set of starting values for an effect kind, offered in place of the bare kind: the
/// values it names override the kind's defaults, and the rest keep them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Preset {
    /// The name shown for it, such as `"Compressor"`.
    pub name: &'static str,
    /// The stable id of the effect kind it starts.
    pub kind: &'static str,
    /// Parameter names and the values they start at.
    pub values: &'static [(&'static str, f32)],
}

/// Every preset, in the order a menu lists them. A kind with presets is offered only through
/// them (`docs/architecture/effects.md` "Effect presets").
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
];

/// The presets of effect kind `kind`, in menu order; empty for a kind offered bare.
pub fn presets(kind: &str) -> impl Iterator<Item = &'static Preset> + '_ {
    PRESETS.iter().filter(move |p| p.kind == kind)
}

/// The preset named `name`, or `None`.
pub fn preset(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

/// The built-in effect of stable kind id `kind`, ready to add to a graph, or `None` for an id
/// this build does not know. Allocates.
pub fn effect(kind: &str) -> Option<Box<dyn Node>> {
    // Each built-in effect adds an arm here, keyed by its saved kind id.
    let node: Box<dyn Node> = match kind {
        "high-pass" => Box::new(Butterworth::new(Response::HighPass).ok()?),
        "low-pass" => Box::new(Butterworth::new(Response::LowPass).ok()?),
        "parametric-eq" => Box::new(ParametricEq::new().ok()?),
        "dynamics" => Box::new(Dynamics::new().ok()?),
        _ => return None,
    };
    Some(node)
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
        assert_eq!(presets("low-pass").count(), 0);
        Ok(())
    }
}
