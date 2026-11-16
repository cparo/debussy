//! Instrument nodes in the routing: event input 0 from a MIDI track, stereo output 0
//! (`docs/architecture/signal-graph.md` "Instruments", ADR-0147).

use std::collections::BTreeMap;
use std::error::Error as StdError;

use audio_buffers::Layout;
use project_model::{Endpoint, Error, NodeKind, Session, Side};

type TestResult = Result<(), Box<dyn StdError>>;

/// An instrument of a kind no build knows: its ports are still the model's.
fn unknown_instrument() -> NodeKind {
    NodeKind::Instrument {
        kind: "from-the-future".to_owned(),
        parameters: BTreeMap::new(),
        zones: Vec::new(),
    }
}

#[test]
fn test_connect_midi_track_to_instrument_accepted() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("m")?;
    let synth = session.add_node(unknown_instrument())?;
    session.connect(Endpoint::Track(track), 0, Endpoint::Node(synth), 0)?;
    session.connect(Endpoint::Node(synth), 0, Endpoint::Output, 0)?;
    assert_eq!(session.state().connections.len(), 2);
    Ok(())
}

#[test]
fn test_connect_midi_track_to_effect_refused() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("m")?;
    let eq = session.add_node(NodeKind::Effect {
        kind: "parametric-eq".to_owned(),
        parameters: BTreeMap::new(),
    })?;
    assert_eq!(
        session.connect(Endpoint::Track(track), 0, Endpoint::Node(eq), 0),
        Err(Error::WrongPortType)
    );
    Ok(())
}

#[test]
fn test_connect_audio_to_instrument_input_refused() -> TestResult {
    let mut session = Session::new();
    let audio = session.add_track("a", Layout::Stereo)?;
    let bus = session.add_node(NodeKind::Bus(Layout::Stereo))?;
    let synth = session.add_node(unknown_instrument())?;
    for from in [Endpoint::Track(audio), Endpoint::Node(bus)] {
        assert_eq!(
            session.connect(from, 0, Endpoint::Node(synth), 0),
            Err(Error::WrongPortType)
        );
    }
    Ok(())
}

#[test]
fn test_connect_instrument_ports_beyond_zero_refused() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("m")?;
    let synth = session.add_node(unknown_instrument())?;
    assert!(matches!(
        session.connect(Endpoint::Track(track), 0, Endpoint::Node(synth), 1),
        Err(Error::WrongPortType | Error::NoSuchPort(_))
    ));
    assert_eq!(
        session.connect(Endpoint::Node(synth), 1, Endpoint::Output, 0),
        Err(Error::NoSuchPort(Endpoint::Node(synth)))
    );
    Ok(())
}

#[test]
fn test_connect_side_of_event_port_refused() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("m")?;
    let synth = session.add_node(unknown_instrument())?;
    assert_eq!(
        session.connect_sides(
            Endpoint::Track(track),
            0,
            None,
            Endpoint::Node(synth),
            0,
            Some(Side::Left)
        ),
        Err(Error::SideOfMonoPort(Endpoint::Node(synth)))
    );
    Ok(())
}

#[test]
fn test_instrument_output_sides_accepted() -> TestResult {
    let mut session = Session::new();
    let synth = session.add_node(unknown_instrument())?;
    session.connect_sides(
        Endpoint::Node(synth),
        0,
        Some(Side::Right),
        Endpoint::Output,
        0,
        Some(Side::Left),
    )?;
    Ok(())
}

#[test]
fn test_set_parameter_on_instrument_undoes() -> TestResult {
    let mut session = Session::new();
    let synth = session.add_node(unknown_instrument())?;
    session.set_parameter(synth, "cutoff", Some(800.0))?;
    let parameters = |s: &Session| match &s.state().nodes[0].kind {
        NodeKind::Instrument { parameters, .. } => parameters.get("cutoff").copied(),
        _ => None,
    };
    assert_eq!(parameters(&session), Some(800.0));
    assert!(session.undo());
    assert_eq!(parameters(&session), None);
    assert_eq!(
        session.set_parameter(synth, "cutoff", Some(f32::NAN)),
        Err(Error::InvalidValue)
    );
    Ok(())
}

#[test]
fn test_instrument_ids_differ_from_effect_ids() {
    for instrument in instruments::INSTRUMENTS {
        assert!(
            audio_effects::display_name(instrument.id).is_none(),
            "{} is also an effect id",
            instrument.id
        );
    }
}

/// The instruments in the state, by kind.
fn instrument_kinds(session: &Session) -> Vec<String> {
    session
        .state()
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::Instrument { kind, .. } => Some(kind.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn test_set_instrument_wires_track_to_project_mixer_and_undoes() -> TestResult {
    let mut session = Session::new_project();
    let mixer = session.state().project_mixer().ok_or("no project mixer")?;
    let track = session.add_midi_track("keys")?;
    let before = session.state().clone();
    let synth = session.set_instrument(track, "synth")?;
    assert_eq!(instrument_kinds(&session), ["synth"]);
    let state = session.state();
    let from_track: Vec<_> = state.connections_of(Endpoint::Track(track)).collect();
    assert_eq!(from_track.len(), 1);
    assert_eq!(
        (from_track[0].to, from_track[0].input),
        (Endpoint::Node(synth), 0)
    );
    let sides: Vec<_> = state
        .connections
        .iter()
        .filter(|c| c.from == Endpoint::Node(synth))
        .map(|c| (c.to, c.from_side))
        .collect();
    assert_eq!(
        sides,
        [
            (Endpoint::Node(mixer), Some(Side::Left)),
            (Endpoint::Node(mixer), Some(Side::Right))
        ]
    );
    assert!(session.undo());
    assert_eq!(session.state(), &before);
    Ok(())
}

#[test]
fn test_set_instrument_replaces_the_one_the_track_fed() -> TestResult {
    let mut session = Session::new_project();
    let mixer = session.state().project_mixer().ok_or("no project mixer")?;
    let track = session.add_midi_track("keys")?;
    let first = session.set_instrument(track, "synth")?;
    let channels = |s: &Session| match s.state().node(mixer).map(|n| &n.kind) {
        Some(NodeKind::Mixer { channels, .. }) => *channels,
        _ => 0,
    };
    let before = channels(&session);
    let second = session.set_instrument(track, "from-the-future")?;
    assert!(session.state().node(first).is_none());
    assert_eq!(instrument_kinds(&session), ["from-the-future"]);
    assert!(
        session
            .state()
            .connections_of(Endpoint::Node(first))
            .next()
            .is_none()
    );
    // The new instrument took the channels the old one left.
    assert_eq!(channels(&session), before);
    assert_eq!(
        session
            .state()
            .connections
            .iter()
            .filter(|c| c.from == Endpoint::Node(second))
            .count(),
        2
    );
    assert!(session.undo());
    assert!(session.state().node(first).is_some());
    assert!(session.state().node(second).is_none());
    Ok(())
}

#[test]
fn test_set_instrument_keeps_an_instrument_another_track_feeds() -> TestResult {
    let mut session = Session::new_project();
    let a = session.add_midi_track("a")?;
    let b = session.add_midi_track("b")?;
    let shared = session.set_instrument(a, "synth")?;
    session.connect(Endpoint::Track(b), 0, Endpoint::Node(shared), 0)?;
    session.set_instrument(a, "synth")?;
    assert!(session.state().node(shared).is_some());
    assert_eq!(instrument_kinds(&session).len(), 2);
    // Only b still feeds the shared one.
    let feeding: Vec<_> = session
        .state()
        .connections
        .iter()
        .filter(|c| c.to == Endpoint::Node(shared))
        .map(|c| c.from)
        .collect();
    assert_eq!(feeding, [Endpoint::Track(b)]);
    Ok(())
}

#[test]
fn test_set_instrument_without_mixer_feeds_the_output() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("keys")?;
    let synth = session.set_instrument(track, "synth")?;
    assert!(
        session
            .state()
            .connections
            .iter()
            .any(|c| c.from == Endpoint::Node(synth) && c.to == Endpoint::Output)
    );
    Ok(())
}

#[test]
fn test_set_instrument_on_audio_track_refused() -> TestResult {
    let mut session = Session::new_project();
    let track = session.add_track("a", Layout::Stereo)?;
    assert_eq!(
        session.set_instrument(track, "synth"),
        Err(Error::KindMismatch)
    );
    Ok(())
}
