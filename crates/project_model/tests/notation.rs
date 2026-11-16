//! A MIDI track's key and pitch names (`docs/architecture/track-pane.md` "Key and pitch names").

use std::error::Error as StdError;

use audio_buffers::Layout;
use project_model::{Accidental, Error, Key, Letter, Mode, Notation, Session};

type TestResult = Result<(), Box<dyn StdError>>;

#[test]
fn test_set_notation_new_midi_track_starts_in_c_major() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("m")?;
    let notation = session
        .state()
        .track(track)
        .and_then(|t| t.notation.clone());
    assert_eq!(notation, Some(Notation::default()));
    let audio = session.add_track("a", Layout::Mono)?;
    assert_eq!(
        session.state().track(audio).map(|t| &t.notation),
        Some(&None)
    );
    Ok(())
}

#[test]
fn test_set_notation_is_one_undo_step() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("m")?;
    let a_minor = Notation {
        key: Key::Tonal {
            letter: Letter::A,
            accidental: Accidental::Natural,
            mode: Mode::Minor,
        },
        names: Default::default(),
    };
    session.set_notation(track, a_minor.clone())?;
    session.set_notation(track, Notation::gm_drums())?;
    let current = |s: &Session| s.state().track(track).and_then(|t| t.notation.clone());
    assert_eq!(current(&session), Some(Notation::gm_drums()));
    assert!(session.undo());
    assert_eq!(current(&session), Some(a_minor));
    assert!(session.undo());
    assert_eq!(current(&session), Some(Notation::default()));
    assert!(session.redo());
    assert!(session.redo());
    assert_eq!(current(&session), Some(Notation::gm_drums()));
    Ok(())
}

#[test]
fn test_set_notation_refuses_audio_tracks_and_bad_names() -> TestResult {
    let mut session = Session::new();
    let audio = session.add_track("a", Layout::Stereo)?;
    assert_eq!(
        session.set_notation(audio, Notation::default()),
        Err(Error::KindMismatch)
    );
    let track = session.add_midi_track("m")?;
    let mut blank = Notation::gm_drums();
    blank.names.insert(36, " ".to_owned());
    assert!(matches!(
        session.set_notation(track, blank),
        Err(Error::InvalidNotation(_))
    ));
    let current = session
        .state()
        .track(track)
        .and_then(|t| t.notation.clone());
    assert_eq!(current, Some(Notation::default()));
    Ok(())
}
