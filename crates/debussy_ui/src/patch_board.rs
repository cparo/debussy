//! The patch board: the routing drawn as nodes and mono wires beside the tracks, with the
//! track lanes' ports in the gutter between them (`docs/architecture/user-interface.md`
//! "Patch board").
//!
//! [`layout`] places everything from the model, so it is tested without a window; [`BoardView`]
//! and [`GutterView`] draw it.

use std::collections::BTreeMap;

use audio_buffers::Layout;
use iced::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke, Text};
use iced::{Point, Rectangle, Renderer, Size, Theme, mouse};
use project_model::{ConnectionId, Endpoint, NodeId, NodeKind, Side, State};

use crate::app::Message;
use crate::theme::{AUDIO, tokens, tone_of};

/// The distance between two port circles on a node, in logical pixels.
pub const PORT_SPACING: f32 = 48.0;
/// A node's width.
pub const NODE_WIDTH: f32 = 160.0;
/// The width of the port gutter between the track pane and the board.
pub const GUTTER_WIDTH: f32 = 40.0;
/// The space between columns of nodes.
const COLUMN_GAP: f32 = 96.0;
/// The space left of the first column and above the first node.
const MARGIN: f32 = 64.0;
/// A port circle's radius.
const PIN_RADIUS: f32 = 10.0;
/// The L and R output circles' radius.
const OUTPUT_RADIUS: f32 = 18.0;
/// How far the output circles sit inside the board's right edge, to their centres.
const OUTPUT_INSET: f32 = 36.0;
/// The width of a wire.
const WIRE_WIDTH: f32 = 3.0;

/// One circle on the board: a mono port, or one side of a stereo one. Every wire joins two.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pin {
    /// The track, node or project output it belongs to.
    pub endpoint: Endpoint,
    /// An output (`true`) or input port.
    pub output: bool,
    /// The port's index.
    pub port: usize,
    /// The side of a stereo port; `None` for a mono one.
    pub side: Option<Side>,
}

/// A node as drawn.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeBox {
    /// The node.
    pub id: NodeId,
    /// Its title.
    pub title: String,
    /// Where it is on the board.
    pub rect: Rectangle,
}

/// A mono wire between two pins, drawn for a connection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wire {
    /// The connection it draws; a whole-port stereo connection draws two.
    pub connection: ConnectionId,
    /// The output end.
    pub from: Pin,
    /// The input end.
    pub to: Pin,
}

/// The board, laid out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Board {
    /// The nodes.
    pub nodes: Vec<NodeBox>,
    /// Every pin's centre on the board; track pins sit on its left edge.
    pub pins: BTreeMap<Pin, Point>,
    /// The wires.
    pub wires: Vec<Wire>,
    /// The board's size: at least the pane's, and enough to hold everything.
    pub size: Size,
}

/// A node's input and output ports' layouts. Effects' ports are not known to the model; they
/// are drawn as one stereo input and output.
fn ports(kind: &NodeKind) -> (Vec<Layout>, Vec<Layout>) {
    match kind {
        NodeKind::Mixer { channels, .. } => (vec![Layout::Mono; *channels], vec![Layout::Stereo]),
        NodeKind::Bus(layout) | NodeKind::FeedbackDelay { layout, .. } => {
            (vec![*layout], vec![*layout])
        }
        NodeKind::Effect { .. } => (vec![Layout::Stereo], vec![Layout::Stereo]),
    }
}

/// A node's title.
pub fn title(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Mixer { .. } => "Mixer".to_owned(),
        NodeKind::Bus(_) => "Bus".to_owned(),
        NodeKind::FeedbackDelay { .. } => "Feedback delay".to_owned(),
        NodeKind::Effect { kind, .. } => kind.clone(),
    }
}

/// The sides of a port of `layout`: one pin for mono, left above right for stereo.
fn sides(layout: Layout) -> &'static [Option<Side>] {
    match layout {
        Layout::Mono => &[None],
        Layout::Stereo => &[Some(Side::Left), Some(Side::Right)],
    }
}

/// The pins of `endpoint`'s ports in order: each port's sides, top to bottom.
fn pins_of(endpoint: Endpoint, output: bool, layouts: &[Layout]) -> Vec<Pin> {
    layouts
        .iter()
        .enumerate()
        .flat_map(|(port, layout)| {
            sides(*layout).iter().map(move |side| Pin {
                endpoint,
                output,
                port,
                side: *side,
            })
        })
        .collect()
}

/// The height of a node with `slots` pins down its taller side.
pub fn node_height(slots: usize) -> f32 {
    PORT_SPACING * (slots.max(1) + 1) as f32
}

/// Each node's column: one more than the deepest node feeding it, from 1. Cycles stop growing
/// after as many rounds as there are nodes.
fn depths(state: &State) -> BTreeMap<NodeId, usize> {
    let mut depth: BTreeMap<NodeId, usize> = state.nodes.iter().map(|n| (n.id, 1)).collect();
    for _ in 0..state.nodes.len() {
        let mut changed = false;
        for c in &state.connections {
            if let (Endpoint::Node(from), Endpoint::Node(to)) = (c.from, c.to) {
                let next = depth.get(&from).copied().unwrap_or(1) + 1;
                if let Some(d) = depth.get_mut(&to)
                    && next > *d
                    && next <= state.nodes.len()
                {
                    *d = next;
                    changed = true;
                }
            }
        }
        if !changed {
            break;
        }
    }
    depth
}

/// Lays out `state`'s routing. `lanes` gives each track's lane centres, top to bottom, in the
/// shared vertical coordinates; the board is at least `min` in size. Nodes are placed in
/// columns by graph depth, each column ordered by where its inputs come from; the L and R
/// outputs sit just inside the right edge, level with what feeds them.
pub fn layout(state: &State, lanes: &[(project_model::TrackId, Vec<f32>)], min: Size) -> Board {
    let mut pins = BTreeMap::new();
    for (track, centres) in lanes {
        let Some(t) = state.track(*track) else {
            continue;
        };
        let endpoint = Endpoint::Track(*track);
        for (pin, y) in pins_of(endpoint, true, &[t.layout])
            .into_iter()
            .zip(centres)
        {
            pins.insert(pin, Point::new(0.0, *y));
        }
    }

    let depth = depths(state);
    let columns = depth.values().copied().max().unwrap_or(0);
    let mut nodes = Vec::new();
    for column in 1..=columns {
        // The column's nodes, by the mean height of the pins feeding them so far.
        let mut placed: Vec<(f32, &project_model::Node)> = state
            .nodes
            .iter()
            .filter(|n| depth.get(&n.id) == Some(&column))
            .map(|n| {
                let ys: Vec<f32> = state
                    .connections
                    .iter()
                    .filter(|c| c.to == Endpoint::Node(n.id))
                    .flat_map(|c| pins.iter().filter(move |(p, _)| p.endpoint == c.from))
                    .map(|(_, at)| at.y)
                    .collect();
                let key = if ys.is_empty() {
                    f32::MAX
                } else {
                    ys.iter().sum::<f32>() / ys.len() as f32
                };
                (key, n)
            })
            .collect();
        placed.sort_by(|a, b| a.0.total_cmp(&b.0));
        let x = MARGIN + (column - 1) as f32 * (NODE_WIDTH + COLUMN_GAP);
        let mut next_free = MARGIN - PORT_SPACING / 2.0;
        for (key, node) in placed {
            let (inputs, outputs) = ports(&node.kind);
            let inputs = pins_of(Endpoint::Node(node.id), false, &inputs);
            let outputs = pins_of(Endpoint::Node(node.id), true, &outputs);
            let height = node_height(inputs.len().max(outputs.len()));
            let wanted = if key == f32::MAX {
                next_free
            } else {
                key - PORT_SPACING
            };
            let y = wanted.max(next_free);
            next_free = y + height + PORT_SPACING;
            for (side, list) in [(x, inputs), (x + NODE_WIDTH, outputs)] {
                for (slot, pin) in list.into_iter().enumerate() {
                    pins.insert(pin, Point::new(side, y + PORT_SPACING * (slot + 1) as f32));
                }
            }
            nodes.push(NodeBox {
                id: node.id,
                title: title(&node.kind),
                rect: Rectangle::new(Point::new(x, y), Size::new(NODE_WIDTH, height)),
            });
        }
    }

    let right = nodes
        .iter()
        .map(|n| n.rect.x + n.rect.width)
        .fold(0.0, f32::max);
    let width = min.width.max(right + COLUMN_GAP + OUTPUT_INSET);
    // Level with the pins feeding the output, else at the top.
    let feeding: Vec<f32> = state
        .connections
        .iter()
        .filter(|c| c.to == Endpoint::Output)
        .flat_map(|c| {
            pins.iter()
                .filter(move |(p, _)| p.endpoint == c.from && p.output)
        })
        .map(|(_, at)| at.y)
        .collect();
    let top = feeding.iter().copied().fold(f32::MAX, f32::min);
    let out_y = if top == f32::MAX { MARGIN } else { top };
    for (slot, pin) in pins_of(Endpoint::Output, false, &[Layout::Stereo])
        .into_iter()
        .enumerate()
    {
        pins.insert(
            pin,
            Point::new(width - OUTPUT_INSET, out_y + PORT_SPACING * slot as f32),
        );
    }

    let wires = wires(state, &pins);
    let bottom = pins.values().map(|p| p.y).fold(0.0, f32::max);
    let bottom = nodes
        .iter()
        .map(|n| n.rect.y + n.rect.height)
        .fold(bottom, f32::max);
    Board {
        nodes,
        pins,
        wires,
        size: Size::new(width, min.height.max(bottom + PORT_SPACING)),
    }
}

/// The mono wires drawing `state`'s connections between placed pins. A connection naming a
/// side joins that side's pin; a whole stereo port joins left to left and right to right, or
/// both sides to a mono end.
fn wires(state: &State, pins: &BTreeMap<Pin, Point>) -> Vec<Wire> {
    let ends = |endpoint: Endpoint, output: bool, port: usize, side: Option<Side>| -> Vec<Pin> {
        pins.keys()
            .filter(|p| p.endpoint == endpoint && p.output == output && p.port == port)
            .filter(|p| side.is_none() || p.side == side)
            .copied()
            .collect()
    };
    let mut wires = Vec::new();
    for c in &state.connections {
        let from = ends(c.from, true, c.output, c.from_side);
        let to = ends(c.to, false, c.input, c.to_side);
        let pairs: Vec<(Pin, Pin)> = if from.len() == to.len() {
            from.into_iter().zip(to).collect()
        } else {
            from.iter()
                .flat_map(|f| to.iter().map(move |t| (*f, *t)))
                .collect()
        };
        wires.extend(pairs.into_iter().map(|(from, to)| Wire {
            connection: c.id,
            from,
            to,
        }));
    }
    wires
}

/// A wire's curve from an output at `a` to an input at `b`.
fn curve(a: Point, b: Point) -> Path {
    let reach = ((b.x - a.x).abs() / 2.0).max(40.0);
    Path::new(|p| {
        p.move_to(a);
        p.bezier_curve_to(
            Point::new(a.x + reach, a.y),
            Point::new(b.x - reach, b.y),
            b,
        );
    })
}

/// A port circle at `at`.
fn pin(frame: &mut Frame, at: Point, radius: f32, fill: iced::Color) {
    let circle = Path::circle(at, radius);
    frame.fill(&circle, fill);
    frame.stroke(
        &circle,
        Stroke::default().with_width(WIRE_WIDTH).with_color(AUDIO),
    );
}

/// Draws a laid-out board.
pub struct BoardView(pub Board);

impl Program<Message> for BoardView {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let t = tokens(tone_of(theme));
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), t.pane);
        let wire = Stroke::default().with_width(WIRE_WIDTH).with_color(AUDIO);
        for w in &self.0.wires {
            if let (Some(a), Some(b)) = (self.0.pins.get(&w.from), self.0.pins.get(&w.to)) {
                frame.stroke(&curve(*a, *b), wire);
            }
        }
        for node in &self.0.nodes {
            frame.fill_rectangle(node.rect.position(), node.rect.size(), t.lane);
            frame.fill_text(Text {
                content: node.title.clone(),
                position: Point::new(node.rect.center_x(), node.rect.y + PORT_SPACING / 2.0),
                color: t.text,
                size: 18.0.into(),
                align_x: iced::alignment::Horizontal::Center.into(),
                align_y: iced::alignment::Vertical::Center,
                ..Text::default()
            });
        }
        for (p, at) in &self.0.pins {
            match p.endpoint {
                // Drawn in the gutter; the wire starts at the board's edge.
                Endpoint::Track(_) => {}
                Endpoint::Output => {
                    pin(&mut frame, *at, OUTPUT_RADIUS, t.lane);
                    frame.fill_text(Text {
                        content: if p.side == Some(Side::Right) {
                            "R"
                        } else {
                            "L"
                        }
                        .to_owned(),
                        position: *at,
                        color: t.text,
                        size: 18.0.into(),
                        align_x: iced::alignment::Horizontal::Center.into(),
                        align_y: iced::alignment::Vertical::Center,
                        ..Text::default()
                    });
                }
                Endpoint::Node(_) => pin(&mut frame, *at, PIN_RADIUS, t.lane),
            }
        }
        vec![frame.into_geometry()]
    }
}

/// Draws the port gutter: a pin centred on each lane (given by its centre), with a stub to the
/// board.
pub struct GutterView(pub Vec<f32>);

impl Program<Message> for GutterView {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let t = tokens(tone_of(theme));
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), t.chrome);
        let x = bounds.width / 2.0;
        for &y in &self.0 {
            frame.fill_rectangle(
                Point::new(x, y - WIRE_WIDTH / 2.0),
                Size::new(bounds.width - x, WIRE_WIDTH),
                AUDIO,
            );
            pin(&mut frame, Point::new(x, y), PIN_RADIUS, t.lane);
        }
        vec![frame.into_geometry()]
    }
}

/// The divider between the track pane and the board: dragging it moves the split.
pub struct Divider {
    /// The window's width, to turn a position into a share of it.
    pub window_width: f32,
}

/// Whether the divider is being dragged.
#[derive(Default)]
pub struct Dragging(bool);

impl Program<Message> for Divider {
    type State = Dragging;

    fn update(
        &self,
        dragging: &mut Dragging,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                if cursor.is_over(bounds) =>
            {
                dragging.0 = true;
                Some(canvas::Action::capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { position }) if dragging.0 => {
                let share = position.x / self.window_width.max(1.0);
                Some(canvas::Action::publish(Message::Split(share)).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if dragging.0 =>
            {
                dragging.0 = false;
                Some(canvas::Action::capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Dragging,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), tokens(tone_of(theme)).line);
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        dragging: &Dragging,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if dragging.0 || cursor.is_over(bounds) {
            mouse::Interaction::ResizingHorizontally
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use project_model::Session;

    type Result = std::result::Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_layout_places_a_new_projects_mixer_and_outputs() -> Result {
        let mut session = Session::new_project();
        let vocal = session.add_track("Vocal", Layout::Mono)?;
        let keys = session.add_track("Keys", Layout::Stereo)?;
        let lanes = vec![(vocal, vec![90.0]), (keys, vec![244.0, 372.0])];
        let board = layout(session.state(), &lanes, Size::new(500.0, 400.0));
        assert_eq!(board.nodes.len(), 1);
        let mixer = &board.nodes[0];
        // Three mono inputs: three slots, plus the title.
        assert_eq!(mixer.rect.height, node_height(3));
        // The gutter pins are on the board's left edge at the lane centres.
        let keys_right = Pin {
            endpoint: Endpoint::Track(keys),
            output: true,
            port: 0,
            side: Some(Side::Right),
        };
        assert_eq!(board.pins.get(&keys_right), Some(&Point::new(0.0, 372.0)));
        // Three track wires and the mixer's stereo output as two.
        assert_eq!(board.wires.len(), 5);
        let left_out = Pin {
            endpoint: Endpoint::Output,
            output: false,
            port: 0,
            side: Some(Side::Left),
        };
        let at = board.pins.get(&left_out).ok_or("no L output")?;
        assert!(at.x > mixer.rect.x + mixer.rect.width);
        assert_eq!(at.x, board.size.width - OUTPUT_INSET);
        assert!(board.size.width >= 500.0 && board.size.height >= 400.0);
        Ok(())
    }

    #[test]
    fn test_layout_orders_columns_by_depth_and_survives_cycles() -> Result {
        let mut session = Session::new_project();
        let mixer = session.state().project_mixer().ok_or("no mixer")?;
        let bus = session.add_node(NodeKind::Bus(Layout::Stereo))?;
        let delay = session.add_node(NodeKind::FeedbackDelay {
            layout: Layout::Mono,
            seconds: 0.25,
        })?;
        session.connect(Endpoint::Node(bus), 0, Endpoint::Node(delay), 0)?;
        session.connect(Endpoint::Node(delay), 0, Endpoint::Node(bus), 0)?;
        session.connect(Endpoint::Node(delay), 0, Endpoint::Node(mixer), 0)?;
        let board = layout(session.state(), &[], Size::new(100.0, 100.0));
        let x = |id| {
            board
                .nodes
                .iter()
                .find(|n| n.id == id)
                .map(|n| n.rect.x)
                .unwrap_or(-1.0)
        };
        assert!(x(bus) >= MARGIN);
        assert!(
            x(delay) > x(bus) || x(bus) > x(delay),
            "a cycle still places both"
        );
        assert!(x(mixer) > x(delay));
        // The board grows to hold them.
        assert!(board.size.width > 100.0);
        Ok(())
    }

    #[test]
    fn test_nodes_in_a_column_do_not_overlap() -> Result {
        let mut session = Session::new();
        for _ in 0..3 {
            session.add_node(NodeKind::Bus(Layout::Stereo))?;
        }
        let board = layout(session.state(), &[], Size::new(100.0, 100.0));
        for pair in board.nodes.windows(2) {
            assert!(pair[1].rect.y >= pair[0].rect.y + pair[0].rect.height);
        }
        Ok(())
    }
}
