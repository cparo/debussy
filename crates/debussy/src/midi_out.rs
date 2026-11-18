//! The MIDI output sender: one thread owning every open MIDI output, sending what the engine's
//! routed event sinks queue at the time each message is due to sound
//! (`docs/architecture/engine.md` "MIDI output", ADR-0162).
//!
//! The audio thread stamps each message with the host time its frame is due at the device:
//! the block's callback time plus the frame's offset. The sender adds the device's output delay
//! and subtracts the port's own latency, from the app settings, so the hardware sounds as the
//! block is heard; a time already past is sent now. A silence (stop or pause) sends sustain off
//! and All Notes Off on every channel just after the latest message scheduled. A port that goes
//! away is marked lost and what reaches it is dropped; when it returns, All Notes Off goes first.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender as CommandSender, TryRecvError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use debussy_ui::MidiOutRoute;
use midi::{Message, OUT_QUEUE_CAPACITY, OutConsumer, OutProducer, Outgoing, out_channel};
use platform_interfaces::{Error as PlatformError, MidiDevices, MidiEndpointId, MidiOutput};
use project_model::NodeId;
use signal_graph::NodeId as GraphNodeId;

/// How long the sender sleeps when it found nothing to send.
const IDLE: Duration = Duration::from_millis(1);

/// How often a lost or missing port is tried again.
const RETRY: Duration = Duration::from_millis(500);

/// Nanoseconds in a millisecond.
const NANOS_PER_MS: f64 = 1_000_000.0;

/// When a message due at the device at host time `due` is sent: `output_nanos` later, for the
/// device's output delay, and `port_nanos` earlier, for the port's own latency; never before
/// `now`.
pub(crate) fn send_time(due: u64, output_nanos: u64, port_nanos: u64, now: u64) -> u64 {
    due.saturating_add(output_nanos)
        .saturating_sub(port_nanos)
        .max(now)
}

/// A port latency in milliseconds as nanoseconds; a negative or unusable one is 0.
pub(crate) fn latency_nanos(ms: f32) -> u64 {
    if ms.is_finite() && ms > 0.0 {
        (f64::from(ms) * NANOS_PER_MS) as u64
    } else {
        0
    }
}

/// Sustain off then All Notes Off, on each of the 16 channels.
fn silence_messages() -> Vec<Message> {
    (0..16)
        .flat_map(|channel| {
            [
                Message::sustain_off(channel),
                Message::all_notes_off(channel),
            ]
        })
        .flatten()
        .collect()
}

/// A request to the sender thread.
enum Command {
    /// Send what `consumer` carries to endpoint `port`, as model node `node`.
    Route {
        node: NodeId,
        port: String,
        consumer: OutConsumer,
        lost: Arc<AtomicBool>,
    },
    /// Silence and close node `node`'s output.
    Unroute(NodeId),
    /// Silence node `node`'s output and move it to endpoint `port`.
    SetPort(NodeId, String),
    /// The device's output delay and each port's latency, in nanoseconds.
    Offsets {
        output_nanos: u64,
        ports: BTreeMap<String, u64>,
    },
    /// Silence every output and end the thread.
    Quit,
}

/// One routed node as the sender keeps it.
struct Route {
    port: String,
    consumer: OutConsumer,
    /// The open output; `None` while the endpoint cannot be opened.
    output: Option<Box<dyn MidiOutput>>,
    /// Set while the port is lost or missing, for the interface.
    lost: Arc<AtomicBool>,
    /// The latest host time a message was scheduled for.
    last: u64,
}

/// The sender's state, driven by [`Sender::apply`] and [`Sender::pump`]; the thread is a loop
/// over them.
pub(crate) struct Sender {
    midi: Arc<dyn MidiDevices>,
    routes: BTreeMap<NodeId, Route>,
    output_nanos: u64,
    ports: BTreeMap<String, u64>,
    silence: Vec<Message>,
}

impl Sender {
    fn new(midi: Arc<dyn MidiDevices>) -> Self {
        Sender {
            midi,
            routes: BTreeMap::new(),
            output_nanos: 0,
            ports: BTreeMap::new(),
            silence: silence_messages(),
        }
    }

    /// Takes one request. Returns `false` for [`Command::Quit`], after silencing every output.
    fn apply(&mut self, command: Command) -> bool {
        match command {
            Command::Route {
                node,
                port,
                consumer,
                lost,
            } => {
                if let Some(mut old) = self.routes.remove(&node) {
                    self.close(&mut old);
                }
                let mut route = Route {
                    port,
                    consumer,
                    output: None,
                    lost,
                    last: 0,
                };
                self.open(&mut route);
                self.routes.insert(node, route);
            }
            Command::Unroute(node) => {
                if let Some(mut old) = self.routes.remove(&node) {
                    self.close(&mut old);
                }
            }
            Command::SetPort(node, port) => {
                if let Some(mut route) = self.routes.remove(&node) {
                    self.close(&mut route);
                    route.port = port;
                    route.last = 0;
                    self.open(&mut route);
                    self.routes.insert(node, route);
                }
            }
            Command::Offsets {
                output_nanos,
                ports,
            } => {
                self.output_nanos = output_nanos;
                self.ports = ports;
            }
            Command::Quit => {
                for mut route in std::mem::take(&mut self.routes).into_values() {
                    self.close(&mut route);
                }
                return false;
            }
        }
        true
    }

    /// Opens `route`'s endpoint, marking it lost if it cannot be.
    fn open(&self, route: &mut Route) {
        match self.midi.open_output(&MidiEndpointId(route.port.clone())) {
            Ok(output) => {
                route.output = Some(output);
                route.lost.store(false, Ordering::Release);
            }
            Err(_) => {
                route.output = None;
                route.lost.store(true, Ordering::Release);
            }
        }
    }

    /// Silences `route`'s output, now and just after its latest message (in case closing
    /// the output drops what is still scheduled, and in case it does not), then closes it.
    fn close(&self, route: &mut Route) {
        if let Some(output) = route.output.as_mut() {
            let now = self.midi.host_nanos();
            let _ = output.send(None, &self.silence);
            if route.last >= now {
                let _ = output.send(Some(route.last.saturating_add(1)), &self.silence);
            }
        }
        route.output = None;
    }

    /// Sends everything queued, then retries lost ports if `retry`. Returns whether anything
    /// was queued.
    fn pump(&mut self, retry: bool) -> bool {
        let mut busy = false;
        let now = self.midi.host_nanos();
        for route in self.routes.values_mut() {
            while let Some(item) = route.consumer.pop() {
                busy = true;
                let port_nanos = self.ports.get(&route.port).copied().unwrap_or(0);
                deliver(
                    route,
                    item,
                    now,
                    self.output_nanos,
                    port_nanos,
                    &self.silence,
                );
            }
        }
        if retry {
            let lost: Vec<NodeId> = self
                .routes
                .iter()
                .filter(|(_, r)| r.lost.load(Ordering::Acquire))
                .map(|(node, _)| *node)
                .collect();
            for node in lost {
                if let Some(mut route) = self.routes.remove(&node) {
                    match route.output.as_mut() {
                        Some(output) => {
                            if output.send(None, &self.silence).is_ok() {
                                route.lost.store(false, Ordering::Release);
                            }
                        }
                        None => {
                            self.open(&mut route);
                            if let Some(output) = route.output.as_mut() {
                                let _ = output.send(None, &self.silence);
                            }
                        }
                    }
                    self.routes.insert(node, route);
                }
            }
        }
        busy
    }
}

/// Sends one queued item on `route`. A lost port is first sent All Notes Off; if that fails too,
/// the item is dropped.
fn deliver(
    route: &mut Route,
    item: Outgoing,
    now: u64,
    output_nanos: u64,
    port_nanos: u64,
    silence: &[Message],
) {
    let Some(output) = route.output.as_mut() else {
        return;
    };
    if route.lost.load(Ordering::Acquire) {
        if output.send(None, silence).is_err() {
            return;
        }
        route.lost.store(false, Ordering::Release);
    }
    let (at, messages): (u64, &[Message]) = match &item {
        Outgoing::Message {
            host_nanos,
            message,
        } => (
            send_time(*host_nanos, output_nanos, port_nanos, now),
            std::slice::from_ref(message),
        ),
        Outgoing::Silence => (route.last.saturating_add(1).max(now), silence),
    };
    match output.send(Some(at), messages) {
        Ok(()) => route.last = route.last.max(at),
        Err(PlatformError::StreamStopped(_)) => route.lost.store(true, Ordering::Release),
        // Anything else loses this message only.
        Err(_) => {}
    }
}

/// The control side of the sender, owned by the services: routes, offsets and lost flags.
/// Dropping it silences every output and ends the thread.
pub(crate) struct MidiOuts {
    commands: CommandSender<Command>,
    /// Each routed node's event sink, port and lost flag.
    routes: BTreeMap<NodeId, (GraphNodeId, String, Arc<AtomicBool>)>,
    thread: Option<JoinHandle<()>>,
}

impl MidiOuts {
    /// Starts the sender thread over `midi`.
    pub(crate) fn start(midi: Arc<dyn MidiDevices>) -> Self {
        let (commands, receiver) = mpsc::channel();
        let thread = std::thread::Builder::new()
            .name("midi-out".to_owned())
            .spawn(move || run(Sender::new(midi), receiver))
            .ok();
        MidiOuts {
            commands,
            routes: BTreeMap::new(),
            thread,
        }
    }

    /// Brings the routes in step with `wanted`: a node no longer wanted is silenced and
    /// closed, one whose port changed is silenced and moved, and one new or with a new event
    /// sink gets a new queue, returned with its sink for the engine to route.
    pub(crate) fn sync(&mut self, wanted: &[MidiOutRoute]) -> Vec<(GraphNodeId, OutProducer)> {
        let keep: BTreeMap<NodeId, &MidiOutRoute> = wanted.iter().map(|r| (r.node, r)).collect();
        let gone: Vec<NodeId> = self
            .routes
            .keys()
            .filter(|node| !keep.contains_key(node))
            .copied()
            .collect();
        for node in gone {
            self.routes.remove(&node);
            let _ = self.commands.send(Command::Unroute(node));
        }
        let mut new = Vec::new();
        for (node, route) in keep {
            match self.routes.get_mut(&node) {
                Some((sink, port, _)) if *sink == route.sink => {
                    if *port != route.port {
                        port.clone_from(&route.port);
                        let _ = self
                            .commands
                            .send(Command::SetPort(node, route.port.clone()));
                    }
                }
                _ => {
                    let Ok((producer, consumer)) = out_channel(OUT_QUEUE_CAPACITY) else {
                        continue;
                    };
                    let lost = Arc::new(AtomicBool::new(false));
                    let sent = self.commands.send(Command::Route {
                        node,
                        port: route.port.clone(),
                        consumer,
                        lost: Arc::clone(&lost),
                    });
                    if sent.is_ok() {
                        self.routes
                            .insert(node, (route.sink, route.port.clone(), lost));
                        new.push((route.sink, producer));
                    }
                }
            }
        }
        new
    }

    /// Sets the device's output delay and each port's latency, in nanoseconds.
    pub(crate) fn set_offsets(&self, output_nanos: u64, ports: BTreeMap<String, u64>) {
        let _ = self.commands.send(Command::Offsets {
            output_nanos,
            ports,
        });
    }

    /// Whether node `node`'s port is lost or missing.
    pub(crate) fn lost(&self, node: NodeId) -> bool {
        self.routes
            .get(&node)
            .is_some_and(|(_, _, lost)| lost.load(Ordering::Acquire))
    }
}

impl Drop for MidiOuts {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// The sender thread: takes requests, sends what is queued, retries lost ports every
/// [`RETRY`], and sleeps [`IDLE`] when there was nothing to do.
fn run(mut sender: Sender, commands: Receiver<Command>) {
    let mut retried = Instant::now();
    loop {
        loop {
            match commands.try_recv() {
                Ok(command) => {
                    if !sender.apply(command) {
                        return;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    sender.apply(Command::Quit);
                    return;
                }
            }
        }
        let retry = retried.elapsed() >= RETRY;
        if retry {
            retried = Instant::now();
        }
        if !sender.pump(retry) {
            std::thread::sleep(IDLE);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;

    use audio_engine::{Engine, Processor};
    use engine_bridge::{Bridge, Samples, midi_tracks};
    use midi::Kind;
    use platform_interfaces::MidiDirection;
    use platform_interfaces::test_doubles::{FakeMidi, SentMessage};
    use project_model::{Clip, Destination, Note, Sequence, Session, Source};
    use signal_graph::Config;

    use super::*;

    type TestResult = Result<(), Box<dyn Error>>;

    /// Streams with nothing to move.
    struct NoStreams;

    impl audio_engine::Streams for NoStreams {
        fn seek(&mut self, _frame: u64) -> Result<(), Box<dyn Error + Send + Sync>> {
            Ok(())
        }
    }

    const RATE: u32 = 48_000;
    const BLOCK: usize = 480; // 10 ms
    const PERIOD: u64 = 10_000_000;
    const OUTPUT: u64 = 5_000_000;
    const PORT_MS: f32 = 2.0;

    #[test]
    fn test_send_time_adds_output_delay_less_port_latency_never_before_now() {
        assert_eq!(send_time(1_000, 300, 100, 0), 1_200);
        // A port slower than the output delay sends earlier, but not before now.
        assert_eq!(send_time(1_000, 100, 300, 0), 800);
        assert_eq!(send_time(1_000, 100, 300, 900), 900);
        assert_eq!(send_time(10, 0, 100, 0), 0);
        assert_eq!(send_time(u64::MAX, 5, 0, 0), u64::MAX);
    }

    #[test]
    fn test_latency_nanos_ignores_unusable_values() {
        assert_eq!(latency_nanos(2.5), 2_500_000);
        assert_eq!(latency_nanos(-1.0), 0);
        assert_eq!(latency_nanos(f32::NAN), 0);
    }

    /// A project of one MIDI track playing a note from frame 600 to 1000, sent to MIDI output
    /// `port`, with its engine and a sender over `midi`.
    struct Rig {
        engine: Engine,
        processor: Processor,
        sender: Sender,
        midi: FakeMidi,
        node: NodeId,
        lost: Arc<AtomicBool>,
        host: u64,
        _bridge: Bridge,
    }

    impl Rig {
        fn new() -> Result<Rig, Box<dyn Error>> {
            let midi = FakeMidi::new();
            midi.add_endpoint("out", "Synth", MidiDirection::Output);
            let mut session = Session::new_project();
            let track = session.add_midi_track("keys")?;
            let note = Note {
                start: 600,
                length: 400,
                channel: 0,
                pitch: 60,
                velocity: 100,
            };
            let sequence = Sequence::new(2000, vec![note], Vec::new())?;
            session.add_clip(
                track,
                Clip::new(Source::Midi(Arc::new(sequence)), 0, 2000, 0),
            )?;
            let node = session.set_destination(
                track,
                Destination::MidiOut {
                    port: "out".to_owned(),
                    name: "Synth".to_owned(),
                },
            )?;
            let (bridge, runner) =
                Bridge::new(session.state(), Config::new(RATE, 512), Samples::default())?;
            let output = bridge.output().ok_or("no output")?;
            let (mut engine, processor) = Engine::new(runner, output, RATE)?;
            engine.set_midi_tracks(midi_tracks(session.state(), &bridge.event_sources()))?;
            let sink = *bridge.event_sinks().get(&node).ok_or("no event sink")?;
            let (producer, consumer) = out_channel(64)?;
            engine.route_midi_out(sink, producer)?;
            let mut sender = Sender::new(Arc::new(midi.clone()));
            let lost = Arc::new(AtomicBool::new(false));
            sender.apply(Command::Route {
                node,
                port: "out".to_owned(),
                consumer,
                lost: Arc::clone(&lost),
            });
            sender.apply(Command::Offsets {
                output_nanos: OUTPUT,
                ports: BTreeMap::from([("out".to_owned(), latency_nanos(PORT_MS))]),
            });
            Ok(Rig {
                engine,
                processor,
                sender,
                midi,
                node,
                lost,
                // Well ahead of the fake clock, so no send time is clamped to now.
                host: platform_interfaces::test_doubles::host_nanos() + 3_600_000_000_000,
                _bridge: bridge,
            })
        }

        fn run(&mut self, blocks: usize) -> TestResult {
            let mut left = vec![0.0; BLOCK];
            let mut right = vec![0.0; BLOCK];
            for _ in 0..blocks {
                self.processor
                    .process(&[], 0, self.host, &mut left, &mut right)?;
                self.host += PERIOD;
            }
            self.sender.pump(false);
            Ok(())
        }
    }

    fn kind(sent: &SentMessage) -> Kind {
        sent.message.kind()
    }

    #[test]
    fn test_clip_reaches_the_port_at_its_send_time() -> TestResult {
        let mut rig = Rig::new()?;
        let first = rig.host;
        rig.engine.play(&mut NoStreams)?;
        rig.run(4)?;
        let offset = OUTPUT - latency_nanos(PORT_MS);
        let sent: Vec<(Option<u64>, Kind)> = rig
            .midi
            .sent()
            .iter()
            .map(|s| (s.at_host_nanos, kind(s)))
            .collect();
        assert_eq!(
            sent,
            [
                (
                    Some(first + PERIOD + 2_500_000 + offset),
                    Kind::NoteOn {
                        note: 60,
                        velocity: 100
                    }
                ),
                (
                    Some(first + 2 * PERIOD + 833_333 + offset),
                    Kind::NoteOff {
                        note: 60,
                        velocity: 0
                    }
                ),
            ]
        );
        assert!(!rig.lost.load(Ordering::Acquire));
        Ok(())
    }

    #[test]
    fn test_stop_sends_sustain_and_all_notes_off_after_the_last_message() -> TestResult {
        let mut rig = Rig::new()?;
        rig.engine.play(&mut NoStreams)?;
        rig.run(2)?;
        let last = rig
            .midi
            .sent()
            .iter()
            .filter_map(|s| s.at_host_nanos)
            .max()
            .ok_or("nothing sent")?;
        let before = rig.midi.sent().len();
        rig.engine.stop()?;
        rig.run(1)?;
        // The engine also releases the note still sounding, in the block after the stop.
        let silence: Vec<SentMessage> = rig
            .midi
            .sent()
            .split_off(before)
            .into_iter()
            .filter(|s| matches!(kind(s), Kind::ControlChange { .. }))
            .collect();
        assert_eq!(silence.len(), 32);
        assert!(silence.iter().all(|s| s.at_host_nanos == Some(last + 1)));
        for (channel, pair) in silence.chunks(2).enumerate() {
            assert_eq!(
                pair[0].message,
                Message::sustain_off(channel as u8).ok_or("ch")?
            );
            assert_eq!(
                pair[1].message,
                Message::all_notes_off(channel as u8).ok_or("ch")?
            );
        }
        Ok(())
    }

    #[test]
    fn test_unplugged_port_drops_and_replug_sends_all_notes_off_first() -> TestResult {
        let mut rig = Rig::new()?;
        rig.midi.unplug("out");
        rig.engine.play(&mut NoStreams)?;
        rig.run(2)?;
        assert!(rig.midi.sent().is_empty());
        assert!(rig.lost.load(Ordering::Acquire));
        rig.midi.replug("out");
        rig.run(1)?;
        let sent = rig.midi.sent();
        assert_eq!(sent.len(), 33);
        assert!(sent[..32].iter().all(|s| s.at_host_nanos.is_none()));
        assert_eq!(sent[1].message, Message::all_notes_off(0).ok_or("ch")?);
        assert!(matches!(kind(&sent[32]), Kind::NoteOff { note: 60, .. }));
        assert!(!rig.lost.load(Ordering::Acquire));
        Ok(())
    }

    #[test]
    fn test_retry_clears_a_lost_port_without_traffic() -> TestResult {
        let mut rig = Rig::new()?;
        rig.midi.unplug("out");
        rig.engine.play(&mut NoStreams)?;
        rig.run(2)?;
        assert!(rig.lost.load(Ordering::Acquire));
        rig.midi.replug("out");
        rig.engine.pause()?;
        rig.sender.pump(true);
        assert!(!rig.lost.load(Ordering::Acquire));
        Ok(())
    }

    #[test]
    fn test_missing_port_is_lost_until_it_appears() -> TestResult {
        let midi = FakeMidi::new();
        let mut sender = Sender::new(Arc::new(midi.clone()));
        let (mut producer, consumer) = out_channel(8)?;
        let lost = Arc::new(AtomicBool::new(false));
        let node = NodeId(7);
        sender.apply(Command::Route {
            node,
            port: "later".to_owned(),
            consumer,
            lost: Arc::clone(&lost),
        });
        assert!(lost.load(Ordering::Acquire));
        producer.push(1, Message::note_on(0, 60, 1).ok_or("note")?);
        sender.pump(false);
        assert!(midi.sent().is_empty());
        midi.add_endpoint("later", "Later", MidiDirection::Output);
        sender.pump(true);
        assert!(!lost.load(Ordering::Acquire));
        assert_eq!(midi.sent().len(), 32);
        Ok(())
    }

    #[test]
    fn test_unroute_and_quit_silence_the_port() -> TestResult {
        let mut rig = Rig::new()?;
        rig.sender.apply(Command::Unroute(rig.node));
        // Nothing was scheduled, so only the immediate silence goes.
        assert_eq!(rig.midi.sent().len(), 32);
        let mut rig = Rig::new()?;
        rig.engine.play(&mut NoStreams)?;
        rig.run(2)?;
        let before = rig.midi.sent().len();
        assert!(!rig.sender.apply(Command::Quit));
        let after = rig.midi.sent().split_off(before);
        assert_eq!(after.len(), 64);
        assert!(after[..32].iter().all(|s| s.at_host_nanos.is_none()));
        assert!(after[32..].iter().all(|s| s.at_host_nanos.is_some()));
        Ok(())
    }

    #[test]
    fn test_midi_outs_sync_routes_moves_and_drops() -> TestResult {
        let midi = FakeMidi::new();
        midi.add_endpoint("a", "A", MidiDirection::Output);
        midi.add_endpoint("b", "B", MidiDirection::Output);
        let mut outs = MidiOuts::start(Arc::new(midi.clone()));
        let (mut graph, _runner) = signal_graph::Graph::new(Config::new(RATE, 512))?;
        let (one, two) = (graph.add_event_sink(), graph.add_event_sink());
        let route = |port: &str, sink: GraphNodeId| MidiOutRoute {
            node: NodeId(1),
            sink,
            port: port.to_owned(),
        };
        assert_eq!(outs.sync(&[route("a", one)]).len(), 1);
        // The same node and sink: nothing new, even with another port.
        assert!(outs.sync(&[route("a", one)]).is_empty());
        assert!(outs.sync(&[route("b", one)]).is_empty());
        // A new sink needs a new queue.
        assert_eq!(outs.sync(&[route("b", two)]).len(), 1);
        assert!(outs.sync(&[]).is_empty());
        assert!(!outs.lost(NodeId(1)));
        drop(outs);
        Ok(())
    }
}
