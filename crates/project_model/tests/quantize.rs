//! Quantizing MIDI notes in a range (`docs/architecture/project.md` "Editing model").

use std::error::Error as StdError;
use std::sync::Arc;

use project_model::{Clip, Error, Grid, Note, Range, Sequence, Session, Source, TrackId};

type TestResult = Result<(), Box<dyn StdError>>;

/// 48 kHz: at the default 120 BPM in 4/4, a sixteenth note is 6 000 frames.
const RATE: u32 = 48_000;

fn note(start: u64, pitch: u8) -> Note {
    Note {
        start,
        length: 500,
        channel: 0,
        pitch,
        velocity: 100,
    }
}

/// A track holding one clip, at `position`, of notes at 1 000, 7 000 and 13 000.
fn session_with_clip(position: u64) -> Result<(Session, TrackId), Box<dyn StdError>> {
    let mut session = Session::new();
    let track = session.add_midi_track("keys")?;
    let notes = vec![note(1_000, 60), note(7_000, 62), note(13_000, 64)];
    let sequence = Arc::new(Sequence::new(24_000, notes, Vec::new())?);
    session.add_clip(
        track,
        Clip::new(Source::Midi(sequence), 0, 24_000, position),
    )?;
    Ok((session, track))
}

/// The note starts of each clip on `track`, bottom layer first.
fn starts(session: &Session, track: TrackId) -> Vec<Vec<u64>> {
    session
        .state()
        .track(track)
        .map(|t| {
            t.clips
                .iter()
                .map(|(_, clip)| match &clip.source {
                    Source::Midi(s) => s.notes().iter().map(|n| n.start).collect(),
                    _ => Vec::new(),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn test_quantize_moves_notes_as_one_step_and_undoes() -> TestResult {
    let (mut session, track) = session_with_clip(0)?;
    let revision = session.revision();
    let range = Range::new(0, 24_000)?;
    assert_eq!(
        session.quantize(&[track], range, Grid::Sixteenth, 1.0, RATE)?,
        3
    );
    assert_eq!(starts(&session, track), [vec![0, 6_000, 12_000]]);
    assert_eq!(session.revision(), revision + 1);
    assert!(session.undo());
    assert_eq!(starts(&session, track), [vec![1_000, 7_000, 13_000]]);
    Ok(())
}

#[test]
fn test_quantize_half_strength_moves_half_way() -> TestResult {
    let (mut session, track) = session_with_clip(0)?;
    session.quantize(&[track], Range::new(0, 24_000)?, Grid::Sixteenth, 0.5, RATE)?;
    assert_eq!(starts(&session, track), [vec![500, 6_500, 12_500]]);
    Ok(())
}

#[test]
fn test_quantize_only_touches_notes_starting_in_the_range() -> TestResult {
    let (mut session, track) = session_with_clip(0)?;
    let found = session.quantize(&[track], Range::new(5_000, 8_000)?, Grid::Beat, 1.0, RATE)?;
    assert_eq!(found, 1);
    // 7 000 is nearer beat 0 than beat 1 (24 000).
    assert_eq!(starts(&session, track), [vec![0, 1_000, 13_000]]);
    Ok(())
}

#[test]
fn test_quantize_leaves_a_split_sibling_alone() -> TestResult {
    let (mut session, track) = session_with_clip(0)?;
    session.delete_range(&[track], Range::new(10_000, 11_000)?)?;
    let sibling = |s: &Session| s.state().track(track).map(|t| t.clips[1].1.clone());
    let before = sibling(&session);
    session.quantize(&[track], Range::new(0, 10_000)?, Grid::Sixteenth, 1.0, RATE)?;
    let after = sibling(&session);
    assert_eq!(after, before);
    if let (
        Some(Clip {
            source: Source::Midi(a),
            ..
        }),
        Some(Clip {
            source: Source::Midi(b),
            ..
        }),
    ) = (&before, &after)
    {
        assert!(Arc::ptr_eq(a, b));
    }
    let clips = starts(&session, track);
    // The note at 13 000 is past the first piece, so it stays.
    assert_eq!(clips[0], [0, 6_000, 13_000]);
    assert_eq!(clips[1], [1_000, 7_000, 13_000]);
    // Both pieces kept their positions and layers.
    let positions: Vec<u64> = session
        .state()
        .track(track)
        .map(|t| t.clips.iter().map(|(_, c)| c.position).collect())
        .unwrap_or_default();
    assert_eq!(positions, [0, 11_000]);
    Ok(())
}

#[test]
fn test_quantize_with_nothing_to_move_adds_no_step() -> TestResult {
    let (mut session, track) = session_with_clip(0)?;
    let range = Range::new(0, 24_000)?;
    session.quantize(&[track], range, Grid::Sixteenth, 1.0, RATE)?;
    let revision = session.revision();
    assert_eq!(
        session.quantize(&[track], range, Grid::Sixteenth, 1.0, RATE)?,
        3
    );
    assert_eq!(
        session.quantize(&[track], range, Grid::Sixteenth, 0.0, RATE)?,
        3
    );
    assert_eq!(
        session.quantize(&[track], Range::new(20_000, 30_000)?, Grid::Bar, 1.0, RATE)?,
        0
    );
    assert_eq!(session.revision(), revision);
    Ok(())
}

#[test]
fn test_quantize_keeps_notes_inside_their_clip() -> TestResult {
    // The clip starts at 1 500: every note would go to bar line 0, before it.
    let (mut session, track) = session_with_clip(1_500)?;
    session.quantize(&[track], Range::new(0, 30_000)?, Grid::Bar, 1.0, RATE)?;
    assert_eq!(starts(&session, track), [vec![0, 0, 0]]);
    Ok(())
}

#[test]
fn test_quantize_refuses_bad_strength_and_unknown_track() -> TestResult {
    let (mut session, track) = session_with_clip(0)?;
    let range = Range::new(0, 24_000)?;
    for strength in [-0.1, 1.1, f32::NAN] {
        assert_eq!(
            session.quantize(&[track], range, Grid::Beat, strength, RATE),
            Err(Error::InvalidValue)
        );
    }
    let missing = TrackId(999);
    assert_eq!(
        session.quantize(&[missing], range, Grid::Beat, 1.0, RATE),
        Err(Error::UnknownTrack(missing))
    );
    Ok(())
}
