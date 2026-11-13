//! [`AppLauncher`]: opening and creating projects for the window, from the projects folder or
//! through file dialogs (`docs/architecture/startup.md` "Application startup",
//! `docs/architecture/project.md` "Projects folder").

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{Context, Result, bail};
use audio_engine::Engine;
use debussy_ui::{App, Launcher, ProjectEntry, Start};
use engine_bridge::Bridge;
use macos_platform::MacDevices;
use project_model::Session;
use project_storage::{AppSettings, Bundle};
use signal_graph::Config;

use crate::{audio, project, services};

/// A project bundle's extension.
const EXTENSION: &str = "debussy";
/// The file every project bundle holds; its modification time is the project's age.
const PROJECT_FILE: &str = "project.json";
/// The most projects the Open browser lists.
const MAX_LISTED: usize = 1000;

/// Opens and creates projects, with the app settings at `settings_path`, keeping new projects
/// in `projects_dir`.
pub struct AppLauncher {
    settings_path: PathBuf,
    projects_dir: Option<PathBuf>,
}

impl AppLauncher {
    /// A launcher whose projects use the app settings at `settings_path`, creating them in and
    /// listing them from `projects_dir`, if there is one.
    pub fn new(settings_path: PathBuf, projects_dir: Option<PathBuf>) -> AppLauncher {
        AppLauncher {
            settings_path,
            projects_dir,
        }
    }

    /// Readies the open `bundle` and its `session`: builds the waveform caches, the bridge and
    /// the engine. The returned [`Start`] does what cannot fail: it loads the app settings,
    /// starts the audio output at the project's rate and builds the [`App`].
    ///
    /// # Errors
    ///
    /// If a waveform cannot be built or the project's routing cannot run.
    pub fn ready(&self, bundle: Bundle, session: Session) -> Result<Start> {
        project::build_waveform_caches(&bundle, &session).context("building waveforms")?;
        let rate = bundle.sample_rate();
        let name = bundle.path().file_stem().map_or_else(
            || "Untitled".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let (bridge, runner) = Bridge::new(session.state(), Config::new(rate, project::MAX_BLOCK))?;
        let output = bridge.output().context("the project has no output")?;
        let (engine, processor) = Engine::new(runner, output, rate)?;
        let settings_path = self.settings_path.clone();
        Ok(Box::new(move || {
            let settings = AppSettings::load(&settings_path);
            let (audio, started) = audio::AudioOutput::start(
                Box::new(MacDevices::new()),
                rate,
                processor,
                settings.device.as_deref(),
            );
            let services = Box::new(services::AppServices::new(
                bundle,
                audio,
                settings,
                settings_path,
            ));
            let mut app = App::new(session, rate, bridge, engine, services);
            app.set_project_name(name);
            if let Err(error) = started {
                // The window still opens; another device can be chosen there.
                app.set_status(format!("No audio output: {error:#}"));
            }
            app
        }))
    }

    /// Opens `path`, or creates it at `create` hertz, refusing a path that exists.
    fn load(&self, path: &Path, create: Option<u32>) -> Result<Start> {
        let opened = match create {
            Some(rate) => {
                if path.exists() {
                    bail!("{} already exists", path.display());
                }
                project::open_or_create(path, rate)?
            }
            None => Bundle::open(path).with_context(|| format!("opening {}", path.display()))?,
        };
        let (bundle, session) = opened;
        self.ready(bundle, session)
    }
}

/// `path` with the bundle extension, added if it has another or none.
fn with_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some_and(|e| e == EXTENSION) {
        path
    } else {
        let mut name = path.clone().into_os_string();
        name.push(".");
        name.push(EXTENSION);
        PathBuf::from(name)
    }
}

/// The projects in `dir`, most recently saved first, at most [`MAX_LISTED`] of them: each
/// directory named `*.debussy` that holds a `project.json`. A missing or unreadable `dir` has
/// none.
fn scan(dir: &Path) -> Vec<ProjectEntry> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let now = SystemTime::now();
    let mut found: Vec<(Option<SystemTime>, ProjectEntry)> = read
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension().is_none_or(|e| e != EXTENSION) {
                return None;
            }
            let project = std::fs::metadata(path.join(PROJECT_FILE)).ok()?;
            if !project.is_file() {
                return None;
            }
            let modified = project.modified().ok();
            let name = path.file_stem()?.to_string_lossy().into_owned();
            // A time in the future counts as just now.
            let age = modified.map(|m| now.duration_since(m).unwrap_or_default());
            Some((modified, ProjectEntry { name, path, age }))
        })
        .collect();
    // Newest first; those of unknown age last, then by name.
    found.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| x.name.cmp(&y.name)));
    found.truncate(MAX_LISTED);
    found.into_iter().map(|(_, entry)| entry).collect()
}

/// The first of `Untitled.debussy`, `Untitled 2.debussy`, `Untitled 3.debussy` and so on not
/// taken in `dir`.
fn next_untitled(dir: &Path) -> PathBuf {
    (1u32..)
        .map(|n| match n {
            1 => dir.join(format!("Untitled.{EXTENSION}")),
            n => dir.join(format!("Untitled {n}.{EXTENSION}")),
        })
        .find(|path| !path.exists())
        .unwrap_or_else(|| dir.join(format!("Untitled.{EXTENSION}")))
}

impl Launcher for AppLauncher {
    fn pick_new(&mut self) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .set_title("New project")
            .add_filter("Debussy project", &[EXTENSION]);
        let mut name = format!("Untitled.{EXTENSION}");
        // Without the folder the dialog starts where it last was.
        if let Some(dir) = &self.projects_dir
            && std::fs::create_dir_all(dir).is_ok()
        {
            if let Some(free) = next_untitled(dir).file_name() {
                name = free.to_string_lossy().into_owned();
            }
            dialog = dialog.set_directory(dir);
        }
        dialog.set_file_name(name).save_file().map(with_extension)
    }

    fn pick_open(&mut self) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new().set_title("Open project");
        if let Some(dir) = self.projects_dir.as_ref().filter(|d| d.is_dir()) {
            dialog = dialog.set_directory(dir);
        }
        dialog.pick_folder()
    }

    fn projects(&mut self) -> Vec<ProjectEntry> {
        self.projects_dir.as_deref().map(scan).unwrap_or_default()
    }

    fn projects_dir(&self) -> Option<PathBuf> {
        self.projects_dir.clone()
    }

    fn prepare(&mut self, path: &Path, create: Option<u32>) -> Result<Start, String> {
        self.load(path, create).map_err(|e| format!("{e:#}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_extension_adds_it_only_when_missing() {
        assert_eq!(
            with_extension(PathBuf::from("/a/Song")),
            PathBuf::from("/a/Song.debussy")
        );
        assert_eq!(
            with_extension(PathBuf::from("/a/Song.debussy")),
            PathBuf::from("/a/Song.debussy")
        );
        assert_eq!(
            with_extension(PathBuf::from("/a/Song.v2")),
            PathBuf::from("/a/Song.v2.debussy")
        );
    }

    #[test]
    fn test_prepare_refuses_to_create_over_an_existing_path() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("debussy-launcher-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let mut launcher = AppLauncher::new(dir.join("settings.json"), None);
        let result = launcher.prepare(&dir, Some(48_000));
        std::fs::remove_dir_all(&dir)?;
        let Err(message) = result else {
            bail!("created over an existing path");
        };
        assert!(message.contains("already exists"), "{message}");
        Ok(())
    }

    #[test]
    fn test_prepare_reports_a_missing_project() {
        let mut launcher = AppLauncher::new(PathBuf::from("settings.json"), None);
        let missing = std::env::temp_dir().join("debussy-launcher-missing.debussy");
        assert!(launcher.prepare(&missing, None).is_err());
        assert!(!missing.exists(), "opening never creates");
    }

    /// A fresh, empty directory for one test, named `name`.
    fn temp_dir(name: &str) -> Result<PathBuf> {
        let dir = std::env::temp_dir().join(format!("debussy-{name}-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(&dir)?;
        Ok(dir)
    }

    /// A bundle named `name` in `dir`, saved `age_secs` seconds ago.
    fn bundle(dir: &Path, name: &str, age_secs: u64) -> Result<()> {
        let path = dir.join(name);
        std::fs::create_dir_all(&path)?;
        let file = std::fs::File::create(path.join(PROJECT_FILE))?;
        let saved = SystemTime::now() - std::time::Duration::from_secs(age_secs);
        file.set_modified(saved)?;
        Ok(())
    }

    #[test]
    fn test_scan_keeps_only_bundles_newest_first() -> Result<()> {
        let dir = temp_dir("scan")?;
        bundle(&dir, "Old.debussy", 3600)?;
        bundle(&dir, "New.debussy", 10)?;
        bundle(&dir, "NotABundle", 0)?;
        std::fs::create_dir_all(dir.join("Empty.debussy"))?;
        std::fs::write(dir.join("File.debussy"), b"")?;
        let entries = scan(&dir);
        std::fs::remove_dir_all(&dir)?;
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["New", "Old"]);
        assert_eq!(entries[0].path, dir.join("New.debussy"));
        let age = entries[1].age.context("no age")?;
        assert!((3600..3700).contains(&age.as_secs()), "{age:?}");
        Ok(())
    }

    #[test]
    fn test_scan_of_a_missing_folder_is_empty() {
        let missing = std::env::temp_dir().join("debussy-scan-missing-folder");
        assert!(scan(&missing).is_empty());
    }

    #[test]
    fn test_next_untitled_skips_taken_names() -> Result<()> {
        let dir = temp_dir("untitled")?;
        let first = next_untitled(&dir);
        std::fs::create_dir_all(dir.join("Untitled.debussy"))?;
        std::fs::create_dir_all(dir.join("Untitled 2.debussy"))?;
        let third = next_untitled(&dir);
        std::fs::remove_dir_all(&dir)?;
        assert_eq!(first, dir.join("Untitled.debussy"));
        assert_eq!(third, dir.join("Untitled 3.debussy"));
        Ok(())
    }
}
