//! Converting between the saved format and `project_model`, checking everything a file from
//! elsewhere could get wrong (`docs/architecture/project.md` "Loading", ADR-0049).

use std::collections::{BTreeMap, BTreeSet};

use audio_buffers::Layout;
use project_model::{
    AudioFile, Clip, ClipId, Connection, ConnectionId, Endpoint, Node, NodeId, NodeKind, Source,
    State, Track, TrackId,
};

use crate::format::{
    FORMAT_VERSION, SavedClip, SavedConnection, SavedEndpoint, SavedFile, SavedLayout, SavedNode,
    SavedNodeKind, SavedProject, SavedSource, SavedTrack,
};

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
/// The longest name, effect id or parameter name, in bytes.
pub const MAX_NAME_BYTES: usize = 1024;
/// The longest audio file name, in bytes.
pub const MAX_FILE_NAME_BYTES: usize = 255;

/// The saved form of `state` at `sample_rate`, as `project.json` (no name or stamp).
pub(crate) fn to_saved(state: &State, sample_rate: u32) -> SavedProject {
    SavedProject {
        format_version: FORMAT_VERSION,
        name: None,
        saved_at: None,
        sample_rate,
        tracks: state.tracks.iter().map(track_to_saved).collect(),
        nodes: state
            .nodes
            .iter()
            .map(|node| SavedNode {
                id: node.id.0,
                kind: kind_to_saved(&node.kind),
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
            })
            .collect(),
    }
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

fn track_to_saved(track: &Track) -> SavedTrack {
    SavedTrack {
        id: track.id.0,
        name: track.name.clone(),
        layout: layout_to_saved(track.layout),
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

fn kind_to_saved(kind: &NodeKind) -> SavedNodeKind {
    match kind {
        NodeKind::Effect { kind, parameters } => SavedNodeKind::Effect {
            effect: kind.clone(),
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
    }
}

fn endpoint_from_saved(endpoint: SavedEndpoint) -> Endpoint {
    match endpoint {
        SavedEndpoint::Track { id } => Endpoint::Track(TrackId(id)),
        SavedEndpoint::Output => Endpoint::Output,
        SavedEndpoint::Node { id } => Endpoint::Node(NodeId(id)),
    }
}

/// Fails with `reason` unless `ok`.
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

    // Tracks and clips share one id space (`project_model::Session`).
    let mut item_ids = BTreeSet::new();
    let mut tracks = Vec::with_capacity(saved.tracks.len());
    for saved_track in saved.tracks {
        check_name(&saved_track.name, "a track name")?;
        ensure(item_ids.insert(saved_track.id), || {
            format!("id {} is used twice", saved_track.id)
        })?;
        let mut track = Track::new(
            TrackId(saved_track.id),
            &saved_track.name,
            layout_from_saved(saved_track.layout),
        );
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
            clip.check(track.layout)
                .map_err(|e| format!("clip {}: {e}", saved_clip.id))?;
            track.clips.push((ClipId(saved_clip.id), clip));
        }
        tracks.push(track);
    }

    let mut nodes = Vec::with_capacity(saved.nodes.len());
    let mut node_ids = BTreeSet::new();
    for node in saved.nodes {
        ensure(node_ids.insert(node.id), || {
            format!("node id {} is used twice", node.id)
        })?;
        nodes.push(Node {
            id: NodeId(node.id),
            kind: kind_from_saved(node.kind)?,
        });
    }

    let mut state = State {
        tracks,
        nodes,
        connections: Vec::new(),
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
        ensure(
            ports.insert((from, saved_connection.output, to, saved_connection.input)),
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
        connections.push(connection);
    }
    state.connections = connections;
    Ok(state)
}
