//! The user interface, in `iced` (`docs/architecture/user-interface.md`, ADR-0003, ADR-0056): a
//! top bar, the tracks as waveform or spectrogram lanes beside the right pane, and a bottom bar
//! with the transport.
//!
//! [`App`] holds the project's [`Session`](project_model::Session), the
//! [`Bridge`](engine_bridge::Bridge) running its routing and the engine's control side; I/O
//! reaches it only through [`Services`], which `debussy` implements. [`run`] opens the window,
//! which opens and creates projects through [`Launcher`], also implemented by `debussy`.
//!
//! # Modules
//!
//! - `app`: [`App`] and [`Message`]: state and update logic, tested without a window.
//! - `view`: drawing the window.
//! - `effect_panel`: a built-in effect's controls, from its parameters' metadata.
//! - `effect_display`: [`EffectDisplay`], the picture above an effect's controls.
//! - `equal_heights`: a row whose children all take its tallest child's height.
//! - `response_display`: the equalisers' display, their frequency response.
//! - `stroke`: anti-aliased strokes drawn as coverage into a mask.
//! - `typeface`: the `Typeface` seam and the in-house stroke font behind it.
//! - `click`: the click, the tempo and the meter.
//! - `grid`: the bars and beats grid's lines over the lanes.
//! - `device`: rebuilding the engine when its audio device is lost.
//! - `range_selection`: the range selected in the lanes.
//! - `recording`: arming, monitoring and recording passes, and [`InputChoice`].
//! - `shortcuts`: keyboard shortcuts.
//! - `waveform`: track lanes from the stored files, and their canvas.
//! - `raster`: lanes painted into memory at the display scale and blurred.
//! - `transport`: zoom steps and the position bar.
//! - `menu`: [`Menu`] and dropdowns.
//! - `patch_board`: the routing laid out as nodes and wires, the port gutter and the divider.
//! - `theme`: the palette and the light and dark themes.
//! - `services`: [`Services`].
//! - `launcher`: [`Launcher`], opening and creating projects.
//! - `shell`: the window's state: the open project, the start screen and switching projects.

#![forbid(unsafe_code)]

mod app;
mod click;
mod device;
mod dynamics_display;
mod effect_display;
mod effect_panel;
mod equal_heights;
mod focus_target;
mod grid;
mod icon;
mod launcher;
mod menu;
mod mixer_strip;
mod patch_board;
mod range_selection;
mod raster;
mod recording;
mod response_display;
mod services;
mod shell;
mod shortcuts;
mod stroke;
mod theme;
mod transport;
mod typeface;
mod view;
mod waveform;

use std::sync::Mutex;

use shell::Shell;

pub use app::{App, DialogFocus, Message, NewNode};
pub use effect_display::{DisplayContext, DisplayParameter, EffectDisplay, Pixels};
pub use launcher::{Launcher, ProjectEntry, Start};
pub use menu::Menu;
pub use mixer_strip::MixerFocus;
pub use patch_board::{Board, Hit, NodeBox, PORT_SPACING, Pin, Selection, Wire};
pub use recording::{InputChoice, METER_FLOOR_DB};
pub use services::{
    ClickPreferences, DeviceChoice, ImportedTrack, MAX_CLICK_LEVEL_DB, MIN_CLICK_LEVEL_DB,
    Services, TakeFiles, TakeStatus,
};
pub use theme::Tone;
pub use transport::MAX_SAMPLES_PER_COLUMN;
pub use waveform::{
    Display, LANE_COLUMNS, Lane, MIN_SAMPLES_PER_COLUMN, SPECTRUM_BANDS, SPECTRUM_COLUMNS,
    Spectrum, spectrum_bands,
};

/// Opens the main window over `app`, or the start screen without one, and runs until it
/// closes; projects are opened and created through `launcher`. The chrome opens dark, whatever
/// the system's appearance, until the user toggles it.
///
/// # Errors
///
/// Whatever stops `iced` from opening the window.
pub fn run(launcher: Box<dyn Launcher>, app: Option<App>) -> iced::Result {
    // iced may ask for the initial state more than once; only the first gets the app and the
    // launcher.
    let initial = Mutex::new(Some((launcher, app)));
    iced::application(
        move || {
            let (launcher, app) = initial
                .lock()
                .ok()
                .and_then(|mut a| a.take())
                .map_or((None, None), |(l, a)| (Some(l), a));
            (
                Shell::new(launcher, app),
                iced::window::latest()
                    .and_then(iced::window::scale_factor)
                    .map(Message::Rescaled),
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
