//! The default project mixer: new tracks join it, stereo tracks as a hard-panned left/right pair,
//! and sided connections are checked (ADR-0058).

use std::error::Error;

use audio_buffers::Layout;
use project_model::{Endpoint, Error as ModelError, NodeKind, Session, Side};

fn pan(session: &Session, channel: usize) -> Option<f32> {
    let mixer = session.state().project_mixer()?;
    match &session.state().node(mixer)?.kind {
        NodeKind::Mixer { parameters, .. } => Some(
            parameters
                .get(&format!("channel {} pan", channel + 1))
                .copied()
                .unwrap_or(0.0),
        ),
        _ => None,
    }
}

fn channels(session: &Session) -> usize {
    let Some(mixer) = session.state().project_mixer() else {
        return 0;
    };
    match session.state().node(mixer).map(|n| &n.kind) {
        Some(NodeKind::Mixer { channels, .. }) => *channels,
        _ => 0,
    }
}

#[test]
fn test_new_project_has_a_mixer_on_the_output() -> Result<(), Box<dyn Error>> {
    let session = Session::new_project();
    let mixer = session.state().project_mixer().ok_or("no project mixer")?;
    assert_eq!(channels(&session), 1);
    assert!(!session.can_undo());
    assert_eq!(session.state().free_mixer_channels(mixer), vec![0]);
    Ok(())
}

#[test]
fn test_tracks_join_the_mixer_with_stereo_as_a_pair() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new_project();
    let mixer = session.state().project_mixer().ok_or("no project mixer")?;
    let vocal = session.add_track("Vocal", Layout::Mono)?;
    let guitar = session.add_track("Guitar", Layout::Stereo)?;
    assert_eq!(channels(&session), 3);
    let into_mixer = |track| {
        session
            .state()
            .connections
            .iter()
            .filter(|c| c.from == Endpoint::Track(track) && c.to == Endpoint::Node(mixer))
            .map(|c| (c.input, c.from_side))
            .collect::<Vec<_>>()
    };
    assert_eq!(into_mixer(vocal), vec![(0, None)]);
    assert_eq!(
        into_mixer(guitar),
        vec![(1, Some(Side::Left)), (2, Some(Side::Right))]
    );
    assert_eq!(
        (pan(&session, 0), pan(&session, 1), pan(&session, 2)),
        (Some(0.0), Some(-1.0), Some(1.0))
    );
    // Nothing goes straight to the output but the mixer.
    assert_eq!(session.state().connections_of(Endpoint::Output).count(), 1);

    // One undo removes the stereo track and the channels it added.
    assert!(session.undo());
    assert_eq!(channels(&session), 1);
    assert!(session.redo());

    // A removed track frees its channels for the next one, which resets their values.
    session.set_parameter(mixer, "channel 1 gain", Some(-12.0))?;
    session.remove_track(vocal)?;
    let bass = session.add_track("Bass", Layout::Mono)?;
    assert_eq!(channels(&session), 3);
    let bass_channel = session
        .state()
        .connections
        .iter()
        .find(|c| c.from == Endpoint::Track(bass))
        .map(|c| c.input);
    assert_eq!(bass_channel, Some(0));
    match &session.state().node(mixer).ok_or("mixer gone")?.kind {
        NodeKind::Mixer { parameters, .. } => assert!(!parameters.contains_key("channel 1 gain")),
        _ => return Err("not a mixer".into()),
    }
    Ok(())
}

#[test]
fn test_without_a_project_mixer_tracks_go_to_the_output() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new();
    let track = session.add_track("Old", Layout::Stereo)?;
    let c = session.state().connections.first().ok_or("no connection")?;
    assert_eq!(
        (c.from, c.to, c.from_side),
        (Endpoint::Track(track), Endpoint::Output, None)
    );
    Ok(())
}

#[test]
fn test_sides_only_of_ports_that_can_be_stereo() -> Result<(), Box<dyn Error>> {
    let mut session = Session::new();
    let mono = session.add_track("m", Layout::Mono)?;
    let stereo = session.add_track("s", Layout::Stereo)?;
    let bus = session.add_node(NodeKind::Bus(Layout::Stereo))?;
    // The left of a stereo track into the right of a stereo bus, and separately the right.
    session.connect_sides(
        Endpoint::Track(stereo),
        0,
        Some(Side::Left),
        Endpoint::Node(bus),
        0,
        Some(Side::Right),
    )?;
    session.connect_sides(
        Endpoint::Track(stereo),
        0,
        Some(Side::Right),
        Endpoint::Node(bus),
        0,
        Some(Side::Right),
    )?;
    // The same sides again are a duplicate.
    assert!(matches!(
        session.connect_sides(
            Endpoint::Track(stereo),
            0,
            Some(Side::Left),
            Endpoint::Node(bus),
            0,
            Some(Side::Right)
        ),
        Err(ModelError::AlreadyConnected)
    ));
    // A side of a mono track is refused.
    assert!(matches!(
        session.connect_sides(
            Endpoint::Track(mono),
            0,
            Some(Side::Left),
            Endpoint::Node(bus),
            0,
            None
        ),
        Err(ModelError::SideOfMonoPort(_))
    ));
    Ok(())
}
