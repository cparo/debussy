//! Recovering takes a crash cut off: the recording marker, header repair on open, and the
//! markers that are discarded.

use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use audio_buffers::Layout;
use midi::{journal_header, journal_record};
use project_model::{AudioFile, Clip, Session, Source, TrackId};
use project_storage::{
    Bundle, MAX_RECORDING_BYTES, Recording, RecordingStart, RecoveredMidi, RecoveredPass, Recovery,
};
use wav_files::{Channels, SampleFormat, Spec, WavReader, WavWriter};

const RATE: u32 = 48_000;

/// A bundle's path, the bundle, its session, and its mono and stereo tracks.
type Fixture = (PathBuf, Bundle, Session, TrackId, TrackId);

/// A fresh bundle with a mono and a stereo track.
fn bundle(name: &str) -> Result<Fixture, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("project-recovery-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    let path = dir.join("Song.debussy");
    let mut bundle = Bundle::create(&path, RATE)?;
    let mut session = Session::new();
    let mono = session.add_track("Vox", Layout::Mono)?;
    let stereo = session.add_track("Keys", Layout::Stereo)?;
    bundle.save(&session)?;
    Ok((path, bundle, session, mono, stereo))
}

/// A take file as a crash leaves it: `frames` frames written and the sizes still zero.
fn crashed_take(path: &Path, frames: usize) -> Result<(), Box<dyn Error>> {
    let spec = Spec {
        channels: Channels::Mono,
        sample_rate: RATE,
        format: SampleFormat::Float32,
    };
    let mut writer = WavWriter::new(File::create_new(path)?, spec)?;
    writer.write(&[&vec![0.5; frames]])?;
    drop(writer);
    Ok(())
}

fn recording(mono: TrackId, stereo: TrackId, start: Option<RecordingStart>) -> Recording {
    Recording {
        device_latency: 256,
        midi_latency: 0,
        start,
        tracks: vec![
            (mono, vec!["Vox.wav".into()]),
            (stereo, vec!["Keys L.wav".into(), "Keys R.wav".into()]),
        ],
    }
}

const START: RecordingStart = RecordingStart {
    start_frame: 96_000,
    plan_latency: 64,
};

/// Writes the three crashed takes: the right channel cut short.
fn crashed_takes(bundle: &Bundle) -> Result<(), Box<dyn Error>> {
    let audio = bundle.audio_dir();
    crashed_take(&audio.join("Vox.wav"), 5000)?;
    crashed_take(&audio.join("Keys L.wav"), 5000)?;
    crashed_take(&audio.join("Keys R.wav"), 4990)?;
    Ok(())
}

fn exists(bundle: &Path, file: &str) -> bool {
    bundle.join("audio").join(file).exists()
}

#[test]
fn test_open_recovers_takes_from_the_marker() -> Result<(), Box<dyn Error>> {
    let (path, bundle, _, mono, stereo) = bundle("recovered")?;
    crashed_takes(&bundle)?;
    bundle.mark_recording(&recording(mono, stereo, Some(START)))?;
    drop(bundle);

    let (mut bundle, _) = Bundle::open(&path)?;
    let recovery = bundle.take_recovery();
    assert_eq!(
        recovery,
        Some(Recovery::Takes(RecoveredPass {
            tracks: recording(mono, stereo, None).tracks,
            frames: 4990,
            midi: Vec::new(),
            start: START,
            device_latency: 256,
            midi_latency: 0,
        }))
    );
    assert_eq!(bundle.take_recovery(), None);
    for file in ["Vox.wav", "Keys L.wav", "Keys R.wav"] {
        assert!(exists(&path, file));
    }
    let reader = WavReader::new(File::open(path.join("audio/Vox.wav"))?)?;
    assert_eq!((reader.frames(), reader.truncated()), (5000, false));
    // The marker stays until the takes are placed.
    assert!(path.join("recording.json").exists());
    bundle.clear_recording()?;
    assert!(!path.join("recording.json").exists());
    bundle.clear_recording()?;
    Ok(())
}

#[test]
fn test_open_skips_tracks_that_are_gone_or_files_that_are_missing() -> Result<(), Box<dyn Error>> {
    let (path, bundle, _, mono, stereo) = bundle("partial")?;
    crashed_take(&bundle.audio_dir().join("Vox.wav"), 100)?;
    // The right channel's file is missing.
    crashed_take(&bundle.audio_dir().join("Keys L.wav"), 100)?;
    let mut marked = recording(mono, stereo, Some(START));
    marked.tracks.push((TrackId(999), vec!["Gone.wav".into()]));
    bundle.mark_recording(&marked)?;
    crashed_take(&bundle.audio_dir().join("Gone.wav"), 100)?;
    drop(bundle);

    let (mut bundle, _) = Bundle::open(&path)?;
    let Some(Recovery::Takes(pass)) = bundle.take_recovery() else {
        return Err("nothing was recovered".into());
    };
    assert_eq!(pass.tracks, [(mono, vec!["Vox.wav".to_owned()])]);
    assert!(!exists(&path, "Keys L.wav"));
    assert!(!exists(&path, "Gone.wav"));
    Ok(())
}

#[test]
fn test_open_discards_a_bad_marker_and_cleans_up() -> Result<(), Box<dyn Error>> {
    let (path, bundle, _, mono, stereo) = bundle("bad")?;
    drop(bundle);
    let saved = |r: &str| {
        format!(r#"{{"format_version":1,"device_latency":0,"start":null,"tracks":[{r}]}}"#)
    };
    let id = mono.0;
    let other = stereo.0;
    let started = |tracks: &str| {
        format!(
            r#"{{"format_version":1,"device_latency":0,"start":{{"start_frame":0,"plan_latency":0}},"tracks":[{tracks}]}}"#
        )
    };
    let markers = [
        "not json".to_owned(),
        " ".repeat(MAX_RECORDING_BYTES as usize + 1),
        // The crash came during the count-in.
        saved(&format!(r#"{{"track":{id},"files":["Vox.wav"]}}"#)),
        started(&format!(r#"{{"track":{id},"files":["../Vox.wav"]}}"#)),
        started(&format!(r#"{{"track":{id},"files":["Vox.txt"]}}"#)),
        started(&format!(r#"{{"track":{id},"files":[]}}"#)),
        started(&format!(
            r#"{{"track":{id},"files":["Vox.wav"]}},{{"track":{other},"files":["Vox.wav","Keys R.wav"]}}"#
        )),
        started(&format!(
            r#"{{"track":{id},"files":["Vox.wav"],"extra":1}}"#
        )),
        started(&format!(r#"{{"track":{id},"files":["Vox.wav"]}},"#)),
        started(&format!(r#"{{"track":{id},"files":["Vox.wav"]}}"#))
            .replace("\"format_version\":1", "\"format_version\":3"),
        // Only a version 2 marker names a journal.
        started(&format!(r#"{{"track":{id},"files":["Vox.wav"]}},{{"track":{other},"files":["k.midilog"]}}"#)),
        started(&format!(r#"{{"track":{other},"files":["k.midilog","l.midilog"]}},{{"track":{id},"files":["Vox.wav"]}}"#))
            .replace("\"format_version\":1", "\"format_version\":2"),
        started(&format!(r#"{{"track":{id},"files":["Vox.wav"]}}"#))
            .replace("\"device_latency\":0", "\"device_latency\":-1"),
    ];
    for marker in markers {
        let (bundle, _) = Bundle::open(&path)?;
        crashed_take(&bundle.audio_dir().join("Vox.wav"), 100)?;
        drop(bundle);
        fs::write(path.join("recording.json"), &marker)?;
        let (mut bundle, _) = Bundle::open(&path)?;
        assert!(
            matches!(bundle.take_recovery(), Some(Recovery::Discarded { .. })),
            "{marker:.80}"
        );
        assert!(!exists(&path, "Vox.wav"), "{marker:.80}");
        assert!(!path.join("recording.json").exists());
    }
    Ok(())
}

#[test]
fn test_open_skips_takes_already_in_the_project() -> Result<(), Box<dyn Error>> {
    let (path, mut bundle, mut session, mono, stereo) = bundle("placed")?;
    crashed_takes(&bundle)?;
    let file = AudioFile {
        name: "Vox.wav".into(),
        frames: 5000,
    };
    session.add_clip(mono, Clip::new(Source::Mono(file), 0, 5000, 0))?;
    bundle.save(&session)?;
    bundle.mark_recording(&Recording {
        tracks: vec![(mono, vec!["Vox.wav".into()])],
        ..recording(mono, stereo, Some(START))
    })?;
    drop(bundle);

    let (mut bundle, _) = Bundle::open(&path)?;
    assert_eq!(bundle.take_recovery(), None);
    assert!(!path.join("recording.json").exists());
    assert!(exists(&path, "Vox.wav"));
    assert!(!exists(&path, "Keys L.wav"));
    Ok(())
}

/// A journal as a crash leaves it: its header, then `records`, then `tail` bytes of a record
/// being written.
fn crashed_journal(
    path: &Path,
    rate: u32,
    records: &[(u64, [u8; 3])],
    tail: usize,
) -> Result<(), Box<dyn Error>> {
    let mut bytes = journal_header(rate).to_vec();
    for &(frame, message) in records {
        bytes.extend(journal_record(frame, message));
    }
    bytes.extend(&journal_record(99_999, [0x90, 1, 1])[..tail]);
    fs::write(path, bytes)?;
    Ok(())
}

/// A bundle with a mono track and a MIDI track, and a version 2 marker naming a take of each:
/// the MIDI one in `k.midilog`.
fn midi_pass(name: &str) -> Result<(PathBuf, Bundle, TrackId, TrackId), Box<dyn Error>> {
    let (path, mut bundle, mut session, mono, _) = bundle(name)?;
    let keys = session.add_midi_track("Synth")?;
    bundle.save(&session)?;
    crashed_take(&bundle.audio_dir().join("Vox.wav"), 5000)?;
    bundle.mark_recording(&Recording {
        device_latency: 256,
        midi_latency: 300,
        start: Some(START),
        tracks: vec![
            (mono, vec!["Vox.wav".into()]),
            (keys, vec!["k.midilog".into()]),
        ],
    })?;
    Ok((path, bundle, mono, keys))
}

const NOTES: [(u64, [u8; 3]); 3] = [
    (10, [0x90, 60, 100]),
    (10, [0xB0, 64, 127]),
    (700, [0x80, 60, 0]),
];

#[test]
fn test_open_recovers_a_midi_take_from_its_journal() -> Result<(), Box<dyn Error>> {
    let (path, bundle, mono, keys) = midi_pass("midi")?;
    // The crash cut the last record short.
    crashed_journal(&bundle.audio_dir().join("k.midilog"), RATE, &NOTES, 9)?;
    drop(bundle);

    let (mut bundle, _) = Bundle::open(&path)?;
    assert_eq!(
        bundle.take_recovery(),
        Some(Recovery::Takes(RecoveredPass {
            tracks: vec![(mono, vec!["Vox.wav".into()])],
            frames: 5000,
            midi: vec![RecoveredMidi {
                track: keys,
                file: "k.midilog".into(),
                messages: NOTES.to_vec(),
                frames: 701,
            }],
            start: START,
            device_latency: 256,
            midi_latency: 300,
        }))
    );
    assert!(exists(&path, "k.midilog"), "kept until placed");
    Ok(())
}

#[test]
fn test_open_recovers_a_midi_only_pass() -> Result<(), Box<dyn Error>> {
    let (path, bundle, mono, keys) = midi_pass("midi-only")?;
    crashed_journal(&bundle.audio_dir().join("k.midilog"), RATE, &NOTES, 0)?;
    fs::remove_file(bundle.audio_dir().join("Vox.wav"))?;
    drop(bundle);
    let (mut bundle, _) = Bundle::open(&path)?;
    let Some(Recovery::Takes(pass)) = bundle.take_recovery() else {
        return Err("nothing was recovered".into());
    };
    assert!(pass.tracks.is_empty(), "{mono} lost its file");
    assert_eq!(pass.midi.len(), 1);
    assert_eq!(pass.midi[0].track, keys);
    Ok(())
}

/// A change that breaks a journal's bytes.
type Break = dyn Fn(&mut Vec<u8>);

#[test]
fn test_open_skips_a_journal_that_breaks_the_format() -> Result<(), Box<dyn Error>> {
    let header_only = |bytes: &mut Vec<u8>| bytes.truncate(10);
    let other_rate = |bytes: &mut Vec<u8>| bytes[12..16].copy_from_slice(&44_100_u32.to_le_bytes());
    let bad_magic = |bytes: &mut Vec<u8>| bytes[0] = b'X';
    let backwards = |bytes: &mut Vec<u8>| bytes.extend(journal_record(5, [0x80, 60, 0]));
    let not_a_message = |bytes: &mut Vec<u8>| bytes.extend(journal_record(800, [0xF8, 0, 0]));
    let breaks: [&Break; 5] = [
        &header_only,
        &other_rate,
        &bad_magic,
        &backwards,
        &not_a_message,
    ];
    for (k, breaks) in breaks.iter().enumerate() {
        let (path, bundle, mono, _) = midi_pass(&format!("midi-bad-{k}"))?;
        let journal = bundle.audio_dir().join("k.midilog");
        crashed_journal(&journal, RATE, &NOTES, 0)?;
        let mut bytes = fs::read(&journal)?;
        breaks(&mut bytes);
        fs::write(&journal, bytes)?;
        drop(bundle);
        let (mut bundle, _) = Bundle::open(&path)?;
        let Some(Recovery::Takes(pass)) = bundle.take_recovery() else {
            return Err(format!("case {k}: the audio take was not recovered").into());
        };
        assert_eq!(
            pass.tracks,
            [(mono, vec!["Vox.wav".to_owned()])],
            "case {k}"
        );
        assert!(pass.midi.is_empty(), "case {k}");
        assert!(!exists(&path, "k.midilog"), "case {k}: cleaned up");
    }
    Ok(())
}

#[test]
fn test_open_treats_a_missing_journal_as_placed() -> Result<(), Box<dyn Error>> {
    let (path, bundle, _, _) = midi_pass("midi-placed")?;
    fs::remove_file(bundle.audio_dir().join("Vox.wav"))?;
    drop(bundle);
    let (mut bundle, _) = Bundle::open(&path)?;
    assert_eq!(bundle.take_recovery(), None);
    assert!(!path.join("recording.json").exists());
    Ok(())
}

#[test]
fn test_open_still_reads_a_version_1_marker() -> Result<(), Box<dyn Error>> {
    let (path, bundle, _, mono, _) = bundle("v1")?;
    crashed_take(&bundle.audio_dir().join("Vox.wav"), 100)?;
    drop(bundle);
    let id = mono.0;
    fs::write(
        path.join("recording.json"),
        format!(
            r#"{{"format_version":1,"device_latency":7,"start":{{"start_frame":0,"plan_latency":0}},"tracks":[{{"track":{id},"files":["Vox.wav"]}}]}}"#
        ),
    )?;
    let (mut bundle, _) = Bundle::open(&path)?;
    let Some(Recovery::Takes(pass)) = bundle.take_recovery() else {
        return Err("nothing was recovered".into());
    };
    assert_eq!(
        (pass.frames, pass.device_latency, pass.midi_latency),
        (100, 7, 0)
    );
    Ok(())
}

#[test]
fn test_open_removes_a_stray_journal() -> Result<(), Box<dyn Error>> {
    let (path, bundle, _, _, _) = bundle("stray")?;
    crashed_journal(&bundle.audio_dir().join("old.midilog"), RATE, &NOTES, 0)?;
    drop(bundle);
    let _ = Bundle::open(&path)?;
    assert!(!exists(&path, "old.midilog"));
    Ok(())
}
