//! The user interface, in `iced` (`docs/architecture/user-interface.md`, ADR-0003, ADR-0056): a
//! transport bar, the tracks as waveform lanes, and the mixer.
//!
//! [`App`] holds the project's [`Session`](project_model::Session), the
//! [`Bridge`](engine_bridge::Bridge) running its routing and the engine's control side; I/O
//! reaches it only through [`Services`], which `debussy` implements. [`run`] opens the window.
//!
//! # Modules
//!
//! - `app`: [`App`] and [`Message`]: state and update logic, tested without a window.
//! - `view`: drawing the window.
//! - `waveform`: waveform lanes from the stored files' summaries, and their canvas.
//! - `services`: [`Services`].

#![forbid(unsafe_code)]

mod app;
mod services;
mod view;
mod waveform;

use std::sync::Mutex;

pub use app::{App, MAX_SAMPLES_PER_COLUMN, Message};
pub use services::{DeviceChoice, ImportedTrack, Services};
pub use waveform::{LANE_COLUMNS, Lane, MIN_SAMPLES_PER_COLUMN};

/// Opens the main window over `app` and runs until it closes.
///
/// # Errors
///
/// Whatever stops `iced` from opening the window.
pub fn run(app: App) -> iced::Result {
    // iced may ask for the initial state more than once; only the first gets the app.
    let initial = Mutex::new(Some(app));
    iced::application(
        move || Shell(initial.lock().ok().and_then(|mut a| a.take())),
        Shell::update,
        Shell::view,
    )
    .title("Debussy")
    .subscription(Shell::subscription)
    .run()
}

/// The running window's state.
struct Shell(Option<App>);

impl Shell {
    fn update(&mut self, message: Message) {
        if let Some(app) = &mut self.0 {
            app.update(message);
        }
    }

    fn view(&self) -> iced::Element<'_, Message> {
        match &self.0 {
            Some(app) => app.view(),
            None => iced::widget::text("Debussy is already open in another window.").into(),
        }
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        match &self.0 {
            Some(app) if app.wants_ticks() => iced::window::frames().map(|_| Message::Tick),
            _ => iced::Subscription::none(),
        }
    }
}
