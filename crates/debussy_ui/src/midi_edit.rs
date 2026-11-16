//! Editing MIDI tracks from the track pane (`docs/architecture/track-pane.md` "Quantize",
//! "Instrument choice" and "Key and pitch names"): the quantize dialog over the range selection,
//! each MIDI track's instrument, and its key dialog.

use std::collections::BTreeSet;

use iced::widget::text_input;
use iced::widget::{Space, button, column, container, opaque, row, scrollable, slider, text};
use iced::{Alignment, Element, Fill, Length};
use project_model::notation::{self, MAX_NAME_CHARS, pitch_class_name, pitch_name};
use project_model::{
    Endpoint, Grid, Key, Mode, NodeKind, Notation, Source, State, Track, TrackId, TrackKind,
};

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

/// The key dialog's width, in logical pixels: room for twelve root buttons.
pub(crate) const KEY_DIALOG_WIDTH: f32 = 400.0;

/// The height of the key dialog's list of pitch names, in logical pixels.
const NAME_LIST_HEIGHT: f32 = 240.0;

/// The key's modes as the dialog offers them.
const MODES: [(Option<Mode>, &str); 3] = [
    (Some(Mode::Major), "Major"),
    (Some(Mode::Minor), "Minor"),
    (None, "Indefinite"),
];

/// A track's key and names; C major with none for a track without (an audio track).
fn notation_of(track: &Track) -> Notation {
    track.notation.clone().unwrap_or_default()
}

/// The pitches the key dialog lists for an indefinite track: those its clips play and those
/// already named, lowest first.
pub(crate) fn listed_pitches(track: &Track) -> Vec<u8> {
    let mut pitches: BTreeSet<u8> = track
        .clips
        .iter()
        .filter_map(|(_, clip)| match &clip.source {
            Source::Midi(sequence) => Some(sequence.notes().iter().map(|n| n.pitch)),
            _ => None,
        })
        .flatten()
        .collect();
    if let Some(notation) = &track.notation {
        pitches.extend(notation.names.keys().copied());
    }
    pitches.into_iter().collect()
}

/// The key a mode button picks from `current`: a tonal mode keeps the root (C coming from
/// indefinite).
fn with_mode(current: Key, mode: Option<Mode>) -> Key {
    match (mode, current) {
        (None, _) => Key::Indefinite,
        (Some(mode), Key::Tonal { root, .. }) => Key::Tonal { root, mode },
        (Some(mode), Key::Indefinite) => Key::Tonal { root: 0, mode },
    }
}

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

    /// Sets MIDI track `track`'s key, as one step. Turning indefinite with no names yet names
    /// the General MIDI drums. Allowed while playing: it changes only the display.
    pub(crate) fn set_key(&mut self, track: TrackId, key: Key) {
        let Some(current) = self.session.state().track(track).map(notation_of) else {
            return;
        };
        if current.key == key {
            return;
        }
        let mut notation = Notation { key, ..current };
        if key == Key::Indefinite && notation.names.is_empty() {
            notation.names = notation::gm_drum_names();
        }
        self.apply_notation(track, notation);
    }

    /// Names every General MIDI drum pitch of MIDI track `track` again, dropping its other
    /// names, as one step.
    pub(crate) fn reset_drum_names(&mut self, track: TrackId) {
        let Some(current) = self.session.state().track(track).map(notation_of) else {
            return;
        };
        let names = notation::gm_drum_names();
        if current.names != names {
            self.apply_notation(track, Notation { names, ..current });
        }
    }

    /// Records the pitch name being typed in the key dialog, as one step: trimmed, and an empty
    /// one clears the name. Nothing happens if none is being typed or it is unchanged.
    pub(crate) fn commit_key_name(&mut self) {
        let Some((track, pitch, typed)) = self.key_name.take() else {
            return;
        };
        let Some(mut notation) = self.session.state().track(track).map(notation_of) else {
            return;
        };
        let typed = typed.trim();
        let changed = if typed.is_empty() {
            notation.names.remove(&pitch).is_some()
        } else {
            notation.names.insert(pitch, typed.to_owned()).as_deref() != Some(typed)
        };
        if changed {
            self.apply_notation(track, notation);
        }
    }

    /// Sets `track`'s notation through the bridge, as one step, and redraws the lanes.
    fn apply_notation(&mut self, track: TrackId, notation: Notation) {
        let result = self
            .bridge
            .edit(&mut self.session, |s| s.set_notation(track, notation));
        self.report(result.map_err(|e| e.to_string()));
        self.refresh_lanes();
    }

    /// The key entry of MIDI track `track`'s ≡ menu, naming its key and opening the key dialog.
    pub(crate) fn key_entry(&self, track: TrackId) -> Entry {
        let key = self
            .session
            .state()
            .track(track)
            .map(notation_of)
            .unwrap_or_default()
            .key;
        Entry::item(
            format!("Key: {}…", key.name()),
            Some(Message::OpenMenu(crate::menu::Menu::Key(track))),
        )
    }

    /// The key dialog of MIDI track `track`: Major, Minor or Indefinite; for a tonal key, its
    /// root; for an indefinite one, a name for each pitch the track plays or has named. Every
    /// choice applies at once; a click elsewhere closes it.
    pub(crate) fn key_dialog(&self, track: TrackId) -> Element<'_, Message> {
        let Some(target) = self.session.state().track(track) else {
            return Space::new().into();
        };
        let notation = notation_of(target);
        let key = notation.key;
        let label = |s: &'static str| text(s).size(TOOL_TEXT);
        let modes = MODES
            .into_iter()
            .map(|(mode, name)| {
                let picked = with_mode(key, mode);
                (
                    Face::Label(name),
                    Some(Message::SetKey(track, picked)),
                    picked == key,
                )
            })
            .collect();
        let mut body = column![label("Key"), group(modes)].spacing(6);
        match key {
            Key::Tonal { root, mode } => {
                let roots = (0..12u8)
                    .map(|class| {
                        let picked = Key::Tonal { root: class, mode };
                        (
                            Face::Label(pitch_class_name(class, picked.uses_flats())),
                            Some(Message::SetKey(track, picked)),
                            class == root,
                        )
                    })
                    .collect();
                body = body.push(label("Root")).push(group(roots));
            }
            Key::Indefinite => {
                let rows = listed_pitches(target).into_iter().map(|pitch| {
                    let typed = match &self.key_name {
                        Some((t, p, typed)) if (*t, *p) == (track, pitch) => typed.as_str(),
                        _ => notation.names.get(&pitch).map_or("", String::as_str),
                    };
                    row![
                        text(pitch_name(pitch, &key))
                            .size(TOOL_TEXT)
                            .width(Length::Fixed(48.0)),
                        text_input("unnamed", typed)
                            .on_input(move |s| {
                                Message::KeyNameText(
                                    track,
                                    pitch,
                                    s.chars().take(MAX_NAME_CHARS).collect(),
                                )
                            })
                            .on_submit(Message::CommitKeyName)
                            .size(TOOL_TEXT)
                            .padding([3, 6]),
                    ]
                    .spacing(8)
                    .align_y(Alignment::Center)
                    .into()
                });
                let list = column(rows).spacing(4).padding([0, 12]);
                let reset = button(text("Reset to GM drums").size(TOOL_TEXT))
                    .on_press(Message::ResetDrumNames(track))
                    .padding([6, 12])
                    .style(theme::flat);
                body = body
                    .push(label("Pitch names"))
                    .push(scrollable(list).height(Length::Fixed(NAME_LIST_HEIGHT)))
                    .push(reset);
            }
        }
        let done = button(text("Done").size(TOOL_TEXT))
            .on_press(Message::CloseMenu)
            .padding([6, 12])
            .style(theme::flat);
        let body = body
            .push(row![Space::new().width(Fill), done].align_y(Alignment::Center))
            .spacing(10);
        // Opaque, so a press on the dialog's background does not close it.
        opaque(
            container(body)
                .padding(12)
                .width(Length::Fixed(KEY_DIALOG_WIDTH))
                .style(theme::popup),
        )
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

    #[test]
    fn test_with_mode_keeps_the_root() {
        let d_minor = Key::Tonal {
            root: 2,
            mode: Mode::Minor,
        };
        let d_major = Key::Tonal {
            root: 2,
            mode: Mode::Major,
        };
        assert_eq!(with_mode(d_minor, Some(Mode::Major)), d_major);
        assert_eq!(with_mode(d_minor, None), Key::Indefinite);
        assert_eq!(
            with_mode(Key::Indefinite, Some(Mode::Minor)).name(),
            "C minor"
        );
    }

    #[test]
    fn test_listed_pitches_joins_played_and_named() -> Result<(), project_model::Error> {
        use project_model::{Clip, ClipId, Note, Sequence};
        use std::sync::Arc;
        let note = |pitch| Note {
            start: 0,
            length: 10,
            channel: 9,
            pitch,
            velocity: 100,
        };
        let sequence = Sequence::new(100, vec![note(36), note(38)], Vec::new())?;
        let mut track = Track::new_midi(TrackId(1), "drums");
        track.clips.push((
            ClipId(2),
            Clip::new(Source::Midi(Arc::new(sequence)), 0, 100, 0),
        ));
        assert_eq!(listed_pitches(&track), [36, 38]);
        track.notation = Some(Notation {
            key: Key::Indefinite,
            names: [(40, "rim".to_owned()), (36, "kick".to_owned())].into(),
        });
        assert_eq!(listed_pitches(&track), [36, 38, 40]);
        Ok(())
    }
}
