//! Editing behaviour: layering, validation, versions.

use std::error::Error as StdError;

use audio_buffers::Layout;
use project_model::{AudioFile, Clip, Edit, Endpoint, Error, NodeKind, Session, Source};

type TestResult = Result<(), Box<dyn StdError>>;

fn mono(name: &str, frames: u64) -> Source {
    Source::Mono(AudioFile {
        name: name.into(),
        frames,
    })
}

fn stereo(frames: u64) -> Source {
    Source::Stereo {
        left: AudioFile {
            name: "take-L.wav".into(),
            frames,
        },
        right: AudioFile {
            name: "take-R.wav".into(),
            frames,
        },
    }
}

#[test]
fn test_add_clip_checks_the_clip() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("Vocals", Layout::Mono)?;
    let source = mono("a.wav", 1_000);
    let clip = |offset, length| Clip::new(source.clone(), offset, length, 0);
    assert_eq!(session.add_clip(track, clip(0, 0)), Err(Error::EmptyClip));
    assert_eq!(
        session.add_clip(track, clip(900, 200)),
        Err(Error::ClipOutsideSource {
            end: 1_100,
            frames: 1_000
        })
    );
    assert_eq!(
        session.add_clip(track, Clip::new(stereo(1_000), 0, 10, 0)),
        Err(Error::LayoutMismatch)
    );
    let faded = Clip {
        fade_in: 6,
        fade_out: 5,
        ..clip(0, 10)
    };
    assert_eq!(
        session.add_clip(track, faded),
        Err(Error::FadesTooLong {
            fades: 11,
            length: 10
        })
    );
    let loud = Clip {
        gain_db: f32::INFINITY,
        ..clip(0, 10)
    };
    assert_eq!(session.add_clip(track, loud), Err(Error::InvalidGain));
    let late = Clip::new(source.clone(), 0, 10, u64::MAX - 5);
    assert_eq!(
        session.add_clip(track, late),
        Err(Error::ClipOutsideTimeline)
    );
    // Refused edits leave no history.
    assert!(session.undo());
    assert!(!session.can_undo());
    Ok(())
}

#[test]
fn test_stereo_track_takes_paired_files() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("Piano", Layout::Stereo)?;
    let clip = session.add_clip(track, Clip::new(stereo(500), 100, 400, 0))?;
    assert_eq!(session.state().clip(clip).map(|c| c.length), Some(400));
    let files: Vec<&str> = session.referenced_files().into_iter().collect();
    assert_eq!(files, ["take-L.wav", "take-R.wav"]);
    Ok(())
}

#[test]
fn test_moved_clip_goes_on_top() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("Guitar", Layout::Mono)?;
    let other = session.add_track("Guitar 2", Layout::Mono)?;
    let first = session.add_clip(track, Clip::new(mono("a.wav", 100), 0, 100, 0))?;
    let second = session.add_clip(track, Clip::new(mono("b.wav", 100), 0, 100, 50))?;
    let layers = |s: &Session| -> Vec<_> {
        s.state().tracks[0]
            .clips
            .iter()
            .map(|(id, _)| *id)
            .collect()
    };
    assert_eq!(layers(&session), [first, second]);
    session.move_clip(first, track, 10)?;
    assert_eq!(layers(&session), [second, first]);
    // A trim keeps the layer; a change of position is a move.
    let mut trimmed = session.state().clip(second).ok_or("missing")?.clone();
    trimmed.length = 60;
    session.set_clip(second, trimmed.clone())?;
    assert_eq!(layers(&session), [second, first]);
    trimmed.position = 0;
    session.set_clip(second, trimmed)?;
    assert_eq!(layers(&session), [first, second]);
    // Across tracks, keeping the id.
    session.move_clip(first, other, 0)?;
    assert_eq!(session.state().find_clip(first), Some((other, 0)));
    // Each gesture is one step.
    session.undo();
    assert_eq!(layers(&session), [first, second]);
    Ok(())
}

#[test]
fn test_batch_is_all_or_nothing() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("Drums", Layout::Mono)?;
    let before = session.state().clone();
    let id = session.new_clip_id();
    let clip = Clip::new(mono("a.wav", 100), 0, 100, 0);
    let batch = Edit::Batch(vec![
        Edit::InsertClip {
            track,
            layer: 0,
            id,
            clip: clip.clone(),
        },
        Edit::RenameTrack {
            track,
            name: "Kit".into(),
        },
        Edit::InsertClip {
            track,
            layer: 0,
            id,
            clip,
        },
    ]);
    assert_eq!(session.apply(batch), Err(Error::ClipExists(id)));
    assert_eq!(session.state(), &before);
    Ok(())
}

#[test]
fn test_restoring_a_version_is_undoable() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("Bass", Layout::Mono)?;
    session.add_clip(track, Clip::new(mono("old.wav", 10), 0, 10, 0))?;
    session.save_version("first mix")?;
    assert_eq!(
        session.save_version("first mix"),
        Err(Error::VersionExists("first mix".into()))
    );
    let saved = session.state().clone();
    session.add_clip(track, Clip::new(mono("new.wav", 10), 0, 10, 20))?;
    session.apply(Edit::RenameTrack {
        track,
        name: "Bass DI".into(),
    })?;
    let edited = session.state().clone();
    assert_eq!(
        session.referenced_files().into_iter().collect::<Vec<_>>(),
        ["new.wav", "old.wav"]
    );

    session.restore_version("first mix")?;
    assert_eq!(session.state(), &saved);
    assert!(session.undo());
    assert_eq!(session.state(), &edited);
    assert!(session.redo());
    assert_eq!(session.state(), &saved);
    assert_eq!(
        session.restore_version("nope"),
        Err(Error::UnknownVersion("nope".into()))
    );
    session.remove_version("first mix")?;
    assert!(session.versions().is_empty());
    Ok(())
}

#[test]
fn test_open_never_reuses_ids() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("A", Layout::Mono)?;
    session.add_clip(track, Clip::new(mono("a.wav", 10), 0, 10, 0))?;
    session.save_version("v")?;
    let mut reopened = Session::open(session.state().clone(), session.versions().clone());
    assert!(!reopened.can_undo());
    let new = reopened.add_track("B", Layout::Mono)?;
    assert!(new.0 > 1);
    Ok(())
}

#[test]
fn test_edits_refuse_bad_targets() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("A", Layout::Mono)?;
    let missing = session.new_clip_id();
    assert_eq!(
        session.apply(Edit::RemoveClip(missing)),
        Err(Error::UnknownClip(missing))
    );
    assert_eq!(
        session.apply(Edit::MoveTrack { track, index: 1 }),
        Err(Error::IndexOutOfRange { index: 1, limit: 1 })
    );
    let clone = session.state().tracks[0].clone();
    assert_eq!(
        session.apply(Edit::InsertTrack {
            index: 0,
            track: clone
        }),
        Err(Error::TrackExists(track))
    );
    Ok(())
}

#[test]
fn test_new_track_is_connected_to_the_output() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("Vocals", Layout::Mono)?;
    let connections: Vec<_> = session
        .state()
        .connections_of(Endpoint::Track(track))
        .collect();
    assert_eq!(connections.len(), 1);
    assert_eq!(
        (connections[0].to, connections[0].pan),
        (Endpoint::Output, 0.0)
    );
    // Adding the track and its connection is one step.
    assert!(session.undo());
    assert_eq!(session.state(), &project_model::State::default());
    Ok(())
}

#[test]
fn test_remove_track_takes_its_connections_and_undoes_whole() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("Guitar", Layout::Mono)?;
    let bus = session.add_node(NodeKind::Bus(Layout::Stereo))?;
    session.connect(Endpoint::Track(track), 0, Endpoint::Node(bus), 0)?;
    session.connect(Endpoint::Node(bus), 0, Endpoint::Output, 0)?;
    let before = session.state().clone();
    assert_eq!(
        session.apply(Edit::RemoveTrack(track)),
        Err(Error::StillConnected(Endpoint::Track(track)))
    );
    session.remove_track(track)?;
    assert!(session.state().track(track).is_none());
    assert_eq!(session.state().connections.len(), 1);
    session.undo();
    assert_eq!(session.state(), &before);
    Ok(())
}

#[test]
fn test_connect_checks_endpoints() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("A", Layout::Mono)?;
    let delay = session.add_node(NodeKind::FeedbackDelay {
        layout: Layout::Mono,
        seconds: 0.25,
    })?;
    assert_eq!(
        session.connect(Endpoint::Output, 0, Endpoint::Node(delay), 0),
        Err(Error::NoSuchPort(Endpoint::Output))
    );
    assert_eq!(
        session.connect(Endpoint::Node(delay), 0, Endpoint::Track(track), 0),
        Err(Error::NoSuchPort(Endpoint::Track(track)))
    );
    assert_eq!(
        session.connect(Endpoint::Node(delay), 1, Endpoint::Output, 0),
        Err(Error::NoSuchPort(Endpoint::Node(delay)))
    );
    assert_eq!(
        session.connect(Endpoint::Track(track), 0, Endpoint::Output, 0),
        Err(Error::AlreadyConnected)
    );
    let missing = project_model::NodeId(999);
    assert_eq!(
        session.connect(Endpoint::Node(missing), 0, Endpoint::Output, 0),
        Err(Error::UnknownNode(missing))
    );
    Ok(())
}

#[test]
fn test_parameters_and_delay_times_are_undoable() -> TestResult {
    let mut session = Session::new();
    let eq = session.add_node(NodeKind::Effect {
        kind: "eq".into(),
        parameters: Default::default(),
    })?;
    let delay = session.add_node(NodeKind::FeedbackDelay {
        layout: Layout::Stereo,
        seconds: 0.5,
    })?;
    let fresh = session.state().clone();
    session.set_parameter(eq, "low gain", Some(3.0))?;
    session.set_parameter(eq, "low gain", Some(-2.0))?;
    session.apply(Edit::SetDelay {
        node: delay,
        seconds: 1.0,
    })?;
    assert_eq!(
        session.set_parameter(delay, "x", Some(1.0)),
        Err(Error::WrongNodeKind(delay))
    );
    assert_eq!(
        session.set_parameter(eq, "low gain", Some(f32::NAN)),
        Err(Error::InvalidValue)
    );
    assert_eq!(
        session.apply(Edit::SetDelay {
            node: delay,
            seconds: 11.0
        }),
        Err(Error::InvalidValue)
    );
    session.undo();
    session.undo();
    let NodeKind::Effect { parameters, .. } = &session.state().nodes[0].kind else {
        return Err("not an effect".into());
    };
    assert_eq!(parameters.get("low gain"), Some(&3.0));
    session.undo();
    assert_eq!(session.state(), &fresh);
    Ok(())
}

#[test]
fn test_version_captures_routing() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("A", Layout::Mono)?;
    session.save_version("dry")?;
    let dry = session.state().clone();
    let bus = session.add_node(NodeKind::Bus(Layout::Stereo))?;
    session.connect(Endpoint::Track(track), 0, Endpoint::Node(bus), 0)?;
    session.restore_version("dry")?;
    assert_eq!(session.state(), &dry);
    // Within the session the bus's id stays taken, though nothing holds it now.
    let id = session.add_node(NodeKind::Bus(Layout::Mono))?;
    assert!(id.0 > bus.0);
    // A reopened session avoids every id in the state and the versions.
    let mut reopened = Session::open(session.state().clone(), session.versions().clone());
    let reopened_id = reopened.add_node(NodeKind::Bus(Layout::Mono))?;
    assert!(reopened_id.0 > id.0);
    Ok(())
}

#[test]
fn test_mixer_ports_follow_its_channels() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("A", Layout::Mono)?;
    assert_eq!(
        session.add_node(NodeKind::Mixer {
            channels: 0,
            parameters: Default::default(),
        }),
        Err(Error::InvalidValue)
    );
    let mixer = session.add_node(NodeKind::Mixer {
        channels: 2,
        parameters: Default::default(),
    })?;
    let node = Endpoint::Node(mixer);
    session.connect(Endpoint::Track(track), 0, node, 1)?;
    session.connect(node, 0, Endpoint::Output, 0)?;
    assert_eq!(
        session.connect(Endpoint::Track(track), 0, node, 2),
        Err(Error::NoSuchPort(node))
    );
    assert_eq!(
        session.connect(node, 1, Endpoint::Output, 0),
        Err(Error::NoSuchPort(node))
    );
    session.add_mixer_channels(mixer, 1)?;
    session.connect(Endpoint::Track(track), 0, node, 2)?;
    Ok(())
}

#[test]
fn test_mixer_channels_and_parameters_are_undoable() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("A", Layout::Mono)?;
    let mixer = session.add_node(NodeKind::Mixer {
        channels: 1,
        parameters: Default::default(),
    })?;
    let fresh = session.state().clone();
    let channels = |session: &Session| match &session.state().nodes[0].kind {
        NodeKind::Mixer { channels, .. } => Some(*channels),
        _ => None,
    };
    session.set_parameter(mixer, "channel 1 mode", Some(2.0))?;
    session.add_mixer_channels(mixer, 2)?;
    assert_eq!(channels(&session), Some(3));
    assert_eq!(
        session.add_mixer_channels(mixer, 0),
        Err(Error::InvalidValue)
    );
    let bus = session.add_node(NodeKind::Bus(Layout::Mono))?;
    assert_eq!(
        session.add_mixer_channels(bus, 1),
        Err(Error::WrongNodeKind(bus))
    );
    session.undo();
    session.undo();
    assert_eq!(channels(&session), Some(1));
    session.undo();
    assert_eq!(session.state(), &fresh);
    // Fewer channels are refused while a removed one is connected.
    session.redo();
    session.redo();
    let connection = session.connect(Endpoint::Track(track), 0, Endpoint::Node(mixer), 2)?;
    assert_eq!(
        session.apply(Edit::SetMixerChannels {
            node: mixer,
            channels: 2,
        }),
        Err(Error::StillConnected(Endpoint::Node(mixer)))
    );
    session.apply(Edit::Disconnect(connection))?;
    session.apply(Edit::SetMixerChannels {
        node: mixer,
        channels: 2,
    })?;
    assert_eq!(channels(&session), Some(2));
    Ok(())
}
