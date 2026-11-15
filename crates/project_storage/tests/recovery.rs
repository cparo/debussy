//! Recovering takes a crash cut off: the recording marker, header repair on open, and the
//! markers that are discarded.

use std::error::Error;
use std::fs::{self, File};
use std::path::{Path, PathBuf};

use audio_buffers::Layout;
use project_model::{AudioFile, Clip, Session, Source, TrackId};
use project_storage::{
    Bundle, MAX_RECORDING_BYTES, Recording, RecordingStart, RecoveredPass, Recovery,
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
            start: START,
            device_latency: 256,
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
        r#"{"format_version":2,"device_latency":0,"start":null,"tracks":[]}"#.to_owned(),
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

#[test]
fn test_open_discards_a_marker_naming_a_midi_track() -> Result<(), Box<dyn Error>> {
    let (path, mut bundle, mut session, mono, _) = bundle("midi")?;
    let keys = session.add_midi_track("Synth")?;
    bundle.save(&session)?;
    crashed_takes(&bundle)?;
    let mut marker = recording(mono, keys, Some(START));
    marker.tracks[1].1 = vec!["Keys L.wav".into()];
    bundle.mark_recording(&marker)?;
    drop(bundle);
    let (mut bundle, _) = Bundle::open(&path)?;
    let Some(Recovery::Discarded { reason }) = bundle.take_recovery() else {
        return Err("the marker was not discarded".into());
    };
    assert!(reason.contains("MIDI"), "{reason}");
    Ok(())
}
