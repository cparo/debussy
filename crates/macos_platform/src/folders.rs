//! Where the app keeps its files on macOS (`docs/architecture/platform.md` "Folders").

use std::ffi::OsStr;
use std::path::PathBuf;

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
    fn test_folders_need_a_home() {
        assert_eq!(projects_dir_in(OsStr::new("")), None);
        assert_eq!(settings_path_in(OsStr::new("")), None);
    }
}
