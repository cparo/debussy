//! A sampler dialog's "Zones" section: one row per zone, and the zones new sample files become
//! (`docs/architecture/node-dialogs.md` "Sampler zones").

use iced::widget::{Space, column, pick_list, row, text};
use iced::{Alignment, Element, Fill, Length};
use std::collections::BTreeMap;

use project_model::notation::{GM_DRUMS, pitch_name};
use project_model::{
    Endpoint, Key, MAX_CHOKE_GROUP, MAX_KEY, MAX_ZONE_TUNE, NodeId, Notation, SampleZone, Source,
    State, ZoneMode,
};

use crate::app::Message;
use crate::node_dialog::DialogMessage;
use crate::services::ImportedTrack;
use crate::view::{Face, tool};

/// The key a sample added on its own plays on, and is pitched from: C3.
pub(crate) const SAMPLE_KEY: u8 = 48;

/// The gated zone on [`SAMPLE_KEY`] that a sample added on its own becomes.
pub(crate) fn sample_zone(file: ImportedTrack) -> SampleZone {
    SampleZone {
        file: file.source,
        low: SAMPLE_KEY,
        high: SAMPLE_KEY,
        root: SAMPLE_KEY,
        mode: ZoneMode::Gated,
        choke: None,
        tune: 0,
    }
}

/// The key and pitch names the sampler's keys are named by: those of the first MIDI track
/// feeding it, or an indefinite key with no names (sharps).
pub(crate) fn notation(state: &State, node: NodeId) -> Notation {
    state
        .connections
        .iter()
        .filter(|c| c.to == Endpoint::Node(node))
        .find_map(|c| match c.from {
            Endpoint::Track(track) => state.track(track)?.notation.clone(),
            _ => None,
        })
        .unwrap_or_else(|| Notation {
            key: Key::Indefinite,
            names: BTreeMap::new(),
        })
}

/// The name of `pitch` as a drum, for a one-shot zone's keys: the track's own name for it under
/// an indefinite key, else its General MIDI drum name, else its pitch name.
pub(crate) fn drum_label(pitch: u8, notation: &Notation) -> String {
    if notation.key == Key::Indefinite
        && let Some(name) = notation.names.get(&pitch)
    {
        return name.clone();
    }
    GM_DRUMS.iter().find(|(p, _)| *p == pitch).map_or_else(
        || pitch_name(pitch, &notation.key),
        |(_, name)| (*name).to_owned(),
    )
}

/// A key in a pick list, shown by its pitch name.
#[derive(Debug, Clone, PartialEq, Eq)]
struct KeyChoice {
    pitch: u8,
    name: String,
}

impl std::fmt::Display for KeyChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.name)
    }
}

/// A choke group in a pick list, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Choke(Option<u8>);

impl std::fmt::Display for Choke {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Some(group) => write!(f, "Choke {group}"),
            None => f.write_str("No choke"),
        }
    }
}

/// A zone's tune in a pick list: semitones, signed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tune(i8);

impl std::fmt::Display for Tune {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            0 => f.write_str("0 st"),
            n if n > 0 => write!(f, "+{n} st"),
            n => write!(f, "\u{2212}{} st", n.unsigned_abs()),
        }
    }
}

/// The name shown for a zone's file.
fn file_name(source: &Source) -> String {
    match source {
        Source::Mono(file) => file.name.clone(),
        Source::Stereo { left, .. } => left.name.clone(),
        Source::Midi(_) => String::new(),
    }
}

/// The "Zones" section of sampler `node`'s dialog: a row per zone (file, low and high keys,
/// root, one-shot or gated, choke group, remove), then the buttons that add samples.
pub(crate) fn zone_panel<'a>(
    node: NodeId,
    zones: &[SampleZone],
    notation: &Notation,
    stopped: bool,
) -> Element<'a, Message> {
    let choices = |label: &dyn Fn(u8) -> String| -> Vec<KeyChoice> {
        (0..=MAX_KEY)
            .map(|pitch| KeyChoice {
                pitch,
                name: label(pitch),
            })
            .collect()
    };
    // Gated zones play notes, named as the lanes name pitches; one-shot zones play drums.
    let notes = choices(&|pitch| pitch_name(pitch, &notation.key));
    let drums = choices(&|pitch| drum_label(pitch, notation));
    let chokes: Vec<Choke> = std::iter::once(Choke(None))
        .chain((1..=MAX_CHOKE_GROUP).map(|g| Choke(Some(g))))
        .collect();
    let tunes: Vec<Tune> = (-MAX_ZONE_TUNE..=MAX_ZONE_TUNE).map(Tune).collect();
    let mut rows = column![text("Zones").size(14)].spacing(6);
    if zones.is_empty() {
        rows = rows.push(text("No samples yet: add a sample or a kit.").size(12));
    }
    for (index, zone) in zones.iter().enumerate() {
        let set =
            move |zone: SampleZone| Message::Dialog(DialogMessage::SetZone { node, index, zone });
        let keys = if zone.mode == ZoneMode::OneShot {
            &drums
        } else {
            &notes
        };
        let key_list = |pitch: u8, change: fn(&mut SampleZone, u8)| {
            let base = zone.clone();
            pick_list(
                keys.clone(),
                keys.get(usize::from(pitch)).cloned(),
                move |choice: KeyChoice| {
                    let mut zone = base.clone();
                    change(&mut zone, choice.pitch);
                    set(zone)
                },
            )
            .text_size(12)
            .padding([1, 6])
        };
        let (gated, one_shot) = (zone.mode == ZoneMode::Gated, zone.mode == ZoneMode::OneShot);
        let toggled = SampleZone {
            mode: if gated {
                ZoneMode::OneShot
            } else {
                ZoneMode::Gated
            },
            ..zone.clone()
        };
        let base = zone.clone();
        let tuned = zone.clone();
        let tune_list = pick_list(tunes.clone(), Some(Tune(zone.tune)), move |t: Tune| {
            set(SampleZone {
                tune: t.0,
                ..tuned.clone()
            })
        })
        .text_size(12)
        .padding([1, 6]);
        rows = rows.push(
            row![
                text(file_name(&zone.file)).size(12).width(Length::Fill),
                key_list(zone.low, |z, k| {
                    z.low = k;
                    z.high = z.high.max(k);
                }),
                text("to").size(12),
                key_list(zone.high, |z, k| {
                    z.high = k;
                    z.low = z.low.min(k);
                }),
                text("root").size(12),
                key_list(zone.root, |z, k| z.root = k),
                tune_list,
                tool(
                    Face::Label(if one_shot { "One-shot" } else { "Gated" }),
                    Some(set(toggled)),
                    false,
                ),
                pick_list(chokes.clone(), Some(Choke(zone.choke)), move |c: Choke| {
                    set(SampleZone {
                        choke: c.0,
                        ..base.clone()
                    })
                })
                .text_size(12)
                .padding([1, 6]),
                tool(
                    Face::Label("Remove"),
                    Some(Message::Dialog(DialogMessage::RemoveZone { node, index })),
                    false,
                ),
            ]
            .spacing(6)
            .align_y(Alignment::Center),
        );
    }
    let add = |label: &'static str, kit: bool| {
        tool(
            Face::Label(label),
            stopped.then_some(Message::Dialog(DialogMessage::AddSamples { node, kit })),
            false,
        )
    };
    rows.push(
        row![
            add("Add sample…", false),
            add("Add kit…", true),
            Space::new().width(Fill)
        ]
        .spacing(6),
    )
    .into()
}

#[cfg(test)]
mod tests {
    use audio_buffers::Layout;
    use project_model::AudioFile;

    use super::*;

    fn imported(name: &str) -> ImportedTrack {
        ImportedTrack {
            name: name.to_owned(),
            layout: Layout::Mono,
            source: Source::Mono(AudioFile {
                name: format!("{name}.wav"),
                frames: 10,
            }),
            frames: 10,
        }
    }

    #[test]
    fn test_drum_label_prefers_track_names_then_general_midi_then_pitch() {
        let mut notation = Notation {
            key: Key::Indefinite,
            names: BTreeMap::from([(36, "808 boom".to_owned())]),
        };
        assert_eq!(drum_label(36, &notation), "808 boom");
        assert_eq!(drum_label(38, &notation), "snare");
        assert_eq!(drum_label(46, &notation), "open-hat");
        assert_eq!(drum_label(82, &notation), "A#5");
        // Under a tonal key the track's names are not shown, as on its lanes.
        notation.key = Key::Tonal {
            letter: project_model::Letter::C,
            accidental: project_model::Accidental::Natural,
            mode: project_model::Mode::Major,
        };
        assert_eq!(drum_label(36, &notation), "kick");
    }

    #[test]
    fn test_tune_shows_its_sign_and_unit() {
        assert_eq!(Tune(0).to_string(), "0 st");
        assert_eq!(Tune(3).to_string(), "+3 st");
        assert_eq!(Tune(-12).to_string(), "\u{2212}12 st");
    }

    #[test]
    fn test_sample_zone_is_gated_on_c3() {
        let zone = sample_zone(imported("piano"));
        assert_eq!((zone.low, zone.high, zone.root), (48, 48, 48));
        assert_eq!(zone.mode, ZoneMode::Gated);
        assert_eq!(pitch_name(SAMPLE_KEY, &Key::Indefinite), "C3");
    }
}
