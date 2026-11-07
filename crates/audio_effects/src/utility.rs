//! Simple nodes the routing needs besides effects: [`Bus`], which passes its summed input
//! through, and [`Silence`], which stands in for an effect this build does not know
//! (`docs/architecture/project.md` "Routing", ADR-0043).

use audio_buffers::Layout;
use signal_graph::{Inputs, Node, Outputs, Port};

/// A bus: one input and one output of a layout. Connections into it are summed by the graph; it
/// passes the sum through unchanged.
#[derive(Debug, Clone, Copy)]
pub struct Bus {
    layout: Layout,
}

impl Bus {
    /// A bus of `layout`.
    pub fn new(layout: Layout) -> Self {
        Bus { layout }
    }
}

impl Node for Bus {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("in", self.layout)]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("out", self.layout)]
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let channels = match self.layout {
            Layout::Mono => 1,
            Layout::Stereo => 2,
        };
        for channel in 0..channels {
            let input = inputs.channel(0, channel);
            if let Some(output) = outputs.channel_mut(0, channel) {
                match input {
                    Some(input) => output.copy_from_slice(&input[..output.len()]),
                    None => output.fill(0.0),
                }
            }
        }
    }
}

/// A stand-in for an effect kind this build does not know: stereo inputs and outputs, enough
/// for the connections saved with it, and silence out.
#[derive(Debug, Clone, Copy)]
pub struct Silence {
    inputs: usize,
    outputs: usize,
}

impl Silence {
    /// A stand-in with `inputs` and `outputs` stereo ports.
    pub fn new(inputs: usize, outputs: usize) -> Self {
        Silence { inputs, outputs }
    }
}

impl Node for Silence {
    fn inputs(&self) -> Vec<Port> {
        (0..self.inputs)
            .map(|n| Port::audio(&format!("in {}", n + 1), Layout::Stereo))
            .collect()
    }

    fn outputs(&self) -> Vec<Port> {
        (0..self.outputs)
            .map(|n| Port::audio(&format!("out {}", n + 1), Layout::Stereo))
            .collect()
    }

    fn process(&mut self, _inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        for port in 0..self.outputs {
            for channel in 0..2 {
                if let Some(output) = outputs.channel_mut(port, channel) {
                    output.fill(0.0);
                }
            }
        }
    }
}

/// The built-in effect of stable kind id `kind`, ready to add to a graph, or `None` for an id
/// this build does not know. No effects are built in yet (#58–#62).
pub fn effect(kind: &str) -> Option<Box<dyn Node>> {
    // Each built-in effect adds an arm here, keyed by its saved kind id.
    let _ = kind;
    None
}
