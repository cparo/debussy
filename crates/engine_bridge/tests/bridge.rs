//! The bridge keeps the running graph in step with the model through random edit sequences,
//! undo and redo; refused edits leave both unchanged; clips become the engine's renderers.

use std::collections::BTreeMap;
use std::error::Error;
use std::sync::Arc;

use audio_buffers::Layout;
use audio_engine::{ClipAudio, RenderFeed, TrackFeed, TrackFeeds, render};
use engine_bridge::{Bridge, Error as BridgeError, render_graph, track_renderers};
use modulation::{Curve, Handle, Point};
use project_model::{AudioFile, Clip, Edit, Endpoint, NodeKind, Session, Side, Source, State};
use signal_graph::Config;

fn config() -> Config {
    Config::new(48_000, 256)
}

/// Checks that the graph holds exactly the model's routing: one node per track, routing node
/// and the output, and one connection per model connection between the matching nodes and
/// ports, with the same pan.
fn assert_in_step(bridge: &Bridge, state: &State) -> Result<(), Box<dyn Error>> {
    let graph = bridge.graph();
    // Each sided end of a connection adds a helper node and a connection (ADR-0058).
    let sided: usize = state
        .connections
        .iter()
        .map(|c| usize::from(c.from_side.is_some()) + usize::from(c.to_side.is_some()))
        .sum();
    let expected_nodes = state.tracks.len() + state.nodes.len() + state.curves.len() + 1 + sided;
    assert_eq!(graph.node_ids().count(), expected_nodes, "node count");
    let mut controls = 0;
    for c in &state.connections {
        if let (Endpoint::Curve(curve), Endpoint::CurvePort(node, port)) = (c.from, c.to) {
            let id = bridge.control(c.id).ok_or("control wire not mirrored")?;
            let g = graph
                .control_connection(id)
                .ok_or("control missing from graph")?;
            assert_eq!(Some(g.curve), bridge.curve(curve));
            assert_eq!(Some(g.node), bridge.node(Endpoint::Node(node)));
            let parameter = state.node(node).and_then(|n| n.curve_port_parameter(port));
            assert_eq!(Some(g.parameter.as_str()), parameter);
            controls += 1;
            continue;
        }
        let id = bridge.connection(c.id).ok_or("connection not mirrored")?;
        let g = graph
            .connection(id)
            .ok_or("connection missing from graph")?;
        if c.from_side.is_none() {
            assert_eq!(Some(g.from), bridge.node(c.from), "from of {}", c.id);
            assert_eq!(g.output, c.output);
        }
        if c.to_side.is_none() {
            assert_eq!(Some(g.to), bridge.node(c.to), "to of {}", c.id);
            assert_eq!(g.input, c.input);
        }
        assert_eq!(g.pan, c.pan);
    }
    assert_eq!(
        graph.connections().count(),
        state.connections.len() - controls + sided,
        "graph connection count"
    );
    assert_eq!(graph.control_connections().count(), controls);
    Ok(())
}

/// A seeded xorshift64 generator.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Every endpoint that can be a connection's source or target in `state`.
fn endpoints(state: &State) -> (Vec<Endpoint>, Vec<Endpoint>) {
    let nodes = state.nodes.iter().map(|n| Endpoint::Node(n.id));
    let from = state
        .tracks
        .iter()
        .map(|t| Endpoint::Track(t.id))
        .chain(nodes.clone())
        .collect();
    let to = nodes.chain(std::iter::once(Endpoint::Output)).collect();
    (from, to)
}

#[test]
fn test_random_routing_edits_keep_model_and_graph_in_step() -> Result<(), Box<dyn Error>> {
    let mut random = Random(0xB41D_6E00_1234_5678);
    let mut session = Session::new();
    let (mut bridge, _runner) = Bridge::new(session.state(), config())?;
    let (mut accepted, mut refused) = (0, 0);
    for _ in 0..1500 {
        let before = session.state().clone();
        let state = session.state().clone();
        let (from, to) = endpoints(&state);
        let result = match random.below(12) {
            0 => {
                let layout = if random.below(2) == 0 {
                    Layout::Mono
                } else {
                    Layout::Stereo
                };
                bridge
                    .edit(&mut session, |s| s.add_track("t", layout))
                    .map(|_| ())
            }
            1 if !state.tracks.is_empty() => {
                let id = state.tracks[random.below(state.tracks.len())].id;
                bridge.edit(&mut session, |s| s.remove_track(id))
            }
            2 => {
                let kind = match random.below(5) {
                    0 => NodeKind::Bus(Layout::Stereo),
                    1 => NodeKind::Bus(Layout::Mono),
                    2 => NodeKind::Mixer {
                        channels: 1 + random.below(3),
                        parameters: BTreeMap::new(),
                    },
                    3 => NodeKind::FeedbackDelay {
                        layout: Layout::Stereo,
                        seconds: 0.001 + random.below(100) as f64 / 1000.0,
                    },
                    _ => NodeKind::Effect {
                        kind: "unknown-reverb".to_owned(),
                        parameters: BTreeMap::new(),
                    },
                };
                bridge.edit(&mut session, |s| s.add_node(kind)).map(|_| ())
            }
            3 if !state.nodes.is_empty() => {
                let id = state.nodes[random.below(state.nodes.len())].id;
                bridge.edit(&mut session, |s| s.remove_node(id))
            }
            4..=6 if !from.is_empty() => {
                // Usually node to node, which is where cycles form.
                let nodes: Vec<Endpoint> =
                    state.nodes.iter().map(|n| Endpoint::Node(n.id)).collect();
                let (a, b) = if nodes.len() >= 2 && random.below(4) != 0 {
                    (
                        nodes[random.below(nodes.len())],
                        nodes[random.below(nodes.len())],
                    )
                } else {
                    (from[random.below(from.len())], to[random.below(to.len())])
                };
                // Mostly port 0, so connections between buses often close cycles.
                let port = |r: &mut Random| if r.below(5) == 0 { 1 } else { 0 };
                let (output, input) = (port(&mut random), port(&mut random));
                bridge
                    .edit(&mut session, |s| s.connect(a, output, b, input))
                    .map(|_| ())
            }
            7 if !state.connections.is_empty() => {
                let id = state.connections[random.below(state.connections.len())].id;
                bridge.edit(&mut session, |s| s.apply(Edit::Disconnect(id)))
            }
            8 if !state.connections.is_empty() => {
                let id = state.connections[random.below(state.connections.len())].id;
                let pan = random.below(21) as f32 / 10.0 - 1.0;
                bridge.edit(&mut session, |s| {
                    s.apply(Edit::SetPan {
                        connection: id,
                        pan,
                    })
                })
            }
            9 => {
                let mixers: Vec<_> = state
                    .nodes
                    .iter()
                    .filter(|n| matches!(n.kind, NodeKind::Mixer { .. }))
                    .map(|n| n.id)
                    .collect();
                match mixers.first() {
                    Some(&id) if random.below(2) == 0 => {
                        bridge.edit(&mut session, |s| s.add_mixer_channels(id, 1))
                    }
                    Some(&id) => {
                        let value = -(random.below(40) as f32);
                        bridge.edit(&mut session, |s| {
                            s.set_parameter(id, "channel 1 gain", Some(value))
                        })
                    }
                    None => Ok(()),
                }
            }
            10 => bridge.undo(&mut session).map(|_| ()),
            _ => bridge.redo(&mut session).map(|_| ()),
        };
        match result {
            Ok(()) => accepted += 1,
            Err(BridgeError::Refused(_)) => {
                refused += 1;
                assert_eq!(session.state(), &before, "a refused edit changed the model");
            }
            Err(BridgeError::Model(_)) => {
                assert_eq!(session.state(), &before, "a model error changed the model");
            }
            Err(other) => return Err(other.into()),
        }
        assert_in_step(&bridge, session.state())?;
    }
    // The sequence exercised both outcomes.
    eprintln!("{accepted} accepted, {refused} refused");
    assert!(accepted > 300, "{accepted} accepted");
    assert!(refused > 20, "{refused} refused");
    Ok(())
}

#[test]
fn test_cycle_without_delay_is_refused_and_through_delay_accepted() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new();
    let (mut bridge, _runner) = Bridge::new(session.state(), config())?;
    let a = bridge.edit(&mut session, |s| s.add_node(NodeKind::Bus(Layout::Stereo)))?;
    let b = bridge.edit(&mut session, |s| s.add_node(NodeKind::Bus(Layout::Stereo)))?;
    bridge.edit(&mut session, |s| {
        s.connect(Endpoint::Node(a), 0, Endpoint::Node(b), 0)
    })?;
    let before = session.state().clone();
    let undo_before = session.can_undo();
    assert!(matches!(
        bridge.edit(&mut session, |s| s.connect(
            Endpoint::Node(b),
            0,
            Endpoint::Node(a),
            0
        )),
        Err(BridgeError::Refused(signal_graph::Error::CycleWithoutDelay))
    ));
    assert_eq!(session.state(), &before);
    assert_eq!(session.can_undo(), undo_before);
    assert!(!session.can_redo(), "a refused edit must not be redoable");

    let delay = bridge.edit(&mut session, |s| {
        s.add_node(NodeKind::FeedbackDelay {
            layout: Layout::Stereo,
            seconds: 0.01,
        })
    })?;
    bridge.edit(&mut session, |s| {
        s.connect(Endpoint::Node(b), 0, Endpoint::Node(delay), 0)
    })?;
    bridge.edit(&mut session, |s| {
        s.connect(Endpoint::Node(delay), 0, Endpoint::Node(a), 0)
    })?;
    assert_in_step(&bridge, session.state())?;
    Ok(())
}

#[test]
fn test_mixer_growth_keeps_connections_and_parameters() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new();
    let (mut bridge, _runner) = Bridge::new(session.state(), config())?;
    let track = bridge.edit(&mut session, |s| s.add_track("vox", Layout::Mono))?;
    let mixer = bridge.edit(&mut session, |s| {
        s.add_node(NodeKind::Mixer {
            channels: 1,
            parameters: BTreeMap::new(),
        })
    })?;
    bridge.edit(&mut session, |s| {
        s.connect(Endpoint::Track(track), 0, Endpoint::Node(mixer), 0)
    })?;
    bridge.edit(&mut session, |s| {
        s.set_parameter(mixer, "channel 1 gain", Some(-6.0))
    })?;
    let mixer_node = bridge.node(Endpoint::Node(mixer));
    bridge.edit(&mut session, |s| s.add_mixer_channels(mixer, 2))?;
    // Replaced in place: same graph node, connection kept, value kept, new channels present.
    assert_eq!(bridge.node(Endpoint::Node(mixer)), mixer_node);
    assert_eq!(
        bridge.parameter(mixer, "channel 1 gain").map(|p| p.get()),
        Some(-6.0)
    );
    assert!(bridge.parameter(mixer, "channel 3 gain").is_some());
    assert_in_step(&bridge, session.state())?;
    // Undo shrinks it again.
    assert!(bridge.undo(&mut session)?);
    assert!(bridge.parameter(mixer, "channel 3 gain").is_none());
    assert_in_step(&bridge, session.state())?;
    Ok(())
}

#[test]
fn test_lookahead_change_replaces_the_node_and_its_latency() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new();
    let (mut bridge, mut runner) = Bridge::new(session.state(), config())?;
    // Runs a block, so the real-time side takes the waiting plans, then commits any held back.
    let mut settle = |bridge: &mut Bridge| -> Result<(), Box<dyn Error>> {
        runner.begin_block(256).map_err(|_| "block refused")?;
        runner.process();
        bridge.commit()?;
        assert!(!bridge.is_pending());
        Ok(())
    };
    let track = bridge.edit(&mut session, |s| s.add_track("drums", Layout::Stereo))?;
    let limiter = bridge.edit(&mut session, |s| {
        s.add_node(NodeKind::Effect {
            kind: "dynamics".to_owned(),
            parameters: BTreeMap::new(),
        })
    })?;
    bridge.edit(&mut session, |s| {
        s.connect(Endpoint::Track(track), 0, Endpoint::Node(limiter), 0)
    })?;
    bridge.edit(&mut session, |s| {
        s.connect(Endpoint::Node(limiter), 0, Endpoint::Output, 0)
    })?;
    settle(&mut bridge)?;
    assert_eq!(bridge.latency(), 0);
    let node = bridge.node(Endpoint::Node(limiter));
    // Set live first, as a control does while dragged, then recorded.
    bridge
        .parameter(limiter, "lookahead")
        .ok_or("no lookahead")?
        .set(5.0);
    bridge.edit(&mut session, |s| {
        s.set_parameter(limiter, "lookahead", Some(5.0))
    })?;
    settle(&mut bridge)?;
    // 5 ms at 48 kHz, on the same graph node, still connected.
    assert_eq!(bridge.latency(), 240);
    assert_eq!(bridge.node(Endpoint::Node(limiter)), node);
    assert_eq!(
        bridge.parameter(limiter, "lookahead").map(|p| p.get()),
        Some(5.0)
    );
    assert_in_step(&bridge, session.state())?;
    assert!(bridge.undo(&mut session)?);
    settle(&mut bridge)?;
    assert_eq!(bridge.latency(), 0);
    assert_in_step(&bridge, session.state())?;
    Ok(())
}

#[test]
fn test_tempo_change_replaces_only_nodes_that_use_it() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new();
    let (mut bridge, _runner) = Bridge::new(session.state(), config())?;
    let add = |bridge: &mut Bridge, session: &mut Session, kind: &str| {
        bridge.edit(session, |s| {
            s.add_node(NodeKind::Effect {
                kind: kind.to_owned(),
                parameters: BTreeMap::new(),
            })
        })
    };
    let delay = add(&mut bridge, &mut session, "delay")?;
    let amplifier = add(&mut bridge, &mut session, "amplifier")?;
    bridge.edit(&mut session, |s| {
        s.connect(Endpoint::Node(delay), 0, Endpoint::Node(amplifier), 0)
    })?;
    // 120 BPM in 4/4 at 48 kHz: a whole note of two seconds.
    assert_eq!(bridge.graph().tempo(), 96_000.0);
    // Handles from before the change: a replaced node's new handles no longer share them.
    let old_mix = bridge.parameter(delay, "mix").ok_or("no mix")?.clone();
    let old_drive = bridge
        .parameter(amplifier, "drive")
        .ok_or("no drive")?
        .clone();
    let delay_node = bridge.node(Endpoint::Node(delay));
    bridge.edit(&mut session, |s| {
        s.set_tempo(project_model::Tempo {
            bpm: 90.0,
            beats_per_bar: 6,
            beat_unit: 8,
        })
    })?;
    // 90 eighth notes a minute: 32 000 frames each, 256 000 a whole note.
    assert_eq!(bridge.graph().tempo(), 256_000.0);
    old_mix.set(77.0);
    old_drive.set(7.0);
    assert_eq!(bridge.parameter(delay, "mix").map(|p| p.get()), Some(30.0));
    assert_eq!(
        bridge.parameter(amplifier, "drive").map(|p| p.get()),
        Some(7.0)
    );
    // The same graph node, still connected.
    assert_eq!(bridge.node(Endpoint::Node(delay)), delay_node);
    assert_in_step(&bridge, session.state())?;
    assert!(bridge.undo(&mut session)?);
    assert_eq!(bridge.graph().tempo(), 96_000.0);
    assert_in_step(&bridge, session.state())?;
    Ok(())
}

/// Stored audio in memory.
struct Memory(Arc<BTreeMap<String, Vec<f32>>>, Vec<String>);

impl ClipAudio for Memory {
    fn read(
        &mut self,
        file: usize,
        frame: u64,
        out: &mut [f32],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let name = self.1.get(file).ok_or("no such file")?;
        let samples = self.0.get(name).ok_or("not stored")?;
        let start = frame as usize;
        out.copy_from_slice(samples.get(start..start + out.len()).ok_or("past end")?);
        Ok(())
    }
}

#[test]
fn test_project_renders_through_its_routing() -> Result<(), Box<dyn Error>> {
    // A mono track at -6 dB into a mixer, a stereo track straight to the output.
    let mut session = Session::new();
    let mono = session.add_track("mono", Layout::Mono)?;
    let stereo = session.add_track("stereo", Layout::Stereo)?;
    let file = |name: &str| AudioFile {
        name: name.to_owned(),
        frames: 1000,
    };
    let mut clip = Clip::new(Source::Mono(file("m.wav")), 0, 1000, 0);
    clip.gain_db = -6.0;
    session.add_clip(mono, clip)?;
    session.add_clip(
        stereo,
        Clip::new(
            Source::Stereo {
                left: file("l.wav"),
                right: file("r.wav"),
            },
            0,
            1000,
            0,
        ),
    )?;
    // Route the mono track through a mixer instead of straight out.
    let direct = session
        .state()
        .connections
        .iter()
        .find(|c| c.from == Endpoint::Track(mono))
        .map(|c| c.id)
        .ok_or("no default connection")?;
    session.apply(Edit::Disconnect(direct))?;
    let mixer = session.add_node(NodeKind::Mixer {
        channels: 1,
        parameters: BTreeMap::new(),
    })?;
    session.connect(Endpoint::Track(mono), 0, Endpoint::Node(mixer), 0)?;
    session.connect(Endpoint::Node(mixer), 0, Endpoint::Output, 0)?;

    let mut stored = BTreeMap::new();
    stored.insert("m.wav".to_owned(), vec![0.5; 1000]);
    stored.insert("l.wav".to_owned(), vec![0.25; 1000]);
    stored.insert("r.wav".to_owned(), vec![-0.25; 1000]);
    let stored = Arc::new(stored);

    let mut graph = render_graph(session.state(), config())?;
    let (renderers, files) = track_renderers(session.state(), &graph.sources, 48_000)?;
    assert_eq!(files.names(), ["m.wav", "l.wav", "r.wav"]);
    let mut tracks: Vec<TrackFeeds> = renderers
        .into_iter()
        .map(|t| TrackFeeds {
            source: t.source,
            feeds: t
                .channels
                .into_iter()
                .map(|r| {
                    Box::new(RenderFeed::new(
                        r,
                        Memory(Arc::clone(&stored), files.names().to_vec()),
                        0,
                    )) as Box<dyn TrackFeed>
                })
                .collect(),
        })
        .collect();
    let (mut left, mut right) = (Vec::new(), Vec::new());
    render(
        &mut graph.runner,
        graph.output,
        &mut tracks,
        0,
        1000,
        128,
        &mut |l, r| {
            left.extend_from_slice(l);
            right.extend_from_slice(r);
            Ok(())
        },
    )?;
    // Mono at −6 dB through the mixer's centre pan (−3 dB each side), plus the stereo track.
    let mono_side = 0.5 * dsp::db_to_gain(-6.0) * dsp::FOLD_DOWN_GAIN;
    // The mixer's gain ramp starts at its value, so every frame is steady.
    for (l, r) in left.iter().zip(&right) {
        assert!((l - (0.25 + mono_side)).abs() < 1e-4, "left {l}");
        assert!((r - (-0.25 + mono_side)).abs() < 1e-4, "right {r}");
    }
    Ok(())
}

#[test]
fn test_stereo_track_through_the_project_mixer_keeps_its_image() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new_project();
    let (mut bridge, _runner) = Bridge::new(session.state(), config())?;
    let guitar = bridge.edit(&mut session, |s| s.add_track("guitar", Layout::Stereo))?;
    let vocal = bridge.edit(&mut session, |s| s.add_track("vocal", Layout::Mono))?;
    assert_in_step(&bridge, session.state())?;
    let file = |name: &str| AudioFile {
        name: name.to_owned(),
        frames: 512,
    };
    session.add_clip(
        guitar,
        Clip::new(
            Source::Stereo {
                left: file("l.wav"),
                right: file("r.wav"),
            },
            0,
            512,
            0,
        ),
    )?;
    session.add_clip(vocal, Clip::new(Source::Mono(file("v.wav")), 0, 512, 0))?;
    let mut stored = BTreeMap::new();
    stored.insert("l.wav".to_owned(), vec![0.25; 512]);
    stored.insert("r.wav".to_owned(), vec![-0.5; 512]);
    stored.insert("v.wav".to_owned(), vec![0.0; 512]);
    let stored = Arc::new(stored);
    let mut graph = render_graph(session.state(), config())?;
    let (renderers, files) = track_renderers(session.state(), &graph.sources, 48_000)?;
    let mut tracks: Vec<TrackFeeds> = renderers
        .into_iter()
        .map(|t| TrackFeeds {
            source: t.source,
            feeds: t
                .channels
                .into_iter()
                .map(|r| {
                    Box::new(RenderFeed::new(
                        r,
                        Memory(Arc::clone(&stored), files.names().to_vec()),
                        0,
                    )) as Box<dyn TrackFeed>
                })
                .collect(),
        })
        .collect();
    let (mut left, mut right) = (Vec::new(), Vec::new());
    render(
        &mut graph.runner,
        graph.output,
        &mut tracks,
        0,
        512,
        128,
        &mut |l, r| {
            left.extend_from_slice(l);
            right.extend_from_slice(r);
            Ok(())
        },
    )?;
    // Left panned hard left and right hard right come out where they went in.
    for (l, r) in left.iter().zip(&right) {
        assert!((l - 0.25).abs() < 1e-5, "left {l}");
        assert!((r + 0.5).abs() < 1e-5, "right {r}");
    }
    Ok(())
}

#[test]
fn test_sided_connections_sync_through_edits_and_undo() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new();
    let (mut bridge, _runner) = Bridge::new(session.state(), config())?;
    let stereo = bridge.edit(&mut session, |s| s.add_track("s", Layout::Stereo))?;
    let bus = bridge.edit(&mut session, |s| s.add_node(NodeKind::Bus(Layout::Stereo)))?;
    let mono_bus = bridge.edit(&mut session, |s| s.add_node(NodeKind::Bus(Layout::Mono)))?;
    bridge.edit(&mut session, |s| {
        s.connect_sides(
            Endpoint::Track(stereo),
            0,
            Some(Side::Right),
            Endpoint::Node(bus),
            0,
            Some(Side::Left),
        )
    })?;
    bridge.edit(&mut session, |s| {
        s.connect_sides(
            Endpoint::Track(stereo),
            0,
            Some(Side::Left),
            Endpoint::Node(mono_bus),
            0,
            None,
        )
    })?;
    assert_in_step(&bridge, session.state())?;
    // Removing the bus removes its connection's helpers too.
    bridge.edit(&mut session, |s| s.remove_node(bus))?;
    assert_in_step(&bridge, session.state())?;
    assert!(bridge.undo(&mut session)?);
    assert_in_step(&bridge, session.state())?;
    // The model refuses a side of a mono port it knows.
    assert!(matches!(
        bridge.edit(&mut session, |s| {
            s.connect_sides(
                Endpoint::Node(mono_bus),
                0,
                Some(Side::Left),
                Endpoint::Output,
                0,
                None,
            )
        }),
        Err(BridgeError::Model(project_model::Error::SideOfMonoPort(_)))
    ));
    assert_in_step(&bridge, session.state())?;
    Ok(())
}

/// A straight curve from value 0 at frame 0 to 1 at frame `end`.
fn ramp_curve(end: i64) -> Result<Curve, modulation::CurveError> {
    let third = end as f64 / 3.0;
    Curve::new(vec![
        Point::new(0, 0.0).with_handles(Handle::default(), Handle::new(third, 1.0 / 3.0)),
        Point::new(end, 1.0).with_handles(Handle::new(-third, -1.0 / 3.0), Handle::default()),
    ])
}

/// A new project with one mono track playing 0.5 for `frames` into the project mixer, whose
/// channel 1 gain is driven by a curve from silence to full.
fn curve_on_mixer_gain(frames: u64) -> Result<Session, Box<dyn Error>> {
    let mut session = Session::new_project();
    let track = session.add_track("mono", Layout::Mono)?;
    let file = AudioFile {
        name: "m.wav".to_owned(),
        frames,
    };
    session.add_clip(track, Clip::new(Source::Mono(file), 0, frames, 0))?;
    let mixer = session.state().project_mixer().ok_or("no project mixer")?;
    let curve = session.add_curve("Curve 1", ramp_curve(frames as i64)?)?;
    let port = session.assign_curve_port(mixer, "channel 1 gain")?;
    session.connect_curve(curve, mixer, port)?;
    Ok(session)
}

/// Renders `session` from frame 0 for `frames` in blocks of `block`, returning the left channel.
fn render_left(session: &Session, frames: u64, block: usize) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut stored = BTreeMap::new();
    stored.insert("m.wav".to_owned(), vec![0.5; frames as usize]);
    let stored = Arc::new(stored);
    let mut graph = render_graph(session.state(), Config::new(48_000, 512))?;
    let (renderers, files) = track_renderers(session.state(), &graph.sources, 48_000)?;
    let mut tracks: Vec<TrackFeeds> = renderers
        .into_iter()
        .map(|t| TrackFeeds {
            source: t.source,
            feeds: t
                .channels
                .into_iter()
                .map(|r| {
                    Box::new(RenderFeed::new(
                        r,
                        Memory(Arc::clone(&stored), files.names().to_vec()),
                        0,
                    )) as Box<dyn TrackFeed>
                })
                .collect(),
        })
        .collect();
    let mut left = Vec::new();
    render(
        &mut graph.runner,
        graph.output,
        &mut tracks,
        0,
        frames,
        block,
        &mut |l, _| {
            left.extend_from_slice(l);
            Ok(())
        },
    )?;
    Ok(left)
}

#[test]
fn test_curve_on_mixer_gain_renders_the_same_at_any_block_size() -> Result<(), Box<dyn Error>> {
    let frames = 9_600;
    let session = curve_on_mixer_gain(frames)?;
    let renders = [64, 100, 512]
        .into_iter()
        .map(|block| render_left(&session, frames, block))
        .collect::<Result<Vec<_>, _>>()?;
    assert_eq!(renders[0], renders[1]);
    assert_eq!(renders[0], renders[2]);
    // The gain moves from its set value (0 dB) to the curve's within one control period, then
    // follows the curve up from silence.
    let left = &renders[0];
    assert!(left[64] < 0.01, "{}", left[64]);
    assert!(left[2_000] < left[6_000] && left[6_000] < left[9_500]);
    Ok(())
}

#[test]
fn test_bridge_follows_curve_edits() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new_project();
    let (mut bridge, _runner) = Bridge::new(session.state(), config())?;
    let mixer = session.state().project_mixer().ok_or("no project mixer")?;
    let curve = bridge.edit(&mut session, |s| s.add_curve("Curve 1", Curve::default()))?;
    let port = bridge.edit(&mut session, |s| {
        s.assign_curve_port(mixer, "channel 1 gain")
    })?;
    bridge.edit(&mut session, |s| s.connect_curve(curve, mixer, port))?;
    assert_in_step(&bridge, session.state())?;
    let controls = |bridge: &Bridge| bridge.graph().control_connections().count();
    assert_eq!(controls(&bridge), 1);
    let node = bridge.curve(curve).ok_or("curve not mirrored")?;

    // A new shape keeps the curve's graph node and its wire.
    bridge.edit(&mut session, |s| {
        s.set_curve(curve, vec![Point::new(0, 0.75)])
    })?;
    assert_eq!(bridge.curve(curve), Some(node));
    assert_eq!(controls(&bridge), 1);

    bridge.edit(&mut session, |s| {
        s.unassign_curve_port(mixer, "channel 1 gain")
    })?;
    assert_eq!(controls(&bridge), 0);
    assert!(bridge.undo(&mut session)?);
    assert_eq!(controls(&bridge), 1);
    bridge.edit(&mut session, |s| s.remove_curve(curve))?;
    assert_eq!(controls(&bridge), 0);
    assert_eq!(bridge.curve(curve), None);
    assert_in_step(&bridge, session.state())?;
    Ok(())
}
