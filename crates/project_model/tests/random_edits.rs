//! Property tests over seeded random edit sequences: undo restores every earlier state exactly,
//! and redo replays them. The generator is in-house (ADR-0042); a failure names its seed.

use std::error::Error as StdError;

use audio_buffers::Layout;
use std::collections::BTreeMap;

use project_model::{
    AudioFile, Clip, ClipId, ConnectionId, Edit, Endpoint, NodeId, NodeKind, Session, Source,
    State, TrackId,
};

type TestResult = Result<(), Box<dyn StdError>>;

/// A xorshift64* generator, enough to pick edits.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // SplitMix64 of the seed, so nearby seeds diverge and the state is never 0.
        let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        Rng((z ^ (z >> 31)) | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A number in `0..n`; 0 if `n` is 0.
    fn below(&mut self, n: u64) -> u64 {
        if n == 0 { 0 } else { self.next() % n }
    }

    fn index(&mut self, n: usize) -> usize {
        self.below(n as u64) as usize
    }
}

fn source(rng: &mut Rng, layout: Layout) -> Source {
    let file = |rng: &mut Rng| AudioFile {
        name: format!("f{}.wav", rng.below(6)),
        frames: 1 + rng.below(2_000),
    };
    match layout {
        Layout::Mono => Source::Mono(file(rng)),
        Layout::Stereo => Source::Stereo {
            left: file(rng),
            right: file(rng),
        },
    }
}

/// A clip that is usually valid, sometimes not.
fn clip(rng: &mut Rng, layout: Layout) -> Clip {
    let source = source(rng, layout);
    let frames = source.frames();
    let offset = rng.below(frames);
    let length = rng.below(frames - offset + 2);
    Clip {
        offset,
        length,
        position: rng.below(10_000),
        gain_db: rng.below(25) as f32 - 12.0,
        fade_in: rng.below(length / 2 + 2),
        fade_out: rng.below(length / 2 + 2),
        source,
    }
}

fn some_track(rng: &mut Rng, state: &State) -> Option<(TrackId, Layout)> {
    let tracks = &state.tracks;
    (!tracks.is_empty()).then(|| {
        let track = &tracks[rng.index(tracks.len())];
        (track.id, track.layout)
    })
}

fn some_node(rng: &mut Rng, state: &State) -> Option<NodeId> {
    let nodes = &state.nodes;
    (!nodes.is_empty()).then(|| nodes[rng.index(nodes.len())].id)
}

fn some_connection(rng: &mut Rng, state: &State) -> Option<ConnectionId> {
    let connections = &state.connections;
    (!connections.is_empty()).then(|| connections[rng.index(connections.len())].id)
}

/// Any endpoint, existing or not, so some connections are refused.
fn some_endpoint(rng: &mut Rng, state: &State) -> Endpoint {
    match rng.below(4) {
        0 => Endpoint::Output,
        1 => some_track(rng, state).map_or(Endpoint::Output, |(id, _)| Endpoint::Track(id)),
        2 => some_node(rng, state).map_or(Endpoint::Output, Endpoint::Node),
        _ => Endpoint::Node(NodeId(rng.below(1_000))),
    }
}

fn node_kind(rng: &mut Rng, layout: Layout) -> NodeKind {
    match rng.below(3) {
        0 => NodeKind::Bus(layout),
        1 => NodeKind::FeedbackDelay {
            layout,
            seconds: rng.below(12_000) as f64 / 1_000.0,
        },
        _ => NodeKind::Effect {
            kind: "eq".into(),
            parameters: BTreeMap::new(),
        },
    }
}

fn some_clip(rng: &mut Rng, state: &State) -> Option<ClipId> {
    let clips: Vec<ClipId> = state
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter().map(|(id, _)| *id))
        .collect();
    (!clips.is_empty()).then(|| clips[rng.index(clips.len())])
}

/// Makes one random gesture and returns whether it was an accepted edit (one undo step).
/// Refused gestures are allowed and must change nothing.
fn random_gesture(rng: &mut Rng, session: &mut Session, versions: &mut u32) -> bool {
    let state = session.state().clone();
    let layout = if rng.below(2) == 0 {
        Layout::Mono
    } else {
        Layout::Stereo
    };
    let track = some_track(rng, &state);
    let clip_id = some_clip(rng, &state);
    let node = some_node(rng, &state);
    let connection = some_connection(rng, &state);
    let result = match (rng.below(18), track, clip_id) {
        (0, _, _) => session.add_track("t", layout).map(|_| ()),
        (1 | 2, Some((track, layout)), _) => {
            let clip = clip(rng, layout);
            session.add_clip(track, clip).map(|_| ())
        }
        (3, Some((track, _)), Some(id)) => session.move_clip(id, track, rng.below(10_000)),
        (4, _, Some(id)) => {
            let layout = state
                .find_clip(id)
                .and_then(|(track, _)| state.track(track))
                .map_or(Layout::Mono, |t| t.layout);
            session.set_clip(id, clip(rng, layout))
        }
        (5, _, Some(id)) => session.apply(Edit::RemoveClip(id)),
        (6, Some((track, _)), _) => session.remove_track(track),
        (7, Some((track, _)), _) => session.apply(Edit::MoveTrack {
            track,
            index: rng.index(state.tracks.len() + 1),
        }),
        (8, Some((track, _)), _) => session.apply(Edit::RenameTrack {
            track,
            name: format!("n{}", rng.below(100)),
        }),
        (9, _, _) => {
            *versions += 1;
            // Saving a version is not an edit.
            let _ = session.save_version(&format!("v{versions}"));
            return false;
        }
        (10, _, _) if *versions > 0 => {
            let name = format!("v{}", 1 + rng.below(u64::from(*versions)));
            session.restore_version(&name)
        }
        (11, _, _) => session.add_node(node_kind(rng, layout)).map(|_| ()),
        (12, _, _) => match node {
            Some(node) => session.remove_node(node),
            None => return false,
        },
        (13, _, _) => {
            let (from, to) = (some_endpoint(rng, &state), some_endpoint(rng, &state));
            let (output, input) = (rng.index(2), rng.index(2));
            session.connect(from, output, to, input).map(|_| ())
        }
        (14, _, _) => match connection {
            Some(id) => session.apply(Edit::Disconnect(id)),
            None => return false,
        },
        (15, _, _) => match node {
            Some(node) => {
                let value = (rng.below(3) > 0).then(|| rng.below(100) as f32 / 10.0);
                session.set_parameter(node, &format!("p{}", rng.below(3)), value)
            }
            None => return false,
        },
        (16, _, _) => match connection {
            Some(connection) => session.apply(Edit::SetPan {
                connection,
                pan: rng.below(25) as f32 / 10.0 - 1.2,
            }),
            None => return false,
        },
        (17, _, _) => match node {
            Some(node) => session.apply(Edit::SetDelay {
                node,
                seconds: rng.below(12_000) as f64 / 1_000.0,
            }),
            None => return false,
        },
        _ => return false,
    };
    result.is_ok()
}

#[test]
fn test_undo_restores_every_earlier_state_exactly() -> TestResult {
    for seed in 0..300 {
        let mut rng = Rng::new(seed);
        let mut session = Session::new();
        let mut versions = 0;
        let mut history = vec![session.state().clone()];
        for _ in 0..60 {
            let before = session.state().clone();
            if random_gesture(&mut rng, &mut session, &mut versions) {
                history.push(session.state().clone());
            } else {
                assert_eq!(
                    session.state(),
                    &before,
                    "seed {seed}: refused edit changed state"
                );
            }
        }
        // Undo walks back through every recorded state; nothing else is in the history.
        let mut undone = vec![session.state().clone()];
        while session.undo() {
            undone.push(session.state().clone());
        }
        undone.reverse();
        assert_eq!(undone.len(), history.len(), "seed {seed}: step count");
        for (step, (got, want)) in undone.iter().zip(&history).enumerate() {
            assert_eq!(got, want, "seed {seed}: undo to step {step}");
        }
        // Redo replays them in order.
        for (step, want) in history.iter().enumerate().skip(1) {
            assert!(session.redo(), "seed {seed}: redo step {step}");
            assert_eq!(session.state(), want, "seed {seed}: redo step {step}");
        }
        assert!(!session.redo());
    }
    Ok(())
}

#[test]
fn test_new_edit_after_undo_drops_redo() -> TestResult {
    let mut session = Session::new();
    session.add_track("a", Layout::Mono)?;
    session.add_track("b", Layout::Mono)?;
    assert!(session.undo());
    session.add_track("c", Layout::Stereo)?;
    assert!(!session.can_redo());
    let names: Vec<&str> = session
        .state()
        .tracks
        .iter()
        .map(|t| t.name.as_str())
        .collect();
    assert_eq!(names, ["a", "c"]);
    Ok(())
}
