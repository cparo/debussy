//! [`MacMidi`] against in-process virtual endpoints, through the real CoreMIDI server.

use std::error::Error as StdError;
use std::sync::atomic::AtomicUsize;
use std::time::{Duration, Instant};

use super::*;
use crate::sys::core_midi::{VirtualDestination, VirtualSource};

type TestResult = Result<(), Box<dyn StdError>>;

/// Polls `done` until it holds or five seconds pass.
fn wait_until(mut done: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    done()
}

/// A virtual source with a name unique to the test.
fn source(name: &str) -> Result<(VirtualSource, MidiEndpointId), Box<dyn StdError>> {
    let source = client()?.virtual_source(name).map_err(platform)?;
    let id = endpoint_id(source.unique_id().ok_or("no unique id")?);
    Ok((source, id))
}

/// Keeps what it receives.
#[derive(Clone, Default)]
struct Collect(Arc<Mutex<Vec<TimedMessage>>>);

impl MidiInputCallback for Collect {
    fn receive(&mut self, message: TimedMessage) -> bool {
        lock(&self.0).push(message);
        true
    }
}

impl Collect {
    fn messages(&self) -> Vec<Message> {
        lock(&self.0).iter().map(|m| m.message).collect()
    }
}

fn note(n: u8) -> Result<Message, &'static str> {
    Message::note_on(0, n, 100).ok_or("bad note")
}

#[test]
fn test_open_input_receives_messages_with_host_time() -> TestResult {
    let midi = MacMidi::new()?;
    let (source, id) = source("debussy-test-input")?;
    let collect = Collect::default();
    let input = midi.open_input(&id, Box::new(collect.clone()))?;
    let before = midi.host_nanos();
    source
        .received(0, &[ump::word(&note(60)?), 0x1000_F800])
        .map_err(platform)?;
    assert!(wait_until(|| !collect.messages().is_empty()));
    let after = midi.host_nanos();
    let got = lock(&collect.0).clone();
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].message, note(60)?);
    assert!((before..=after).contains(&got[0].host_nanos));
    // The system real-time message (UMP type 1) was dropped and counted.
    assert!(wait_until(|| input.dropped() == 1));
    Ok(())
}

#[test]
fn test_send_reaches_a_virtual_destination() -> TestResult {
    let midi = MacMidi::new()?;
    let received = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&received);
    let destination: VirtualDestination = client()?
        .virtual_destination(
            "debussy-test-output",
            Box::new(move |_, words: &[u32]| lock(&sink).extend_from_slice(words)),
        )
        .map_err(platform)?;
    let id = endpoint_id(destination.unique_id().ok_or("no unique id")?);
    let mut output = midi.open_output(&id)?;
    output.send(None, &[note(61)?, note(62)?])?;
    output.send(Some(midi.host_nanos() + 1_000_000), &[note(63)?])?;
    let expected: Vec<u32> = [note(61)?, note(62)?, note(63)?]
        .iter()
        .map(ump::word)
        .collect();
    assert!(wait_until(|| *lock(&received) == expected));
    Ok(())
}

#[test]
fn test_endpoint_changes_fire_listeners_and_update_endpoints() -> TestResult {
    let midi = MacMidi::new()?;
    let changes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&changes);
    midi.on_change(Box::new(move || {
        counter.fetch_add(1, Ordering::AcqRel);
    }));
    let listed = |id: &MidiEndpointId| {
        midi.endpoints().is_ok_and(|all| {
            all.iter()
                .any(|e| e.id == *id && e.direction == MidiDirection::Input)
        })
    };
    // CoreMIDI's endpoint list can lag a source made or disposed while other tests are changing
    // the setup too, so the list is polled rather than read once.
    let (source, id) = source("debussy-test-hotplug")?;
    assert!(wait_until(|| listed(&id)));
    assert!(wait_until(|| changes.load(Ordering::Acquire) > 0));
    let seen = changes.load(Ordering::Acquire);
    drop(source);
    assert!(wait_until(|| !listed(&id)));
    assert!(wait_until(|| changes.load(Ordering::Acquire) > seen));
    Ok(())
}

#[test]
fn test_open_input_reconnects_when_its_source_returns() -> TestResult {
    let midi = MacMidi::new()?;
    let (first, id) = source("debussy-test-replug")?;
    let unique = unique_id(&id)?;
    let collect = Collect::default();
    let mut input = midi.open_input(&id, Box::new(collect.clone()))?;
    drop(first);
    assert!(wait_until(|| matches!(
        input.take_error(),
        Some(Error::StreamStopped(_))
    )));

    let second = client()?
        .virtual_source("debussy-test-replug")
        .map_err(platform)?;
    second.set_unique_id(unique).map_err(platform)?;
    // Keep sending until the reconnection takes, which follows the property-change notice.
    let word = ump::word(&note(64)?);
    assert!(wait_until(|| {
        let _ = second.received(0, &[word]);
        !collect.messages().is_empty()
    }));
    assert_eq!(collect.messages()[0], note(64)?);
    Ok(())
}

#[test]
fn test_input_flood_is_bounded_and_counted() -> TestResult {
    let midi = MacMidi::new()?;
    let (source, id) = source("debussy-test-flood")?;
    let (producer, _live) = midi::channel(midi::LIVE_QUEUE_CAPACITY)?;
    let input = midi.open_input(&id, Box::new(producer))?;
    let word = ump::word(&note(65)?);
    let total = midi::LIVE_QUEUE_CAPACITY + 100;
    let mut sent = 0;
    while sent < total {
        let n = (total - sent).min(core_midi::PACKET_WORDS);
        source.received(0, &vec![word; n]).map_err(platform)?;
        sent += n;
    }
    assert!(
        wait_until(|| input.dropped() == 100),
        "dropped {}",
        input.dropped()
    );
    Ok(())
}

#[test]
fn test_open_refuses_unknown_endpoints() -> TestResult {
    let midi = MacMidi::new()?;
    let missing = MidiEndpointId("not a number".into());
    assert!(matches!(
        midi.open_input(&missing, Box::new(Collect::default())),
        Err(Error::MidiEndpointNotFound(_))
    ));
    assert!(matches!(
        midi.open_output(&MidiEndpointId("0".into())),
        Err(Error::MidiEndpointNotFound(_))
    ));
    Ok(())
}
