//! Project archives: a bundle written as one zip file and unpacked again
//! (`docs/architecture/project.md` "Project archives", ADR-0140, ADR-0141).
//!
//! An archive is untrusted input when opened: every entry is checked against the bundle layout
//! before anything is written, and the bytes written are counted rather than taken from the
//! archive's headers.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZIP64_BYTES_THR, ZipArchive, ZipWriter};

use crate::bundle::{AUDIO_DIR, CACHE_DIR, MAX_FILE_BYTES, PROJECT_FILE, VERSIONS_DIR, civil};
use crate::error::Error;
use crate::validate::{MAX_NAME_BYTES, is_plain_file_name};

/// The end of an archive's file name: `<name>.debussy.zip`.
pub const ARCHIVE_SUFFIX: &str = ".debussy.zip";

/// The most entries an archive may hold.
pub const MAX_ARCHIVE_ENTRIES: usize = 100_000;

/// The free space an unpacked archive must leave on the projects folder's volume, in bytes.
pub const ARCHIVE_SPACE_MARGIN: u64 = 1 << 30;

/// A bundle directory's extension, with its dot.
const BUNDLE_SUFFIX: &str = ".debussy";

/// The most "<name> <n>" names tried for an unpacked bundle.
const MAX_NAME_TRIES: u32 = 10_000;

/// Writes the bundle at `bundle` (a directory named `<name>.debussy`) to the archive `dest`,
/// replacing any file there: `project.json`, the named versions and the stored audio inside one
/// `<name>.debussy/` directory, without `cache/`, `lock` or `recording.json`. The archive is
/// written beside `dest`, synced, then renamed into place; a failure leaves no file behind.
///
/// The caller saves the project first; audio must not be written while this runs.
///
/// # Errors
///
/// [`Error::Invalid`] if `bundle` is not named `<name>.debussy`, [`Error::Io`],
/// [`Error::Archive`].
pub fn write_archive(bundle: &Path, dest: &Path) -> Result<(), Error> {
    let top = bundle
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| bundle_stem(n).is_some())
        .ok_or_else(|| Error::Invalid {
            path: bundle.to_owned(),
            reason: "not a bundle named <name>.debussy".to_owned(),
        })?;
    let mut temp = dest.as_os_str().to_owned();
    temp.push(".tmp");
    let temp = PathBuf::from(temp);
    let result = write_entries(bundle, top, &temp).and_then(|()| {
        fs::rename(&temp, dest).map_err(|source| Error::Io {
            path: dest.to_owned(),
            source,
        })
    });
    if result.is_err() {
        // The temporary file may not exist; nothing more can be done if removal fails.
        let _ = fs::remove_file(&temp);
    }
    result
}

/// Writes the archive of `bundle`, whose directory is named `top`, to `path` and syncs it.
fn write_entries(bundle: &Path, top: &str, path: &Path) -> Result<(), Error> {
    let file = File::create(path).map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })?;
    let mut zip = ZipWriter::new(BufWriter::new(file));
    let dir = SimpleFileOptions::default()
        .unix_permissions(0o755)
        .last_modified_time(zip_time(Some(SystemTime::now())));
    let archive_err = |e| zip_error(path, e);
    zip.add_directory(format!("{top}/"), dir)
        .map_err(archive_err)?;
    add_file(
        &mut zip,
        path,
        &bundle.join(PROJECT_FILE),
        &format!("{top}/{PROJECT_FILE}"),
    )?;
    for (sub, extension) in [(VERSIONS_DIR, ".json"), (AUDIO_DIR, ".wav")] {
        zip.add_directory(format!("{top}/{sub}/"), dir)
            .map_err(archive_err)?;
        for name in plain_files(&bundle.join(sub), extension)? {
            let source = bundle.join(sub).join(&name);
            add_file(&mut zip, path, &source, &format!("{top}/{sub}/{name}"))?;
        }
    }
    let writer = zip.finish().map_err(archive_err)?;
    let file = writer.into_inner().map_err(|e| Error::Io {
        path: path.to_owned(),
        source: e.into_error(),
    })?;
    file.sync_all().map_err(|source| Error::Io {
        path: path.to_owned(),
        source,
    })
}

/// Adds the file `source` to `zip` (being written to `archive`) as `name`: JSON deflated, audio
/// stored.
fn add_file<W: Write + Seek>(
    zip: &mut ZipWriter<W>,
    archive: &Path,
    source: &Path,
    name: &str,
) -> Result<(), Error> {
    let io_err = |source_err| Error::Io {
        path: source.to_owned(),
        source: source_err,
    };
    let mut file = File::open(source).map_err(io_err)?;
    let metadata = file.metadata().map_err(io_err)?;
    let len = metadata.len();
    let method = if name.ends_with(".json") {
        CompressionMethod::Deflated
    } else {
        CompressionMethod::Stored
    };
    let options = SimpleFileOptions::default()
        .compression_method(method)
        .unix_permissions(0o644)
        .large_file(len >= ZIP64_BYTES_THR)
        .last_modified_time(zip_time(metadata.modified().ok()));
    zip.start_file(name, options)
        .map_err(|e| zip_error(archive, e))?;
    io::copy(&mut file, zip).map_err(|e| Error::Io {
        path: archive.to_owned(),
        source: e,
    })?;
    Ok(())
}

/// `modified` as a zip entry's time, in UTC; zip's earliest time (1980) if unknown or outside
/// the years zip can hold.
fn zip_time(modified: Option<SystemTime>) -> DateTime {
    modified
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| {
            let (year, month, day, hour, minute, second) = civil(d.as_secs());
            DateTime::from_date_and_time(
                u16::try_from(year).ok()?,
                u8::try_from(month).ok()?,
                u8::try_from(day).ok()?,
                u8::try_from(hour).ok()?,
                u8::try_from(minute).ok()?,
                u8::try_from(second).ok()?,
            )
            .ok()
        })
        .unwrap_or_default()
}

/// The names of the regular files in `dir` that are plain names ending in `extension`, sorted;
/// empty if `dir` does not exist.
fn plain_files(dir: &Path, extension: &str) -> Result<Vec<String>, Error> {
    let io_err = |source| Error::Io {
        path: dir.to_owned(),
        source,
    };
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(io_err)? {
        let entry = entry.map_err(io_err)?;
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if is_plain_file_name(&name)
            && name.ends_with(extension)
            && entry.file_type().map_err(io_err)?.is_file()
        {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Where an accepted archive entry goes inside the bundle.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
    /// The bundle directory, or its `versions/` or `audio/` directory: nothing to write.
    Dir,
    /// A file at this path inside the bundle; `json` caps its size at [`MAX_FILE_BYTES`].
    File { path: String, json: bool },
}

/// Unpacks the archive at `archive` into `projects_dir` and returns the new bundle's path: the
/// archive's `<name>.debussy`, or the first free `<name> <n>.debussy`. The archive is checked
/// as untrusted input (`docs/architecture/project.md` "Project archives"), and at most
/// `max_bytes` are written; any failure leaves nothing behind. The bundle is not opened: the
/// caller opens it, which applies the usual load checks.
///
/// # Errors
///
/// [`Error::Archive`] for an archive that is not a project archive or breaks a rule,
/// [`Error::TooLarge`] for one over a limit, [`Error::Io`].
pub fn unpack_archive(
    archive: &Path,
    projects_dir: &Path,
    max_bytes: u64,
) -> Result<PathBuf, Error> {
    let file = File::open(archive).map_err(|source| Error::Io {
        path: archive.to_owned(),
        source,
    })?;
    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|e| zip_error(archive, e))?;
    if zip.len() > MAX_ARCHIVE_ENTRIES {
        return Err(Error::TooLarge {
            path: archive.to_owned(),
            what: "number of entries",
            limit: MAX_ARCHIVE_ENTRIES as u64,
        });
    }
    let (top, entries) = plan(&mut zip, archive)?;
    let Some(stem) = bundle_stem(&top) else {
        return Err(refuse(archive, "no project inside"));
    };

    let temp = Unpacking::create(projects_dir, stem)?;
    for sub in [AUDIO_DIR, VERSIONS_DIR, CACHE_DIR] {
        let dir = temp.path.join(sub);
        fs::create_dir(&dir).map_err(|source| Error::Io { path: dir, source })?;
    }
    let mut total = 0u64;
    for (index, entry) in entries {
        if let Entry::File { path, json } = entry {
            let mut reader = zip.by_index(index).map_err(|e| zip_error(archive, e))?;
            let declared = reader.size();
            let out = temp.path.join(&path);
            let limit = Limits {
                declared,
                entry: if json { MAX_FILE_BYTES } else { u64::MAX },
                total: max_bytes,
            };
            copy_limited(&mut reader, &out, archive, &path, limit, &mut total)?;
        }
    }
    let dest = free_bundle_path(projects_dir, stem)
        .ok_or_else(|| Error::Exists(projects_dir.join(&top)))?;
    temp.finish(&dest)?;
    Ok(dest)
}

/// Checks every entry of `zip` (read from `archive`) and returns the bundle directory's name and
/// what to do with each entry, by index. Nothing is decompressed.
fn plan<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    archive: &Path,
) -> Result<(String, Vec<(usize, Entry)>), Error> {
    let mut top: Option<String> = None;
    let mut seen = HashSet::new();
    let mut entries = Vec::with_capacity(zip.len());
    let mut has_project = false;
    for index in 0..zip.len() {
        let file = zip.by_index_raw(index).map_err(|e| zip_error(archive, e))?;
        let Ok(name) = std::str::from_utf8(file.name_raw()) else {
            return Err(refuse(archive, "an entry's name is not UTF-8"));
        };
        if file.is_symlink() {
            return Err(refuse(archive, &format!("{name:?} is a symbolic link")));
        }
        if file.encrypted() {
            return Err(refuse(archive, &format!("{name:?} is encrypted")));
        }
        if !matches!(
            file.compression(),
            CompressionMethod::Stored | CompressionMethod::Deflated
        ) {
            return Err(refuse(
                archive,
                &format!("{name:?} uses an unsupported compression method"),
            ));
        }
        let Some((entry_top, entry)) = classify(name, file.is_dir()) else {
            return Err(refuse(
                archive,
                &format!("{name:?} is not part of a project bundle"),
            ));
        };
        match &top {
            Some(t) if *t != entry_top => {
                return Err(refuse(archive, "more than one project inside"));
            }
            Some(_) => {}
            None => top = Some(entry_top),
        }
        if !seen.insert(name.trim_end_matches('/').to_owned()) {
            return Err(refuse(archive, &format!("{name:?} appears twice")));
        }
        if let Entry::File { path, json } = &entry {
            has_project |= path == PROJECT_FILE;
            if *json && file.size() > MAX_FILE_BYTES {
                return Err(Error::TooLarge {
                    path: archive.to_owned(),
                    what: "a JSON entry's size",
                    limit: MAX_FILE_BYTES,
                });
            }
        }
        entries.push((index, entry));
    }
    match top {
        Some(top) if has_project => Ok((top, entries)),
        _ => Err(refuse(archive, &format!("no {PROJECT_FILE} inside"))),
    }
}

/// The bundle directory's name and what an entry named `name` is, if it belongs in a bundle:
/// `<name>.debussy/` itself, `project.json`, `versions/` and `versions/*.json`, `audio/` and
/// `audio/*.wav`, each part a plain name. `None` for anything else.
fn classify(name: &str, is_dir: bool) -> Option<(String, Entry)> {
    if name.contains(['\\', '\0']) || name.starts_with('/') {
        return None;
    }
    let trimmed = if is_dir {
        name.strip_suffix('/')?
    } else {
        name
    };
    let parts: Vec<&str> = trimmed.split('/').collect();
    let top = *parts.first()?;
    bundle_stem(top)?;
    let entry = match (&parts[1..], is_dir) {
        ([], true) => Entry::Dir,
        ([sub], true) if *sub == VERSIONS_DIR || *sub == AUDIO_DIR => Entry::Dir,
        ([file], false) if *file == PROJECT_FILE => Entry::File {
            path: PROJECT_FILE.to_owned(),
            json: true,
        },
        ([sub, file], false)
            if is_plain_file_name(file)
                && ((*sub == VERSIONS_DIR && file.ends_with(".json"))
                    || (*sub == AUDIO_DIR && file.ends_with(".wav"))) =>
        {
            Entry::File {
                path: format!("{sub}/{file}"),
                json: *sub == VERSIONS_DIR,
            }
        }
        _ => return None,
    };
    Some((top.to_owned(), entry))
}

/// The name before `.debussy` of a bundle directory named `name`, if `name` is a plain name of
/// that form within the name length limit.
fn bundle_stem(name: &str) -> Option<&str> {
    let stem = name.strip_suffix(BUNDLE_SUFFIX)?;
    (!stem.is_empty() && name.len() <= MAX_NAME_BYTES && is_plain_file_name(name)).then_some(stem)
}

/// The byte limits on one entry being unpacked.
#[derive(Debug, Clone, Copy)]
struct Limits {
    /// The size the archive declares for the entry; more or fewer bytes refuse it.
    declared: u64,
    /// The most the entry may hold.
    entry: u64,
    /// The most all entries together may hold.
    total: u64,
}

/// Copies an entry, `name` in `archive`, from `reader` to a new file `out`, refusing it once it
/// passes any of `limits` or ends short of its declared size; `total` counts the bytes written
/// across entries.
fn copy_limited(
    reader: &mut impl Read,
    out: &Path,
    archive: &Path,
    name: &str,
    limits: Limits,
    total: &mut u64,
) -> Result<(), Error> {
    let out_err = |source| Error::Io {
        path: out.to_owned(),
        source,
    };
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(out)
        .map_err(out_err)?;
    let mut writer = BufWriter::new(file);
    let mut buffer = vec![0u8; 64 * 1024];
    let mut written = 0u64;
    loop {
        let n = reader.read(&mut buffer).map_err(|source| Error::Io {
            path: archive.to_owned(),
            source,
        })?;
        if n == 0 {
            break;
        }
        written += n as u64;
        *total += n as u64;
        if written > limits.declared {
            return Err(refuse(
                archive,
                &format!("{name} holds more than its declared size"),
            ));
        }
        if written > limits.entry {
            return Err(Error::TooLarge {
                path: archive.to_owned(),
                what: "a JSON entry's size",
                limit: limits.entry,
            });
        }
        if *total > limits.total {
            return Err(Error::TooLarge {
                path: archive.to_owned(),
                what: "unpacked size",
                limit: limits.total,
            });
        }
        writer.write_all(&buffer[..n]).map_err(out_err)?;
    }
    if written != limits.declared {
        return Err(refuse(
            archive,
            &format!("{name} is shorter than its declared size"),
        ));
    }
    let file = writer.into_inner().map_err(|e| out_err(e.into_error()))?;
    file.sync_all().map_err(out_err)
}

/// A hidden directory in the projects folder that an archive is unpacked into, deleted when
/// dropped unless [`Unpacking::finish`] renamed it into place.
#[derive(Debug)]
struct Unpacking {
    path: PathBuf,
    done: bool,
}

impl Unpacking {
    /// Creates the directory in `projects_dir`, named after the bundle's `stem`.
    fn create(projects_dir: &Path, stem: &str) -> Result<Unpacking, Error> {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let path = projects_dir.join(format!(".{stem}-{}-{nanos}.unpacking", std::process::id()));
        fs::create_dir(&path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        Ok(Unpacking { path, done: false })
    }

    /// Renames the directory to `dest`.
    fn finish(mut self, dest: &Path) -> Result<(), Error> {
        fs::rename(&self.path, dest).map_err(|source| Error::Io {
            path: dest.to_owned(),
            source,
        })?;
        self.done = true;
        Ok(())
    }
}

impl Drop for Unpacking {
    fn drop(&mut self) {
        if !self.done {
            // Best effort: a leftover hidden directory is harmless, and Open does not list it.
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// `<stem>.debussy` in `dir`, or the first `<stem> <n>.debussy` (from 2) that does not exist.
fn free_bundle_path(dir: &Path, stem: &str) -> Option<PathBuf> {
    let first = dir.join(format!("{stem}{BUNDLE_SUFFIX}"));
    if !first.exists() {
        return Some(first);
    }
    (2..MAX_NAME_TRIES)
        .map(|n| dir.join(format!("{stem} {n}{BUNDLE_SUFFIX}")))
        .find(|p| !p.exists())
}

/// An archive refused for `reason`.
fn refuse(archive: &Path, reason: &str) -> Error {
    Error::Archive {
        path: archive.to_owned(),
        reason: reason.to_owned(),
    }
}

/// A `zip` error on `archive` as this crate's error.
fn zip_error(archive: &Path, error: zip::result::ZipError) -> Error {
    match error {
        zip::result::ZipError::Io(source) => Error::Io {
            path: archive.to_owned(),
            source,
        },
        other => refuse(archive, &other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_accepts_bundle_layout() {
        let file = |path: &str, json| {
            Some((
                "Song.debussy".to_owned(),
                Entry::File {
                    path: path.to_owned(),
                    json,
                },
            ))
        };
        assert_eq!(
            classify("Song.debussy/", true),
            Some(("Song.debussy".to_owned(), Entry::Dir))
        );
        assert_eq!(
            classify("Song.debussy/audio/", true),
            Some(("Song.debussy".to_owned(), Entry::Dir))
        );
        assert_eq!(
            classify("Song.debussy/project.json", false),
            file("project.json", true)
        );
        assert_eq!(
            classify("Song.debussy/versions/a.json", false),
            file("versions/a.json", true)
        );
        assert_eq!(
            classify("Song.debussy/audio/Gtr 1.wav", false),
            file("audio/Gtr 1.wav", false)
        );
    }

    #[test]
    fn test_classify_refuses_unsafe_and_unknown_names() {
        for (name, is_dir) in [
            ("/Song.debussy/project.json", false),
            ("Song.debussy/../evil.json", false),
            ("Song.debussy/audio/../../x.wav", false),
            ("Song.debussy\\project.json", false),
            ("Song.debussy/audio/.hidden.wav", false),
            ("Song.debussy/audio/a.mp3", false),
            ("Song.debussy/cache/x", false),
            ("Song.debussy/lock", false),
            ("Song.debussy/recording.json", false),
            ("Song.debussy/audio/sub/a.wav", false),
            ("Song.debussy//project.json", false),
            ("Song.debussy/project.json", true),
            ("Song.debussy/audio", false),
            ("Song/project.json", false),
            (".debussy/project.json", false),
            ("__MACOSX/Song.debussy/project.json", false),
            ("project.json", false),
        ] {
            assert_eq!(classify(name, is_dir), None, "{name}");
        }
    }

    #[test]
    fn test_zip_time_converts_and_falls_back() -> Result<(), Box<dyn std::error::Error>> {
        // 2026-11-07 15:30:12 UTC.
        let t = UNIX_EPOCH + std::time::Duration::from_secs(1_794_065_412);
        assert_eq!(
            zip_time(Some(t)),
            DateTime::from_date_and_time(2026, 11, 7, 15, 30, 12)?
        );
        assert_eq!(zip_time(Some(UNIX_EPOCH)), DateTime::default());
        assert_eq!(zip_time(None), DateTime::default());
        Ok(())
    }

    #[test]
    fn test_free_bundle_path_numbers_after_existing() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::temp_dir().join(format!("archive-free-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Song.debussy"))?;
        fs::create_dir_all(dir.join("Song 2.debussy"))?;
        assert_eq!(
            free_bundle_path(&dir, "Song"),
            Some(dir.join("Song 3.debussy"))
        );
        assert_eq!(
            free_bundle_path(&dir, "Other"),
            Some(dir.join("Other.debussy"))
        );
        fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
