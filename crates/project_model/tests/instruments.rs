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
