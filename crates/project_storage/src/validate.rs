//! Converting between the saved format and `project_model`, checking everything a file from
//! elsewhere could get wrong (`docs/architecture/project.md` "Loading", ADR-0049).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use audio_buffers::Layout;
use modulation::{Curve, Handle, Point};
use project_model::{
    AudioFile, Clip, ClipId, Connection, ConnectionId, Control, CurveId, CurveLane, Edit, Endpoint,
    Key, MAX_BOARD_COORDINATE, MAX_WIRE_POINTS, Mode, Node, NodeId, NodeKind, Notation, Note,
    Position, Sequence, Side, Source, State, Tempo, Track, TrackId, TrackKind, WireHandle,
    WirePoint, WireShape,
};

use crate::format::{
    FORMAT_VERSION, SavedClip, SavedConnection, SavedCurve, SavedEndpoint, SavedFile, SavedHandle,
    SavedKey, SavedLayout, SavedName, SavedNode, SavedNodeKind, SavedPoint, SavedPosition,
    SavedProject, SavedSequence, SavedSide, SavedSource, SavedTempo, SavedTrack, SavedTrackKind,
    SavedWireHandle, SavedWirePoint,
};

fn side_to_saved(side: Option<Side>) -> Option<SavedSide> {
    side.map(|s| match s {
        Side::Left => SavedSide::Left,
        Side::Right => SavedSide::Right,
    })
}

fn side_from_saved(side: Option<SavedSide>) -> Option<Side> {
    side.map(|s| match s {
        SavedSide::Left => Side::Left,
        SavedSide::Right => Side::Right,
    })
}

/// The project sample rates (`docs/architecture/engine.md` "Sample rate").
pub const SAMPLE_RATES: [u32; 4] = [44_100, 48_000, 88_200, 96_000];

/// The most tracks a file may hold.
pub const MAX_TRACKS: usize = 10_000;
/// The most clips a file may hold, over all tracks.
pub const MAX_CLIPS: usize = 1_000_000;
/// The most routing nodes a file may hold.
pub const MAX_NODES: usize = 10_000;
/// The most connections a file may hold.
pub const MAX_CONNECTIONS: usize = 100_000;
/// The most parameters one node may hold.
pub const MAX_PARAMETERS: usize = 4096;
/// The most channels a mixer may have in a file.
pub const MAX_MIXER_CHANNELS: usize = 4096;
/// The most automation curves a file may hold.
pub const MAX_CURVES: usize = 10_000;
/// The most curve points a file may hold, over all curves.
pub const MAX_CURVE_POINTS: usize = 1_000_000;
/// The most MIDI sequences a file may hold.
pub const MAX_SEQUENCES: usize = 1_000_000;
/// The most MIDI notes and control changes a file may hold, together, over all sequences.
pub const MAX_MIDI_EVENTS: usize = 2_000_000;
/// The longest name, effect id or parameter name, in bytes.
pub const MAX_NAME_BYTES: usize = 1024;
/// The longest audio file name, in bytes.
pub const MAX_FILE_NAME_BYTES: usize = 255;

/// The MIDI sequences of a state being saved, each once, in the order clips first refer to them.
#[derive(Default)]
struct SequenceTable {
    /// Each saved sequence's index, by the address of the shared sequence.
    index: HashMap<*const Sequence, u32>,
    saved: Vec<SavedSequence>,
}

impl SequenceTable {
    /// The index of `sequence` in the table, adding it if it is new.
    fn index_of(&mut self, sequence: &Arc<Sequence>) -> u32 {
        // Within one state every sequence fits a `u32`: there are at most as many as clips.
        let next = u32::try_from(self.saved.len()).unwrap_or(u32::MAX);
        *self.index.entry(Arc::as_ptr(sequence)).or_insert_with(|| {
            self.saved.push(sequence_to_saved(sequence));
            next
        })
    }
}

fn sequence_to_saved(sequence: &Sequence) -> SavedSequence {
    SavedSequence {
        frames: sequence.frames(),
        notes: sequence
            .notes()
            .iter()
            .map(|n| {
                [
                    n.start,
                    n.length,
                    n.channel.into(),
                    n.pitch.into(),
                    n.velocity.into(),
                ]
            })
            .collect(),
        controls: sequence
            .controls()
            .iter()
            .map(|c| {
                [
                    c.frame,
                    c.channel.into(),
                    c.controller.into(),
                    c.value.into(),
                ]
            })
            .collect(),
    }
}

/// A MIDI data byte from a saved number: refused past `u8`, and left to [`Sequence::new`] to
/// check against its field's range.
fn byte(value: u64) -> Result<u8, String> {
    u8::try_from(value).map_err(|_| format!("{value} is not a MIDI value"))
}

/// The sequence a saved one describes, after [`Sequence::new`]'s checks.
fn sequence_from_saved(index: usize, saved: SavedSequence) -> Result<Sequence, String> {
    let notes = saved
        .notes
        .iter()
        .map(|&[start, length, channel, pitch, velocity]| {
            Ok(Note {
                start,
                length,
                channel: byte(channel)?,
                pitch: byte(pitch)?,
                velocity: byte(velocity)?,
            })
        })
        .collect::<Result<Vec<_>, String>>();
    let controls = saved
        .controls
        .iter()
        .map(|&[frame, channel, controller, value]| {
            Ok(Control {
                frame,
                channel: byte(channel)?,
                controller: byte(controller)?,
                value: byte(value)?,
            })
        })
        .collect::<Result<Vec<_>, String>>();
    notes
        .and_then(|notes| Ok((notes, controls?)))
        .and_then(|(notes, controls)| {
            Sequence::new(saved.frames, notes, controls).map_err(|e| e.to_string())
        })
        .map_err(|reason| format!("sequence {index}: {reason}"))
}

/// Checks the MIDI counts against `max_sequences` and `max_events` before any sequence is built.
fn check_midi_counts(
    sequences: &[SavedSequence],
    max_sequences: usize,
    max_events: usize,
) -> Result<(), String> {
    ensure(sequences.len() <= max_sequences, || {
        format!("more than {max_sequences} MIDI sequences")
    })?;
    let events: usize = sequences
        .iter()
        .map(|s| s.notes.len() + s.controls.len())
        .sum();
    ensure(events <= max_events, || {
        format!("more than {max_events} MIDI notes and control changes")
    })
}

/// The saved form of `state` at `sample_rate`, as `project.json` (no name or stamp).
pub(crate) fn to_saved(state: &State, sample_rate: u32) -> SavedProject {
    let mut sequences = SequenceTable::default();
    let tracks = state
        .tracks
        .iter()
        .map(|track| track_to_saved(track, &mut sequences))
        .collect();
    SavedProject {
        format_version: FORMAT_VERSION,
        name: None,
        saved_at: None,
        sample_rate,
        tracks,
        nodes: state
            .nodes
            .iter()
            .map(|node| SavedNode {
                id: node.id.0,
                kind: kind_to_saved(&node.kind),
                position: node.position.map(|p| SavedPosition { x: p.x, y: p.y }),
                curve_ports: node.curve_ports.clone(),
            })
            .collect(),
        connections: state
            .connections
            .iter()
            .map(|c| SavedConnection {
                id: c.id.0,
                from: endpoint_to_saved(c.from),
                output: c.output,
                to: endpoint_to_saved(c.to),
                input: c.input,
                pan: c.pan,
                from_side: side_to_saved(c.from_side),
                to_side: side_to_saved(c.to_side),
                out_handle: c.shape.out_handle.map(wire_handle_to_saved),
                in_handle: c.shape.in_handle.map(wire_handle_to_saved),
                points: c
                    .shape
                    .points
                    .iter()
                    .map(|p| SavedWirePoint {
                        x: p.at.x,
                        y: p.at.y,
                        in_handle: wire_handle_to_saved(p.in_handle),
                        out_handle: wire_handle_to_saved(p.out_handle),
                    })
                    .collect(),
            })
            .collect(),
        output_y: state.output_y,
        tempo: SavedTempo {
            bpm: state.tempo.bpm,
            beats_per_bar: state.tempo.beats_per_bar,
            beat_unit: state.tempo.beat_unit,
        },
        curves: state.curves.iter().map(curve_to_saved).collect(),
        sequences: sequences.saved,
    }
}

fn wire_handle_to_saved(handle: WireHandle) -> SavedWireHandle {
    SavedWireHandle {
        dx: handle.dx,
        dy: handle.dy,
    }
}

/// A saved wire handle, its offsets checked as patch-board coordinates.
fn wire_handle_from_saved(handle: SavedWireHandle) -> Result<WireHandle, String> {
    check_coordinate(handle.dx)?;
    check_coordinate(handle.dy)?;
    Ok(WireHandle {
        dx: handle.dx,
        dy: handle.dy,
    })
}

/// A saved connection's wire shape: its handles and at most [`MAX_WIRE_POINTS`] points, every
/// coordinate checked.
fn wire_shape_from_saved(saved: &SavedConnection) -> Result<WireShape, String> {
    ensure(saved.points.len() <= MAX_WIRE_POINTS, || {
        format!("{} points, over {MAX_WIRE_POINTS}", saved.points.len())
    })?;
    let points = saved
        .points
        .iter()
        .map(|p| {
            check_coordinate(p.x)?;
            check_coordinate(p.y)?;
            Ok(WirePoint {
                at: Position { x: p.x, y: p.y },
                in_handle: wire_handle_from_saved(p.in_handle)?,
                out_handle: wire_handle_from_saved(p.out_handle)?,
            })
        })
        .collect::<Result<_, String>>()?;
    Ok(WireShape {
        out_handle: saved.out_handle.map(wire_handle_from_saved).transpose()?,
        in_handle: saved.in_handle.map(wire_handle_from_saved).transpose()?,
        points,
    })
}

fn handle_to_saved(handle: Handle) -> SavedHandle {
    SavedHandle {
        d_frame: handle.d_frame,
        d_value: handle.d_value,
    }
}

fn handle_from_saved(handle: SavedHandle) -> Handle {
    Handle::new(handle.d_frame, handle.d_value)
}

fn curve_to_saved(lane: &CurveLane) -> SavedCurve {
    SavedCurve {
        id: lane.id.0,
        name: lane.name.clone(),
        points: lane
            .curve
            .points()
            .iter()
            .map(|p| SavedPoint {
                frame: p.frame,
                value: p.value,
                in_handle: handle_to_saved(p.in_handle),
                out_handle: handle_to_saved(p.out_handle),
            })
            .collect(),
    }
}

/// The curve lane a saved curve describes, after checking its name and that its points make a
/// valid curve (`modulation::Curve::validate`).
fn curve_from_saved(saved: SavedCurve) -> Result<CurveLane, String> {
    check_name(&saved.name, "a curve name")?;
    let points = saved
        .points
        .into_iter()
        .map(|p| {
            Point::new(p.frame, p.value).with_handles(
                handle_from_saved(p.in_handle),
                handle_from_saved(p.out_handle),
            )
        })
        .collect();
    let curve = Curve::new(points).map_err(|e| format!("curve {}: {e}", saved.id))?;
    Ok(CurveLane {
        id: CurveId(saved.id),
        name: saved.name,
        curve,
    })
}

fn layout_to_saved(layout: Layout) -> SavedLayout {
    match layout {
        Layout::Mono => SavedLayout::Mono,
        Layout::Stereo => SavedLayout::Stereo,
    }
}

fn layout_from_saved(layout: SavedLayout) -> Layout {
    match layout {
        SavedLayout::Mono => Layout::Mono,
        SavedLayout::Stereo => Layout::Stereo,
    }
}

fn file_to_saved(file: &AudioFile) -> SavedFile {
    SavedFile {
        name: file.name.clone(),
        frames: file.frames,
    }
}

fn track_to_saved(track: &Track, sequences: &mut SequenceTable) -> SavedTrack {
    SavedTrack {
        id: track.id.0,
        name: track.name.clone(),
        kind: match track.kind {
            TrackKind::Audio(layout) => SavedTrackKind::Audio {
                layout: layout_to_saved(layout),
            },
            TrackKind::Midi => {
                let notation = track.notation.clone().unwrap_or_default();
                SavedTrackKind::Midi {
                    key: match notation.key {
                        Key::Tonal {
                            root,
                            mode: Mode::Major,
                        } => SavedKey::Major { root },
                        Key::Tonal {
                            root,
                            mode: Mode::Minor,
                        } => SavedKey::Minor { root },
                        Key::Indefinite => SavedKey::Indefinite {},
                    },
                    names: notation
                        .names
                        .into_iter()
                        .map(|(pitch, name)| SavedName { pitch, name })
                        .collect(),
                }
            }
        },
        clips: track
            .clips
            .iter()
            .map(|(id, clip)| SavedClip {
                id: id.0,
                source: match &clip.source {
                    Source::Mono(file) => SavedSource::Mono {
                        file: file_to_saved(file),
                    },
                    Source::Stereo { left, right } => SavedSource::Stereo {
                        left: file_to_saved(left),
                        right: file_to_saved(right),
                    },
                    Source::Midi(sequence) => SavedSource::Midi {
                        sequence: sequences.index_of(sequence),
                    },
                },
                offset: clip.offset,
                length: clip.length,
                position: clip.position,
                gain_db: clip.gain_db,
                fade_in: clip.fade_in,
                fade_out: clip.fade_out,
            })
            .collect(),
    }
}

/// A MIDI track's key and names from the file: a root of 0 to 11, names in strictly ascending
/// pitch order, within `project_model::notation`'s limits.
fn notation_from_saved(key: SavedKey, names: Vec<SavedName>) -> Result<Notation, String> {
    let key = match key {
        SavedKey::Major { root } => Key::Tonal {
            root,
            mode: Mode::Major,
        },
        SavedKey::Minor { root } => Key::Tonal {
            root,
            mode: Mode::Minor,
        },
        SavedKey::Indefinite {} => Key::Indefinite,
    };
    ensure(names.windows(2).all(|w| w[0].pitch < w[1].pitch), || {
        "pitch names are not in strictly ascending pitch order".to_owned()
    })?;
    let notation = Notation {
        key,
        names: names.into_iter().map(|n| (n.pitch, n.name)).collect(),
    };
    notation.check().map_err(|e| e.to_string())?;
    Ok(notation)
}

fn kind_to_saved(kind: &NodeKind) -> SavedNodeKind {
    match kind {
        NodeKind::Effect { kind, parameters } => SavedNodeKind::Effect {
            effect: kind.clone(),
            parameters: parameters.clone(),
        },
        NodeKind::Instrument { kind, parameters } => SavedNodeKind::Instrument {
            instrument: kind.clone(),
            parameters: parameters.clone(),
        },
        NodeKind::Mixer {
            channels,
            parameters,
        } => SavedNodeKind::Mixer {
            channels: *channels,
            parameters: parameters.clone(),
        },
        NodeKind::Bus(layout) => SavedNodeKind::Bus {
            layout: layout_to_saved(*layout),
        },
        NodeKind::FeedbackDelay { layout, seconds } => SavedNodeKind::FeedbackDelay {
            layout: layout_to_saved(*layout),
            seconds: *seconds,
        },
    }
}

fn endpoint_to_saved(endpoint: Endpoint) -> SavedEndpoint {
    match endpoint {
        Endpoint::Track(id) => SavedEndpoint::Track { id: id.0 },
        Endpoint::Output => SavedEndpoint::Output,
        Endpoint::Node(id) => SavedEndpoint::Node { id: id.0 },
        Endpoint::Curve(id) => SavedEndpoint::Curve { id: id.0 },
        Endpoint::CurvePort(node, port) => SavedEndpoint::CurvePort { node: node.0, port },
    }
}

fn endpoint_from_saved(endpoint: SavedEndpoint) -> Endpoint {
    match endpoint {
        SavedEndpoint::Track { id } => Endpoint::Track(TrackId(id)),
        SavedEndpoint::Output => Endpoint::Output,
        SavedEndpoint::Node { id } => Endpoint::Node(NodeId(id)),
        SavedEndpoint::Curve { id } => Endpoint::Curve(CurveId(id)),
        SavedEndpoint::CurvePort { node, port } => Endpoint::CurvePort(NodeId(node), port),
    }
}

/// Fails with `reason` unless `ok`.
/// Checks a patch-board coordinate: finite and within `MAX_BOARD_COORDINATE`.
fn check_coordinate(value: f32) -> Result<(), String> {
    ensure(
        value.is_finite() && value.abs() <= MAX_BOARD_COORDINATE,
        || format!("{value} is not a patch board position"),
    )
}

fn ensure(ok: bool, reason: impl FnOnce() -> String) -> Result<(), String> {
    if ok { Ok(()) } else { Err(reason()) }
}

/// Checks a name, effect id or parameter name.
fn check_name(name: &str, what: &str) -> Result<(), String> {
    ensure(name.len() <= MAX_NAME_BYTES, || {
        format!("{what} is longer than {MAX_NAME_BYTES} bytes")
    })
}

/// Whether `name` is a plain file name inside `audio/`: no directory parts, no `..`, not hidden,
/// not empty, at most [`MAX_FILE_NAME_BYTES`].
pub(crate) fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= MAX_FILE_NAME_BYTES
        && !name.starts_with('.')
        && !name.contains(['/', '\\', ':', '\0'])
}

fn file_from_saved(file: SavedFile) -> Result<AudioFile, String> {
    ensure(is_plain_file_name(&file.name), || {
        format!(
            "audio file name {:?} is not a plain name inside audio/",
            file.name
        )
    })?;
    Ok(AudioFile {
        name: file.name,
        frames: file.frames,
    })
}

fn parameters_from_saved(
    parameters: BTreeMap<String, f32>,
) -> Result<BTreeMap<String, f32>, String> {
    ensure(parameters.len() <= MAX_PARAMETERS, || {
        format!("a node has more than {MAX_PARAMETERS} parameters")
    })?;
    for (name, value) in &parameters {
        check_name(name, "a parameter name")?;
        ensure(value.is_finite(), || {
            format!("parameter {name:?} is not a finite number")
        })?;
    }
    Ok(parameters)
}

fn kind_from_saved(kind: SavedNodeKind) -> Result<NodeKind, String> {
    Ok(match kind {
        SavedNodeKind::Effect { effect, parameters } => {
            check_name(&effect, "an effect id")?;
            NodeKind::Effect {
                kind: effect,
                parameters: parameters_from_saved(parameters)?,
            }
        }
        SavedNodeKind::Instrument {
            instrument,
            parameters,
        } => {
            check_name(&instrument, "an instrument id")?;
            NodeKind::Instrument {
                kind: instrument,
                parameters: parameters_from_saved(parameters)?,
            }
        }
        SavedNodeKind::Mixer {
            channels,
            parameters,
        } => {
            ensure((1..=MAX_MIXER_CHANNELS).contains(&channels), || {
                format!("a mixer of {channels} channels")
            })?;
            NodeKind::Mixer {
                channels,
                parameters: parameters_from_saved(parameters)?,
            }
        }
        SavedNodeKind::Bus { layout } => NodeKind::Bus(layout_from_saved(layout)),
        SavedNodeKind::FeedbackDelay { layout, seconds } => {
            ensure(seconds.is_finite() && seconds >= 0.0, || {
                format!("a feedback delay of {seconds} s")
            })?;
            NodeKind::FeedbackDelay {
                layout: layout_from_saved(layout),
                seconds,
            }
        }
    })
}

/// The model state a saved file describes, after checking it: sample rate, counts, names, unique
/// ids, every clip against its track, and every connection's endpoints. Returns the reason it is
/// invalid otherwise.
pub(crate) fn from_saved(saved: SavedProject) -> Result<State, String> {
    ensure(SAMPLE_RATES.contains(&saved.sample_rate), || {
        format!("{} Hz is not a project sample rate", saved.sample_rate)
    })?;
    ensure(saved.tracks.len() <= MAX_TRACKS, || {
        format!("more than {MAX_TRACKS} tracks")
    })?;
    let clips: usize = saved.tracks.iter().map(|t| t.clips.len()).sum();
    ensure(clips <= MAX_CLIPS, || {
        format!("more than {MAX_CLIPS} clips")
    })?;
    ensure(saved.nodes.len() <= MAX_NODES, || {
        format!("more than {MAX_NODES} nodes")
    })?;
    ensure(saved.connections.len() <= MAX_CONNECTIONS, || {
        format!("more than {MAX_CONNECTIONS} connections")
    })?;
    ensure(saved.curves.len() <= MAX_CURVES, || {
        format!("more than {MAX_CURVES} curves")
    })?;
    let points: usize = saved.curves.iter().map(|c| c.points.len()).sum();
    ensure(points <= MAX_CURVE_POINTS, || {
        format!("more than {MAX_CURVE_POINTS} curve points")
    })?;
    check_midi_counts(&saved.sequences, MAX_SEQUENCES, MAX_MIDI_EVENTS)?;

    // Each sequence is built once, so the clips that refer to it share it.
    let sequences = saved
        .sequences
        .into_iter()
        .enumerate()
        .map(|(index, s)| sequence_from_saved(index, s).map(Arc::new))
        .collect::<Result<Vec<_>, String>>()?;
    let mut referenced = vec![false; sequences.len()];

    // Tracks and clips share one id space (`project_model::Session`).
    let mut item_ids = BTreeSet::new();
    let mut tracks = Vec::with_capacity(saved.tracks.len());
    for saved_track in saved.tracks {
        check_name(&saved_track.name, "a track name")?;
        ensure(item_ids.insert(saved_track.id), || {
            format!("id {} is used twice", saved_track.id)
        })?;
        let (kind, notation) = match saved_track.kind {
            SavedTrackKind::Audio { layout } => (TrackKind::Audio(layout_from_saved(layout)), None),
            SavedTrackKind::Midi { key, names } => (
                TrackKind::Midi,
                Some(
                    notation_from_saved(key, names)
                        .map_err(|e| format!("track {}: {e}", saved_track.id))?,
                ),
            ),
        };
        let mut track = Track::of_kind(TrackId(saved_track.id), &saved_track.name, kind);
        track.notation = notation;
        for saved_clip in saved_track.clips {
            ensure(item_ids.insert(saved_clip.id), || {
                format!("id {} is used twice", saved_clip.id)
            })?;
            let source = match saved_clip.source {
                SavedSource::Mono { file } => Source::Mono(file_from_saved(file)?),
                SavedSource::Stereo { left, right } => Source::Stereo {
                    left: file_from_saved(left)?,
                    right: file_from_saved(right)?,
                },
                SavedSource::Midi { sequence } => {
                    let index = usize::try_from(sequence).unwrap_or(usize::MAX);
                    let shared = sequences.get(index).ok_or_else(|| {
                        format!("clip {} refers to no sequence {sequence}", saved_clip.id)
                    })?;
                    referenced[index] = true;
                    Source::Midi(Arc::clone(shared))
                }
            };
            let clip = Clip {
                source,
                offset: saved_clip.offset,
                length: saved_clip.length,
                position: saved_clip.position,
                gain_db: saved_clip.gain_db,
                fade_in: saved_clip.fade_in,
                fade_out: saved_clip.fade_out,
            };
            clip.check(track.kind)
                .map_err(|e| format!("clip {}: {e}", saved_clip.id))?;
            track.clips.push((ClipId(saved_clip.id), clip));
        }
        tracks.push(track);
    }
    if let Some(unused) = referenced.iter().position(|used| !used) {
        return Err(format!("no clip refers to sequence {unused}"));
    }

    let mut nodes = Vec::with_capacity(saved.nodes.len());
    let mut node_ids = BTreeSet::new();
    for node in saved.nodes {
        ensure(node_ids.insert(node.id), || {
            format!("node id {} is used twice", node.id)
        })?;
        let position = match node.position {
            Some(p) => {
                check_coordinate(p.x)?;
                check_coordinate(p.y)?;
                Some(Position { x: p.x, y: p.y })
            }
            None => None,
        };
        ensure(node.curve_ports.len() <= MAX_PARAMETERS, || {
            format!(
                "node {} has more than {MAX_PARAMETERS} curve ports",
                node.id
            )
        })?;
        for name in node.curve_ports.keys() {
            check_name(name, "a parameter name")?;
        }
        nodes.push(Node {
            id: NodeId(node.id),
            kind: kind_from_saved(node.kind)?,
            position,
            curve_ports: node.curve_ports,
        });
    }

    let mut curve_ids = BTreeSet::new();
    let mut curves = Vec::with_capacity(saved.curves.len());
    for saved_curve in saved.curves {
        ensure(curve_ids.insert(saved_curve.id), || {
            format!("curve id {} is used twice", saved_curve.id)
        })?;
        curves.push(curve_from_saved(saved_curve)?);
    }

    if let Some(y) = saved.output_y {
        check_coordinate(y)?;
    }
    let tempo = Tempo {
        bpm: saved.tempo.bpm,
        beats_per_bar: saved.tempo.beats_per_bar,
        beat_unit: saved.tempo.beat_unit,
    };
    tempo.check().map_err(|_| {
        format!(
            "the tempo {} BPM in {}/{} is out of range",
            tempo.bpm, tempo.beats_per_bar, tempo.beat_unit
        )
    })?;
    let mut state = State {
        tracks,
        nodes,
        connections: Vec::new(),
        output_y: saved.output_y,
        tempo,
        curves,
    };
    let mut connection_ids = BTreeSet::new();
    let mut ports = BTreeSet::new();
    let mut connections = Vec::with_capacity(saved.connections.len());
    for saved_connection in saved.connections {
        let from = endpoint_from_saved(saved_connection.from);
        let to = endpoint_from_saved(saved_connection.to);
        let id = saved_connection.id;
        ensure(connection_ids.insert(id), || {
            format!("connection id {id} is used twice")
        })?;
        ensure(from != Endpoint::Output, || {
            format!("connection {id} starts at the output")
        })?;
        ensure(!matches!(to, Endpoint::Track(_)), || {
            format!("connection {id} ends at a track")
        })?;
        ensure(state.has_endpoint(from) && state.has_endpoint(to), || {
            format!("connection {id} refers to something that does not exist")
        })?;
        ensure(
            saved_connection.pan.is_finite() && saved_connection.pan.abs() <= 1.0,
            || format!("connection {id} has pan {}", saved_connection.pan),
        )?;
        let (from_side, to_side) = (
            side_from_saved(saved_connection.from_side),
            side_from_saved(saved_connection.to_side),
        );
        ensure(
            ports.insert((
                from,
                saved_connection.output,
                from_side,
                to,
                saved_connection.input,
                to_side,
            )),
            || format!("connection {id} repeats another"),
        )?;
        let mut connection = Connection::new(
            ConnectionId(id),
            from,
            saved_connection.output,
            to,
            saved_connection.input,
        );
        connection.pan = saved_connection.pan;
        connection.shape = wire_shape_from_saved(&saved_connection)
            .map_err(|reason| format!("connection {id} has a bad wire shape: {reason}"))?;
        connections.push(connection.with_sides(from_side, to_side));
    }
    state.connections = connections;
    // Last, the model's own whole-state check: ports it knows, sides only of stereo ports.
    let mut probe = State::default();
    Edit::Restore(state.clone())
        .apply(&mut probe)
        .map_err(|e| format!("the routing is invalid: {e}"))?;
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequence(notes: usize, controls: usize) -> SavedSequence {
        SavedSequence {
            frames: 100,
            notes: vec![[0, 1, 0, 60, 100]; notes],
            controls: vec![[0, 0, 7, 0]; controls],
        }
    }

    fn name(pitch: u8, name: &str) -> SavedName {
        SavedName {
            pitch,
            name: name.to_owned(),
        }
    }

    #[test]
    fn test_notation_round_trips_tonal_and_indefinite() -> Result<(), String> {
        for notation in [
            Notation {
                key: Key::Tonal {
                    root: 10,
                    mode: Mode::Minor,
                },
                names: BTreeMap::from([(36, "kick".to_owned())]),
            },
            Notation::gm_drums(),
        ] {
            let mut track = Track::new_midi(TrackId(1), "T");
            track.notation = Some(notation.clone());
            let state = State {
                tracks: vec![track],
                ..State::default()
            };
            let saved = to_saved(&state, 48_000);
            let SavedTrackKind::Midi { key, names } = saved.tracks[0].kind.clone() else {
                return Err("not saved as MIDI".to_owned());
            };
            assert_eq!(notation_from_saved(key, names)?, notation);
        }
        Ok(())
    }

    #[test]
    fn test_notation_from_saved_refuses_bad_keys_and_names() {
        let major = SavedKey::Major { root: 0 };
        assert!(notation_from_saved(SavedKey::Minor { root: 12 }, Vec::new()).is_err());
        for names in [
            vec![name(128, "x")],
            vec![name(36, "kick"), name(36, "kick")],
            vec![name(38, "snare"), name(36, "kick")],
            vec![name(36, "")],
            vec![name(36, "   ")],
            vec![name(36, "a\u{7}")],
            vec![name(36, &"x".repeat(33))],
        ] {
            assert!(
                notation_from_saved(major, names.clone()).is_err(),
                "{names:?}"
            );
        }
        assert!(notation_from_saved(SavedKey::Indefinite {}, vec![name(36, "kick")]).is_ok());
    }

    #[test]
    fn test_check_midi_counts_refuses_over_limits() {
        let sequences = [sequence(2, 1), sequence(1, 0)];
        assert!(check_midi_counts(&sequences, 2, 4).is_ok());
        assert!(check_midi_counts(&sequences, 1, 4).is_err());
        assert!(check_midi_counts(&sequences, 2, 3).is_err());
    }

    #[test]
    fn test_to_saved_writes_a_shared_sequence_once() -> Result<(), project_model::Error> {
        let shared = Arc::new(Sequence::new(100, Vec::new(), Vec::new())?);
        let other = Arc::new(Sequence::new(100, Vec::new(), Vec::new())?);
        let mut track = Track::new_midi(TrackId(1), "T");
        for (id, sequence) in [(2, &shared), (3, &other), (4, &shared)] {
            let clip = Clip::new(Source::Midi(Arc::clone(sequence)), 0, 100, 0);
            track.clips.push((ClipId(id), clip));
        }
        let state = State {
            tracks: vec![track],
            ..State::default()
        };
        let saved = to_saved(&state, 48_000);
        assert_eq!(saved.sequences.len(), 2);
        let indices: Vec<_> = saved.tracks[0]
            .clips
            .iter()
            .map(|c| c.source.clone())
            .collect();
        assert_eq!(
            indices,
            [0, 1, 0].map(|sequence| SavedSource::Midi { sequence })
        );
        Ok(())
    }
}
