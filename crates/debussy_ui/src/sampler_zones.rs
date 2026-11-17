//! A sampler dialog's "Zones" section: one row per zone, and the zones new sample files become
//! (`docs/architecture/node-dialogs.md` "Sampler zones").

use iced::widget::{Space, column, pick_list, row, text};
use iced::{Alignment, Element, Fill, Length};
use project_model::notation::pitch_name;
use project_model::{
    Endpoint, Key, MAX_CHOKE_GROUP, MAX_KEY, NodeId, SampleZone, Source, State, ZoneMode,
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
    }
}

/// The key the sampler's notes are spelled in: that of the first MIDI track feeding it, or
/// sharps.
pub(crate) fn spelling(state: &State, node: NodeId) -> Key {
    state
        .connections
        .iter()
        .filter(|c| c.to == Endpoint::Node(node))
        .find_map(|c| match c.from {
            Endpoint::Track(track) => state.track(track)?.notation.as_ref().map(|n| n.key),
            _ => None,
        })
        .unwrap_or(Key::Indefinite)
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
    key: Key,
    stopped: bool,
) -> Element<'a, Message> {
    let keys: Vec<KeyChoice> = (0..=MAX_KEY)
        .map(|pitch| KeyChoice {
            pitch,
            name: pitch_name(pitch, &key),
        })
        .collect();
    let chokes: Vec<Choke> = std::iter::once(Choke(None))
        .chain((1..=MAX_CHOKE_GROUP).map(|g| Choke(Some(g))))
        .collect();
    let mut rows = column![text("Zones").size(14)].spacing(6);
    if zones.is_empty() {
        rows = rows.push(text("No samples yet: add a sample or a kit.").size(12));
    }
    for (index, zone) in zones.iter().enumerate() {
        let set =
            move |zone: SampleZone| Message::Dialog(DialogMessage::SetZone { node, index, zone });
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
    fn test_sample_zone_is_gated_on_c3() {
        let zone = sample_zone(imported("piano"));
        assert_eq!((zone.low, zone.high, zone.root), (48, 48, 48));
        assert_eq!(zone.mode, ZoneMode::Gated);
        assert_eq!(pitch_name(SAMPLE_KEY, &Key::Indefinite), "C3");
    }
}
