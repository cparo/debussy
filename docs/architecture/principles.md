# Design Principles

## Crates map to a logical subsystem or collection of modules of a similar shape and purpose

For example:
* project_persistence (a logical subsystem)
* audio_filters (a collection of modules of a similar shape and purpose)

Changes to the functionality provided by any of these crates should not require changes elsewhere
in order to apply and test those changes.

Cases where new functionality is introduced may lead to creating additional changes in other
crates in order to _make use_ of that new functionality - but these additional changes should be
made in separate commits, with each such commit passing our build-time tests.

## Separating platform-dependent and platform-independent code

Platform-dependent code (such as I/O for audio streaming and data persistence) must be kept in
separate crates from platform-independent code (such as audio-effects filters and generation of
visualizations).

## Avoidance of cyclical dependencies

Cyclical dependencies must not be created between crates. This is enforced by Rust itself.

Cyclical dependencies may be tolerated between modules _within_ a crate, but must be flagged for
human review in PR descriptions when introduced.

## Give things in interfaces names that describe what they mean to end users

Use common domain-appropriate jargon, where available. Otherwise use a literal description of what
the thing is or does _from an end-user perspective_.

For example: "low-pass-equalizer", "audio-track"

## Give implementation details names that describe what they do

Implementation details behind a crate's interface boundary can have more "technical" names, but
should be oriented toward quickly describing the structure or algorithm they represent.

For example: "butterworth-filter", "audio-track-storage-chunk"
