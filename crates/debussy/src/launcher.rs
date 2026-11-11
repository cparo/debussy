//! [`AppLauncher`]: opening and creating projects for the window, through file dialogs
//! (`docs/architecture/user-interface.md` "Application startup").

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use audio_engine::Engine;
use debussy_ui::{App, Launcher, Start};
use engine_bridge::Bridge;
use macos_platform::MacDevices;
use project_model::Session;
use project_storage::{AppSettings, Bundle};
use signal_graph::Config;

use crate::{audio, project, services};

/// A project bundle's extension.
const EXTENSION: &str = "debussy";

/// Opens and creates projects, with the app settings at `settings_path`.
pub struct AppLauncher {
    settings_path: PathBuf,
}

impl AppLauncher {
    /// A launcher whose projects use the app settings at `settings_path`.
    pub fn new(settings_path: PathBuf) -> AppLauncher {
        AppLauncher { settings_path }
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

impl Launcher for AppLauncher {
    fn pick_new(&mut self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("New project")
            .set_file_name("Untitled.debussy")
            .add_filter("Debussy project", &[EXTENSION])
            .save_file()
            .map(with_extension)
    }

    fn pick_open(&mut self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .set_title("Open project")
            .pick_folder()
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
        let mut launcher = AppLauncher::new(dir.join("settings.json"));
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
        let mut launcher = AppLauncher::new(PathBuf::from("settings.json"));
        let missing = std::env::temp_dir().join("debussy-launcher-missing.debussy");
        assert!(launcher.prepare(&missing, None).is_err());
        assert!(!missing.exists(), "opening never creates");
    }
}
