//! [`Silence`], which stands in for an instrument kind this build does not know
//! (`docs/architecture/project.md` "Routing").

use audio_buffers::Layout;
use signal_graph::{Inputs, Node, Outputs, Port};

/// A stand-in for an instrument kind this build does not know: the ports every instrument has,
/// an event input and a stereo output, and silence out.
#[derive(Debug, Clone, Copy, Default)]
pub struct Silence;

impl Node for Silence {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::event("notes")]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", Layout::Stereo)]
    }

    fn process(&mut self, _inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        for channel in 0..2 {
            if let Some(output) = outputs.channel_mut(0, channel) {
                output.fill(0.0);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use signal_graph::PortType;

    #[test]
    fn test_silence_ports() {
        let node = Silence;
        assert_eq!(node.inputs().len(), 1);
        assert_eq!(node.inputs()[0].kind, PortType::Event);
        assert_eq!(node.outputs().len(), 1);
        assert_eq!(node.outputs()[0].layout, Layout::Stereo);
    }
}
