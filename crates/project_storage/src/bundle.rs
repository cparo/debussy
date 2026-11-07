//! The bundle directory: creating, opening, saving and cleaning up (`docs/architecture/project.md`
//! "Project bundle", "Saving", "Loading", ADR-0013, ADR-0049).

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use project_model::{Session, State};

use crate::error::Error;
use crate::format::SavedProject;
use crate::migrate;
use crate::validate::{self, MAX_NAME_BYTES, SAMPLE_RATES, is_plain_file_name};

/// The largest file this build will read, in bytes.
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// The most named versions a bundle may hold.
pub const MAX_VERSIONS: usize = 10_000;

const PROJECT_FILE: &str = "project.json";
const VERSIONS_DIR: &str = "versions";
const AUDIO_DIR: &str = "audio";
const CACHE_DIR: &str = "cache";
/// The suffix of a file being written; one left by a crash is deleted on open.
const TEMP_SUFFIX: &str = ".tmp";

/// A named version as last saved: its stamp (file name) and its state.
#[derive(Debug, Clone)]
struct SavedVersion {
    stamp: String,
    state: State,
}

/// An open project bundle. It remembers which versions are on disk, so a save writes only what
/// changed.
#[derive(Debug)]
pub struct Bundle {
    path: PathBuf,
    sample_rate: u32,
    versions: BTreeMap<String, SavedVersion>,
}

impl Bundle {
    /// Makes a new bundle at `path` (normally `<name>.debussy`) for a new project at
    /// `sample_rate` (no tracks and a project mixer, ADR-0058), with its `audio/`, `versions/`
    /// and `cache/` directories and a `project.json`.
    ///
    /// # Errors
    ///
    /// [`Error::SampleRate`], [`Error::Exists`], [`Error::Io`].
    pub fn create(path: &Path, sample_rate: u32) -> Result<Bundle, Error> {
        if !SAMPLE_RATES.contains(&sample_rate) {
            return Err(Error::SampleRate(sample_rate));
        }
        if path.exists() {
            return Err(Error::Exists(path.to_owned()));
        }
        for dir in [AUDIO_DIR, VERSIONS_DIR, CACHE_DIR] {
            let dir = path.join(dir);
            fs::create_dir_all(&dir).map_err(io(&dir))?;
        }
        let mut bundle = Bundle {
            path: path.to_owned(),
            sample_rate,
            versions: BTreeMap::new(),
        };
        bundle.save(&Session::new_project())?;
        Ok(bundle)
    }

    /// Opens the bundle at `path`: deletes files left half-written by a crash, reads and checks
    /// `project.json` and every version (a file from elsewhere is untrusted), then deletes audio
    /// files that neither the state nor any version uses.
    ///
    /// # Errors
    ///
    /// [`Error::Io`], [`Error::Json`], [`Error::NewerFormat`], [`Error::TooLarge`],
    /// [`Error::Invalid`].
    pub fn open(path: &Path) -> Result<(Bundle, Session), Error> {
        let versions_dir = path.join(VERSIONS_DIR);
        remove_temp_files(path)?;
        if versions_dir.is_dir() {
            remove_temp_files(&versions_dir)?;
        }
        let project_path = path.join(PROJECT_FILE);
        let saved = read_saved(&project_path)?;
        let sample_rate = saved.sample_rate;
        if saved.name.is_some() || saved.saved_at.is_some() {
            return Err(invalid(&project_path, "project.json names a version"));
        }
        let state = validate::from_saved(saved).map_err(|r| invalid(&project_path, &r))?;

        let mut versions = BTreeMap::new();
        if versions_dir.is_dir() {
            for file in json_files(&versions_dir)? {
                if versions.len() == MAX_VERSIONS {
                    return Err(Error::TooLarge {
                        path: versions_dir,
                        what: "number of versions",
                        limit: MAX_VERSIONS as u64,
                    });
                }
                let (name, version) = read_version(&file, sample_rate)?;
                if versions.insert(name.clone(), version).is_some() {
                    return Err(invalid(&file, &format!("a second version named {name:?}")));
                }
            }
        }
        let session = Session::open(
            state,
            versions
                .iter()
                .map(|(name, v)| (name.clone(), v.state.clone()))
                .collect(),
        );
        let bundle = Bundle {
            path: path.to_owned(),
            sample_rate,
            versions,
        };
        bundle.clean_up(&session)?;
        Ok((bundle, session))
    }

    /// Saves `session`: its current state to `project.json`, each new or changed named version to
    /// `versions/<stamp>.json`, and removes the files of deleted versions. Every file is written
    /// crash-safely: to a temporary file, synced, then renamed over the old one.
    ///
    /// # Errors
    ///
    /// [`Error::Io`], [`Error::Json`].
    pub fn save(&mut self, session: &Session) -> Result<(), Error> {
        let saved = validate::to_saved(session.state(), self.sample_rate);
        write_json(&self.path.join(PROJECT_FILE), &saved)?;

        let versions_dir = self.path.join(VERSIONS_DIR);
        fs::create_dir_all(&versions_dir).map_err(io(&versions_dir))?;
        for (name, state) in session.versions() {
            if self.versions.get(name).is_some_and(|v| v.state == *state) {
                continue;
            }
            let stamp = match self.versions.get(name) {
                Some(version) => version.stamp.clone(),
                None => self.new_stamp(),
            };
            let mut saved = validate::to_saved(state, self.sample_rate);
            saved.name = Some(name.clone());
            saved.saved_at = Some(stamp.clone());
            write_json(&versions_dir.join(format!("{stamp}.json")), &saved)?;
            self.versions.insert(
                name.clone(),
                SavedVersion {
                    stamp,
                    state: state.clone(),
                },
            );
        }
        let removed: Vec<String> = self
            .versions
            .keys()
            .filter(|name| !session.versions().contains_key(*name))
            .cloned()
            .collect();
        for name in removed {
            if let Some(version) = self.versions.remove(&name) {
                let file = versions_dir.join(format!("{}.json", version.stamp));
                fs::remove_file(&file).map_err(io(&file))?;
            }
        }
        sync_dir(&versions_dir)?;
        Ok(())
    }

    /// Deletes every audio file in `audio/` that neither `session`'s current state nor any of its
    /// versions uses, returning their names. Call it only when no undo history can refer to them:
    /// [`Bundle::open`] does, and the application does on closing the project.
    ///
    /// # Errors
    ///
    /// [`Error::Io`].
    pub fn clean_up(&self, session: &Session) -> Result<Vec<String>, Error> {
        let audio = self.audio_dir();
        if !audio.is_dir() {
            return Ok(Vec::new());
        }
        let keep: BTreeSet<&str> = session.referenced_files();
        let mut removed = Vec::new();
        for entry in fs::read_dir(&audio).map_err(io(&audio))? {
            let entry = entry.map_err(io(&audio))?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let is_file = entry.file_type().map_err(io(&audio))?.is_file();
            if is_file
                && is_plain_file_name(&name)
                && name.to_ascii_lowercase().ends_with(".wav")
                && !keep.contains(name.as_str())
            {
                let path = entry.path();
                fs::remove_file(&path).map_err(io(&path))?;
                removed.push(name);
            }
        }
        Ok(removed)
    }

    /// The bundle's directory.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The project sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// The directory of the project's audio files.
    pub fn audio_dir(&self) -> PathBuf {
        self.path.join(AUDIO_DIR)
    }

    /// The directory of derived data, which may be deleted at any time.
    pub fn cache_dir(&self) -> PathBuf {
        self.path.join(CACHE_DIR)
    }

    /// A stamp for a new version: now, in ISO 8601 basic UTC with milliseconds, moved on a
    /// millisecond at a time past any stamp in use.
    fn new_stamp(&self) -> String {
        let mut millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        let used: BTreeSet<&str> = self.versions.values().map(|v| v.stamp.as_str()).collect();
        loop {
            let stamp = stamp(millis);
            let taken = used.contains(stamp.as_str())
                || self
                    .path
                    .join(VERSIONS_DIR)
                    .join(format!("{stamp}.json"))
                    .exists();
            if !taken {
                return stamp;
            }
            millis += 1;
        }
    }
}

/// `millis` since the Unix epoch as an ISO 8601 basic UTC stamp, `YYYYMMDDTHHMMSS.mmmZ`.
pub(crate) fn stamp(millis: u64) -> String {
    let secs = millis / 1000;
    let days = (secs / 86_400) as i64;
    let rest = secs % 86_400;
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}.{:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        millis % 1000
    )
}

/// Whether `text` has the shape of a stamp from [`stamp`].
fn is_stamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 20
        && bytes.iter().enumerate().all(|(i, &b)| match i {
            8 => b == b'T',
            15 => b == b'.',
            19 => b == b'Z',
            _ => b.is_ascii_digit(),
        })
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_owned(),
        source,
    }
}

fn invalid(path: &Path, reason: &str) -> Error {
    Error::Invalid {
        path: path.to_owned(),
        reason: reason.to_owned(),
    }
}

/// Reads and parses a saved file, refusing one over [`MAX_FILE_BYTES`].
fn read_saved(path: &Path) -> Result<SavedProject, Error> {
    let file = File::open(path).map_err(io(path))?;
    let mut text = String::new();
    file.take(MAX_FILE_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(io(path))?;
    if text.len() as u64 > MAX_FILE_BYTES {
        return Err(Error::TooLarge {
            path: path.to_owned(),
            what: "file size",
            limit: MAX_FILE_BYTES,
        });
    }
    migrate::parse(path, &text)
}

/// Reads and checks one version file, returning its name and state.
fn read_version(path: &Path, sample_rate: u32) -> Result<(String, SavedVersion), Error> {
    let saved = read_saved(path)?;
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let (Some(name), Some(stamp)) = (saved.name.clone(), saved.saved_at.clone()) else {
        return Err(invalid(path, "a version needs a name and saved_at"));
    };
    if !is_stamp(&stamp) || stamp != stem {
        return Err(invalid(path, "saved_at does not match the file name"));
    }
    if name.is_empty() || name.len() > MAX_NAME_BYTES {
        return Err(invalid(path, "a version name must be 1 to 1024 bytes"));
    }
    if saved.sample_rate != sample_rate {
        return Err(invalid(
            path,
            "the version's sample rate differs from the project's",
        ));
    }
    let state = validate::from_saved(saved).map_err(|r| invalid(path, &r))?;
    Ok((name, SavedVersion { stamp, state }))
}

/// The `*.json` files directly in `dir`, in name order.
fn json_files(dir: &Path) -> Result<Vec<PathBuf>, Error> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).map_err(io(dir))? {
        let path = entry.map_err(io(dir))?.path();
        if path.is_file() && path.extension().is_some_and(|e| e == "json") {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

/// Deletes files in `dir` left half-written by a crash.
fn remove_temp_files(dir: &Path) -> Result<(), Error> {
    for entry in fs::read_dir(dir).map_err(io(dir))? {
        let path = entry.map_err(io(dir))?.path();
        let temp = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(TEMP_SUFFIX));
        if temp && path.is_file() {
            fs::remove_file(&path).map_err(io(&path))?;
        }
    }
    Ok(())
}

/// Writes `saved` to `path` crash-safely: to `<path>.tmp`, synced, renamed over `path`, then the
/// directory synced, so `path` always holds either the old or the new file.
fn write_json(path: &Path, saved: &SavedProject) -> Result<(), Error> {
    let text = serde_json::to_vec_pretty(saved).map_err(|source| Error::Json {
        path: path.to_owned(),
        source,
    })?;
    let mut temp = path.as_os_str().to_owned();
    temp.push(TEMP_SUFFIX);
    let temp = PathBuf::from(temp);
    let mut file = File::create(&temp).map_err(io(&temp))?;
    file.write_all(&text).map_err(io(&temp))?;
    file.sync_all().map_err(io(&temp))?;
    drop(file);
    fs::rename(&temp, path).map_err(io(path))?;
    if let Some(dir) = path.parent() {
        sync_dir(dir)?;
    }
    Ok(())
}

/// Syncs a directory, so renames and removals in it survive a crash.
fn sync_dir(dir: &Path) -> Result<(), Error> {
    File::open(dir).and_then(|d| d.sync_all()).map_err(io(dir))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stamp_formats_iso_8601_basic_utc() {
        assert_eq!(stamp(0), "19700101T000000.000Z");
        // 2026-11-07 15:30:12.345 UTC.
        assert_eq!(stamp(1_794_065_412_345), "20261107T153012.345Z");
        // A leap day.
        assert_eq!(stamp(951_782_400_000), "20000229T000000.000Z");
        assert!(is_stamp(&stamp(1_794_065_412_345)));
        assert!(!is_stamp("2026-11-07T15:30:12Z"));
    }
}
