//! MIDI tracks in the saved format: shared sequences reload shared, and untrusted MIDI data is
//! refused (`docs/architecture/project.md` "Saved format", "Loading", ADR-0144).

use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use project_model::Source;
use project_storage::{Bundle, Error as StorageError};

fn scratch(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!(
        "project-storage-midi-{}-{name}",
        std::process::id()
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir.join("Song.debussy"))
}

const NOTES: &str = "[0,10,0,60,100],[20,10,0,62,100]";
const CONTROLS: &str = "[0,0,7,100]";

/// A version 7 project with these tracks and sequences.
fn project(tracks: &str, sequences: &str) -> String {
    format!(
        r#"{{"format_version":7,"sample_rate":48000,"tracks":[{tracks}],"nodes":[],"connections":[],"tempo":{{"bpm":120.0,"beats_per_bar":4,"beat_unit":4}},"curves":[],"sequences":[{sequences}]}}"#
    )
}

fn sequence(frames: u64, notes: &str, controls: &str) -> String {
    format!(r#"{{"frames":{frames},"notes":[{notes}],"controls":[{controls}]}}"#)
}

/// A clip with `id` playing sequence `index` from 0 for `length` frames.
fn midi_clip(id: u64, index: u32, length: u64) -> String {
    format!(
        r#"{{"id":{id},"source":{{"kind":"midi","sequence":{index}}},"offset":0,"length":{length},"position":0,"gain_db":0.0,"fade_in":0,"fade_out":0}}"#
    )
}

fn track(id: u64, kind: &str, clips: &str) -> String {
    format!(r#"{{"id":{id},"name":"T","kind":{kind},"clips":[{clips}]}}"#)
}

const MIDI: &str = r#"{"kind":"midi"}"#;
const MONO: &str = r#"{"kind":"audio","layout":"mono"}"#;

/// The valid fixture: one MIDI track with two clips sharing sequence 0.
fn valid() -> String {
    project(
        &track(
            1,
            MIDI,
            &format!("{},{}", midi_clip(2, 0, 50), midi_clip(3, 0, 100)),
        ),
        &sequence(100, NOTES, CONTROLS),
    )
}

fn open_with(path: &Path, text: &str) -> Result<StorageError, Box<dyn Error>> {
    fs::write(path.join("project.json"), text)?;
    match Bundle::open(path) {
        Ok(_) => Err(format!("a bad project.json was accepted: {text}").into()),
        Err(error) => Ok(error),
    }
}

#[test]
fn test_open_valid_midi_project_shares_sequence() -> Result<(), Box<dyn Error>> {
    let path = scratch("valid")?;
    Bundle::create(&path, 48_000)?;
    fs::write(path.join("project.json"), valid())?;
    let (mut bundle, session) = Bundle::open(&path)?;
    let clips = &session.state().tracks[0].clips;
    let (Source::Midi(a), Source::Midi(b)) = (&clips[0].1.source, &clips[1].1.source) else {
        return Err("clips are not MIDI".into());
    };
    assert!(Arc::ptr_eq(a, b));
    assert_eq!(a.notes().len(), 2);
    assert_eq!(a.controls().len(), 1);

    // Saved again, the sequence is written once.
    bundle.save(&session)?;
    let text = fs::read_to_string(path.join("project.json"))?;
    assert_eq!(text.matches("\"frames\": 100").count(), 1, "{text}");
    Ok(())
}

#[test]
fn test_open_refuses_untrusted_midi() -> Result<(), Box<dyn Error>> {
    let path = scratch("untrusted")?;
    Bundle::create(&path, 48_000)?;
    let one = |clip: &str, seq: &str| project(&track(1, MIDI, clip), seq);
    let good_clip = midi_clip(2, 0, 100);
    let bad_notes = |notes: &str| one(&good_clip, &sequence(100, notes, ""));
    let invalid = [
        // An index out of range, and a sequence no clip refers to.
        one(&midi_clip(2, 1, 100), &sequence(100, NOTES, "")),
        one(
            &good_clip,
            &format!("{},{}", sequence(100, NOTES, ""), sequence(10, "", "")),
        ),
        project(&track(1, MIDI, ""), &sequence(100, NOTES, "")),
        // Bad channel, pitch and velocity, and values past a byte.
        bad_notes("[0,10,16,60,100]"),
        bad_notes("[0,10,0,128,100]"),
        bad_notes("[0,10,0,60,0]"),
        bad_notes("[0,10,0,60,128]"),
        bad_notes("[0,10,0,60,300]"),
        one(&good_clip, &sequence(100, "", "[0,0,128,0]")),
        one(&good_clip, &sequence(100, "", "[0,0,7,256]")),
        // Unsorted, overlapping, empty, and past the end.
        bad_notes("[20,10,0,62,100],[0,10,0,60,100]"),
        bad_notes("[0,10,0,60,100],[5,10,0,60,100]"),
        bad_notes("[0,0,0,60,100]"),
        bad_notes("[95,10,0,60,100]"),
        one(&good_clip, &sequence(100, "", "[5,0,7,0],[1,0,7,0]")),
        one(&good_clip, &sequence(100, "", "[100,0,7,0]")),
        // A clip past its sequence, with fades, or with gain.
        one(&midi_clip(2, 0, 101), &sequence(100, NOTES, "")),
        one(
            &good_clip.replace("\"fade_in\":0", "\"fade_in\":5"),
            &sequence(100, NOTES, ""),
        ),
        one(
            &good_clip.replace("\"gain_db\":0.0", "\"gain_db\":-6.0"),
            &sequence(100, NOTES, ""),
        ),
        // A MIDI source on an audio track.
        project(&track(1, MONO, &good_clip), &sequence(100, NOTES, "")),
    ];
    for text in &invalid {
        let error = open_with(&path, text)?;
        assert!(
            matches!(error, StorageError::Invalid { .. }),
            "{text}: {error}"
        );
    }
    let malformed = [
        // A note of the wrong arity, a track kind with a stray layout, a missing table.
        bad_notes("[0,10,0,60]"),
        project(
            &track(1, r#"{"kind":"midi","layout":"mono"}"#, &good_clip),
            &sequence(100, NOTES, ""),
        ),
        valid().replace(
            &format!(",\"sequences\":[{}]", sequence(100, NOTES, CONTROLS)),
            "",
        ),
    ];
    for text in &malformed {
        let error = open_with(&path, text)?;
        assert!(
            matches!(error, StorageError::Json { .. }),
            "{text}: {error}"
        );
    }
    // The fixture itself is valid.
    fs::write(path.join("project.json"), valid())?;
    assert!(Bundle::open(&path).is_ok());
    Ok(())
}

/// A version 9 project with one MIDI track of `kind` and no clips.
fn notation_project(kind: &str) -> String {
    project(&track(1, kind, ""), "").replace("\"format_version\":7", "\"format_version\":9")
}

#[test]
fn test_open_reads_key_and_names_and_refuses_bad_ones() -> Result<(), Box<dyn Error>> {
    use project_model::{Key, Notation};
    let path = scratch("notation")?;
    Bundle::create(&path, 48_000)?;
    let good = notation_project(
        r#"{"kind":"midi","key":{"mode":"indefinite"},"names":[{"pitch":36,"name":"kick"}]}"#,
    );
    fs::write(path.join("project.json"), &good)?;
    let (_, session) = Bundle::open(&path)?;
    let notation = session.state().tracks[0].notation.clone();
    let expected = Notation {
        key: Key::Indefinite,
        names: [(36, "kick".to_owned())].into(),
    };
    assert_eq!(notation, Some(expected));
    for bad in [
        r#"{"kind":"midi","key":{"mode":"major","root":12},"names":[]}"#,
        r#"{"kind":"midi","key":{"mode":"indefinite"},"names":[{"pitch":200,"name":"x"}]}"#,
        r#"{"kind":"midi","key":{"mode":"indefinite"},"names":[{"pitch":36,"name":""}]}"#,
    ] {
        let error = open_with(&path, &notation_project(bad))?;
        assert!(
            matches!(error, StorageError::Invalid { .. }),
            "{bad}: {error}"
        );
    }
    let error = open_with(
        &path,
        &notation_project(r#"{"kind":"midi","key":{"mode":"lydian","root":0},"names":[]}"#),
    )?;
    assert!(matches!(error, StorageError::Json { .. }), "{error}");
    Ok(())
}
