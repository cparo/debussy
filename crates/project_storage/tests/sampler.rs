//! Sampler zones on disk: format 11 round trips, untrusted zones, older files and cleanup
//! (`docs/architecture/project.md` "Saved format", ADR-0154).

use std::error::Error;
use std::fs;
use std::path::PathBuf;

use project_model::{AudioFile, NodeKind, SAMPLER_KIND, SampleZone, Session, Source, ZoneMode};
use project_storage::{Bundle, Error as StorageError};

/// A fresh, empty directory for one test.
fn scratch(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!(
        "project-storage-sampler-{}-{name}",
        std::process::id()
    ));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir.join("Song.debussy"))
}

fn file(name: &str) -> AudioFile {
    AudioFile {
        name: name.to_owned(),
        frames: 480,
    }
}

fn kit() -> Vec<SampleZone> {
    vec![
        SampleZone {
            file: Source::Mono(file("kick.wav")),
            low: 36,
            high: 36,
            root: 36,
            mode: ZoneMode::OneShot,
            choke: None,
        },
        SampleZone {
            file: Source::Stereo {
                left: file("hat-l.wav"),
                right: file("hat-r.wav"),
            },
            low: 0,
            high: 127,
            root: 60,
            mode: ZoneMode::Gated,
            choke: Some(16),
        },
    ]
}

/// A session with a sampler on a MIDI track playing `kit()`.
fn session_with_kit() -> Result<Session, Box<dyn Error>> {
    let mut session = Session::new();
    let track = session.add_midi_track("Drums")?;
    let node = session.set_instrument(track, SAMPLER_KIND)?;
    session.set_zones(node, kit())?;
    Ok(session)
}

#[test]
fn test_zones_round_trip() -> Result<(), Box<dyn Error>> {
    let path = scratch("round-trip")?;
    let session = session_with_kit()?;
    let mut bundle = Bundle::create(&path, 48_000)?;
    bundle.save(&session)?;
    drop(bundle);
    let (_, loaded) = Bundle::open(&path)?;
    assert_eq!(loaded.state(), session.state());
    let text = fs::read_to_string(path.join("project.json"))?;
    assert!(text.contains("\"one_shot\""), "{text}");
    Ok(())
}

/// A project of `version` with one instrument node of kind `instrument` and these zones.
fn instrument_project(version: u32, instrument: &str, zones: &str) -> String {
    let zones = if zones.is_empty() {
        String::new()
    } else {
        format!(r#","zones":[{zones}]"#)
    };
    format!(
        r#"{{"format_version":{version},"sample_rate":48000,"tracks":[],"nodes":[{{"id":1,"kind":{{"kind":"instrument","instrument":"{instrument}","parameters":{{}}{zones}}},"curve_ports":{{}}}}],"connections":[],"tempo":{{"bpm":120.0,"beats_per_bar":4,"beat_unit":4}},"curves":[],"sequences":[]}}"#
    )
}

fn zone(name: &str, low: u8, high: u8, root: u8, choke: &str) -> String {
    format!(
        r#"{{"file":{{"kind":"mono","file":{{"name":"{name}","frames":10}}}},"low":{low},"high":{high},"root":{root},"mode":"gated"{choke}}}"#
    )
}

#[test]
fn test_open_reads_version_10_instruments_and_refuses_bad_zones() -> Result<(), Box<dyn Error>> {
    let path = scratch("untrusted")?;
    Bundle::create(&path, 48_000)?;
    let write = |text: String| fs::write(path.join("project.json"), text);
    write(instrument_project(10, "synth", ""))?;
    let (bundle, _) = Bundle::open(&path)?;
    drop(bundle);
    write(instrument_project(
        11,
        SAMPLER_KIND,
        &zone("a.wav", 0, 127, 60, r#","choke":1"#),
    ))?;
    let (bundle, _) = Bundle::open(&path)?;
    drop(bundle);

    let bad = [
        instrument_project(11, SAMPLER_KIND, &zone("../a.wav", 0, 1, 0, "")),
        instrument_project(11, SAMPLER_KIND, &zone(".a.wav", 0, 1, 0, "")),
        instrument_project(11, SAMPLER_KIND, &zone("a.wav", 2, 1, 0, "")),
        instrument_project(11, SAMPLER_KIND, &zone("a.wav", 0, 128, 0, "")),
        instrument_project(11, SAMPLER_KIND, &zone("a.wav", 0, 1, 128, "")),
        instrument_project(11, SAMPLER_KIND, &zone("a.wav", 0, 1, 0, r#","choke":0"#)),
        instrument_project(11, SAMPLER_KIND, &zone("a.wav", 0, 1, 0, r#","choke":17"#)),
        instrument_project(11, SAMPLER_KIND, &zone("a.wav", 0, 1, 0, ""))
            .replace(r#""mode":"gated""#, r#""mode":"looped""#),
        instrument_project(11, "synth", &zone("a.wav", 0, 1, 0, "")),
        instrument_project(
            11,
            SAMPLER_KIND,
            r#"{"file":{"kind":"midi","sequence":0},"low":0,"high":1,"root":0,"mode":"gated"}"#,
        ),
    ];
    for text in bad {
        write(text.clone())?;
        match Bundle::open(&path) {
            Ok(_) => return Err(format!("accepted: {text}").into()),
            Err(error) => assert!(
                matches!(
                    error,
                    StorageError::Invalid { .. } | StorageError::Json { .. }
                ),
                "{text}: {error}"
            ),
        }
    }
    Ok(())
}

#[test]
fn test_clean_up_keeps_zone_files() -> Result<(), Box<dyn Error>> {
    let path = scratch("cleanup")?;
    let mut bundle = Bundle::create(&path, 48_000)?;
    let mut session = session_with_kit()?;
    session.save_version("Kit")?;
    let node = session
        .state()
        .nodes
        .iter()
        .find(|n| matches!(n.kind, NodeKind::Instrument { .. }))
        .map(|n| n.id)
        .ok_or("no sampler")?;
    session.set_zones(node, kit()[..1].to_vec())?;
    bundle.save(&session)?;
    let audio = bundle.audio_dir();
    for name in ["kick.wav", "hat-l.wav", "hat-r.wav", "stale.wav"] {
        fs::write(audio.join(name), b"x")?;
    }
    assert_eq!(bundle.clean_up(&session)?, ["stale.wav"]);
    session.remove_version("Kit")?;
    bundle.save(&session)?;
    let mut removed = bundle.clean_up(&session)?;
    removed.sort();
    assert_eq!(removed, ["hat-l.wav", "hat-r.wav"]);
    drop(bundle);
    Ok(())
}
