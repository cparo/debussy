//! MIDI output nodes in the interface: the track menu's "MIDI Out" choices, the node dialog's
//! endpoint and latency, the lost-endpoint marks, and keeping the engine's routes and the
//! services' senders in step with the project (`docs/architecture/engine.md` "MIDI output",
//! `docs/architecture/track-pane.md` "Instrument choice", ADR-0162).

use std::collections::BTreeSet;

use iced::widget::{column, pick_list, row, text, text_input};
use iced::{Alignment, Element, Length};
use project_model::{Destination, Endpoint, NodeId, NodeKind, State, TrackId};

use crate::app::{App, Message};
use crate::menu::Entry;
use crate::services::{MidiOutRoute, MidiOutputChoice};

/// The largest latency the dialog accepts, in milliseconds; the app settings' limit.
const MAX_LATENCY_MS: f32 = 1000.0;

/// The MIDI output node MIDI track `track` feeds first, with its endpoint id and name.
pub(crate) fn midi_out_of(state: &State, track: TrackId) -> Option<(NodeId, &str, &str)> {
    state.connections_of(Endpoint::Track(track)).find_map(|c| {
        let node = c.to.node()?;
        match &state.node(node)?.kind {
            NodeKind::MidiOut { port, name } => Some((node, port.as_str(), name.as_str())),
            _ => None,
        }
    })
}

/// Every MIDI output node in `state` that `sinks` holds an event sink for, with its endpoint.
fn routes(
    state: &State,
    sinks: &std::collections::BTreeMap<NodeId, signal_graph::NodeId>,
) -> Vec<MidiOutRoute> {
    state
        .nodes
        .iter()
        .filter_map(|node| match &node.kind {
            NodeKind::MidiOut { port, .. } => Some(MidiOutRoute {
                node: node.id,
                sink: *sinks.get(&node.id)?,
                port: port.clone(),
            }),
            _ => None,
        })
        .collect()
}

impl App {
    /// Sends MIDI track `track`'s notes to a new MIDI output node on `choice`, replacing the
    /// instrument or output it fed, as one step.
    pub(crate) fn set_midi_out(&mut self, track: TrackId, choice: &MidiOutputChoice) {
        if self.refuse_while_playing("change the instrument") {
            return;
        }
        let destination = Destination::MidiOut {
            port: choice.id.clone(),
            name: choice.name.clone(),
        };
        let result = self
            .bridge
            .edit(&mut self.session, |s| s.set_destination(track, destination));
        self.report(result.map(|_| ()).map_err(|e| e.to_string()));
    }

    /// Points MIDI output node `node` at `choice`, as one step.
    pub(crate) fn set_midi_port(&mut self, node: NodeId, choice: &MidiOutputChoice) {
        let edit = project_model::Edit::SetMidiPort {
            node,
            port: choice.id.clone(),
            name: choice.name.clone(),
        };
        let result = self.bridge.edit(&mut self.session, |s| s.apply(edit));
        self.report(result.map_err(|e| e.to_string()));
    }

    /// A new MIDI output node's kind: on the first endpoint present, or on none.
    pub(crate) fn first_midi_out(&mut self) -> NodeKind {
        self.midi_outputs = self.services.midi_outputs();
        let first = self.midi_outputs.first();
        NodeKind::MidiOut {
            port: first.map(|c| c.id.clone()).unwrap_or_default(),
            name: first.map(|c| c.name.clone()).unwrap_or_default(),
        }
    }

    /// Remembers `typed`, in milliseconds, as endpoint `port`'s latency in the app settings.
    pub(crate) fn set_midi_out_latency(&mut self, port: &str, typed: &str) {
        let ms = match typed.trim().parse::<f32>() {
            Ok(ms) if ms.is_finite() && (0.0..=MAX_LATENCY_MS).contains(&ms) => ms,
            _ => {
                self.status = Some(format!(
                    "{typed:?} is not a latency from 0 to {MAX_LATENCY_MS} ms"
                ));
                return;
            }
        };
        match self.services.set_midi_output_latency(port, ms) {
            Ok(()) => {
                self.midi_out_latency.insert(port.to_owned(), ms);
            }
            Err(message) => self.status = Some(message),
        }
    }

    /// Brings the engine's MIDI output routes and the services' senders in step with the MIDI
    /// output nodes the running graph holds, and reads which endpoints are lost.
    pub(crate) fn sync_midi_outs(&mut self) {
        let wanted = routes(self.session.state(), &self.bridge.event_sinks());
        if wanted != self.midi_out_routes {
            for old in &self.midi_out_routes {
                if !wanted.iter().any(|r| r.sink == old.sink) {
                    let _ = self.engine.unroute_midi_out(old.sink);
                }
            }
            let mut routed = wanted.clone();
            for (sink, producer) in self.services.sync_midi_outputs(&wanted) {
                if let Err(error) = self.engine.route_midi_out(sink, producer) {
                    // Left out, so the next update tries again.
                    routed.retain(|r| r.sink != sink);
                    self.status = Some(format!("MIDI output not connected: {error}"));
                }
            }
            if routed.len() != wanted.len() {
                self.services.sync_midi_outputs(&routed);
            }
            for route in &routed {
                let ms = self.services.midi_output_latency(&route.port);
                self.midi_out_latency.insert(route.port.clone(), ms);
            }
            self.midi_out_routes = routed;
        }
        self.midi_out_lost = self
            .midi_out_routes
            .iter()
            .map(|r| r.node)
            .filter(|&node| self.services.midi_output_lost(node))
            .collect();
    }

    /// Forgets the MIDI output routes the engine had, closing their senders: the engine has
    /// been rebuilt. The next update routes them again.
    pub(crate) fn forget_midi_outs(&mut self) {
        self.midi_out_routes.clear();
        self.services.sync_midi_outputs(&[]);
        self.sync_midi_outs();
    }

    /// MIDI output nodes whose endpoint is lost, or has no id (none was present when it was
    /// made).
    pub(crate) fn missing_midi_outs(&self) -> BTreeSet<NodeId> {
        self.session
            .state()
            .nodes
            .iter()
            .filter(|n| match &n.kind {
                NodeKind::MidiOut { port, .. } => {
                    port.is_empty() || self.midi_out_lost.contains(&n.id)
                }
                _ => false,
            })
            .map(|n| n.id)
            .collect()
    }

    /// The "MIDI Out" entries of MIDI track `track`'s ≡ menu, after the instruments: one per
    /// endpoint present, the one it feeds ticked; one it feeds that is missing is listed
    /// greyed.
    pub(crate) fn midi_out_entries(&self, track: TrackId) -> Vec<Entry> {
        let stopped = !self.engine.is_playing();
        let current = midi_out_of(self.session.state(), track);
        let mut entries: Vec<Entry> = self
            .midi_outputs
            .iter()
            .map(|choice| {
                let tick = if current.is_some_and(|(_, port, _)| port == choice.id) {
                    "✓ "
                } else {
                    "    "
                };
                Entry::item(
                    format!("{tick}MIDI Out: {}", choice.name),
                    stopped.then(|| Message::SetMidiOut(track, choice.clone())),
                )
            })
            .collect();
        if let Some((_, port, name)) = current
            && !self.midi_outputs.iter().any(|c| c.id == port)
        {
            entries.push(Entry::item(format!("✓ MIDI Out: {name} (missing)"), None));
        }
        entries
    }

    /// The body of MIDI output node `id`'s dialog: its endpoint, picked from those present,
    /// and the endpoint's latency, kept in the app settings.
    pub(crate) fn midi_out_panel<'a>(
        &'a self,
        id: NodeId,
        port: &'a str,
        name: &'a str,
    ) -> Element<'a, Message> {
        let selected = self
            .midi_outputs
            .iter()
            .find(|c| c.id == port)
            .cloned()
            .or_else(|| {
                (!port.is_empty()).then(|| MidiOutputChoice {
                    id: port.to_owned(),
                    name: format!("{name} (missing)"),
                })
            });
        let picker = pick_list(self.midi_outputs.clone(), selected, move |choice| {
            Message::SetMidiPort(id, choice)
        })
        .placeholder("No MIDI output");
        let typed = match &self.node_text {
            Some((node, typed)) if *node == id => typed.clone(),
            _ => format!(
                "{}",
                self.midi_out_latency.get(port).copied().unwrap_or(0.0)
            ),
        };
        let latency = text_input("0", &typed)
            .on_input(move |t| Message::NodeText(id, t))
            .width(Length::Fixed(100.0));
        let latency = if port.is_empty() {
            latency
        } else {
            latency.on_submit(Message::CommitNodeText)
        };
        let mut body = column![
            row![text("Port").size(14), picker]
                .spacing(10)
                .align_y(Alignment::Center),
            row![text("Latency (ms)").size(14), latency]
                .spacing(10)
                .align_y(Alignment::Center),
        ]
        .spacing(12);
        if self.midi_out_lost.contains(&id) {
            body = body.push(text("The port is missing; notes to it are dropped.").size(14));
        }
        body.into()
    }
}
