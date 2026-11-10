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

/// Which channel of a stereo signal [`Pick`] takes or [`Place`] fills: 0 left, 1 right.
fn channel_of(right: bool) -> usize {
    usize::from(right)
}

/// Takes one channel of a stereo input as a mono output: how a connection from one side of a
/// stereo port is realised (ADR-0058).
#[derive(Debug, Clone, Copy)]
pub struct Pick {
    right: bool,
}

impl Pick {
    /// Takes the left channel, or the right if `right`.
    pub fn new(right: bool) -> Self {
        Pick { right }
    }
}

impl Node for Pick {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("stereo", Layout::Stereo)]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("side", Layout::Mono)]
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let input = inputs.channel(0, channel_of(self.right));
        if let Some(output) = outputs.channel_mut(0, 0) {
            match input {
                Some(input) => output.copy_from_slice(&input[..output.len()]),
                None => output.fill(0.0),
            }
        }
    }
}

/// Puts a mono input on one channel of a stereo output, the other silent: how a connection into
/// one side of a stereo port is realised (ADR-0058).
#[derive(Debug, Clone, Copy)]
pub struct Place {
    right: bool,
}

impl Place {
    /// Fills the left channel, or the right if `right`.
    pub fn new(right: bool) -> Self {
        Place { right }
    }
}

impl Node for Place {
    fn inputs(&self) -> Vec<Port> {
        vec![Port::audio("side", Layout::Mono)]
    }

    fn outputs(&self) -> Vec<Port> {
        vec![Port::audio("stereo", Layout::Stereo)]
    }

    fn process(&mut self, inputs: &Inputs<'_>, outputs: &mut Outputs<'_>) {
        let input = inputs.channel(0, 0);
        let target = channel_of(self.right);
        for channel in 0..2 {
            if let Some(output) = outputs.channel_mut(0, channel) {
                match input {
                    Some(input) if channel == target => {
                        output.copy_from_slice(&input[..output.len()]);
                    }
                    _ => output.fill(0.0),
                }
            }
        }
    }
}
