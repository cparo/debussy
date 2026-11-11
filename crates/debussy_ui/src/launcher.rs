//! [`Launcher`]: how the window opens and creates projects, which `debussy` implements
//! (`docs/architecture/user-interface.md` "Application startup").

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::app::App;

/// The last step of switching to a prepared project: opens the audio output and builds the
/// [`App`]. It cannot fail; a missing audio output shows in the app's status line.
pub type Start = Box<dyn FnOnce() -> App>;

/// A project in the projects folder, as the Open browser lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectEntry {
    /// The project's name: its bundle's name without the extension.
    pub name: String,
    /// The project bundle's path.
    pub path: PathBuf,
    /// How long ago the project was last saved; `None` if unknown.
    pub age: Option<Duration>,
}

/// Opens and creates projects for the window, which does no I/O of its own.
///
/// Switching is split in two so that a project that fails to open leaves the current one
/// untouched: [`Launcher::prepare`] does everything that can fail while the current project
/// still runs, and its [`Start`] runs after the current project's [`App`] (and with it its
/// audio output) has been dropped.
pub trait Launcher {
    /// Asks where to create a new project, through a save dialog; `None` if cancelled.
    fn pick_new(&mut self) -> Option<PathBuf>;

    /// Asks which project to open, through a folder dialog (the Open browser's Browse…);
    /// `None` if cancelled.
    fn pick_open(&mut self) -> Option<PathBuf>;

    /// The projects in the projects folder, most recently saved first; empty if there is no
    /// such folder.
    fn projects(&mut self) -> Vec<ProjectEntry>;

    /// The projects folder, which the Open browser names; `None` if there is none.
    fn projects_dir(&self) -> Option<PathBuf>;

    /// Opens the project at `path`, or with `create` set creates it there at that sample rate
    /// in hertz, refusing a path that exists.
    ///
    /// # Errors
    ///
    /// A message for the user if the project cannot be created or opened.
    fn prepare(&mut self, path: &Path, create: Option<u32>) -> Result<Start, String>;
}
