# ADR-0079 — Arm state is session-only and inputs open only while a track is armed

- **Status:** Accepted
- **Decided by:** Chris Paro
- **Date:** 2026-11-09
- **Issue:** #67
- **Architecture:** [Recording](../architecture/user-interface.md#recording)

## Context

Recording (#67) needs per-track arm, monitor and input settings in the window, and needs the
device's inputs open (ADR-0078). Opening the inputs is a privacy decision: on macOS it triggers the
microphone permission prompt and the system's microphone indicator, and recorded takes may hold
people's voices, stored as files in the project bundle. This was raised for human review
(`flag-risk`, tag `privacy`) and answered by Chris Paro on 2026-11-09.

## Decision

- The device opens duplex only while at least one track is armed. Arming the first track reopens
  it with its inputs (up to `MAX_INPUTS`); disarming the last reopens it output-only. Arming,
  and so reopening, happens only while stopped or paused, as device switching does (ADR-0059).
- Recorded takes are stored like imported audio, in the bundle's `audio/` directory, and are never
  logged or sent anywhere.
- Arm, monitor and input choices are session state of the window, not saved in the project, and
  are refused while playing. An input defaults to channel 1 for a mono track and 1–2 for stereo.
- Record is refused while nothing is armed, while the device has no inputs, and while the previous
  pass's files are still being written. The pass's takes are placed as one undo step once every
  file has finished (ADR-0078).
- Take files are named `<track name> <n>[ L| R].wav` from `import_export`'s naming, kept distinct
  across the pass's tracks.

## Alternatives considered

- **Always open the device duplex** — arming would be instant, but the permission prompt would
  appear at launch and the microphone indicator stay on for people who never record.
- **Build the controls but leave the device output-only** — recording would still not work.
- **Save arm and monitor in the project** — a persisted format change for state that depends on
  the machine's inputs and the moment.
- **Allow monitor and input changes while playing** — arming is refused by the engine while
  playing (ADR-0078); an atomic monitor switch would allow it and can be added later.

## Consequences

- Arming the first track and disarming the last cause a short output dropout while the device
  reopens.
- A device needs both inputs and outputs: on Macs whose built-in speakers and microphone are
  separate devices, recording needs an interface or an aggregate device.
- Without an app bundle and `Info.plist`, macOS grants microphone access to the terminal that
  launched Debussy.
- Settings are lost when the window closes; a later ADR could save them per machine.
