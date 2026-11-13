//! The running window's state: the open project's [`App`], if any, and switching projects
//! through the [`Launcher`] (`docs/architecture/startup.md` "Application startup").

use std::path::Path;
use std::time::Duration;

use iced::widget::{
    Space, button, column, container, mouse_area, opaque, row, scrollable, stack, text,
};
use iced::{Alignment, Element, Length, Subscription, Task};

use crate::app::{App, Message};
use crate::launcher::{Launcher, ProjectEntry};
use crate::mixer_strip::{self, MixerFocus};
use crate::node_dialog::DialogMessage;
use crate::reveal;
use crate::shortcuts;
use crate::theme::{self, Tone};
use crate::view::{self, Face, RENAME_INPUT, group, tool};

/// The sample rates a new project can have, in hertz, with their labels.
const RATES: [(u32, &str); 4] = [
    (44_100, "44.1 kHz"),
    (48_000, "48 kHz"),
    (88_200, "88.2 kHz"),
    (96_000, "96 kHz"),
];
/// The sample rate a new project has unless another is chosen.
const DEFAULT_RATE: u32 = 48_000;
/// The new project panel's width.
const PANEL_WIDTH: f32 = 420.0;
/// The height of a row in the Open browser's list, in logical pixels.
const ENTRY_HEIGHT: f32 = 28.0;
/// The tallest the Open browser's list grows before it scrolls, in logical pixels.
const LIST_HEIGHT: f32 = 280.0;

/// The running window's state.
pub(crate) struct Shell {
    /// The open project; `None` shows the start screen.
    app: Option<App>,
    /// How projects are opened; `None` in a second window, which can do nothing.
    launcher: Option<Box<dyn Launcher>>,
    /// Whether the new project panel is shown over the open project.
    asking: bool,
    /// The projects folder's projects while the Open browser is shown, or on the start screen.
    browsing: Option<Vec<ProjectEntry>>,
    /// The sample rate chosen for a new project.
    rate: u32,
    /// The start screen's status line.
    status: Option<String>,
    /// The window's scale factor, carried to each project.
    scale: f32,
    /// The chrome's tone, carried to each project.
    tone: Tone,
}

impl Shell {
    /// A window over `app`, if one opened, switching projects through `launcher`.
    pub(crate) fn new(mut launcher: Option<Box<dyn Launcher>>, app: Option<App>) -> Shell {
        // The start screen lists the projects folder.
        let browsing = match (&mut launcher, &app) {
            (Some(launcher), None) => Some(launcher.projects()),
            _ => None,
        };
        Shell {
            app,
            launcher,
            asking: false,
            browsing,
            rate: DEFAULT_RATE,
            status: None,
            scale: 1.0,
            // Dark at launch, whatever the system's appearance.
            tone: Tone::Dark,
        }
    }

    pub(crate) fn update(&mut self, message: Message) -> Task<Message> {
        if self.switching(&message) {
            return Task::none();
        }
        let task = match &message {
            // The app sees Quit first: it finishes a rename and saves it.
            Message::Quit => iced::exit(),
            Message::StartRename(_) => iced::widget::operation::focus(RENAME_INPUT),
            _ => Task::none(),
        };
        let moves_focus = matches!(
            message,
            Message::Dialog(DialogMessage::FocusNext | DialogMessage::FocusPrevious)
        );
        match &mut self.app {
            Some(app) => {
                app.update(message);
                // Tab into a mixer's "Channels" field focuses it; Tab out of it, or through any
                // other dialog control, takes its focus away. Either way the newly focused
                // control is scrolled into view.
                if moves_focus && app.node_dialog_open() {
                    let field = match app.mixer_focus() {
                        Some(MixerFocus::Channels) => mixer_strip::CHANNELS_INPUT,
                        _ => mixer_strip::NO_FIELD,
                    };
                    return iced::widget::operation::focus(field).chain(reveal::reveal());
                }
            }
            None => {
                if let Message::Rescaled(scale) = message
                    && scale.is_finite()
                    && scale > 0.0
                {
                    self.scale = scale;
                }
            }
        }
        task
    }

    /// Handles a message about the new project panel, the Open browser or switching projects,
    /// returning whether it was one.
    fn switching(&mut self, message: &Message) -> bool {
        let switch = matches!(
            message,
            Message::NewProject
                | Message::CreateProject
                | Message::OpenProject
                | Message::OpenListed(_)
                | Message::BrowseProjects
        );
        let panel = matches!(
            message,
            Message::NewProjectRate(_) | Message::CancelNewProject | Message::CancelOpen
        );
        // Escape closes the panel or the browser.
        let escape = self.app.is_some()
            && (self.asking || self.browsing.is_some())
            && *message == Message::ClearRange;
        if !switch && !panel && !escape {
            return false;
        }
        if let Some(app) = &mut self.app {
            // Closes the menu the message came from.
            app.update(message.clone());
            if switch && !app.can_switch_project() {
                app.set_status("Stop and disarm every track to switch projects.".into());
                return true;
            }
        }
        if self.launcher.is_none() {
            return true;
        }
        match message {
            Message::NewProject => {
                self.asking = true;
                self.status = None;
                if self.app.is_some() {
                    self.browsing = None;
                }
            }
            &Message::NewProjectRate(rate) => {
                if RATES.iter().any(|&(r, _)| r == rate) {
                    self.rate = rate;
                }
            }
            Message::CancelNewProject => self.asking = false,
            Message::CancelOpen | Message::ClearRange => {
                self.asking = false;
                // The start screen's list stays.
                if self.app.is_some() {
                    self.browsing = None;
                }
            }
            Message::CreateProject => {
                let path = self.launcher.as_mut().and_then(|l| l.pick_new());
                if let Some(path) = path {
                    self.switch(&path, Some(self.rate));
                }
            }
            Message::OpenProject => {
                self.asking = false;
                self.browsing = self.launcher.as_mut().map(|l| l.projects());
                if self.app.is_none() {
                    self.status = None;
                }
            }
            Message::OpenListed(path) => self.switch(path, None),
            Message::BrowseProjects => {
                let path = self.launcher.as_mut().and_then(|l| l.pick_open());
                if let Some(path) = path {
                    self.switch(&path, None);
                }
            }
            _ => {}
        }
        true
    }

    /// Opens the project at `path`, or creates it at `create` hertz, in place of the open
    /// project. If that fails, the open project carries on and the status line says why.
    fn switch(&mut self, path: &Path, create: Option<u32>) {
        let Some(launcher) = &mut self.launcher else {
            return;
        };
        if let Some(app) = &mut self.app {
            app.close();
            self.tone = app.tone();
            self.scale = app.scale_factor();
        }
        match launcher.prepare(path, create) {
            Ok(start) => {
                // The old project's audio output closes before the new one's opens.
                drop(self.app.take());
                let mut app = start();
                app.set_tone(self.tone);
                app.set_scale_factor(self.scale);
                self.app = Some(app);
                self.asking = false;
                self.browsing = None;
                self.status = None;
            }
            Err(message) => match &mut self.app {
                Some(app) => app.set_status(message),
                None => self.status = Some(message),
            },
        }
    }

    pub(crate) fn view(&self) -> Element<'_, Message> {
        if self.launcher.is_none() && self.app.is_none() {
            return text("Debussy is already open in another window.").into();
        }
        let Some(app) = &self.app else {
            let mut screen = column![self.panel(false)].spacing(12);
            if let Some(entries) = &self.browsing {
                screen = screen.push(self.browser(entries, false));
            }
            return container(screen)
                .center(Length::Fill)
                .style(theme::chrome)
                .into();
        };
        let (modal, cancel) = match (&self.browsing, self.asking) {
            (Some(entries), _) => (self.browser(entries, true), Message::CancelOpen),
            (None, true) => (self.panel(true), Message::CancelNewProject),
            (None, false) => return app.view(),
        };
        stack![
            app.view(),
            // A press outside the modal cancels it.
            mouse_area(
                container(opaque(modal))
                    .center(Length::Fill)
                    .style(theme::scrim)
            )
            .on_press(cancel),
        ]
        .into()
    }

    /// The Open browser: the projects folder's `entries` with Browse…, beneath the start
    /// screen's panel or as a modal with Cancel.
    fn browser<'a>(&'a self, entries: &'a [ProjectEntry], modal: bool) -> Element<'a, Message> {
        let folder = self
            .launcher
            .as_ref()
            .and_then(|l| l.projects_dir())
            .map(|dir| abbreviate(&dir, std::env::var_os("HOME").as_deref().map(Path::new)));
        let list: Element<'a, Message> = if entries.is_empty() {
            let place = folder.as_deref().unwrap_or("the projects folder");
            text(format!("No projects in {place} yet"))
                .size(view::TOOL_TEXT)
                .into()
        } else {
            let rows = entries.iter().map(|entry| {
                button(
                    row![
                        text(entry.name.as_str()).size(view::TOOL_TEXT),
                        Space::new().width(Length::Fill),
                        text(age_label(entry.age)).size(view::TOOL_TEXT),
                    ]
                    .spacing(10)
                    .align_y(Alignment::Center),
                )
                .on_press(Message::OpenListed(entry.path.clone()))
                .width(Length::Fill)
                .height(Length::Fixed(ENTRY_HEIGHT))
                .style(theme::flat)
                .into()
            });
            // Rows shown as a whole, up to the list's height.
            let height = (ENTRY_HEIGHT * entries.len() as f32).min(LIST_HEIGHT);
            scrollable(column(rows))
                .height(Length::Fixed(height))
                .width(Length::Fill)
                .into()
        };
        let mut buttons = row![
            text(folder.unwrap_or_default()).size(view::TOOL_TEXT),
            Space::new().width(Length::Fill),
        ]
        .spacing(6)
        .align_y(Alignment::Center);
        if modal {
            buttons = buttons.push(tool(
                Face::Label("Cancel"),
                Some(Message::CancelOpen),
                false,
            ));
        }
        buttons = buttons.push(tool(
            Face::Label("Browse…"),
            Some(Message::BrowseProjects),
            false,
        ));
        let title = if modal { "Open project" } else { "Projects" };
        let body = column![text(title).size(18), list, buttons].spacing(16);
        container(body)
            .padding(16)
            .width(Length::Fixed(PANEL_WIDTH))
            .style(theme::popup)
            .into()
    }

    /// The new project panel: the start screen's with no project open, otherwise a modal with
    /// Cancel.
    fn panel(&self, modal: bool) -> Element<'_, Message> {
        let rates = RATES
            .iter()
            .map(|&(rate, label)| {
                (
                    Face::Label(label),
                    Some(Message::NewProjectRate(rate)),
                    rate == self.rate,
                )
            })
            .collect();
        let rate_row = row![
            text("Sample rate").size(view::TOOL_TEXT),
            Space::new().width(Length::Fill),
            group(rates),
        ]
        .spacing(10)
        .align_y(Alignment::Center);
        let buttons = if modal {
            row![
                Space::new().width(Length::Fill),
                tool(
                    Face::Label("Cancel"),
                    Some(Message::CancelNewProject),
                    false
                ),
                tool(Face::Label("Create…"), Some(Message::CreateProject), true),
            ]
        } else {
            row![
                Space::new().width(Length::Fill),
                tool(
                    Face::Label("New project…"),
                    Some(Message::CreateProject),
                    true
                ),
            ]
        }
        .spacing(6);
        let title = if modal { "New project" } else { "Debussy" };
        let mut body = column![text(title).size(18), rate_row, buttons].spacing(16);
        if let Some(status) = &self.status {
            body = body.push(text(status.as_str()).size(view::TOOL_TEXT));
        }
        container(body)
            .padding(16)
            .width(Length::Fixed(PANEL_WIDTH))
            .style(theme::popup)
            .into()
    }

    pub(crate) fn theme(&self) -> iced::Theme {
        theme::theme(self.app.as_ref().map_or(self.tone, App::tone))
    }

    pub(crate) fn subscription(&self) -> Subscription<Message> {
        let rescaled = iced::window::events().filter_map(|(_, event)| match event {
            iced::window::Event::Rescaled(scale) => Some(Message::Rescaled(scale)),
            _ => None,
        });
        let keys = iced::keyboard::listen().filter_map(shortcuts::shortcut);
        let mut subscriptions = vec![rescaled, keys];
        if let Some(app) = &self.app {
            if app.wants_ticks() {
                subscriptions.push(iced::window::frames().map(|_| Message::Tick));
            }
            // A drag from a lane ends wherever the button is released.
            if app.is_dragging() {
                subscriptions.push(iced::event::listen_with(|event, _, _| match event {
                    iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                        iced::mouse::Button::Left,
                    )) => Some(Message::LaneRelease),
                    _ => None,
                }));
            }
        }
        Subscription::batch(subscriptions)
    }
}

/// `path` with the home folder `home`, if it starts with it, written as `~`.
fn abbreviate(path: &Path, home: Option<&Path>) -> String {
    match home.and_then(|home| path.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// How long ago a project was saved, `age`, in words: "just now", "5 min ago", "3 h ago" or
/// "12 days ago"; empty if unknown.
fn age_label(age: Option<Duration>) -> String {
    const MINUTE: u64 = 60;
    const HOUR: u64 = 60 * MINUTE;
    const DAY: u64 = 24 * HOUR;
    let Some(age) = age else {
        return String::new();
    };
    match age.as_secs() {
        s if s < MINUTE => "just now".to_owned(),
        s if s < HOUR => format!("{} min ago", s / MINUTE),
        s if s < DAY => format!("{} h ago", s / HOUR),
        s if s < 2 * DAY => "1 day ago".to_owned(),
        s => format!("{} days ago", s / DAY),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::error::Error;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    use audio_analysis::Bucket;
    use audio_engine::{Engine, Processor, Streams, TrackFeeds};
    use engine_bridge::Bridge;
    use project_model::{Session, State, TrackId};
    use signal_graph::{Config, NodeId};

    use crate::launcher::Start;
    use crate::services::{
        ClickPreferences, DeviceChoice, ImportedTrack, Services, TakeFiles, TakeStatus,
    };
    use crate::waveform::Spectrum;

    struct NoStreams;

    impl Streams for NoStreams {
        fn seek(&mut self, _frame: u64) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }
    }

    /// Services that do nothing, holding the engine's real-time side.
    struct Quiet {
        streams: NoStreams,
        _processor: Processor,
    }

    impl Services for Quiet {
        fn streams(&mut self) -> &mut dyn Streams {
            &mut self.streams
        }
        fn track_feeds(
            &mut self,
            _: &State,
            _: &BTreeMap<TrackId, NodeId>,
        ) -> Result<Vec<TrackFeeds>, String> {
            Ok(Vec::new())
        }
        fn waveform(&mut self, _: &str, _: u64, _: u64, _: &mut [Bucket]) -> usize {
            0
        }
        fn samples(&mut self, _: &str, _: u64, _: &mut [f32]) -> usize {
            0
        }
        fn spectrogram(&mut self, _: &str, _: u64, _: u64, _: &mut [Spectrum]) -> usize {
            0
        }
        fn save(&mut self, _: &Session) -> Result<(), String> {
            Ok(())
        }
        fn import(&mut self) -> Result<Vec<ImportedTrack>, String> {
            Ok(Vec::new())
        }
        fn export(&mut self, _: &State, _: bool) -> Result<Option<String>, String> {
            Ok(None)
        }
        fn devices(&mut self) -> (Vec<DeviceChoice>, Option<String>) {
            (Vec::new(), None)
        }
        fn audio_error(&mut self) -> Option<String> {
            None
        }
        fn audio_lost(&mut self) -> bool {
            false
        }
        fn adopt_engine(&mut self, _: Processor, _: Option<&str>) -> Result<(), String> {
            Ok(())
        }
        fn set_device(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn set_input(&mut self, _: bool) -> Result<(), String> {
            Ok(())
        }
        fn input_channels(&mut self) -> usize {
            0
        }
        fn device_latency(&mut self) -> u64 {
            0
        }
        fn start_takes(&mut self, _: &[(String, usize)]) -> Result<Vec<TakeFiles>, String> {
            Err("no recording here".into())
        }
        fn take_status(&mut self) -> Vec<TakeStatus> {
            Vec::new()
        }
        fn take_overruns(&mut self) -> u64 {
            0
        }
        fn keep_takes(&mut self, _: &[String]) -> Result<(), String> {
            Ok(())
        }
        fn discard_takes(&mut self, _: &[String]) {}
        fn click_preferences(&mut self) -> ClickPreferences {
            ClickPreferences::default()
        }
        fn set_click_preferences(&mut self, _: ClickPreferences) -> Result<(), String> {
            Ok(())
        }
    }

    /// An empty project's app at `rate`.
    fn app(rate: u32) -> Result<App, Box<dyn Error>> {
        let session = Session::new_project();
        let (bridge, runner) = Bridge::new(session.state(), Config::new(rate, 512))?;
        let output = bridge.output().ok_or("no output")?;
        let (engine, processor) = Engine::new(runner, output, rate)?;
        let services = Box::new(Quiet {
            streams: NoStreams,
            _processor: processor,
        });
        Ok(App::new(session, rate, bridge, engine, services))
    }

    /// What the fake launcher was asked, and what it answers.
    #[derive(Default)]
    struct Calls {
        /// The path each dialog picks; `None` cancels it.
        pick: Option<PathBuf>,
        /// Whether preparing fails.
        fails: bool,
        /// The projects folder's projects.
        listed: Vec<ProjectEntry>,
        dialogs: Vec<&'static str>,
        prepared: Vec<(PathBuf, Option<u32>)>,
    }

    struct FakeLauncher(Rc<RefCell<Calls>>);

    impl Launcher for FakeLauncher {
        fn pick_new(&mut self) -> Option<PathBuf> {
            let mut calls = self.0.borrow_mut();
            calls.dialogs.push("new");
            calls.pick.clone()
        }
        fn pick_open(&mut self) -> Option<PathBuf> {
            let mut calls = self.0.borrow_mut();
            calls.dialogs.push("open");
            calls.pick.clone()
        }
        fn projects(&mut self) -> Vec<ProjectEntry> {
            let mut calls = self.0.borrow_mut();
            calls.dialogs.push("list");
            calls.listed.clone()
        }
        fn projects_dir(&self) -> Option<PathBuf> {
            Some(PathBuf::from("/Users/a/Music/Debussy"))
        }
        fn prepare(&mut self, path: &Path, create: Option<u32>) -> Result<Start, String> {
            let mut calls = self.0.borrow_mut();
            calls.prepared.push((path.to_owned(), create));
            if calls.fails {
                return Err("no such project".into());
            }
            // An opened project is at 44.1 kHz.
            let rate = create.unwrap_or(44_100);
            Ok(Box::new(move || {
                app(rate).unwrap_or_else(|e| panic!("building the app failed: {e}"))
            }))
        }
    }

    fn shell(app: Option<App>, calls: Calls) -> (Shell, Rc<RefCell<Calls>>) {
        let calls = Rc::new(RefCell::new(calls));
        let launcher = Box::new(FakeLauncher(Rc::clone(&calls)));
        (Shell::new(Some(launcher), app), calls)
    }

    fn picking(path: &str) -> Calls {
        Calls {
            pick: Some(PathBuf::from(path)),
            ..Calls::default()
        }
    }

    #[test]
    fn test_update_creates_a_project_at_the_chosen_rate_from_the_start_screen() {
        let (mut shell, calls) = shell(None, picking("New.debussy"));
        let _ = shell.update(Message::NewProjectRate(44_100));
        let _ = shell.update(Message::CreateProject);
        assert_eq!(
            calls.borrow().prepared,
            vec![(PathBuf::from("New.debussy"), Some(44_100))]
        );
        assert_eq!(shell.app.as_ref().map(App::sample_rate), Some(44_100));
    }

    #[test]
    fn test_update_ignores_a_rate_not_offered() {
        let (mut shell, _) = shell(None, Calls::default());
        let _ = shell.update(Message::NewProjectRate(12_345));
        assert_eq!(shell.rate, DEFAULT_RATE);
    }

    #[test]
    fn test_update_cancel_and_cancelled_dialogs_change_nothing() -> Result<(), Box<dyn Error>> {
        let (mut shell, calls) = shell(Some(app(48_000)?), Calls::default());
        let _ = shell.update(Message::NewProject);
        assert!(shell.asking);
        let _ = shell.update(Message::CancelNewProject);
        assert!(!shell.asking);
        let _ = shell.update(Message::NewProject);
        let _ = shell.update(Message::ClearRange);
        assert!(!shell.asking, "Escape closes the panel");
        let _ = shell.update(Message::CreateProject);
        let _ = shell.update(Message::BrowseProjects);
        assert_eq!(calls.borrow().dialogs, vec!["new", "open"]);
        assert!(calls.borrow().prepared.is_empty());
        assert_eq!(shell.app.as_ref().map(App::sample_rate), Some(48_000));
        Ok(())
    }

    #[test]
    fn test_update_failed_open_keeps_the_app_and_says_why() -> Result<(), Box<dyn Error>> {
        let calls = Calls {
            fails: true,
            ..picking("Missing.debussy")
        };
        let (mut shell, _) = shell(Some(app(48_000)?), calls);
        let _ = shell.update(Message::BrowseProjects);
        let app = shell.app.as_ref().ok_or("the app was dropped")?;
        assert_eq!(app.sample_rate(), 48_000);
        assert_eq!(app.status(), Some("no such project"));
        Ok(())
    }

    #[test]
    fn test_update_failed_create_on_the_start_screen_shows_status() {
        let calls = Calls {
            fails: true,
            ..picking("New.debussy")
        };
        let (mut shell, _) = shell(None, calls);
        let _ = shell.update(Message::CreateProject);
        assert!(shell.app.is_none());
        assert_eq!(shell.status.as_deref(), Some("no such project"));
    }

    #[test]
    fn test_update_open_replaces_the_app_keeping_tone_and_scale() -> Result<(), Box<dyn Error>> {
        let (mut shell, calls) = shell(Some(app(48_000)?), picking("Song.debussy"));
        let _ = shell.update(Message::ToggleTone);
        let _ = shell.update(Message::Rescaled(2.0));
        let _ = shell.update(Message::BrowseProjects);
        assert_eq!(
            calls.borrow().prepared,
            vec![(PathBuf::from("Song.debussy"), None)]
        );
        let app = shell.app.as_ref().ok_or("no app")?;
        assert_eq!(app.sample_rate(), 44_100);
        assert_eq!(app.tone(), Tone::Light);
        assert_eq!(app.scale_factor(), 2.0);
        Ok(())
    }

    #[test]
    fn test_update_refuses_to_switch_while_playing() -> Result<(), Box<dyn Error>> {
        let (mut shell, calls) = shell(Some(app(48_000)?), picking("Song.debussy"));
        let _ = shell.update(Message::Play);
        let _ = shell.update(Message::OpenProject);
        let _ = shell.update(Message::BrowseProjects);
        assert!(calls.borrow().dialogs.is_empty());
        assert!(shell.browsing.is_none(), "the browser stays closed");
        assert!(shell.app.as_ref().and_then(App::status).is_some());
        Ok(())
    }

    fn listing() -> Calls {
        let entry = |name: &str, minutes: u64| ProjectEntry {
            name: name.to_owned(),
            path: PathBuf::from(format!("/Users/a/Music/Debussy/{name}.debussy")),
            age: Some(Duration::from_secs(minutes * 60)),
        };
        Calls {
            listed: vec![entry("Newer", 1), entry("Older", 90)],
            ..Calls::default()
        }
    }

    #[test]
    fn test_update_open_project_lists_the_projects() -> Result<(), Box<dyn Error>> {
        let (mut shell, calls) = shell(Some(app(48_000)?), listing());
        let _ = shell.update(Message::OpenProject);
        let names: Vec<_> = shell
            .browsing
            .iter()
            .flatten()
            .map(|e| e.name.as_str())
            .collect();
        assert_eq!(names, vec!["Newer", "Older"]);
        assert_eq!(calls.borrow().dialogs, vec!["list"]);
        Ok(())
    }

    #[test]
    fn test_update_open_listed_switches_to_that_path() -> Result<(), Box<dyn Error>> {
        let (mut shell, calls) = shell(Some(app(48_000)?), listing());
        let _ = shell.update(Message::OpenProject);
        let path = PathBuf::from("/Users/a/Music/Debussy/Older.debussy");
        let _ = shell.update(Message::OpenListed(path.clone()));
        assert_eq!(calls.borrow().prepared, vec![(path, None)]);
        assert_eq!(shell.app.as_ref().map(App::sample_rate), Some(44_100));
        assert!(shell.browsing.is_none(), "the browser closes");
        Ok(())
    }

    #[test]
    fn test_update_cancelled_browse_changes_nothing() -> Result<(), Box<dyn Error>> {
        let (mut shell, calls) = shell(Some(app(48_000)?), listing());
        let _ = shell.update(Message::OpenProject);
        let _ = shell.update(Message::BrowseProjects);
        assert_eq!(calls.borrow().dialogs, vec!["list", "open"]);
        assert!(calls.borrow().prepared.is_empty());
        assert!(shell.browsing.is_some(), "the browser stays open");
        assert_eq!(shell.app.as_ref().map(App::sample_rate), Some(48_000));
        Ok(())
    }

    #[test]
    fn test_update_cancel_and_escape_close_the_browser() -> Result<(), Box<dyn Error>> {
        let (mut shell, _) = shell(Some(app(48_000)?), listing());
        let _ = shell.update(Message::OpenProject);
        let _ = shell.update(Message::CancelOpen);
        assert!(shell.browsing.is_none());
        let _ = shell.update(Message::OpenProject);
        let _ = shell.update(Message::ClearRange);
        assert!(shell.browsing.is_none(), "Escape closes the browser");
        Ok(())
    }

    #[test]
    fn test_new_start_screen_lists_the_projects_and_keeps_them() {
        let (mut shell, _) = shell(None, listing());
        assert_eq!(shell.browsing.as_ref().map(Vec::len), Some(2));
        let _ = shell.update(Message::ClearRange);
        let _ = shell.update(Message::NewProject);
        assert_eq!(shell.browsing.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn test_age_label_formats_each_unit() {
        let label = |secs: u64| age_label(Some(Duration::from_secs(secs)));
        assert_eq!(age_label(None), "");
        assert_eq!(label(0), "just now");
        assert_eq!(label(59), "just now");
        assert_eq!(label(5 * 60), "5 min ago");
        assert_eq!(label(3 * 3600 + 59), "3 h ago");
        assert_eq!(label(30 * 3600), "1 day ago");
        assert_eq!(label(12 * 86_400), "12 days ago");
    }

    #[test]
    fn test_abbreviate_writes_home_as_tilde() {
        let home = Path::new("/Users/a");
        let dir = Path::new("/Users/a/Music/Debussy");
        assert_eq!(abbreviate(dir, Some(home)), "~/Music/Debussy");
        assert_eq!(abbreviate(home, Some(home)), "~");
        assert_eq!(
            abbreviate(dir, Some(Path::new("/Users/b"))),
            "/Users/a/Music/Debussy"
        );
        assert_eq!(abbreviate(dir, None), "/Users/a/Music/Debussy");
    }

    #[test]
    fn test_update_second_window_does_nothing() {
        let mut shell = Shell::new(None, None);
        let _ = shell.update(Message::CreateProject);
        assert!(shell.app.is_none());
    }
}
