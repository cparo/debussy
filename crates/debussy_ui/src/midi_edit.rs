//! Editing MIDI tracks from the track pane (`docs/architecture/track-pane.md` "Quantize" and
//! "Instrument choice"): the quantize dialog over the range selection, and each MIDI track's
//! instrument.

use iced::widget::{Space, button, column, container, opaque, row, slider, text};
use iced::{Alignment, Element, Fill, Length};
use project_model::{Endpoint, Grid, NodeKind, State, TrackId, TrackKind};

use crate::app::{App, Message};
use crate::menu::Entry;
use crate::theme;
use crate::view::{Face, TOOL_TEXT, group};

/// The quantize dialog's width, in logical pixels.
pub(crate) const QUANTIZE_DIALOG_WIDTH: f32 = 260.0;

/// The grids the quantize dialog offers, coarsest first, with their labels.
pub(crate) const GRIDS: [(Grid, &str); 4] = [
    (Grid::Bar, "Bar"),
    (Grid::Beat, "Beat"),
    (Grid::Eighth, "1/8"),
    (Grid::Sixteenth, "1/16"),
];

/// The quantize dialog's choices, remembered for the session.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuantizeSettings {
    /// The grid notes move to.
    pub grid: Grid,
    /// How far they move, in percent: 0 to 100 in steps of 5.
    pub strength: f32,
}

impl Default for QuantizeSettings {
    /// Sixteenth notes, all the way.
    fn default() -> Self {
        QuantizeSettings {
            grid: Grid::Sixteenth,
            strength: 100.0,
        }
    }
}

/// The instrument MIDI track `track` feeds: the kind id of the first instrument its event output
/// is connected to.
pub(crate) fn instrument_of(state: &State, track: TrackId) -> Option<&str> {
    state.connections_of(Endpoint::Track(track)).find_map(|c| {
        match &state.node(c.to.node()?)?.kind {
            NodeKind::Instrument { kind, .. } => Some(kind.as_str()),
            _ => None,
        }
    })
}

/// The name shown for instrument kind `kind`: its display name, or the id of one this build
/// does not know.
pub(crate) fn instrument_name(kind: &str) -> &str {
    instruments::display_name(kind).unwrap_or(kind)
}

impl App {
    /// Whether quantize is available: stopped, with a range selected over a MIDI track.
    pub fn can_quantize(&self) -> bool {
        if self.engine.is_playing() {
            return false;
        }
        let state = self.session.state();
        self.selected_range().is_some_and(|(tracks, _)| {
            tracks
                .iter()
                .any(|&id| state.track(id).is_some_and(|t| t.kind == TrackKind::Midi))
        })
    }

    /// The quantize dialog's choices.
    pub fn quantize_settings(&self) -> QuantizeSettings {
        self.quantize
    }

    /// Quantizes the notes in the range selection with the dialog's choices, as one step, and
    /// says how many there were.
    pub(crate) fn quantize(&mut self) {
        let Some((tracks, range)) = self.selected_range() else {
            return;
        };
        if self.refuse_while_playing("quantize") {
            return;
        }
        let QuantizeSettings { grid, strength } = self.quantize;
        let rate = self.sample_rate();
        let result = self.bridge.edit(&mut self.session, |s| {
            s.quantize(&tracks, range, grid, strength / 100.0, rate)
        });
        self.status = Some(match result {
            Ok(0) => "No notes in the selection.".to_owned(),
            Ok(1) => "Quantized 1 note.".to_owned(),
            Ok(n) => format!("Quantized {n} notes."),
            Err(error) => error.to_string(),
        });
        self.refresh_lanes();
    }

    /// Gives MIDI track `track` a new instrument of kind `kind`, wired to the project mixer, as
    /// one step.
    pub(crate) fn set_instrument(&mut self, track: TrackId, kind: &str) {
        if self.refuse_while_playing("change the instrument") {
            return;
        }
        let result = self
            .bridge
            .edit(&mut self.session, |s| s.set_instrument(track, kind));
        self.report(result.map(|_| ()).map_err(|e| e.to_string()));
    }

    /// The instrument entries of MIDI track `track`'s ≡ menu, the one it feeds ticked.
    pub(crate) fn instrument_entries(&self, track: TrackId) -> Vec<Entry> {
        let stopped = !self.engine.is_playing();
        let current = instrument_of(self.session.state(), track);
        instruments::INSTRUMENTS
            .iter()
            .map(|instrument| {
                let tick = if current == Some(instrument.id) {
                    "✓ "
                } else {
                    "    "
                };
                Entry::item(
                    format!("{tick}Instrument: {}", instrument.name),
                    stopped.then_some(Message::SetInstrument(track, instrument.id)),
                )
            })
            .collect()
    }

    /// The quantize dialog: the grid, the strength, then Cancel and Quantize. Its grid and
    /// strength leave it open; a click elsewhere closes it.
    pub(crate) fn quantize_dialog(&self) -> Element<'_, Message> {
        let settings = self.quantize;
        let label = |s: String| text(s).size(TOOL_TEXT);
        let grids = GRIDS
            .into_iter()
            .map(|(grid, name)| {
                (
                    Face::Label(name),
                    Some(Message::QuantizeGrid(grid)),
                    grid == settings.grid,
                )
            })
            .collect();
        let action = |name: &'static str, message: Option<Message>| {
            button(text(name).size(TOOL_TEXT))
                .on_press_maybe(message)
                .padding([6, 12])
                .style(theme::flat)
        };
        let body = column![
            column![label("Grid".to_owned()), group(grids)].spacing(6),
            column![
                label(format!("Strength {:.0} %", settings.strength)),
                slider(0.0..=100.0, settings.strength, Message::QuantizeStrength).step(5.0_f32),
            ]
            .spacing(6),
            row![
                Space::new().width(Fill),
                action("Cancel", Some(Message::CloseMenu)),
                action("Quantize", self.can_quantize().then_some(Message::Quantize)),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        ]
        .spacing(14);
        // Opaque, so a press on the dialog's background does not close it.
        opaque(
            container(body)
                .padding(12)
                .width(Length::Fixed(QUANTIZE_DIALOG_WIDTH))
                .style(theme::popup),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use project_model::Session;

    #[test]
    fn test_instrument_of_finds_the_fed_instrument() -> Result<(), project_model::Error> {
        let mut session = Session::new_project();
        let track = session.add_midi_track("keys")?;
        assert_eq!(instrument_of(session.state(), track), None);
        session.set_instrument(track, "synth")?;
        assert_eq!(instrument_of(session.state(), track), Some("synth"));
        assert_eq!(instrument_name("synth"), "Synthesizer");
        assert_eq!(instrument_name("from-the-future"), "from-the-future");
        Ok(())
    }
}
