//! Bundles on disk: round trips over random sessions, crash leftovers, untrusted files,
//! versions and cleanup.

use std::collections::BTreeMap;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};

use audio_buffers::Layout;
use modulation::{Curve, Handle, Point};
use project_model::{AudioFile, Clip, Endpoint, NodeKind, Session, Source};
use project_storage::{Bundle, Error as StorageError, FORMAT_VERSION, MAX_FILE_BYTES};

/// A fresh, empty directory for one test.
fn scratch(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("project-storage-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir.join("Song.debussy"))
}

/// A seeded xorshift64 generator, for reproducible random sessions.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
}

fn file(name: String, frames: u64) -> AudioFile {
    AudioFile { name, frames }
}

/// A random session: tracks of both layouts with clips, nodes of every kind with parameters,
/// connections, and some named versions taken along the way.
fn random_session(random: &mut Random) -> Result<Session, Box<dyn Error>> {
    let mut session = Session::new();
    let mut tracks = Vec::new();
    for t in 0..random.below(6) + 1 {
        let layout = if random.below(2) == 0 {
            Layout::Mono
        } else {
            Layout::Stereo
        };
        let id = session.add_track(&format!("Track {t} \"ünï\" / {}", random.next()), layout)?;
        tracks.push(id);
        for c in 0..random.below(5) {
            let frames = 1000 + random.below(100_000);
            let source = match layout {
                Layout::Mono => Source::Mono(file(format!("t{t}c{c}.wav"), frames)),
                Layout::Stereo => Source::Stereo {
                    left: file(format!("t{t}c{c}L.wav"), frames),
                    right: file(format!("t{t}c{c}R.wav"), frames),
                },
            };
            let length = 1 + random.below(frames - 1);
            let offset = random.below(frames - length + 1);
            let mut clip = Clip::new(source, offset, length, random.below(1_000_000));
            clip.gain_db = (random.below(4000) as f32 - 2000.0) / 97.0;
            clip.fade_in = random.below(length / 2 + 1);
            clip.fade_out = random.below(length / 2 + 1);
            session.add_clip(id, clip)?;
        }
        if random.below(3) == 0 {
            session.save_version(&format!("Take {t} / ✓ {}", random.next()))?;
        }
    }
    let mut parameters = BTreeMap::new();
    parameters.insert("frequency".to_owned(), 1234.5);
    parameters.insert("q".to_owned(), 0.707);
    let effect = session.add_node(NodeKind::Effect {
        kind: "eq".to_owned(),
        parameters,
    })?;
    let mixer = session.add_node(NodeKind::Mixer {
        channels: tracks.len(),
        parameters: BTreeMap::new(),
    })?;
    session.set_parameter(mixer, "channel 1 gain", Some(-3.25))?;
    session.add_node(NodeKind::Bus(Layout::Stereo))?;
    session.add_node(NodeKind::FeedbackDelay {
        layout: Layout::Mono,
        seconds: 0.25,
    })?;
    for (i, &track) in tracks.iter().enumerate() {
        session.connect(Endpoint::Track(track), 0, Endpoint::Node(mixer), i)?;
    }
    session.connect(Endpoint::Node(mixer), 0, Endpoint::Node(effect), 0)?;
    session.connect(Endpoint::Node(effect), 0, Endpoint::Output, 0)?;
    let mut points = Vec::new();
    let mut frame = random.below(1_000) as i64 - 500;
    for _ in 0..random.below(5) {
        // Gaps of at least 3 000 frames leave room for handles reaching up to 1 000 either way.
        let gap = 3_000 + random.below(10_000);
        let value = random.below(1_001) as f32 / 1_000.0;
        let reach = (1 + random.below(1_000)) as f64;
        points.push(
            Point::new(frame, value)
                .with_handles(Handle::new(-reach * 0.5, -0.125), Handle::new(reach, 0.25)),
        );
        frame += gap as i64;
    }
    let curve = session.add_curve("Curve ✓", Curve::new(points)?)?;
    let port = session.assign_curve_port(mixer, "channel 1 gain")?;
    session.connect_curve(curve, mixer, port)?;
    session.save_version("Final mix")?;
    Ok(session)
}

#[test]
fn test_save_then_open_round_trips_random_sessions() -> Result<(), Box<dyn Error>> {
    let path = scratch("round-trip")?;
    let mut random = Random(0x5EED_1234_ABCD_0001);
    for _ in 0..50 {
        let session = random_session(&mut random)?;
        if path.exists() {
            fs::remove_dir_all(&path)?;
        }
        let mut bundle = Bundle::create(&path, 48_000)?;
        bundle.save(&session)?;
        let (bundle, loaded) = Bundle::open(&path)?;
        assert_eq!(bundle.sample_rate(), 48_000);
        assert_eq!(loaded.state(), session.state());
        assert_eq!(loaded.versions(), session.versions());
    }
    Ok(())
}

#[test]
fn test_version_files_are_stamped_kept_and_removed() -> Result<(), Box<dyn Error>> {
    let path = scratch("versions")?;
    let mut session = Session::new();
    let mut bundle = Bundle::create(&path, 44_100)?;
    session.add_track("Drums", Layout::Stereo)?;
    session.save_version("First")?;
    session.save_version("Second")?;
    bundle.save(&session)?;
    let names = |path: &Path| -> Result<Vec<String>, Box<dyn Error>> {
        let mut names: Vec<String> = fs::read_dir(path.join("versions"))?
            .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
            .collect::<Result<_, _>>()?;
        names.sort();
        Ok(names)
    };
    let before = names(&path)?;
    assert_eq!(before.len(), 2);
    for name in &before {
        // YYYYMMDDTHHMMSS.mmmZ.json
        assert_eq!(name.len(), 25, "{name}");
        assert_eq!(&name[8..9], "T");
        assert!(name.ends_with("Z.json"));
    }
    // Saving again keeps the same files; reopening keeps them too.
    bundle.save(&session)?;
    assert_eq!(names(&path)?, before);
    let (mut bundle, mut session) = Bundle::open(&path)?;
    bundle.save(&session)?;
    assert_eq!(names(&path)?, before);
    // Removing a version removes its file.
    session.remove_version("First")?;
    bundle.save(&session)?;
    assert_eq!(names(&path)?.len(), 1);
    let (_, reopened) = Bundle::open(&path)?;
    assert_eq!(reopened.versions().keys().collect::<Vec<_>>(), ["Second"]);
    Ok(())
}

#[test]
fn test_open_deletes_crash_leftovers() -> Result<(), Box<dyn Error>> {
    let path = scratch("leftovers")?;
    let mut session = Session::new();
    session.add_track("Bass", Layout::Mono)?;
    Bundle::create(&path, 48_000)?.save(&session)?;
    // A save interrupted before its rename leaves the old file and a temporary one.
    fs::write(path.join("project.json.tmp"), b"{ half writ")?;
    fs::write(
        path.join("versions").join("20260101T000000.000Z.json.tmp"),
        b"[",
    )?;
    let (_, loaded) = Bundle::open(&path)?;
    assert_eq!(loaded.state(), session.state());
    assert!(!path.join("project.json.tmp").exists());
    assert!(
        !path
            .join("versions")
            .join("20260101T000000.000Z.json.tmp")
            .exists()
    );
    Ok(())
}

/// Replaces `project.json` with `text` and opens the bundle.
fn open_with(path: &Path, text: &str) -> Result<StorageError, Box<dyn Error>> {
    fs::write(path.join("project.json"), text)?;
    match Bundle::open(path) {
        Ok(_) => Err("a bad project.json was accepted".into()),
        Err(error) => Ok(error),
    }
}

fn project(tracks: &str, connections: &str) -> String {
    format!(
        r#"{{"format_version":1,"sample_rate":48000,"tracks":[{tracks}],"nodes":[],"connections":[{connections}]}}"#
    )
}

/// A version 4 project with this tempo and meter.
fn tempo(bpm: f64, beats_per_bar: u8, beat_unit: u8) -> String {
    format!(
        r#"{{"format_version":4,"sample_rate":48000,"tracks":[],"nodes":[],"connections":[],"tempo":{{"bpm":{bpm},"beats_per_bar":{beats_per_bar},"beat_unit":{beat_unit}}}}}"#
    )
}

/// A version 5 project with one mixer, whose curve ports are `ports`, and these curves and
/// connections.
fn curves(ports: &str, curves: &str, connections: &str) -> String {
    format!(
        r#"{{"format_version":5,"sample_rate":48000,"tracks":[],"nodes":[{{"id":1,"kind":{{"kind":"mixer","channels":1,"parameters":{{}}}},"curve_ports":{{{ports}}}}}],"connections":[{connections}],"tempo":{{"bpm":120.0,"beats_per_bar":4,"beat_unit":4}},"curves":[{curves}]}}"#
    )
}

/// A saved curve point at `frame` and `value` whose out-handle reaches `out` frames.
fn point(frame: i64, value: f32, out: f64) -> String {
    format!(
        r#"{{"frame":{frame},"value":{value},"in_handle":{{"d_frame":0.0,"d_value":0.0}},"out_handle":{{"d_frame":{out},"d_value":0.0}}}}"#
    )
}

fn mono_track(file_name: &str) -> String {
    format!(
        r#"{{"id":1,"name":"T","layout":"mono","clips":[{{"id":2,"source":{{"kind":"mono","file":{{"name":"{file_name}","frames":100}}}},"offset":0,"length":10,"position":0,"gain_db":0.0,"fade_in":0,"fade_out":0}}]}}"#
    )
}

#[test]
fn test_open_refuses_untrusted_files() -> Result<(), Box<dyn Error>> {
    let path = scratch("untrusted")?;
    Bundle::create(&path, 48_000)?;
    // The fixtures are valid apart from the one fault each.
    assert!(Bundle::open(&path).is_ok());
    fs::write(path.join("project.json"), project(&mono_track("a.wav"), ""))?;
    assert!(Bundle::open(&path).is_ok());
    fs::write(path.join("project.json"), tempo(97.25, 7, 8))?;
    assert!(Bundle::open(&path).is_ok());
    let curve = format!(
        r#"{{"id":2,"name":"C","points":[{},{}]}}"#,
        point(0, 0.25, 10.0),
        point(100, 1.0, 0.0)
    );
    let wire = r#"{"id":3,"from":{"kind":"curve","id":2},"output":0,"to":{"kind":"curve_port","node":1,"port":1},"input":0,"pan":0.0}"#;
    fs::write(
        path.join("project.json"),
        curves(r#""channel 1 gain":1"#, &curve, wire),
    )?;
    assert!(Bundle::open(&path).is_ok());

    for name in [
        "../escape.wav",
        "/etc/passwd",
        "a/b.wav",
        ".hidden.wav",
        "",
        "C:x.wav",
    ] {
        let error = open_with(&path, &project(&mono_track(name), ""))?;
        assert!(
            matches!(error, StorageError::Invalid { .. }),
            "{name:?}: {error}"
        );
    }
    let newer = project("", "").replace(
        "\"format_version\":1",
        &format!("\"format_version\":{}", FORMAT_VERSION + 1),
    );
    assert!(matches!(
        open_with(&path, &newer)?,
        StorageError::NewerFormat { .. }
    ));
    let invalid = [
        project("", "").replace("48000", "12345"),
        project("", "").replace("\"nodes\":[]", "\"nodes\":[],\"extra\":1"),
        project(
            &mono_track("a.wav").replace("\"length\":10", "\"length\":1000"),
            "",
        ),
        project(
            &mono_track("a.wav"),
            r#"{"id":3,"from":{"kind":"track","id":9},"output":0,"to":{"kind":"output"},"input":0,"pan":0.0}"#,
        ),
        project(
            &mono_track("a.wav"),
            r#"{"id":3,"from":{"kind":"output"},"output":0,"to":{"kind":"output"},"input":0,"pan":0.0}"#,
        ),
        project(
            &mono_track("a.wav"),
            r#"{"id":3,"from":{"kind":"track","id":1},"output":0,"to":{"kind":"output"},"input":0,"pan":2.0}"#,
        ),
        project(
            &format!("{},{}", mono_track("a.wav"), mono_track("b.wav")),
            "",
        ),
        tempo(400.0, 4, 4),
        tempo(120.0, 0, 4),
        tempo(120.0, 4, 3),
        tempo(120.005, 4, 4),
        // A segment turning back in time.
        curves(
            "",
            &format!(
                r#"{{"id":2,"name":"C","points":[{},{}]}}"#,
                point(0, 0.5, -1.0),
                point(9, 0.5, 0.0)
            ),
            "",
        ),
        // A value outside 0 to 1.
        curves(
            "",
            &format!(r#"{{"id":2,"name":"C","points":[{}]}}"#, point(0, 1.5, 0.0)),
            "",
        ),
        // A curve port on a parameter that cannot be automated.
        curves(r#""channel 1 mode":1"#, "", ""),
        // A curve port numbered 0.
        curves(r#""channel 1 gain":0"#, "", ""),
        // A wire from a curve to an audio input.
        curves(
            "",
            &curve,
            r#"{"id":3,"from":{"kind":"curve","id":2},"output":0,"to":{"kind":"node","id":1},"input":0,"pan":0.0}"#,
        ),
        // A wire to a curve port no parameter takes.
        curves("", &curve, wire),
        // A version 5 node without its curve ports.
        curves("", "", "").replace(r#","curve_ports":{}"#, ""),
    ];
    for text in invalid {
        let error = open_with(&path, &text)?;
        assert!(
            matches!(
                error,
                StorageError::Invalid { .. } | StorageError::Json { .. }
            ),
            "{error}"
        );
    }
    let deep = format!("{}{}", "[".repeat(100), "]".repeat(100));
    assert!(matches!(
        open_with(&path, &deep)?,
        StorageError::TooLarge { .. }
    ));
    let huge = " ".repeat(MAX_FILE_BYTES as usize + 1);
    assert!(matches!(
        open_with(&path, &huge)?,
        StorageError::TooLarge { .. }
    ));
    Ok(())
}

#[test]
fn test_clean_up_deletes_only_unreferenced_audio() -> Result<(), Box<dyn Error>> {
    let path = scratch("cleanup")?;
    let mut bundle = Bundle::create(&path, 48_000)?;
    let mut session = Session::new();
    let track = session.add_track("Vox", Layout::Mono)?;
    let used = Clip::new(Source::Mono(file("used.wav".into(), 100)), 0, 10, 0);
    let versioned = Clip::new(Source::Mono(file("versioned.wav".into(), 100)), 0, 10, 0);
    let clip = session.add_clip(track, versioned)?;
    session.save_version("Old")?;
    session.set_clip(clip, used)?;
    bundle.save(&session)?;
    let audio = bundle.audio_dir();
    for name in ["used.wav", "versioned.wav", "stale.wav", "notes.txt"] {
        fs::write(audio.join(name), b"x")?;
    }
    fs::create_dir(audio.join("folder.wav"))?;
    let (_, _) = Bundle::open(&path)?;
    let mut left: Vec<String> = fs::read_dir(&audio)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    left.sort();
    assert_eq!(
        left,
        ["folder.wav", "notes.txt", "used.wav", "versioned.wav"]
    );
    // Once the version goes, closing cleans up its audio too.
    let (mut bundle, mut session) = Bundle::open(&path)?;
    session.remove_version("Old")?;
    bundle.save(&session)?;
    assert_eq!(bundle.clean_up(&session)?, ["versioned.wav"]);
    Ok(())
}

#[test]
fn test_create_refuses_bad_rate_and_existing_path() -> Result<(), Box<dyn Error>> {
    let path = scratch("create")?;
    assert!(matches!(
        Bundle::create(&path, 22_050),
        Err(StorageError::SampleRate(22_050))
    ));
    Bundle::create(&path, 96_000)?;
    assert!(matches!(
        Bundle::create(&path, 96_000),
        Err(StorageError::Exists(_))
    ));
    for dir in ["audio", "versions", "cache"] {
        assert!(path.join(dir).is_dir());
    }
    Ok(())
}

#[test]
fn test_board_positions_round_trip_and_bad_ones_are_refused() -> Result<(), Box<dyn Error>> {
    let path = scratch("board-positions")?;
    let mut bundle = Bundle::create(&path, 48_000)?;
    let mut session = Session::new_project();
    let mixer = session.state().project_mixer().ok_or("no mixer")?;
    session.move_node(mixer, Some(project_model::Position { x: 320.0, y: -12.5 }))?;
    session.move_output(Some(96.0))?;
    bundle.save(&session)?;
    let (_, opened) = Bundle::open(&path)?;
    assert_eq!(opened.state(), session.state());
    // An older file without them opens to the automatic layout.
    fs::write(path.join("project.json"), project("", ""))?;
    let (_, opened) = Bundle::open(&path)?;
    assert_eq!(opened.state().output_y, None);

    let node = |position: &str| {
        project("", "").replace(
            "\"nodes\":[]",
            &format!(
                r#""nodes":[{{"id":5,"kind":{{"kind":"bus","layout":"stereo"}},"position":{position}}}]"#
            ),
        )
    };
    assert!(Bundle::open(&path).is_ok());
    fs::write(path.join("project.json"), node(r#"{"x":10.0,"y":20.0}"#))?;
    assert!(Bundle::open(&path).is_ok());
    for text in [
        node(r#"{"x":1e9,"y":0.0}"#),
        node(r#"{"x":0.0,"y":-1e30}"#),
        node(r#"{"x":0.0,"y":0.0,"z":1.0}"#),
        project("", "").replace("\"nodes\":[]", "\"nodes\":[],\"output_y\":1e12"),
    ] {
        let error = open_with(&path, &text)?;
        assert!(
            matches!(
                error,
                StorageError::Invalid { .. } | StorageError::Json { .. }
            ),
            "{error}"
        );
    }
    Ok(())
}
