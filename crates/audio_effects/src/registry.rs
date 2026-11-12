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
}
