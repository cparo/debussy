//! The patch board: the routing drawn as nodes and mono wires beside the tracks, with the
//! track lanes' and curve lanes' ports in the gutter between them
//! (`docs/architecture/user-interface.md` "Patch board").
//!
//! [`layout`] places everything from the model, so it is tested without a window; [`BoardView`]
//! and [`GutterView`] draw it.

use std::collections::BTreeMap;

use audio_buffers::Layout;
use iced::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke, Text};
use iced::{Point, Rectangle, Renderer, Size, Theme, mouse};
use project_model::{
    ConnectionId, CurveId, Endpoint, NodeId, NodeKind, Side, State, TrackKind, WireShape,
};

use crate::app::Message;
use crate::theme::{CONTROL, Tokens, mix, tokens, tone_of};
use crate::wire_edit::{self, Grip, Segment};

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
pub(crate) const PIN_RADIUS: f32 = 10.0;
/// How far a gutter pin's outer edge, its stroke included, sits from the divider.
const GUTTER_PIN_INSET: f32 = 4.0;
/// The L and R output circles' radius.
const OUTPUT_RADIUS: f32 = 18.0;
/// How far the output circles sit inside the board's right edge, to their centres.
const OUTPUT_INSET: f32 = 36.0;
/// The width of a wire.
const WIRE_WIDTH: f32 = 3.0;
/// A wire handle's radius.
const HANDLE_RADIUS: f32 = 5.0;
/// How close to a wire handle or point a press must be to catch it.
const HANDLE_REACH: f32 = 6.0;
/// The side of a wire point's square.
const WIRE_POINT_SIZE: f32 = 8.0;

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
#[derive(Debug, Clone, PartialEq)]
pub struct Wire {
    /// The connection it draws; a whole-port stereo connection draws two.
    pub connection: ConnectionId,
    /// The output end.
    pub from: Pin,
    /// The input end.
    pub to: Pin,
    /// How far this wire passes from the connection's points: zero for its first wire, and for
    /// the second of a stereo pair the distance between their pins.
    pub shift: iced::Vector,
    /// Its course, from the output end.
    pub segments: Vec<Segment>,
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
    /// Each connection's shape, as drawn.
    pub shapes: BTreeMap<ConnectionId, WireShape>,
    /// The board's size: at least the pane's, and enough to hold everything.
    pub size: Size,
}

/// A node's input and output ports' layouts. Effects' ports are not known to the model; they
/// are drawn as one stereo input and output. An instrument's event input is one pin, like a mono
/// port's.
fn ports(kind: &NodeKind) -> (Vec<Layout>, Vec<Layout>) {
    match kind {
        NodeKind::Mixer { channels, .. } => (vec![Layout::Mono; *channels], vec![Layout::Stereo]),
        NodeKind::Bus(layout) | NodeKind::FeedbackDelay { layout, .. } => {
            (vec![*layout], vec![*layout])
        }
        NodeKind::Effect { .. } => (vec![Layout::Stereo], vec![Layout::Stereo]),
        NodeKind::Instrument { .. } => (vec![Layout::Mono], vec![Layout::Stereo]),
    }
}

/// A node's title.
pub fn title(kind: &NodeKind) -> String {
    match kind {
        NodeKind::Mixer { .. } => "Mixer".to_owned(),
        NodeKind::Bus(_) => "Bus".to_owned(),
        NodeKind::FeedbackDelay { .. } => "Feedback delay".to_owned(),
        NodeKind::Effect { kind, .. } => {
            audio_effects::display_name(kind).map_or_else(|| kind.clone(), str::to_owned)
        }
        NodeKind::Instrument { kind, .. } => {
            instruments::display_name(kind).map_or_else(|| kind.clone(), str::to_owned)
        }
    }
}

/// The sides of a port of `layout`: one pin for mono, left above right for stereo.
fn sides(layout: Layout) -> &'static [Option<Side>] {
    match layout {
        Layout::Mono => &[None],
        Layout::Stereo => &[Some(Side::Left), Some(Side::Right)],
    }
}

/// What is under a point on the board.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    /// A port circle.
    Pin(Pin),
    /// A node's body.
    Node(NodeId),
    /// A wire.
    Wire(ConnectionId),
    /// A grip of wire `usize` (in [`Board::wires`]) of the connection being shaped.
    Grip(ConnectionId, usize, Grip),
    /// Wire `usize` of the connection being shaped, at `f32` along its segment `usize`.
    WireAt(ConnectionId, usize, usize, f32),
    /// Empty board.
    Nothing,
}

/// How close to a wire a click must be to select it.
const WIRE_REACH: f32 = 6.0;

impl Board {
    /// What is at `at`, while the wires of connection `shaping` show `open`'s handles: one of
    /// their grips first, then a pin, a node, and a wire.
    pub fn hit(&self, at: Point, shaping: Option<ConnectionId>, open: Option<usize>) -> Hit {
        for (index, wire) in self.wires.iter().enumerate() {
            if Some(wire.connection) != shaping {
                continue;
            }
            for (grip, place, _) in wire_edit::grips(&wire.segments, open) {
                if place.distance(at) <= HANDLE_REACH {
                    return Hit::Grip(wire.connection, index, grip);
                }
            }
        }
        for (pin, centre) in &self.pins {
            let radius = match pin.endpoint {
                Endpoint::Output => OUTPUT_RADIUS,
                _ => PIN_RADIUS,
            } + 3.0;
            if centre.distance(at) <= radius {
                return Hit::Pin(*pin);
            }
        }
        if let Some(node) = self.nodes.iter().rev().find(|n| n.rect.contains(at)) {
            return Hit::Node(node.id);
        }
        for (index, wire) in self.wires.iter().enumerate() {
            if let Some((segment, t)) = wire_edit::nearest(&wire.segments, at, WIRE_REACH) {
                return if Some(wire.connection) == shaping {
                    Hit::WireAt(wire.connection, index, segment, t)
                } else {
                    Hit::Wire(wire.connection)
                };
            }
        }
        Hit::Nothing
    }

    /// The pins at wire `index`'s ends.
    fn ends(&self, index: usize) -> Option<(Point, Point)> {
        let wire = self.wires.get(index)?;
        Some((*self.pins.get(&wire.from)?, *self.pins.get(&wire.to)?))
    }
}

/// Whether input `pin` already takes a wire: a connection to that side, or to the whole port.
pub fn occupied(state: &State, pin: Pin) -> bool {
    !pin.output
        && state.connections.iter().any(|c| {
            c.to == pin.endpoint
                && c.input == pin.port
                && (c.to_side.is_none() || pin.side.is_none() || c.to_side == pin.side)
        })
}

/// Whether a wire may run from output `from` to input `to`: an output to a free input of
/// another endpoint carrying the same kind of signal: audio to audio, control to control, or
/// events (a MIDI track's output) to events (an instrument's input).
pub fn can_connect(state: &State, from: Pin, to: Pin) -> bool {
    from.output
        && !to.output
        && from.endpoint != to.endpoint
        && from.endpoint.is_control() == to.endpoint.is_control()
        && is_event(state, from) == is_event(state, to)
        && !occupied(state, to)
}

/// Whether `pin` carries events: a MIDI track's output, or an instrument's input 0.
fn is_event(state: &State, pin: Pin) -> bool {
    match pin.endpoint {
        Endpoint::Track(id) => {
            pin.output && state.track(id).is_some_and(|t| t.kind == TrackKind::Midi)
        }
        Endpoint::Node(id) => {
            !pin.output
                && pin.port == 0
                && matches!(
                    state.node(id).map(|n| &n.kind),
                    Some(NodeKind::Instrument { .. })
                )
        }
        _ => false,
    }
}

/// The colour of a signal from `endpoint`: lavender for control, the tone's audio colour
/// otherwise.
fn signal_colour(t: &Tokens, endpoint: Endpoint) -> iced::Color {
    if endpoint.is_control() {
        CONTROL
    } else {
        t.audio
    }
}

/// Node `node`'s curve port pins, one per assigned parameter, by port number.
fn curve_port_pins(node: &project_model::Node) -> Vec<Pin> {
    let mut numbers: Vec<u32> = node.curve_ports.values().copied().collect();
    numbers.sort_unstable();
    numbers
        .into_iter()
        .map(|n| Pin {
            endpoint: Endpoint::CurvePort(node.id, n),
            output: false,
            port: 0,
            side: None,
        })
        .collect()
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

/// Lays out `state`'s routing. `lanes` gives each track's lane centres, top to bottom, and
/// `curves` each curve lane's centre, in the shared vertical coordinates; the board is at least
/// `min` in size. Nodes are placed in columns by graph depth, each column ordered by where its
/// inputs come from; a node's curve ports follow its audio inputs. The L and R outputs sit just
/// inside the right edge, level with what feeds them.
pub fn layout(
    state: &State,
    lanes: &[(project_model::TrackId, Vec<f32>)],
    curves: &[(CurveId, f32)],
    min: Size,
) -> Board {
    let mut pins = BTreeMap::new();
    for &(curve, y) in curves {
        if state.curve(curve).is_none() {
            continue;
        }
        let pin = Pin {
            endpoint: Endpoint::Curve(curve),
            output: true,
            port: 0,
            side: None,
        };
        pins.insert(pin, Point::new(0.0, y));
    }
    for (track, centres) in lanes {
        let Some(t) = state.track(*track) else {
            continue;
        };
        let endpoint = Endpoint::Track(*track);
        // A MIDI track's event output is one pin, like a mono port's.
        let layout = t.layout().unwrap_or(Layout::Mono);
        for (pin, y) in pins_of(endpoint, true, &[layout]).into_iter().zip(centres) {
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
            let mut inputs = pins_of(Endpoint::Node(node.id), false, &inputs);
            inputs.extend(curve_port_pins(node));
            let outputs = pins_of(Endpoint::Node(node.id), true, &outputs);
            let height = node_height(inputs.len().max(outputs.len()));
            // A node the user placed stays there; the rest stack down their column.
            let (x, y) = match node.position {
                Some(p) => (p.x.max(0.0), p.y.max(0.0)),
                None => {
                    let wanted = if key == f32::MAX {
                        next_free
                    } else {
                        key - PORT_SPACING
                    };
                    let y = wanted.max(next_free);
                    next_free = y + height + PORT_SPACING;
                    (x, y)
                }
            };
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
    let out_y = match state.output_y {
        Some(y) => y.max(0.0),
        None if top == f32::MAX => MARGIN,
        None => top,
    };
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
    let bottom = wires
        .iter()
        .flat_map(|w| w.segments.iter().flatten())
        .map(|p| p.y)
        .fold(bottom, f32::max);
    let shapes = state
        .connections
        .iter()
        .map(|c| (c.id, c.shape.clone()))
        .collect();
    Board {
        nodes,
        pins,
        wires,
        shapes,
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
        // Later wires of the connection pass its points moved by half the sum of how far
        // their pins are from the first wire's, keeping a stereo pair parallel.
        let first = pairs
            .first()
            .and_then(|(f, t)| Some((*pins.get(f)?, *pins.get(t)?)));
        wires.extend(pairs.into_iter().filter_map(|(from, to)| {
            let (a, b) = (*pins.get(&from)?, *pins.get(&to)?);
            let (a0, b0) = first?;
            let shift = ((a - a0) + (b - b0)) * 0.5;
            Some(Wire {
                connection: c.id,
                from,
                to,
                shift,
                segments: wire_edit::segments(a, b, &c.shape, shift),
            })
        }));
    }
    wires
}

/// A wire's course as a path.
fn curve(segments: &[Segment]) -> Path {
    Path::new(|p| {
        if let Some(first) = segments.first() {
            p.move_to(first[0]);
        }
        for [_, c1, c2, end] in segments {
            p.bezier_curve_to(*c1, *c2, *end);
        }
    })
}

/// A port circle at `at`, filled with `fill` and outlined in `stroke`.
fn pin(frame: &mut Frame, at: Point, radius: f32, fill: iced::Color, stroke: iced::Color) {
    let circle = Path::circle(at, radius);
    frame.fill(&circle, fill);
    frame.stroke(
        &circle,
        Stroke::default().with_width(WIRE_WIDTH).with_color(stroke),
    );
}

/// What is selected on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Selection {
    /// A node, whose dialog is open.
    Node(NodeId),
    /// A connection.
    Wire(ConnectionId),
    /// A grip on a connection's wires: an end handle, a point, or a point's handle.
    Grip(ConnectionId, Grip),
}

impl Selection {
    /// The connection whose wires show their handles: a selected wire's, or a selected
    /// handle's.
    pub fn shaping(self) -> Option<ConnectionId> {
        match self {
            Selection::Wire(id) | Selection::Grip(id, _) => Some(id),
            Selection::Node(_) => None,
        }
    }

    /// The point whose handles show: a selected point's, or that of a selected point handle.
    pub fn open_point(self) -> Option<usize> {
        match self {
            Selection::Grip(_, grip) => grip.point(),
            _ => None,
        }
    }
}

/// Draws a laid-out board and turns presses, drags and keys on it into messages: drag from an
/// output to an input to connect, drag a node, the L/R outputs, or a selected wire's handles and
/// points to move them, press a selected wire to add a point there, click to select, Delete or
/// Backspace to remove a selected wire, point or handle.
pub struct BoardView {
    /// The laid-out board.
    pub board: Board,
    /// What is selected.
    pub selection: Option<Selection>,
    /// The output a wire is being dragged from.
    pub wiring: Option<Pin>,
}

/// A drag the board is following.
#[derive(Debug, Clone, Copy, Default)]
pub enum BoardDrag {
    /// None.
    #[default]
    Idle,
    /// A node, held at this offset from its top-left corner.
    Node(NodeId, iced::Vector),
    /// The L/R outputs, held this far below L's centre.
    Output(f32),
    /// A grip of wire `wire` (in [`Board::wires`]) of `connection`, held at `grab` from its
    /// centre.
    Grip {
        /// The connection.
        connection: ConnectionId,
        /// The wire it was caught on.
        wire: usize,
        /// What was caught.
        grip: Grip,
        /// Where the press was, from the grip's centre.
        grab: iced::Vector,
    },
}

/// The board's own state: the drag it is following, the pointer for a wire being drawn, and
/// whether ⌥ is held.
#[derive(Default)]
pub struct BoardState {
    drag: BoardDrag,
    pointer: Option<Point>,
    alt: bool,
}

/// `cursor` relative to `bounds`' top-left, inside or not.
fn relative(cursor: mouse::Cursor, bounds: Rectangle) -> Option<Point> {
    cursor
        .position()
        .map(|p| Point::new(p.x - bounds.x, p.y - bounds.y))
}

impl Program<Message> for BoardView {
    type State = BoardState;

    fn update(
        &self,
        state: &mut BoardState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        use iced::keyboard::{self, key::Named};
        let open = self.selection.and_then(Selection::open_point);
        match event {
            canvas::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
                state.alt = modifiers.alt();
                None
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let at = cursor.position_in(bounds)?;
                let shaping = self.selection.and_then(Selection::shaping);
                let message = match self.board.hit(at, shaping, open) {
                    Hit::Pin(pin) if pin.endpoint == Endpoint::Output => {
                        let centre = self.board.pins.get(&Pin {
                            side: Some(Side::Left),
                            ..pin
                        })?;
                        state.drag = BoardDrag::Output(at.y - centre.y);
                        return Some(canvas::Action::capture());
                    }
                    Hit::Pin(pin) if pin.output => Message::StartWire(pin),
                    Hit::Pin(_) => return Some(canvas::Action::capture()),
                    Hit::Node(id) => {
                        let corner = self
                            .board
                            .nodes
                            .iter()
                            .find(|n| n.id == id)
                            .map(|n| n.rect.position())?;
                        state.drag = BoardDrag::Node(id, at - corner);
                        Message::Select(Some(Selection::Node(id)))
                    }
                    Hit::Wire(id) => Message::Select(Some(Selection::Wire(id))),
                    Hit::Grip(connection, wire, grip) => {
                        let place = wire_edit::grips(&self.board.wires.get(wire)?.segments, open)
                            .into_iter()
                            .find(|g| g.0 == grip)
                            .map(|g| g.1)?;
                        state.drag = BoardDrag::Grip {
                            connection,
                            wire,
                            grip,
                            grab: at - place,
                        };
                        Message::Select(Some(Selection::Grip(connection, grip)))
                    }
                    Hit::WireAt(connection, wire, segment, t) => {
                        let (a, b) = self.board.ends(wire)?;
                        let shift = self.board.wires.get(wire)?.shift;
                        let shape = self.board.shapes.get(&connection)?;
                        let (shape, index) = wire_edit::split(a, b, shape, shift, segment, t)?;
                        let point = shape.points.get(index)?;
                        let place = Point::new(point.at.x, point.at.y) + shift;
                        state.drag = BoardDrag::Grip {
                            connection,
                            wire,
                            grip: Grip::Point(index),
                            grab: at - place,
                        };
                        Message::AddWirePoint(connection, shape, index)
                    }
                    Hit::Nothing => Message::Select(None),
                };
                Some(canvas::Action::publish(message).and_capture())
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                let at = relative(cursor, bounds)?;
                match state.drag {
                    BoardDrag::Node(id, grab) => {
                        let corner = at - grab;
                        Some(canvas::Action::publish(Message::DragNode(
                            id,
                            project_model::Position {
                                x: corner.x.max(0.0),
                                y: corner.y.max(0.0),
                            },
                        )))
                    }
                    BoardDrag::Output(grab) => Some(canvas::Action::publish(Message::DragOutput(
                        (at.y - grab).max(0.0),
                    ))),
                    BoardDrag::Grip {
                        connection,
                        wire,
                        grip,
                        grab,
                    } => {
                        // Kept on the board, so it can always be caught again.
                        let to = at - grab;
                        let to = Point::new(to.x.clamp(0.0, self.board.size.width), to.y.max(0.0));
                        let ends = self.board.ends(wire)?;
                        let shift = self.board.wires.get(wire)?.shift;
                        let shape = self.board.shapes.get(&connection)?;
                        let shape = wire_edit::drag(shape, grip, to, ends, shift, state.alt)?;
                        Some(canvas::Action::publish(Message::DragShape(
                            connection, shape,
                        )))
                    }
                    BoardDrag::Idle if self.wiring.is_some() => {
                        state.pointer = Some(at);
                        Some(canvas::Action::request_redraw())
                    }
                    BoardDrag::Idle => None,
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                if !matches!(state.drag, BoardDrag::Idle) {
                    state.drag = BoardDrag::Idle;
                    return Some(canvas::Action::publish(Message::Drop).and_capture());
                }
                self.wiring?;
                state.pointer = None;
                let target = match cursor
                    .position_in(bounds)
                    .map(|at| self.board.hit(at, None, None))
                {
                    Some(Hit::Pin(pin)) if !pin.output => Some(pin),
                    _ => None,
                };
                Some(canvas::Action::publish(Message::FinishWire(target)).and_capture())
            }
            canvas::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(Named::Delete | Named::Backspace),
                ..
            }) if self.selection.is_some() => {
                // Captured, so the shortcut for deleting a range does not also see it.
                Some(canvas::Action::publish(Message::DeleteSelection).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &BoardState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let t = tokens(tone_of(theme));
        let selected = mix(t.audio, t.text, 0.6);
        // A selected wire's tint, mixed from its signal's colour.
        let tint = |endpoint: Endpoint| mix(signal_colour(t, endpoint), t.text, 0.6);
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), t.pane);
        let board = &self.board;
        let shaping = self.selection.and_then(Selection::shaping);
        for w in &board.wires {
            let chosen = shaping == Some(w.connection);
            let colour = if chosen {
                tint(w.from.endpoint)
            } else {
                signal_colour(t, w.from.endpoint)
            };
            let stroke = Stroke::default()
                .with_width(if chosen { WIRE_WIDTH + 2.0 } else { WIRE_WIDTH })
                .with_color(colour);
            frame.stroke(&curve(&w.segments), stroke);
        }
        // The shaped connection's grips over every wire: handles as circles on stems, points
        // as squares; the selected one filled.
        let open = self.selection.and_then(Selection::open_point);
        for w in board.wires.iter().filter(|w| shaping == Some(w.connection)) {
            let colour = tint(w.from.endpoint);
            for (grip, place, stem) in wire_edit::grips(&w.segments, open) {
                let held = self.selection == Some(Selection::Grip(w.connection, grip));
                let fill = if held { colour } else { t.pane };
                let outline = Stroke::default().with_width(1.5).with_color(colour);
                if let Some(stem) = stem {
                    frame.stroke(
                        &Path::line(stem, place),
                        Stroke::default().with_width(1.0).with_color(colour),
                    );
                    let circle = Path::circle(place, HANDLE_RADIUS);
                    frame.fill(&circle, fill);
                    frame.stroke(&circle, outline);
                } else {
                    let half = WIRE_POINT_SIZE / 2.0;
                    let square = Path::rectangle(
                        Point::new(place.x - half, place.y - half),
                        Size::new(WIRE_POINT_SIZE, WIRE_POINT_SIZE),
                    );
                    frame.fill(&square, fill);
                    frame.stroke(&square, outline);
                }
            }
        }
        if let (Some(from), Some(to)) = (self.wiring, state.pointer)
            && let Some(a) = board.pins.get(&from)
        {
            let stroke = Stroke::default()
                .with_width(WIRE_WIDTH)
                .with_color(tint(from.endpoint));
            let (c1, c2) = wire_edit::automatic(*a, to);
            frame.stroke(&curve(&[[*a, c1, c2, to]]), stroke);
        }
        for node in &board.nodes {
            frame.fill_rectangle(node.rect.position(), node.rect.size(), t.lane);
            if self.selection == Some(Selection::Node(node.id)) {
                frame.stroke(
                    &Path::rectangle(node.rect.position(), node.rect.size()),
                    Stroke::default().with_width(2.0).with_color(selected),
                );
            }
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
        for (p, at) in &board.pins {
            match p.endpoint {
                // Drawn in the gutter; the wire starts at the board's edge.
                Endpoint::Track(_) | Endpoint::Curve(_) => {}
                Endpoint::Output => {
                    pin(&mut frame, *at, OUTPUT_RADIUS, t.lane, t.audio);
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
                Endpoint::Node(_) => pin(&mut frame, *at, PIN_RADIUS, t.lane, t.audio),
                Endpoint::CurvePort(_, number) => {
                    pin(&mut frame, *at, PIN_RADIUS, t.lane, CONTROL);
                    frame.fill_text(Text {
                        content: number.to_string(),
                        position: *at,
                        color: t.text,
                        size: 11.0.into(),
                        align_x: iced::alignment::Horizontal::Center.into(),
                        align_y: iced::alignment::Vertical::Center,
                        ..Text::default()
                    });
                }
            }
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        state: &BoardState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        match state.drag {
            BoardDrag::Node(..) => return mouse::Interaction::Grabbing,
            BoardDrag::Output(_) => return mouse::Interaction::ResizingVertically,
            BoardDrag::Grip { .. } => return mouse::Interaction::Grabbing,
            BoardDrag::Idle => {}
        }
        let shaping = self.selection.and_then(Selection::shaping);
        match cursor.position_in(bounds).map(|at| {
            let open = self.selection.and_then(Selection::open_point);
            self.board.hit(at, shaping, open)
        }) {
            Some(Hit::Pin(p)) if p.endpoint == Endpoint::Output => {
                mouse::Interaction::ResizingVertically
            }
            Some(Hit::Pin(p)) if p.output => mouse::Interaction::Crosshair,
            Some(Hit::Node(_)) => mouse::Interaction::Grab,
            Some(Hit::Wire(_)) => mouse::Interaction::Pointer,
            Some(Hit::Grip(..)) => mouse::Interaction::Grab,
            Some(Hit::WireAt(..)) => mouse::Interaction::Crosshair,
            _ => mouse::Interaction::default(),
        }
    }
}

/// Draws the port gutter on the board's colour: a pin on each lane near the divider, with a stub
/// to the board, in the audio colour for a track and lavender for a curve. Pressing a pin starts
/// a wire from it.
pub struct GutterView(pub Vec<(Pin, f32)>);

/// The x of a gutter pin's centre: its outer edge, stroke included, sits [`GUTTER_PIN_INSET`] from
/// the divider.
pub(crate) fn gutter_pin_x() -> f32 {
    GUTTER_PIN_INSET + WIRE_WIDTH / 2.0 + PIN_RADIUS
}

impl Program<Message> for GutterView {
    type State = ();

    fn update(
        &self,
        _state: &mut (),
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event else {
            return None;
        };
        let at = cursor.position_in(bounds)?;
        self.0
            .iter()
            .find(|(_, y)| Point::new(gutter_pin_x(), *y).distance(at) <= PIN_RADIUS + 3.0)
            .map(|(pin, _)| canvas::Action::publish(Message::StartWire(*pin)).and_capture())
    }

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
        let x = gutter_pin_x();
        for &(p, y) in &self.0 {
            let colour = signal_colour(t, p.endpoint);
            frame.fill_rectangle(
                Point::new(x, y - WIRE_WIDTH / 2.0),
                Size::new(bounds.width - x, WIRE_WIDTH),
                colour,
            );
            pin(&mut frame, Point::new(x, y), PIN_RADIUS, t.lane, colour);
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &(),
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        let over = cursor.position_in(bounds).is_some_and(|at| {
            self.0
                .iter()
                .any(|(_, y)| Point::new(gutter_pin_x(), *y).distance(at) <= PIN_RADIUS + 3.0)
        });
        if over {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
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
        let board = layout(session.state(), &lanes, &[], Size::new(500.0, 400.0));
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
        let board = layout(session.state(), &[], &[], Size::new(100.0, 100.0));
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
        let board = layout(session.state(), &[], &[], Size::new(100.0, 100.0));
        for pair in board.nodes.windows(2) {
            assert!(pair[1].rect.y >= pair[0].rect.y + pair[0].rect.height);
        }
        Ok(())
    }

    #[test]
    fn test_hit_finds_pins_nodes_wires_and_nothing() -> Result {
        let mut session = Session::new_project();
        let vocal = session.add_track("Vocal", Layout::Mono)?;
        let board = layout(
            session.state(),
            &[(vocal, vec![90.0])],
            &[],
            Size::new(600.0, 400.0),
        );
        let mixer = &board.nodes[0];
        let input = Pin {
            endpoint: Endpoint::Node(mixer.id),
            output: false,
            port: 0,
            side: None,
        };
        let at = *board.pins.get(&input).ok_or("no input")?;
        assert_eq!(
            board.hit(Point::new(at.x + 4.0, at.y), None, None),
            Hit::Pin(input)
        );
        assert_eq!(
            board.hit(mixer.rect.center(), None, None),
            Hit::Node(mixer.id)
        );
        // Midway along the vocal's wire.
        let (index, wire) = board
            .wires
            .iter()
            .enumerate()
            .find(|(_, w)| w.to == input)
            .ok_or("no wire")?;
        let midway = wire_edit::along(&wire.segments[0], 0.5);
        assert_eq!(board.hit(midway, None, None), Hit::Wire(wire.connection));
        assert_eq!(board.hit(Point::new(5.0, 390.0), None, None), Hit::Nothing);
        // On the connection being shaped, a press on the wire is a place to add a point, and
        // its handles are caught; elsewhere they are not.
        let shaping = Some(wire.connection);
        assert!(matches!(
            board.hit(midway, shaping, None),
            Hit::WireAt(c, i, 0, _) if c == wire.connection && i == index
        ));
        let [_, c1, c2, _] = wire.segments[0];
        assert!(!matches!(board.hit(c1, None, None), Hit::Grip(..)));
        assert_eq!(
            board.hit(c1, shaping, None),
            Hit::Grip(wire.connection, index, Grip::OutHandle)
        );
        assert_eq!(
            board.hit(c2, shaping, None),
            Hit::Grip(wire.connection, index, Grip::InHandle)
        );
        Ok(())
    }

    #[test]
    fn test_wires_follow_their_shape_and_a_stereo_pair_runs_parallel() -> Result {
        use project_model::{WireHandle, WirePoint, WireShape};
        let mut session = Session::new_project();
        session.add_track("Keys", Layout::Stereo)?;
        let mixer = session.state().project_mixer().ok_or("no mixer")?;
        // The mixer's whole-port connection to the output, drawn as two wires.
        let id = session
            .state()
            .connections
            .iter()
            .find(|c| c.to == Endpoint::Output)
            .map(|c| c.id)
            .ok_or("no output wire")?;
        let unshaped = layout(session.state(), &[], &[], Size::new(800.0, 400.0));
        let shape = WireShape {
            out_handle: Some(WireHandle { dx: 50.0, dy: 70.0 }),
            in_handle: None,
            points: vec![WirePoint {
                at: project_model::Position { x: 400.0, y: 500.0 },
                in_handle: WireHandle { dx: -30.0, dy: 0.0 },
                out_handle: WireHandle { dx: 30.0, dy: 0.0 },
            }],
        };
        session.shape_wire(id, shape)?;
        let check = |board: &Board| -> Result {
            let wires: Vec<&Wire> = board.wires.iter().filter(|w| w.connection == id).collect();
            assert_eq!(wires.len(), 2);
            for w in &wires {
                let a = *board.pins.get(&w.from).ok_or("no from")?;
                let b = *board.pins.get(&w.to).ok_or("no to")?;
                assert_eq!(w.segments.len(), 2);
                // Both wires share the end handle's offset; the unshaped end is automatic.
                assert_eq!(w.segments[0][1], Point::new(a.x + 50.0, a.y + 70.0));
                assert_eq!(
                    w.segments[1][2],
                    wire_edit::automatic(w.segments[1][0], b).1
                );
            }
            // The second passes the point moved as far as its pins are from the first's.
            assert_eq!(wires[0].segments[0][3], Point::new(400.0, 500.0));
            assert_eq!(
                wires[1].segments[0][3],
                Point::new(400.0, 500.0 + PORT_SPACING)
            );
            Ok(())
        };
        let board = layout(session.state(), &[], &[], Size::new(800.0, 400.0));
        check(&board)?;
        assert_ne!(board.wires, unshaped.wires);
        session.move_node(mixer, Some(project_model::Position { x: 120.0, y: 300.0 }))?;
        let board = layout(session.state(), &[], &[], Size::new(800.0, 400.0));
        check(&board)?;
        // The board grows to hold the point.
        assert!(board.size.height > 500.0 + PORT_SPACING);
        Ok(())
    }

    #[test]
    fn test_midi_track_pin_is_laid_out_but_connects_to_nothing() -> Result {
        let mut session = Session::new_project();
        let keys = session.add_midi_track("Keys")?;
        let mixer = session.state().project_mixer().ok_or("no mixer")?;
        let out = Pin {
            endpoint: Endpoint::Track(keys),
            output: true,
            port: 0,
            side: None,
        };
        let board = layout(
            session.state(),
            &[(keys, vec![50.0])],
            &[],
            Size::new(500.0, 400.0),
        );
        assert!(board.pins.contains_key(&out));
        for to in [Endpoint::Node(mixer), Endpoint::Output] {
            let input = Pin {
                endpoint: to,
                output: false,
                port: 0,
                side: None,
            };
            assert!(!can_connect(session.state(), out, input));
        }
        Ok(())
    }

    #[test]
    fn test_midi_track_connects_only_to_instrument_input() -> Result {
        let mut session = Session::new_project();
        let keys = session.add_midi_track("Keys")?;
        let vocal = session.add_track("Vocal", Layout::Mono)?;
        let synth = session.add_node(NodeKind::Instrument {
            kind: "from-the-future".to_owned(),
            parameters: std::collections::BTreeMap::new(),
            zones: Vec::new(),
        })?;
        let bus = session.add_node(NodeKind::Bus(Layout::Stereo))?;
        let pin = |endpoint, output, port| Pin {
            endpoint,
            output,
            port,
            side: None,
        };
        let board = layout(
            session.state(),
            &[(keys, vec![50.0]), (vocal, vec![150.0])],
            &[],
            Size::new(500.0, 400.0),
        );
        let event_in = pin(Endpoint::Node(synth), false, 0);
        assert!(board.pins.contains_key(&event_in));
        assert_eq!(
            board
                .nodes
                .iter()
                .find(|n| n.id == synth)
                .map(|n| &n.title[..]),
            Some("from-the-future")
        );
        let state = session.state();
        assert!(can_connect(
            state,
            pin(Endpoint::Track(keys), true, 0),
            event_in
        ));
        assert!(!can_connect(
            state,
            pin(Endpoint::Track(vocal), true, 0),
            event_in
        ));
        let stereo_out = Pin {
            side: Some(Side::Left),
            ..pin(Endpoint::Node(synth), true, 0)
        };
        assert!(board.pins.contains_key(&stereo_out));
        let bus_in = Pin {
            side: Some(Side::Left),
            ..pin(Endpoint::Node(bus), false, 0)
        };
        assert!(can_connect(state, stereo_out, bus_in));
        assert!(!can_connect(
            state,
            pin(Endpoint::Track(keys), true, 0),
            bus_in
        ));
        Ok(())
    }

    #[test]
    fn test_inputs_take_one_wire() -> Result {
        let mut session = Session::new_project();
        let vocal = session.add_track("Vocal", Layout::Mono)?;
        let mixer = session.state().project_mixer().ok_or("no mixer")?;
        let out = Pin {
            endpoint: Endpoint::Track(vocal),
            output: true,
            port: 0,
            side: None,
        };
        let taken = Pin {
            endpoint: Endpoint::Node(mixer),
            output: false,
            port: 0,
            side: None,
        };
        let left = Pin {
            endpoint: Endpoint::Output,
            output: false,
            port: 0,
            side: Some(Side::Left),
        };
        assert!(occupied(session.state(), taken));
        assert!(!can_connect(session.state(), out, taken));
        // The mixer feeds the whole output port, so both sides are taken.
        assert!(occupied(session.state(), left));
        assert!(!can_connect(session.state(), taken, out), "input to output");
        session.add_mixer_channels(mixer, 1)?;
        let free = Pin { port: 1, ..taken };
        assert!(can_connect(session.state(), out, free));
        Ok(())
    }

    #[test]
    fn test_layout_adds_curve_pins_and_ports_and_grows_nodes() -> Result {
        let mut session = Session::new_project();
        let mixer = session.state().project_mixer().ok_or("no mixer")?;
        let curve = session.add_curve("Curve 1", modulation::Curve::default())?;
        let before = layout(
            session.state(),
            &[],
            &[(curve, 50.0)],
            Size::new(100.0, 100.0),
        );
        let gutter = Pin {
            endpoint: Endpoint::Curve(curve),
            output: true,
            port: 0,
            side: None,
        };
        assert_eq!(before.pins.get(&gutter), Some(&Point::new(0.0, 50.0)));
        let height = before.nodes[0].rect.height;
        session.assign_curve_port(mixer, "channel 1 gain")?;
        session.assign_curve_port(mixer, "channel 1 pan")?;
        session.connect_curve(curve, mixer, 2)?;
        let after = layout(
            session.state(),
            &[],
            &[(curve, 50.0)],
            Size::new(100.0, 100.0),
        );
        // One mono input and two curve ports now outnumber the stereo output's two pins.
        assert!(after.nodes[0].rect.height > height);
        assert_eq!(after.nodes[0].rect.height, node_height(3));
        let port = |n| Pin {
            endpoint: Endpoint::CurvePort(mixer, n),
            output: false,
            port: 0,
            side: None,
        };
        // Below the audio inputs, in number order.
        let (one, two) = (after.pins[&port(1)], after.pins[&port(2)]);
        assert!(two.y > one.y);
        assert!(
            after
                .wires
                .iter()
                .any(|w| w.from == gutter && w.to == port(2))
        );
        // A curve wires only to a curve port.
        let input = Pin {
            endpoint: Endpoint::Node(mixer),
            output: false,
            port: 1,
            side: None,
        };
        session.add_mixer_channels(mixer, 1)?;
        assert!(!can_connect(session.state(), gutter, input));
        assert!(can_connect(session.state(), gutter, port(1)));
        Ok(())
    }

    #[test]
    fn test_placed_nodes_and_outputs_stay_where_put() -> Result {
        let mut session = Session::new_project();
        let mixer = session.state().project_mixer().ok_or("no mixer")?;
        session.move_node(mixer, Some(project_model::Position { x: 300.0, y: 500.0 }))?;
        session.move_output(Some(40.0))?;
        let board = layout(session.state(), &[], &[], Size::new(100.0, 100.0));
        assert_eq!(board.nodes[0].rect.position(), Point::new(300.0, 500.0));
        let left = Pin {
            endpoint: Endpoint::Output,
            output: false,
            port: 0,
            side: Some(Side::Left),
        };
        assert_eq!(board.pins.get(&left).map(|p| p.y), Some(40.0));
        // Still to the right of every node, and the board grows to hold it.
        assert!(
            board
                .pins
                .get(&left)
                .is_some_and(|p| p.x > 300.0 + NODE_WIDTH)
        );
        assert!(board.size.height > 500.0);
        Ok(())
    }
}
