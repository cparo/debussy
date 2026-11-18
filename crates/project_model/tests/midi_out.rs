//! MIDI output nodes in the routing: one event input 0, no outputs
//! (`docs/architecture/signal-graph.md` "Instruments", `docs/architecture/engine.md`
//! "MIDI output").

use std::error::Error as StdError;

use audio_buffers::Layout;
use project_model::{Destination, Edit, Endpoint, Error, MAX_PORT_BYTES, NodeKind, Session};

type TestResult = Result<(), Box<dyn StdError>>;

fn midi_out(port: &str) -> Destination {
    Destination::MidiOut {
        port: port.to_owned(),
        name: format!("Synth {port}"),
    }
}

#[test]
fn test_set_destination_midi_out_wires_only_the_events_and_undoes() -> TestResult {
    let mut session = Session::new_project();
    let mixer = session.state().project_mixer().ok_or("no project mixer")?;
    let track = session.add_midi_track("keys")?;
    let synth = session.set_instrument(track, "synth")?;
    let channels = |s: &Session| match s.state().node(mixer).map(|n| &n.kind) {
        Some(NodeKind::Mixer { channels, .. }) => *channels,
        _ => 0,
    };
    let with_synth = session.state().clone();
    let out = session.set_destination(track, midi_out("a"))?;
    let state = session.state();
    assert!(state.node(synth).is_none());
    assert_eq!(
        state.node(out).map(|n| &n.kind),
        Some(&NodeKind::MidiOut {
            port: "a".to_owned(),
            name: "Synth a".to_owned()
        })
    );
    let connections: Vec<_> = state
        .connections
        .iter()
        .filter(|c| c.touches(Endpoint::Node(out)))
        .map(|c| (c.from, c.to, c.input))
        .collect();
    assert_eq!(
        connections,
        [(Endpoint::Track(track), Endpoint::Node(out), 0)]
    );
    // The synth's mixer channels are left as they were, unconnected.
    assert_eq!(channels(&session), channels(&Session::new_project()) + 1);
    assert!(session.undo());
    assert_eq!(session.state(), &with_synth);
    Ok(())
}

#[test]
fn test_set_destination_replaces_a_midi_out_with_an_instrument() -> TestResult {
    let mut session = Session::new_project();
    let track = session.add_midi_track("keys")?;
    let out = session.set_destination(track, midi_out("a"))?;
    let synth = session.set_destination(track, Destination::Instrument("synth".to_owned()))?;
    assert!(session.state().node(out).is_none());
    assert_eq!(
        session
            .state()
            .connections
            .iter()
            .filter(|c| c.from == Endpoint::Node(synth))
            .count(),
        2
    );
    Ok(())
}

#[test]
fn test_set_destination_on_audio_track_refused() -> TestResult {
    let mut session = Session::new_project();
    let track = session.add_track("a", Layout::Stereo)?;
    assert_eq!(
        session.set_destination(track, midi_out("a")),
        Err(Error::KindMismatch)
    );
    Ok(())
}

#[test]
fn test_set_destination_refuses_an_overlong_port() -> TestResult {
    let mut session = Session::new_project();
    let track = session.add_midi_track("keys")?;
    let before = session.state().clone();
    let long = "x".repeat(MAX_PORT_BYTES + 1);
    assert_eq!(
        session.set_destination(track, midi_out(&long)),
        Err(Error::InvalidValue)
    );
    assert_eq!(session.state(), &before);
    Ok(())
}

#[test]
fn test_midi_out_has_no_output_and_takes_no_audio() -> TestResult {
    let mut session = Session::new();
    let midi = session.add_midi_track("m")?;
    let audio = session.add_track("a", Layout::Mono)?;
    let out = session.add_node(NodeKind::MidiOut {
        port: "p".to_owned(),
        name: "P".to_owned(),
    })?;
    assert!(
        session
            .connect(Endpoint::Node(out), 0, Endpoint::Output, 0)
            .is_err()
    );
    assert_eq!(
        session.connect(Endpoint::Track(audio), 0, Endpoint::Node(out), 0),
        Err(Error::WrongPortType)
    );
    assert!(
        session
            .connect(Endpoint::Track(midi), 0, Endpoint::Node(out), 1)
            .is_err()
    );
    session.connect(Endpoint::Track(midi), 0, Endpoint::Node(out), 0)?;
    Ok(())
}

#[test]
fn test_set_midi_port_changes_and_undoes() -> TestResult {
    let mut session = Session::new();
    let out = session.add_node(NodeKind::MidiOut {
        port: "p".to_owned(),
        name: "P".to_owned(),
    })?;
    let before = session.state().clone();
    session.apply(Edit::SetMidiPort {
        node: out,
        port: "q".to_owned(),
        name: "Q".to_owned(),
    })?;
    assert_eq!(
        session.state().node(out).map(|n| &n.kind),
        Some(&NodeKind::MidiOut {
            port: "q".to_owned(),
            name: "Q".to_owned()
        })
    );
    assert!(session.undo());
    assert_eq!(session.state(), &before);
    let delay = session.add_node(NodeKind::FeedbackDelay {
        layout: Layout::Mono,
        seconds: 0.1,
    })?;
    assert_eq!(
        session.apply(Edit::SetMidiPort {
            node: delay,
            port: "q".to_owned(),
            name: "Q".to_owned(),
        }),
        Err(Error::WrongNodeKind(delay))
    );
    Ok(())
}
