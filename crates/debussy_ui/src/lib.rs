//! The user interface, in `iced` (`docs/architecture/user-interface.md`, ADR-0003, ADR-0056): a
//! top bar, the tracks as waveform or spectrogram lanes beside the right pane, and a bottom bar
//! with the transport.
//!
//! [`App`] holds the project's [`Session`](project_model::Session), the
//! [`Bridge`](engine_bridge::Bridge) running its routing and the engine's control side; I/O
//! reaches it only through [`Services`], which `debussy` implements. [`run`] opens the window.
//!
//! # Modules
//!
//! - `app`: [`App`] and [`Message`]: state and update logic, tested without a window.
//! - `view`: drawing the window.
//! - `waveform`: track lanes from the stored files, and their canvas.
//! - `raster`: lanes painted into memory at the display scale and blurred.
//! - `transport`: zoom steps and the position bar.
//! - `menu`: [`Menu`] and dropdowns.
//! - `patch_board`: the routing laid out as nodes and wires, the port gutter and the divider.
//! - `theme`: the palette and the light and dark themes.
//! - `services`: [`Services`].

#![forbid(unsafe_code)]

mod app;
mod menu;
mod patch_board;
mod raster;
mod services;
mod theme;
mod transport;
mod view;
mod waveform;

use std::sync::Mutex;

pub use app::{App, Message, NewNode};
pub use menu::Menu;
pub use patch_board::{Board, Hit, NodeBox, PORT_SPACING, Pin, Selection, Wire};
pub use services::{DeviceChoice, ImportedTrack, Services};
pub use theme::Tone;
pub use transport::MAX_SAMPLES_PER_COLUMN;
pub use waveform::{
    Display, LANE_COLUMNS, Lane, MIN_SAMPLES_PER_COLUMN, SPECTRUM_BANDS, SPECTRUM_COLUMNS,
    Spectrum, spectrum_bands,
};

/// Opens the main window over `app` and runs until it closes. The chrome follows the system's
/// appearance until the user toggles it.
///
/// # Errors
///
/// Whatever stops `iced` from opening the window.
pub fn run(app: App) -> iced::Result {
    // iced may ask for the initial state more than once; only the first gets the app.
    let initial = Mutex::new(Some(app));
    iced::application(
        move || {
            (
                Shell(initial.lock().ok().and_then(|mut a| a.take())),
                iced::Task::batch([
                    iced::system::theme().map(|mode| Message::SystemTone(tone(mode))),
                    iced::window::latest()
                        .and_then(iced::window::scale_factor)
                        .map(Message::Rescaled),
                ]),
            )
        },
        Shell::update,
        Shell::view,
    )
    .title("Debussy")
    .theme(Shell::theme)
    .subscription(Shell::subscription)
    .run()
}

/// The chrome for the system's appearance; dark when it has none.
fn tone(mode: iced::theme::Mode) -> Tone {
    match mode {
        iced::theme::Mode::Light => Tone::Light,
        iced::theme::Mode::Dark | iced::theme::Mode::None => Tone::Dark,
    }
}

/// The running window's state.
struct Shell(Option<App>);

impl Shell {
    fn update(&mut self, message: Message) -> iced::Task<Message> {
        let task = match &message {
            // The app sees Quit first: it finishes a rename and saves it.
            Message::Quit => iced::exit(),
            Message::StartRename(_) => iced::widget::operation::focus(view::RENAME_INPUT),
            _ => iced::Task::none(),
        };
        if let Some(app) = &mut self.0 {
            app.update(message);
        }
        task
    }

    fn view(&self) -> iced::Element<'_, Message> {
        match &self.0 {
            Some(app) => app.view(),
            None => iced::widget::text("Debussy is already open in another window.").into(),
        }
    }

    fn theme(&self) -> iced::Theme {
        theme::theme(self.0.as_ref().map_or(Tone::Dark, App::tone))
    }

    fn subscription(&self) -> iced::Subscription<Message> {
        let appearance = iced::system::theme_changes().map(|mode| Message::SystemTone(tone(mode)));
        let rescaled = iced::window::events().filter_map(|(_, event)| match event {
            iced::window::Event::Rescaled(scale) => Some(Message::Rescaled(scale)),
            _ => None,
        });
        match &self.0 {
            Some(app) if app.wants_ticks() => iced::Subscription::batch([
                appearance,
                rescaled,
                iced::window::frames().map(|_| Message::Tick),
            ]),
            _ => iced::Subscription::batch([appearance, rescaled]),
        }
    }
}
