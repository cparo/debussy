//! The built-in instruments by stable kind id: what each is called and its parameters
//! (`docs/architecture/signal-graph.md` "Instruments").

use signal_graph::{Node, ParameterInfo};

use crate::synth::Synth;

/// A built-in instrument kind.
#[derive(Debug, Clone, Copy)]
pub struct InstrumentKind {
    /// The stable id saved projects name it by, such as `"synth"`.
    pub id: &'static str,
    /// The name shown for it, such as `"Synthesizer"`.
    pub name: &'static str,
    /// Builds a new instrument of this kind at its defaults, ready to add to a graph; `None` if
    /// it cannot be made. Allocates.
    pub build: fn() -> Option<Box<dyn Node>>,
}

/// Every built-in instrument kind, in the order a menu lists them.
pub const INSTRUMENTS: &[InstrumentKind] = &[InstrumentKind {
    id: "synth",
    name: "Synthesizer",
    build: build_synth,
}];

fn build_synth() -> Option<Box<dyn Node>> {
    Synth::new().ok().map(|s| Box::new(s) as Box<dyn Node>)
}

/// A new instrument of kind `kind` at its defaults, or `None` if this build does not know it.
/// Allocates.
pub fn instrument(kind: &str) -> Option<Box<dyn Node>> {
    (INSTRUMENTS.iter().find(|i| i.id == kind)?.build)()
}

/// The name shown for instrument kind `kind`, or `None` if this build does not know it.
pub fn display_name(kind: &str) -> Option<&'static str> {
    INSTRUMENTS.iter().find(|i| i.id == kind).map(|i| i.name)
}

/// The parameters of instrument kind `kind`, or `None` if this build does not know it.
/// Allocates: builds the instrument to ask it.
pub fn parameter_infos(kind: &str) -> Option<Vec<ParameterInfo>> {
    instrument(kind).map(|node| node.parameters().iter().map(|p| p.info().clone()).collect())
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use audio_buffers::Layout;
    use signal_graph::PortType;

    use super::*;

    #[test]
    fn test_instrument_unknown_kind_is_none() {
        assert!(instrument("no-such-instrument").is_none());
        assert!(display_name("no-such-instrument").is_none());
        assert!(parameter_infos("no-such-instrument").is_none());
    }

    #[test]
    fn test_instruments_ids_are_unique_and_build() {
        for (n, kind) in INSTRUMENTS.iter().enumerate() {
            assert!(
                INSTRUMENTS[..n].iter().all(|k| k.id != kind.id),
                "{}",
                kind.id
            );
            let node = instrument(kind.id);
            assert!(node.is_some(), "{} does not build", kind.id);
        }
    }

    #[test]
    fn test_instruments_have_event_input_and_stereo_output() -> Result<(), Box<dyn Error>> {
        for kind in INSTRUMENTS {
            let node = instrument(kind.id).ok_or(format!("{} does not build", kind.id))?;
            assert_eq!(node.inputs()[0].kind, PortType::Event, "{}", kind.id);
            assert_eq!(node.outputs()[0].kind, PortType::Audio, "{}", kind.id);
            assert_eq!(node.outputs()[0].layout, Layout::Stereo, "{}", kind.id);
        }
        Ok(())
    }
}
