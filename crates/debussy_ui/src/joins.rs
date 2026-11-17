//! Butted nodes on the patch board: where two nodes pushed edge to edge line up an output pin with
//! an input pin, a connection between those two pins is a joint, drawn as one pin and wired with
//! no wire (`docs/architecture/patch-board.md` "Patch board").
//!
//! Everything here works on a laid-out [`Board`], so it is tested without a window.

use std::collections::{BTreeMap, BTreeSet};

use iced::{Point, Rectangle, Vector};
use project_model::{ConnectionId, Endpoint, JoinSpec, NodeId, Position, Side, State, WireShape};

use crate::patch_board::{Board, Pin, Wire, kinds_match, occupied};

/// How close two pins must be to meet.
const MEET: f32 = 0.5;
/// How close to a butted place a dragged node must come to snap to it.
pub const SNAP_REACH: f32 = 8.0;

/// An output pin and an input pin that meet on the board.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Join {
    /// The output.
    pub from: Pin,
    /// The input.
    pub to: Pin,
}

/// The joins on a laid-out board: each wire whose two node pins meet.
pub(crate) fn joins(pins: &BTreeMap<Pin, Point>, wires: &[Wire]) -> Vec<Join> {
    let mut joins: Vec<Join> = wires
        .iter()
        .filter(|w| is_node(w.from) && is_node(w.to))
        .filter(|w| match (pins.get(&w.from), pins.get(&w.to)) {
            (Some(a), Some(b)) => a.distance(*b) <= MEET,
            _ => false,
        })
        .map(|w| Join {
            from: w.from,
            to: w.to,
        })
        .collect();
    joins.sort();
    joins.dedup();
    joins
}

/// Whether `pin` is one of a routing node's audio or event pins.
fn is_node(pin: Pin) -> bool {
    matches!(pin.endpoint, Endpoint::Node(_))
}

/// The node a pin is on.
fn node_of(pin: Pin) -> Option<NodeId> {
    match pin.endpoint {
        Endpoint::Node(id) => Some(id),
        _ => None,
    }
}

/// Whether `join` may be a joint: its pins carry the same kind of signal, and each is wired to
/// nothing but the other. A whole-port connection counts for both of its sides.
pub fn eligible(board: &Board, state: &State, join: Join) -> bool {
    let touching = |w: &&Wire| w.from == join.from || w.to == join.to;
    let only_each_other = board
        .wires
        .iter()
        .filter(touching)
        .all(|w| w.from == join.from && w.to == join.to);
    let wired = board
        .wires
        .iter()
        .any(|w| w.from == join.from && w.to == join.to);
    kinds_match(state, join.from, join.to)
        && only_each_other
        && (wired || !occupied(state, join.to))
}

/// `node` and every node reachable from it through joins.
pub fn chain(board: &Board, node: NodeId) -> BTreeSet<NodeId> {
    let mut found = BTreeSet::from([node]);
    let mut next = vec![node];
    while let Some(at) = next.pop() {
        for join in &board.joins {
            let (Some(a), Some(b)) = (node_of(join.from), node_of(join.to)) else {
                continue;
            };
            for (here, there) in [(a, b), (b, a)] {
                if here == at && found.insert(there) {
                    next.push(there);
                }
            }
        }
    }
    found
}

/// A butted place for the nodes being moved, and what dropping them there does.
#[derive(Debug, Clone, PartialEq)]
pub struct Snap {
    /// How far to move every moving node from where it is on the board.
    pub offset: Vector,
    /// The connections to make for pins that meet there and are not yet wired.
    pub joins: Vec<JoinSpec>,
    /// Joined connections already wired whose shape is not the automatic one.
    pub unshape: Vec<ConnectionId>,
    /// Butted neighbours placed automatically, to be kept where they are drawn.
    pub pin: Vec<(NodeId, Position)>,
}

/// The nearest butted place within [`SNAP_REACH`] for the `moving` nodes, as `board` lays them
/// out: one where a moving node's left edge meets a resting node's right edge, or the reverse,
/// with their pin rows in step, at least one output and input pin meet, and every pair that
/// meets is [`eligible`].
pub fn snap(board: &Board, state: &State, moving: &BTreeSet<NodeId>) -> Option<Snap> {
    let (going, resting): (Vec<_>, Vec<_>) =
        board.nodes.iter().partition(|n| moving.contains(&n.id));
    let spacing = crate::patch_board::PORT_SPACING;
    let mut candidates: Vec<Vector> = Vec::new();
    for m in &going {
        for n in &resting {
            for dx in [
                n.rect.x + n.rect.width - m.rect.x,
                n.rect.x - (m.rect.x + m.rect.width),
            ] {
                let k = ((m.rect.y - n.rect.y) / spacing).round();
                let dy = n.rect.y + k * spacing - m.rect.y;
                if dx.abs() <= SNAP_REACH && dy.abs() <= SNAP_REACH {
                    candidates.push(Vector::new(dx, dy));
                }
            }
        }
    }
    candidates.sort_by(|a, b| length(*a).total_cmp(&length(*b)));
    let on_board = |offset: Vector| {
        going
            .iter()
            .all(|m| m.rect.x + offset.x >= 0.0 && m.rect.y + offset.y >= 0.0)
    };
    candidates.into_iter().find_map(|offset| {
        let meeting = meeting(board, moving, offset);
        let fits = on_board(offset)
            && !meeting.is_empty()
            && meeting.iter().all(|j| eligible(board, state, *j));
        fits.then(|| place(board, state, moving, offset, &meeting))
    })
}

/// A vector's length.
fn length(v: Vector) -> f32 {
    v.x.hypot(v.y)
}

/// The output and input pins that meet between a moving node and a resting one when the moving
/// ones move by `offset`.
fn meeting(board: &Board, moving: &BTreeSet<NodeId>, offset: Vector) -> Vec<Join> {
    let moves = |pin: Pin| node_of(pin).is_some_and(|id| moving.contains(&id));
    let at = |pin: Pin, p: Point| if moves(pin) { p + offset } else { p };
    let node_pins = || board.pins.iter().filter(|(p, _)| is_node(**p));
    let mut found = Vec::new();
    for (&from, &a) in node_pins().filter(|(p, _)| p.output) {
        for (&to, &b) in node_pins().filter(|(p, _)| !p.output) {
            if moves(from) != moves(to) && at(from, a).distance(at(to, b)) <= MEET {
                found.push(Join { from, to });
            }
        }
    }
    found
}

/// What snapping by `offset` does, given the pins `meeting` there.
fn place(
    board: &Board,
    state: &State,
    moving: &BTreeSet<NodeId>,
    offset: Vector,
    meeting: &[Join],
) -> Snap {
    let wire_of = |j: &Join| {
        board
            .wires
            .iter()
            .find(|w| w.from == j.from && w.to == j.to)
            .map(|w| w.connection)
    };
    let mut unshape: Vec<ConnectionId> = meeting
        .iter()
        .filter_map(wire_of)
        .filter(|id| {
            board
                .shapes
                .get(id)
                .is_some_and(|s| *s != WireShape::default())
        })
        .collect();
    unshape.sort();
    unshape.dedup();
    // New pins by the ports they join: both sides of a stereo output meeting both sides of a
    // stereo input in order make one whole-port connection.
    let mut ports: BTreeMap<(Endpoint, usize, Endpoint, usize), Vec<Join>> = BTreeMap::new();
    for j in meeting.iter().filter(|j| wire_of(j).is_none()) {
        ports
            .entry((j.from.endpoint, j.from.port, j.to.endpoint, j.to.port))
            .or_default()
            .push(*j);
    }
    let mut joins = Vec::new();
    for ((from, output, to, input), pairs) in ports {
        let sides: Vec<_> = pairs.iter().map(|j| (j.from.side, j.to.side)).collect();
        let whole = [
            (Some(Side::Left), Some(Side::Left)),
            (Some(Side::Right), Some(Side::Right)),
        ];
        let spec = |from_side, to_side| JoinSpec {
            from,
            output,
            from_side,
            to,
            input,
            to_side,
        };
        if sides.len() == 2 && whole.iter().all(|s| sides.contains(s)) {
            joins.push(spec(None, None));
        } else {
            joins.extend(sides.into_iter().map(|(f, t)| spec(f, t)));
        }
    }
    let mut pin: Vec<(NodeId, Position)> = meeting
        .iter()
        .flat_map(|j| [j.from, j.to])
        .filter_map(node_of)
        .filter(|id| !moving.contains(id))
        .filter(|id| state.node(*id).is_some_and(|n| n.position.is_none()))
        .filter_map(|id| {
            let rect: Rectangle = board.nodes.iter().find(|n| n.id == id)?.rect;
            Some((
                id,
                Position {
                    x: rect.x,
                    y: rect.y,
                },
            ))
        })
        .collect();
    pin.sort_by_key(|(id, _)| *id);
    pin.dedup_by_key(|(id, _)| *id);
    Snap {
        offset,
        joins,
        unshape,
        pin,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::patch_board::{Hit, NODE_WIDTH, PORT_SPACING, layout};
    use audio_buffers::Layout;
    use iced::Size;
    use project_model::{NodeKind, Session};

    type Result = std::result::Result<(), Box<dyn std::error::Error>>;

    /// A stereo bus placed at (`x`, `y`).
    fn bus_at(
        session: &mut Session,
        x: f32,
        y: f32,
    ) -> std::result::Result<NodeId, Box<dyn std::error::Error>> {
        let id = session.add_node(NodeKind::Bus(Layout::Stereo))?;
        session.move_node(id, Some(Position { x, y }))?;
        Ok(id)
    }

    /// `session`'s board.
    fn board(session: &Session) -> Board {
        layout(session.state(), &[], &[], Size::new(800.0, 600.0))
    }

    /// Where `moving` would snap on `session`'s board.
    fn snapped(session: &Session, moving: NodeId) -> Option<Snap> {
        snap(&board(session), session.state(), &BTreeSet::from([moving]))
    }

    fn whole(from: NodeId, to: NodeId) -> JoinSpec {
        JoinSpec {
            from: Endpoint::Node(from),
            output: 0,
            from_side: None,
            to: Endpoint::Node(to),
            input: 0,
            to_side: None,
        }
    }

    #[test]
    fn test_snap_aligns_level_nodes_and_joins_whole_ports() -> Result {
        let mut session = Session::new();
        let a = bus_at(&mut session, 100.0, 100.0)?;
        let b = bus_at(&mut session, 100.0 + NODE_WIDTH + 5.0, 103.0)?;
        let snap = snapped(&session, b).ok_or("no snap")?;
        assert_eq!(snap.offset, Vector::new(-5.0, -3.0));
        assert_eq!(snap.joins, vec![whole(a, b)]);
        assert!(snap.unshape.is_empty() && snap.pin.is_empty());
        // Too far away: no snap.
        session.move_node(
            b,
            Some(Position {
                x: 100.0 + NODE_WIDTH + 20.0,
                y: 100.0,
            }),
        )?;
        assert_eq!(snapped(&session, b), None);
        Ok(())
    }

    #[test]
    fn test_snap_aligns_a_slot_lower_and_joins_one_side() -> Result {
        let mut session = Session::new();
        let a = bus_at(&mut session, 300.0, 100.0)?;
        // B to A's left, one slot lower: B's left output meets A's right input.
        let b = bus_at(
            &mut session,
            300.0 - NODE_WIDTH - 3.0,
            100.0 + PORT_SPACING + 4.0,
        )?;
        let snap = snapped(&session, b).ok_or("no snap")?;
        assert_eq!(snap.offset, Vector::new(3.0, -4.0));
        assert_eq!(
            snap.joins,
            vec![JoinSpec {
                from_side: Some(Side::Left),
                to_side: Some(Side::Right),
                ..whole(b, a)
            }]
        );
        Ok(())
    }

    #[test]
    fn test_snap_refuses_pins_wired_elsewhere() -> Result {
        let mut session = Session::new();
        let a = bus_at(&mut session, 100.0, 100.0)?;
        let b = bus_at(&mut session, 100.0 + NODE_WIDTH + 2.0, 100.0)?;
        let c = bus_at(&mut session, 500.0, 400.0)?;
        // B's input is fed from elsewhere.
        let elsewhere = session.connect(Endpoint::Node(c), 0, Endpoint::Node(b), 0)?;
        assert_eq!(snapped(&session, b), None);
        session.disconnect(elsewhere)?;
        // A's output goes elsewhere.
        session.connect(Endpoint::Node(a), 0, Endpoint::Node(c), 0)?;
        assert_eq!(snapped(&session, b), None);
        Ok(())
    }

    #[test]
    fn test_snap_accepts_pins_already_wired_together() -> Result {
        let mut session = Session::new();
        let a = bus_at(&mut session, 100.0, 100.0)?;
        let b = bus_at(&mut session, 400.0, 100.0)?;
        let id = session.connect(Endpoint::Node(a), 0, Endpoint::Node(b), 0)?;
        session.shape_wire(
            id,
            WireShape {
                out_handle: Some(project_model::WireHandle { dx: 9.0, dy: 9.0 }),
                ..WireShape::default()
            },
        )?;
        session.move_node(
            b,
            Some(Position {
                x: 100.0 + NODE_WIDTH + 1.0,
                y: 100.0,
            }),
        )?;
        let snap = snapped(&session, b).ok_or("no snap")?;
        assert!(snap.joins.is_empty());
        assert_eq!(snap.unshape, vec![id]);
        Ok(())
    }

    #[test]
    fn test_snap_joins_stereo_to_mono_inputs_side_by_side() -> Result {
        let mut session = Session::new_project();
        let mixer = session.state().project_mixer().ok_or("no mixer")?;
        session.add_mixer_channels(mixer, 2)?;
        session.move_node(mixer, Some(Position { x: 400.0, y: 100.0 }))?;
        let a = bus_at(&mut session, 400.0 - NODE_WIDTH - 4.0, 100.0)?;
        let snap = snapped(&session, a).ok_or("no snap")?;
        let side = |side, input| JoinSpec {
            from_side: Some(side),
            input,
            ..whole(a, mixer)
        };
        assert_eq!(snap.joins, vec![side(Side::Left, 0), side(Side::Right, 1)]);
        Ok(())
    }

    #[test]
    fn test_snap_refuses_an_event_input() -> Result {
        let mut session = Session::new();
        bus_at(&mut session, 100.0, 100.0)?;
        let synth = session.add_node(NodeKind::Instrument {
            kind: "from-the-future".to_owned(),
            parameters: BTreeMap::new(),
            zones: Vec::new(),
        })?;
        session.move_node(
            synth,
            Some(Position {
                x: 100.0 + NODE_WIDTH + 2.0,
                y: 100.0,
            }),
        )?;
        assert_eq!(snapped(&session, synth), None);
        Ok(())
    }

    #[test]
    fn test_snap_keeps_an_automatically_placed_neighbour_where_it_is() -> Result {
        let mut session = Session::new();
        let a = session.add_node(NodeKind::Bus(Layout::Stereo))?;
        let at = board(&session).nodes[0].rect;
        let b = bus_at(&mut session, at.x + NODE_WIDTH + 2.0, at.y)?;
        let snap = snapped(&session, b).ok_or("no snap")?;
        assert_eq!(snap.pin, vec![(a, Position { x: at.x, y: at.y })]);
        Ok(())
    }

    #[test]
    fn test_chain_follows_joins_and_hit_passes_through_a_joint() -> Result {
        let mut session = Session::new();
        let a = bus_at(&mut session, 100.0, 100.0)?;
        let b = bus_at(&mut session, 100.0 + NODE_WIDTH, 100.0)?;
        let c = bus_at(&mut session, 100.0 + 2.0 * NODE_WIDTH, 100.0)?;
        let apart = bus_at(&mut session, 100.0 + 3.0 * NODE_WIDTH, 100.0)?;
        session.connect(Endpoint::Node(a), 0, Endpoint::Node(b), 0)?;
        session.connect(Endpoint::Node(b), 0, Endpoint::Node(c), 0)?;
        let board = board(&session);
        // Two joins, two pins each; C and the unwired node meet but are not joined.
        assert_eq!(board.joins.len(), 4);
        assert_eq!(chain(&board, c), BTreeSet::from([a, b, c]));
        assert_eq!(chain(&board, apart), BTreeSet::from([apart]));
        let joint = board.joins[0].from;
        let centre = board.pins[&joint];
        assert!(board.joints.contains(&joint));
        assert!(matches!(
            board.hit(centre + Vector::new(-2.0, 0.0), None, None),
            Hit::Node(n) if n == a
        ));
        assert!(matches!(
            board.hit(centre + Vector::new(2.0, 0.0), None, None),
            Hit::Node(n) if n == b
        ));
        // C's free output, meeting the unwired node's input, is still a pin.
        let free = Pin {
            endpoint: Endpoint::Node(c),
            output: true,
            port: 0,
            side: Some(Side::Left),
        };
        assert_eq!(board.hit(board.pins[&free], None, None), Hit::Pin(free));
        Ok(())
    }
}
