//! Editing behaviour: layering, validation, versions.

use std::error::Error as StdError;

use audio_buffers::Layout;
use project_model::{AudioFile, Clip, Edit, Error, Session, Source};

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
