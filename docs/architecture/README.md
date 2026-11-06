# Architecture: Debussy

This is an overview of how this application is built.

For the reasons _behind_ our design choices, see our [ADRs](../../adrs/README.md).

"Must", "should", and "may" are used according to the conventions defined by the Network Working
Group and IETF. In short:
* "Must" means an absolute requirement.
* "Should" means a recommentation which may be ignored - but only if discussed with or flagged for
  explicit review by a human.
* "May" means an option which can be exercised when it makes sense to do so.

Within this document, "module" refers to the general concept of an interface-bearing unit of
functionality as defined in the `codebase-design` skill. The phrase "Rust module" is used where
specifically describing Rust's own unit of organization below the level of a "crate".

Architecture is documented here down to the level of the application's crates and a _listing_ of
the modules present in each crate. All lower-level documentation (for Rust modules, functions,
methods, etc) belongs _only_ in the RustDoc commentary provided alongside the things being
documented.

## Design Principles

### Crates map to a logical subsystem or collection of modules of a similar shape and purpose

For example:
* project_persistence (a logical subsystem)
* audio_fitlers (a collection of modules of a similar shape and purpose)

Changes to the functionality provided by any of these crates should not require changes elsewhere
in order to apply and test those changes.

Cases where new functionality is introduced may lead to creating additional changes in other crates
crates in order to _make use_ of that new functionality - but these additional changes should be
made in separate commits, with each such commit passing our build-time tests.

### Separating platform-dependent and platform-independent code

Platform-dependent code (such as I/O for audio streaming and data persistence) must be kept in
separate crates from platform-independent code (such as audio-effects filters and generation of
visualizations).

### Avoidance of cyclical dependencies

Cyclical dependencies must not be created between crates. This is enforced by Rust itself.

Cyclical dependencies may be tolerated between modules _within_ a crate, but must be flagged for
human review in PR descriptions when introduced.

### Give things in interfaces names that describe what they mean to end users

Use common domain-appropriate jargon, where available. Otherwise use a literal description of what
the thing is or does _from an end-user perspective_.

For example: "low-pass-equalizer", "audio-track"

### Give implementation details names that describe what they do

Implementation deatails behind a crate's interface boundary can have more "technical" names, but
should be oriented toward quickly describing the structure or algorithm they represent.

For example: "butterworth-filter", "audio-track-storage-chunk"

## Modules

<!-- TODO: Create an xtask script to generate and maintain a table-of-content here corresponding
     to the other documents under this directory and their headings. -->
