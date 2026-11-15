//! MIDI tracks and clips: the same clip operations and undo as audio clips
//! (`docs/architecture/project.md` "Editing model", ADR-0144).

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::sync::Arc;

use audio_buffers::Layout;
use project_model::{
    AudioFile, Clip, ClipId, Endpoint, Error, NodeKind, Note, Range, Sequence, Session, Source,
    TrackId, TrackKind,
};

type TestResult = Result<(), Box<dyn StdError>>;

fn sequence() -> Result<Arc<Sequence>, Error> {
    let note = |start, pitch| Note {
        start,
        length: 100,
        channel: 0,
        pitch,
        velocity: 100,
    };
    let notes = vec![note(0, 60), note(200, 62), note(400, 64)];
    Ok(Arc::new(Sequence::new(1_000, notes, Vec::new())?))
}

fn midi_clip(position: u64) -> Result<Clip, Error> {
    Ok(Clip::new(Source::Midi(sequence()?), 0, 1_000, position))
}

fn clips(session: &Session, track: TrackId) -> Vec<(ClipId, Clip)> {
    session
        .state()
        .track(track)
        .map(|t| t.clips.clone())
        .unwrap_or_default()
}

fn shares_sequence(a: &Clip, b: &Clip) -> bool {
    match (&a.source, &b.source) {
        (Source::Midi(x), Source::Midi(y)) => Arc::ptr_eq(x, y),
        _ => false,
    }
}

#[test]
fn test_add_midi_track_has_midi_kind_and_no_routing() -> TestResult {
    let mut session = Session::new_project();
    let before = session.state().connections.len();
    let track = session.add_midi_track("keys")?;
    let state = session.state();
    assert_eq!(state.track(track).map(|t| t.kind), Some(TrackKind::Midi));
    assert_eq!(state.track(track).and_then(|t| t.layout()), None);
    assert_eq!(state.connections.len(), before);
    assert!(session.undo());
    assert!(session.state().track(track).is_none());
    Ok(())
}

#[test]
fn test_move_midi_clip_and_undo() -> TestResult {
    let mut session = Session::new();
    let a = session.add_midi_track("a")?;
    let b = session.add_midi_track("b")?;
    let id = session.add_clip(a, midi_clip(0)?)?;
    let before = session.state().clone();

    session.move_clip(id, b, 5_000)?;
    assert!(clips(&session, a).is_empty());
    assert_eq!(clips(&session, b)[0].1.position, 5_000);

    assert!(session.undo());
    assert_eq!(session.state(), &before);
    assert!(session.redo());
    assert_eq!(clips(&session, b)[0].1.position, 5_000);
    Ok(())
}

#[test]
fn test_trim_midi_clip_through_set_clip_and_undo() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("a")?;
    let whole = midi_clip(0)?;
    let id = session.add_clip(track, whole.clone())?;
    let trimmed = Clip {
        offset: 150,
        length: 300,
        position: 150,
        ..whole.clone()
    };
    session.set_clip(id, trimmed.clone())?;
    assert_eq!(session.state().clip(id), Some(&trimmed));
    assert!(session.undo());
    assert_eq!(session.state().clip(id), Some(&whole));
    assert!(session.redo());
    assert_eq!(session.state().clip(id), Some(&trimmed));

    let past_end = Clip {
        offset: 900,
        length: 200,
        ..whole
    };
    assert!(matches!(
        session.set_clip(id, past_end),
        Err(Error::ClipOutsideSource { .. })
    ));
    Ok(())
}

#[test]
fn test_cut_and_paste_midi_across_split_shares_sequence() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("a")?;
    let whole = midi_clip(1_000)?;
    session.add_clip(track, whole.clone())?;

    let excerpt = session.cut_range(&[track], Range::new(1_300, 1_500)?)?;
    let after = clips(&session, track);
    assert_eq!(after.len(), 2);
    assert_eq!((after[0].1.offset, after[0].1.length), (0, 300));
    assert_eq!((after[1].1.offset, after[1].1.length), (500, 500));
    assert!(shares_sequence(&after[0].1, &after[1].1));
    assert_eq!(excerpt.lanes()[0].kind(), TrackKind::Midi);

    let pasted = session.paste(&excerpt, track, 3_000)?;
    let clip = session.state().clip(pasted[0]).cloned();
    assert_eq!(
        clip.as_ref().map(|c| (c.offset, c.length, c.position)),
        Some((300, 200, 3_000))
    );
    assert!(clip.is_some_and(|c| shares_sequence(&c, &whole)));

    assert!(session.undo());
    assert!(session.undo());
    assert_eq!(clips(&session, track).len(), 1);
    assert_eq!(clips(&session, track)[0].1, whole);
    Ok(())
}

#[test]
fn test_copy_midi_and_delete_range_undo() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("a")?;
    session.add_clip(track, midi_clip(0)?)?;
    let before = session.state().clone();
    let excerpt = session.copy_range(&[track], Range::new(100, 200)?)?;
    assert_eq!(excerpt.lanes()[0].clips().len(), 1);
    session.delete_range(&[track], Range::new(100, 200)?)?;
    assert_eq!(clips(&session, track).len(), 2);
    assert!(session.undo());
    assert_eq!(session.state(), &before);
    Ok(())
}

#[test]
fn test_paste_midi_onto_audio_track_refused() -> TestResult {
    let mut session = Session::new();
    let midi = session.add_midi_track("m")?;
    let audio = session.add_track("a", Layout::Mono)?;
    session.add_clip(midi, midi_clip(0)?)?;
    let excerpt = session.copy_range(&[midi], Range::new(0, 500)?)?;
    assert_eq!(session.paste(&excerpt, audio, 0), Err(Error::KindMismatch));

    let audio_clip = Clip::new(
        Source::Mono(AudioFile {
            name: "a.wav".into(),
            frames: 1_000,
        }),
        0,
        1_000,
        0,
    );
    assert_eq!(
        session.add_clip(midi, audio_clip).map(|_| ()),
        Err(Error::KindMismatch)
    );
    assert_eq!(
        session.add_clip(audio, midi_clip(0)?).map(|_| ()),
        Err(Error::KindMismatch)
    );
    Ok(())
}

#[test]
fn test_connect_midi_track_to_audio_input_refused() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("m")?;
    let mixer = session.add_node(NodeKind::Mixer {
        channels: 1,
        parameters: BTreeMap::new(),
    })?;
    let bus = session.add_node(NodeKind::Bus(Layout::Stereo))?;
    for (to, input) in [
        (Endpoint::Output, 0),
        (Endpoint::Node(mixer), 0),
        (Endpoint::Node(bus), 0),
    ] {
        assert_eq!(
            session.connect(Endpoint::Track(track), 0, to, input),
            Err(Error::WrongPortType)
        );
    }
    Ok(())
}

#[test]
fn test_midi_clip_gain_or_fade_refused() -> TestResult {
    let mut session = Session::new();
    let track = session.add_midi_track("a")?;
    let whole = midi_clip(0)?;
    let id = session.add_clip(track, whole.clone())?;
    for clip in [
        Clip {
            gain_db: -3.0,
            ..whole.clone()
        },
        Clip {
            fade_in: 10,
            ..whole.clone()
        },
        Clip {
            fade_out: 10,
            ..whole.clone()
        },
    ] {
        assert_eq!(session.set_clip(id, clip), Err(Error::MidiClipGainOrFade));
    }
    assert_eq!(session.state().clip(id), Some(&whole));
    Ok(())
}
