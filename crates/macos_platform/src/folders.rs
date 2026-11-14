//! Where the app keeps its files on macOS (`docs/architecture/platform.md` "Folders"), and where
//! it looks for a host-installed `lame` (`docs/architecture/import-export.md` "MP3 export").

use std::ffi::OsStr;
use std::path::PathBuf;

/// The package managers' program folders searched for `lame` before `PATH`: Homebrew on Apple
/// silicon, Homebrew on Intel, and MacPorts.
const PACKAGE_BIN_DIRS: [&str; 3] = ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"];

/// Where to look for `lame`, in order: [`PACKAGE_BIN_DIRS`], then each folder on `PATH`. An app
/// launched from the Finder has a minimal `PATH`, so the package folders come first.
pub fn lame_candidates() -> Vec<PathBuf> {
    lame_candidates_in(std::env::var_os("PATH").as_deref())
}

/// The folder new projects go in and Open lists: `~/Music/Debussy`; `None` if `HOME` is not
/// set.
pub fn projects_dir() -> Option<PathBuf> {
    projects_dir_in(std::env::var_os("HOME")?.as_os_str())
}

/// Where the app settings live: `~/Library/Application Support/Debussy/settings.json`; `None` if
/// `HOME` is not set.
pub fn settings_path() -> Option<PathBuf> {
    settings_path_in(std::env::var_os("HOME")?.as_os_str())
}

/// The projects folder under the home folder `home`; `None` if `home` is empty.
fn projects_dir_in(home: &OsStr) -> Option<PathBuf> {
    (!home.is_empty()).then(|| PathBuf::from(home).join("Music").join("Debussy"))
}

/// The settings file under the home folder `home`; `None` if `home` is empty.
fn settings_path_in(home: &OsStr) -> Option<PathBuf> {
    (!home.is_empty()).then(|| {
        PathBuf::from(home)
            .join("Library")
            .join("Application Support")
            .join("Debussy")
            .join("settings.json")
    })
}

/// [`lame_candidates`] for the search path `path`: absolute folders only, without repeats.
fn lame_candidates_in(path: Option<&OsStr>) -> Vec<PathBuf> {
    let on_path = path.into_iter().flat_map(std::env::split_paths);
    let mut candidates: Vec<PathBuf> = Vec::new();
    for dir in PACKAGE_BIN_DIRS.iter().map(PathBuf::from).chain(on_path) {
        let candidate = dir.join("lame");
        // A relative entry such as `.` would depend on the working directory.
        if dir.is_absolute() && !candidates.contains(&candidate) {
            candidates.push(candidate);
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_projects_dir_in_is_music_debussy() {
        assert_eq!(
            projects_dir_in(OsStr::new("/Users/a")),
            Some(PathBuf::from("/Users/a/Music/Debussy"))
        );
    }

    #[test]
    fn test_settings_path_in_is_under_application_support() {
        assert_eq!(
            settings_path_in(OsStr::new("/Users/a")),
            Some(PathBuf::from(
                "/Users/a/Library/Application Support/Debussy/settings.json"
            ))
        );
    }

    #[test]
    fn test_lame_candidates_in_puts_package_folders_before_path() {
        let got = lame_candidates_in(Some(OsStr::new("/usr/bin:/opt/homebrew/bin:.:/bin")));
        let want: Vec<PathBuf> = [
            "/opt/homebrew/bin/lame",
            "/usr/local/bin/lame",
            "/opt/local/bin/lame",
            "/usr/bin/lame",
            "/bin/lame",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        assert_eq!(got, want);
        assert_eq!(lame_candidates_in(None).len(), 3);
    }

    #[test]
    fn test_folders_need_a_home() {
        assert_eq!(projects_dir_in(OsStr::new("")), None);
        assert_eq!(settings_path_in(OsStr::new("")), None);
    }
}
