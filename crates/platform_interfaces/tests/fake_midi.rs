//! `FakeMidi` behaves as the MIDI traits promise, and its input reaches a `LiveInput`.

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use midi::{EventBuffer, Message, TimedMessage};
use platform_interfaces::test_doubles::{FakeMidi, SentMessage};
use platform_interfaces::{Error as PlatformError, MidiDevices, MidiDirection, MidiEndpointId};

type TestResult = Result<(), Box<dyn Error>>;

fn note(n: u8) -> Result<Message, &'static str> {
    Message::note_on(0, n, 90).ok_or("bad note")
}

fn keyboard() -> FakeMidi {
    let midi = FakeMidi::new();
    midi.add_endpoint("kbd", "Keyboard", MidiDirection::Input);
    midi.add_endpoint("synth", "Synth", MidiDirection::Output);
    midi
}

#[test]
fn test_endpoints_lists_added_endpoints() -> TestResult {
    let midi = keyboard();
    let endpoints = midi.endpoints()?;
    assert_eq!(endpoints.len(), 2);
    assert_eq!(endpoints[0].name, "Keyboard");
    assert_eq!(endpoints[1].direction, MidiDirection::Output);
    assert!(endpoints.iter().all(|e| e.online));
    Ok(())
}

#[test]
fn test_open_input_refuses_unknown_or_wrong_direction() {
    let midi = keyboard();
    let refused = |id: &str| {
        matches!(
            midi.open_input(&MidiEndpointId(id.into()), Box::new(NullCallback)),
            Err(PlatformError::MidiEndpointNotFound(_))
        )
    };
    assert!(refused("nothing"));
    assert!(refused("synth"));
    assert!(matches!(
        midi.open_output(&MidiEndpointId("kbd".into())),
        Err(PlatformError::MidiEndpointNotFound(_))
    ));
}

struct NullCallback;

impl platform_interfaces::MidiInputCallback for NullCallback {
    fn receive(&mut self, _: TimedMessage) -> bool {
        true
    }
}

#[test]
fn test_input_reaches_live_input_and_counts_drops() -> TestResult {
    let midi = keyboard();
    let (producer, mut live) = midi::channel(2)?;
    let input = midi.open_input(&MidiEndpointId("kbd".into()), Box::new(producer))?;
    let now = midi.host_nanos();
    for n in 1..=3 {
        midi.inject(
            "kbd",
            TimedMessage {
                host_nanos: now,
                message: note(n)?,
            },
        );
    }
    // SysEx is not a channel message.
    assert_eq!(midi.inject_bytes("kbd", now, &[0xF0, 0x7E, 0xF7]), 0);
    assert_eq!(input.dropped(), 2, "one overflow and one invalid message");
    let mut out = EventBuffer::new(8);
    assert_eq!(live.fill(&mut out, now + 1, 64, 48_000), 2);
    Ok(())
}

#[test]
fn test_unplug_stops_input_and_output_until_replugged() -> TestResult {
    let midi = keyboard();
    let changes = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&changes);
    midi.on_change(Box::new(move || {
        counter.fetch_add(1, Ordering::AcqRel);
    }));
    let mut input = midi.open_input(&MidiEndpointId("kbd".into()), Box::new(NullCallback))?;
    let mut output = midi.open_output(&MidiEndpointId("synth".into()))?;

    midi.unplug("kbd");
    midi.unplug("synth");
    assert_eq!(changes.load(Ordering::Acquire), 2);
    assert!(matches!(
        input.take_error(),
        Some(PlatformError::StreamStopped(_))
    ));
    assert!(input.take_error().is_none());
    assert!(matches!(
        output.send(None, &[note(1)?]),
        Err(PlatformError::StreamStopped(_))
    ));
    assert!(!midi.endpoints()?.iter().any(|e| e.online));

    midi.replug("kbd");
    midi.replug("synth");
    assert_eq!(changes.load(Ordering::Acquire), 4);
    let message = TimedMessage {
        host_nanos: 1,
        message: note(2)?,
    };
    assert_eq!(midi.inject("kbd", message), 1);
    output.send(Some(42), &[note(3)?])?;
    assert_eq!(
        midi.sent(),
        [SentMessage {
            endpoint: MidiEndpointId("synth".into()),
            at_host_nanos: Some(42),
            message: note(3)?,
        }]
    );
    Ok(())
}
