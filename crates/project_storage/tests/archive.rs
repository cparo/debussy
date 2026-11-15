//! Project archives on disk: round trips, what is left out, and archives refused as untrusted
//! input without leaving anything behind.

use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use audio_buffers::Layout;
use project_model::{AudioFile, Clip, Session, Source};
use project_storage::{
    Bundle, Error as StorageError, MAX_FILE_BYTES, unpack_archive, write_archive,
};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// A zip's entries: each name and its contents.
type Entries<'a> = Vec<(&'a str, &'a [u8])>;

/// A fresh, empty directory for one test.
fn scratch(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("project-archive-{}-{name}", std::process::id()));
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
    }
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// The names in `dir`, sorted.
fn names(dir: &Path) -> Result<Vec<String>, Box<dyn Error>> {
    let mut names: Vec<String> = fs::read_dir(dir)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<Result<_, _>>()?;
    names.sort();
    Ok(names)
}

/// A saved bundle `<dir>/Song.debussy` with one track, a clip on `take.wav`, a named version,
/// a cache file and a recording marker; returns its path and session.
fn song(dir: &Path) -> Result<(PathBuf, Session), Box<dyn Error>> {
    let path = dir.join("Song.debussy");
    let mut bundle = Bundle::create(&path, 48_000)?;
    let mut session = Session::new();
    let track = session.add_track("Vox", Layout::Mono)?;
    let take = AudioFile {
        name: "take.wav".into(),
        frames: 100,
    };
    session.add_clip(track, Clip::new(Source::Mono(take), 0, 10, 0))?;
    session.save_version("First")?;
    bundle.save(&session)?;
    fs::write(bundle.audio_dir().join("take.wav"), vec![7u8; 300_000])?;
    fs::write(bundle.cache_dir().join("take.summary"), b"derived")?;
    fs::write(path.join("recording.json"), b"{}")?;
    Ok((path, session))
}

/// Writes a zip at `path` with `entries` of (name, contents); a name ending in `/` is a
/// directory.
fn zip_of(path: &Path, entries: &[(&str, &[u8])]) -> Result<(), Box<dyn Error>> {
    let mut zip = ZipWriter::new(fs::File::create(path)?);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, contents) in entries {
        if name.ends_with('/') {
            zip.add_directory(*name, options)?;
        } else {
            zip.start_file(*name, options)?;
            zip.write_all(contents)?;
        }
    }
    zip.finish()?;
    Ok(())
}

#[test]
fn test_write_archive_then_unpack_round_trips() -> Result<(), Box<dyn Error>> {
    let dir = scratch("round-trip")?;
    let (bundle, session) = song(&dir)?;
    let archive = dir.join("Song.debussy.zip");
    write_archive(&bundle, &archive)?;
    assert!(!dir.join("Song.debussy.zip.tmp").exists());

    let projects = dir.join("projects");
    fs::create_dir(&projects)?;
    let unpacked = unpack_archive(&archive, &projects, u64::MAX)?;
    assert_eq!(unpacked, projects.join("Song.debussy"));
    assert_eq!(names(&projects)?, ["Song.debussy"]);
    assert_eq!(
        names(&unpacked)?,
        ["audio", "cache", "project.json", "versions"]
    );
    assert_eq!(names(&unpacked.join("cache"))?, Vec::<String>::new());
    assert_eq!(
        fs::read(unpacked.join("audio").join("take.wav"))?,
        fs::read(bundle.join("audio").join("take.wav"))?
    );
    let (_, opened) = Bundle::open(&unpacked)?;
    assert_eq!(opened.state(), session.state());
    assert_eq!(opened.versions().len(), 1);

    // A second unpack takes the next free name.
    assert_eq!(
        unpack_archive(&archive, &projects, u64::MAX)?,
        projects.join("Song 2.debussy")
    );
    Ok(())
}

#[test]
fn test_write_archive_replaces_and_refuses_non_bundle() -> Result<(), Box<dyn Error>> {
    let dir = scratch("replace")?;
    let (bundle, _) = song(&dir)?;
    let archive = dir.join("out.debussy.zip");
    fs::write(&archive, b"old")?;
    write_archive(&bundle, &archive)?;
    assert_ne!(fs::read(&archive)?, b"old");
    let plain = dir.join("plain");
    fs::create_dir(&plain)?;
    assert!(matches!(
        write_archive(&plain, &dir.join("x.zip")),
        Err(StorageError::Invalid { .. })
    ));
    assert!(!dir.join("x.zip").exists());
    Ok(())
}

#[test]
fn test_unpack_archive_refuses_unsafe_entries_leaving_nothing() -> Result<(), Box<dyn Error>> {
    let dir = scratch("refuse")?;
    let projects = dir.join("projects");
    fs::create_dir(&projects)?;
    let project: &[u8] = b"{}";
    let cases: [(&str, Entries); 7] = [
        (
            "slip",
            vec![
                ("Song.debussy/project.json", project),
                ("Song.debussy/../../evil.wav", b"x"),
            ],
        ),
        ("absolute", vec![("/Song.debussy/project.json", project)]),
        (
            "unknown",
            vec![
                ("Song.debussy/project.json", project),
                ("Song.debussy/notes.txt", b"x"),
            ],
        ),
        (
            "macosx",
            vec![
                ("Song.debussy/project.json", project),
                ("__MACOSX/._Song.debussy", b"x"),
            ],
        ),
        (
            "two-projects",
            vec![
                ("A.debussy/project.json", project),
                ("B.debussy/project.json", project),
            ],
        ),
        ("no-project", vec![("Song.debussy/audio/a.wav", b"x")]),
        (
            "twice",
            vec![
                ("Song.debussy/project.json", project),
                ("Song.debussy/project.json", project),
            ],
        ),
    ];
    for (name, entries) in cases {
        let archive = dir.join(format!("{name}.zip"));
        // The zip writer itself refuses a repeated name; skip that case if so.
        if zip_of(&archive, &entries).is_err() {
            continue;
        }
        let result = unpack_archive(&archive, &projects, u64::MAX);
        assert!(
            matches!(result, Err(StorageError::Archive { .. })),
            "{name}: {result:?}"
        );
        assert_eq!(names(&projects)?, Vec::<String>::new(), "{name}");
    }
    assert!(!dir.join("evil.wav").exists());
    fs::write(dir.join("junk.zip"), b"not a zip")?;
    assert!(unpack_archive(&dir.join("junk.zip"), &projects, u64::MAX).is_err());
    Ok(())
}

#[test]
fn test_unpack_archive_refuses_symlink() -> Result<(), Box<dyn Error>> {
    let dir = scratch("symlink")?;
    let projects = dir.join("projects");
    fs::create_dir(&projects)?;
    let archive = dir.join("link.zip");
    let mut zip = ZipWriter::new(fs::File::create(&archive)?);
    let options = SimpleFileOptions::default();
    zip.start_file("Song.debussy/project.json", options)?;
    zip.write_all(b"{}")?;
    zip.add_symlink("Song.debussy/audio/a.wav", "/etc/passwd", options)?;
    zip.finish()?;
    let result = unpack_archive(&archive, &projects, u64::MAX);
    assert!(
        matches!(result, Err(StorageError::Archive { .. })),
        "{result:?}"
    );
    assert_eq!(names(&projects)?, Vec::<String>::new());
    Ok(())
}

#[test]
fn test_unpack_archive_stops_at_size_limits() -> Result<(), Box<dyn Error>> {
    let dir = scratch("limits")?;
    let projects = dir.join("projects");
    fs::create_dir(&projects)?;
    // Highly compressible audio: a small archive that unpacks large.
    let audio = vec![0u8; 4 * 1024 * 1024];
    let archive = dir.join("bomb.zip");
    zip_of(
        &archive,
        &[
            ("Song.debussy/project.json", b"{}"),
            ("Song.debussy/audio/a.wav", &audio),
        ],
    )?;
    assert!(fs::metadata(&archive)?.len() < 100_000);
    let result = unpack_archive(&archive, &projects, 1024 * 1024);
    assert!(
        matches!(
            result,
            Err(StorageError::TooLarge {
                what: "unpacked size",
                ..
            })
        ),
        "{result:?}"
    );
    assert_eq!(names(&projects)?, Vec::<String>::new());

    let json = vec![b' '; usize::try_from(MAX_FILE_BYTES)? + 1];
    let big = dir.join("big-json.zip");
    zip_of(&big, &[("Song.debussy/project.json", &json)])?;
    let result = unpack_archive(&big, &projects, u64::MAX);
    assert!(
        matches!(result, Err(StorageError::TooLarge { .. })),
        "{result:?}"
    );
    assert_eq!(names(&projects)?, Vec::<String>::new());
    Ok(())
}
