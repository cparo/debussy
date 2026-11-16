//! A sampler's zones in the model: edits, undo and the files they use
//! (`docs/architecture/instruments.md` "Sampler", ADR-0154).

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error as StdError;

use project_model::{
    AudioFile, Error, NodeId, NodeKind, SAMPLER_KIND, SampleZone, Session, Source, ZoneMode,
};

type TestResult = Result<(), Box<dyn StdError>>;

fn zone(name: &str, key: u8) -> SampleZone {
    SampleZone {
        file: Source::Mono(AudioFile {
            name: name.to_owned(),
            frames: 480,
        }),
        low: key,
        high: key,
        root: key,
        mode: ZoneMode::OneShot,
        choke: None,
    }
}

fn zones_of(session: &Session, node: NodeId) -> Vec<SampleZone> {
    match session.state().node(node).map(|n| &n.kind) {
        Some(NodeKind::Instrument { zones, .. }) => zones.clone(),
        _ => Vec::new(),
    }
}

/// A MIDI track with a sampler, and the sampler's id.
fn sampler(session: &mut Session) -> Result<NodeId, Error> {
    let track = session.add_midi_track("Drums")?;
    session.set_instrument(track, SAMPLER_KIND)
}

#[test]
fn test_set_zones_is_one_undo_step() -> TestResult {
    let mut session = Session::new();
    let node = sampler(&mut session)?;
    session.set_zones(node, vec![zone("kick.wav", 36)])?;
    session.set_zones(node, vec![zone("kick.wav", 36), zone("snare.wav", 38)])?;
    assert_eq!(zones_of(&session, node).len(), 2);
    session.undo();
    assert_eq!(zones_of(&session, node), [zone("kick.wav", 36)]);
    session.undo();
    assert!(zones_of(&session, node).is_empty());
    session.redo();
    assert_eq!(zones_of(&session, node), [zone("kick.wav", 36)]);
    Ok(())
}

#[test]
fn test_set_zones_refuses_bad_zones_and_other_kinds() -> TestResult {
    let mut session = Session::new();
    let node = sampler(&mut session)?;
    let backwards = SampleZone {
        low: 40,
        high: 30,
        ..zone("a.wav", 30)
    };
    assert!(matches!(
        session.set_zones(node, vec![backwards]),
        Err(Error::InvalidZone(_))
    ));
    let track = session.add_midi_track("Keys")?;
    let synth = session.set_instrument(track, "synth")?;
    assert!(matches!(
        session.set_zones(synth, vec![zone("a.wav", 30)]),
        Err(Error::InvalidZone(_))
    ));
    let bus = session.add_node(NodeKind::Bus(audio_buffers::Layout::Stereo))?;
    assert_eq!(
        session.set_zones(bus, Vec::new()),
        Err(Error::WrongNodeKind(bus))
    );
    let unknown = session.add_node(NodeKind::Instrument {
        kind: "from-the-future".to_owned(),
        parameters: BTreeMap::new(),
        zones: vec![zone("a.wav", 30)],
    });
    assert!(matches!(unknown, Err(Error::InvalidZone(_))));
    Ok(())
}

#[test]
fn test_referenced_files_include_zone_files_in_versions() -> TestResult {
    let mut session = Session::new();
    let node = sampler(&mut session)?;
    let hat = SampleZone {
        file: Source::Stereo {
            left: AudioFile {
                name: "hat-l.wav".to_owned(),
                frames: 10,
            },
            right: AudioFile {
                name: "hat-r.wav".to_owned(),
                frames: 10,
            },
        },
        ..zone("", 42)
    };
    session.set_zones(node, vec![zone("old.wav", 36)])?;
    session.save_version("Old kit")?;
    session.set_zones(node, vec![hat])?;
    let files: BTreeSet<&str> = session.referenced_files();
    assert_eq!(files, BTreeSet::from(["hat-l.wav", "hat-r.wav", "old.wav"]));
    Ok(())
}

#[test]
fn test_set_instrument_drops_the_zones() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("Drums")?;
    let node = session.set_instrument(track, SAMPLER_KIND)?;
    session.set_zones(node, vec![zone("kick.wav", 36)])?;
    let synth = session.set_instrument(track, "synth")?;
    assert!(session.state().node(node).is_none());
    assert!(zones_of(&session, synth).is_empty());
    assert!(session.referenced_files().is_empty());
    Ok(())
}
