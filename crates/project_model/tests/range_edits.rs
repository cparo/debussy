//! Range edits: cut, copy, delete and paste across tracks (`docs/architecture/project.md`
//! "Range edits").

use std::error::Error as StdError;

use audio_buffers::Layout;
use project_model::{AudioFile, Clip, ClipId, Error, Range, Session, Source, TrackId};

type TestResult = Result<(), Box<dyn StdError>>;

fn mono(name: &str) -> Source {
    Source::Mono(AudioFile {
        name: name.into(),
        frames: 10_000,
    })
}

fn stereo() -> Source {
    let file = |name: &str| AudioFile {
        name: name.into(),
        frames: 10_000,
    };
    Source::Stereo {
        left: file("s-L.wav"),
        right: file("s-R.wav"),
    }
}

/// The ids and clips of a track, bottom layer first.
fn clips(session: &Session, track: TrackId) -> Vec<(ClipId, Clip)> {
    session
        .state()
        .track(track)
        .map(|t| t.clips.clone())
        .unwrap_or_default()
}

#[test]
fn test_cut_range_splits_a_clip_in_two() -> TestResult {
    let mut session = Session::new();
    let track = session.add_track("a", Layout::Mono)?;
    let whole = Clip {
        fade_in: 100,
        fade_out: 100,
        ..Clip::new(mono("a.wav"), 0, 1_000, 1_000)
    };
    let below = session.add_clip(track, whole.clone())?;
    let above = session.add_clip(track, Clip::new(mono("b.wav"), 0, 100, 8_000))?;

    let excerpt = session.cut_range(&[track], Range::new(1_200, 1_500)?)?;

    let after = clips(&session, track);
    assert_eq!(after.len(), 3);
    let left = Clip {
        length: 200,
        fade_out: 0,
        ..whole.clone()
    };
    assert_eq!(after[0], (below, left));
    let right = Clip {
        offset: 500,
        length: 500,
        position: 1_500,
        fade_in: 0,
        ..whole.clone()
    };
    assert_ne!(after[1].0, below);
    assert_ne!(after[1].0, above);
    assert_eq!(after[1].1, right);
    assert_eq!(after[2].0, above);

    assert_eq!(excerpt.length(), 300);
    let lane = &excerpt.lanes()[0];
    assert_eq!(lane.layout(), Layout::Mono);
    let middle = Clip {
        offset: 200,
        length: 300,
        position: 0,
        fade_in: 0,
        fade_out: 0,
        ..whole
    };
    assert_eq!(lane.clips(), [middle]);
    Ok(())
}

#[test]
fn test_delete_range_across_tracks_keeps_layer_order() -> TestResult {
    let mut session = Session::new();
    let a = session.add_track("a", Layout::Mono)?;
    let b = session.add_track("b", Layout::Stereo)?;
    let x = session.add_clip(a, Clip::new(mono("x.wav"), 0, 1_000, 0))?;
    let y = session.add_clip(a, Clip::new(mono("y.wav"), 0, 1_000, 500))?;
    session.add_clip(b, Clip::new(stereo(), 0, 100, 450))?;
    let kept = session.add_clip(b, Clip::new(stereo(), 0, 100, 2_000))?;

    // Duplicate ids are ignored.
    session.delete_range(&[b, a, b], Range::new(400, 600)?)?;

    let on_a = clips(&session, a);
    let layers: Vec<(Option<ClipId>, u64, u64)> = on_a
        .iter()
        .map(|(id, c)| ([x, y].contains(id).then_some(*id), c.position, c.end()))
        .collect();
    assert_eq!(
        layers,
        [(Some(x), 0, 400), (None, 600, 1_000), (Some(y), 600, 1_500)]
    );
    let on_b: Vec<ClipId> = clips(&session, b).iter().map(|(id, _)| *id).collect();
    assert_eq!(on_b, [kept]);
    Ok(())
}

#[test]
fn test_cut_then_paste_puts_back_the_same_parts() -> TestResult {
    let mut session = Session::new();
    let a = session.add_track("a", Layout::Mono)?;
    let b = session.add_track("b", Layout::Stereo)?;
    session.add_clip(a, Clip::new(mono("x.wav"), 0, 1_000, 0))?;
    session.add_clip(a, Clip::new(mono("y.wav"), 50, 1_000, 500))?;
    session.add_clip(b, Clip::new(stereo(), 0, 300, 300))?;
    let range = Range::new(400, 700)?;
    let before: Vec<Vec<Clip>> = [a, b]
        .iter()
        .map(|&t| {
            clips(&session, t)
                .into_iter()
                .filter_map(|(_, c)| {
                    let start = c.position.max(range.start());
                    let end = c.end().min(range.end());
                    (start < end).then(|| Clip {
                        offset: c.offset + (start - c.position),
                        length: end - start,
                        position: start,
                        ..c
                    })
                })
                .collect()
        })
        .collect();

    let excerpt = session.cut_range(&[a, b], range)?;
    let counts: Vec<usize> = [a, b].iter().map(|&t| clips(&session, t).len()).collect();
    let pasted = session.paste(&excerpt, a, range.start())?;

    assert_eq!(pasted.len(), before.iter().map(Vec::len).sum::<usize>());
    for ((track, want), count) in [a, b].iter().zip(&before).zip(counts) {
        let on_top: Vec<(ClipId, Clip)> = clips(&session, *track).split_off(count);
        let got: Vec<Clip> = on_top.iter().map(|(_, c)| c.clone()).collect();
        assert_eq!(&got, want);
        assert!(on_top.iter().all(|(id, _)| pasted.contains(id)));
    }
    Ok(())
}

#[test]
fn test_paste_refuses_mismatched_layout_and_missing_tracks() -> TestResult {
    let mut session = Session::new();
    let a = session.add_track("a", Layout::Mono)?;
    let b = session.add_track("b", Layout::Stereo)?;
    session.add_clip(a, Clip::new(mono("x.wav"), 0, 1_000, 0))?;
    session.add_clip(b, Clip::new(stereo(), 0, 1_000, 0))?;
    let excerpt = session.copy_range(&[a, b], Range::new(100, 200)?)?;
    let state = session.state().clone();
    let revision = session.revision();

    assert_eq!(
        session.paste(&excerpt, b, 0),
        Err(Error::NotEnoughTracks {
            needed: 2,
            available: 1
        })
    );
    let mono_only = session.copy_range(&[a], Range::new(100, 200)?)?;
    assert_eq!(session.paste(&mono_only, b, 0), Err(Error::LayoutMismatch));
    assert_eq!(
        session.paste(&mono_only, a, u64::MAX),
        Err(Error::ClipOutsideTimeline)
    );
    assert_eq!(
        session.paste(&mono_only, TrackId(999), 0),
        Err(Error::UnknownTrack(TrackId(999)))
    );

    assert_eq!(session.state(), &state);
    assert_eq!(session.revision(), revision);
    Ok(())
}

#[test]
fn test_range_new_refuses_empty() {
    assert_eq!(Range::new(5, 5), Err(Error::EmptyRange));
    assert_eq!(Range::new(6, 5), Err(Error::EmptyRange));
}

#[test]
fn test_undo_restores_cut_and_paste_exactly() -> TestResult {
    let mut session = Session::new();
    let a = session.add_track("a", Layout::Mono)?;
    let b = session.add_track("b", Layout::Mono)?;
    let fades = Clip {
        fade_in: 30,
        fade_out: 40,
        ..Clip::new(mono("x.wav"), 10, 1_000, 100)
    };
    session.add_clip(a, fades.clone())?;
    session.add_clip(b, fades)?;
    let original = session.state().clone();

    let excerpt = session.cut_range(&[a, b], Range::new(300, 600)?)?;
    let cut = session.state().clone();
    assert!(session.undo());
    assert_eq!(session.state(), &original);
    assert!(session.redo());
    assert_eq!(session.state(), &cut);

    session.paste(&excerpt, a, 5_000)?;
    assert!(session.undo());
    assert_eq!(session.state(), &cut);
    Ok(())
}

#[test]
fn test_no_op_range_edits_add_no_undo_step() -> TestResult {
    let mut session = Session::new();
    let a = session.add_track("a", Layout::Mono)?;
    session.add_clip(a, Clip::new(mono("x.wav"), 0, 100, 0))?;
    let revision = session.revision();

    session.delete_range(&[a], Range::new(500, 600)?)?;
    let empty = session.cut_range(&[a], Range::new(500, 600)?)?;
    assert_eq!(empty.lanes()[0].clips(), []);
    assert_eq!(session.paste(&empty, a, 0)?, []);
    assert_eq!(
        session.delete_range(&[TrackId(999)], Range::new(0, 1)?),
        Err(Error::UnknownTrack(TrackId(999)))
    );

    assert_eq!(session.revision(), revision);
    Ok(())
}
