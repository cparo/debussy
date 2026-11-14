# Architecture: Debussy

This is an overview of how this application is built.

For the reasons _behind_ our design choices, see our [ADRs](../adrs/README.md).

"Must", "should", and "may" are used according to the conventions defined by the Network Working
Group and IETF. In short:
* "Must" means an absolute requirement.
* "Should" means a recommendation which may be ignored - but only if discussed with or flagged for
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
* audio_filters (a collection of modules of a similar shape and purpose)

Changes to the functionality provided by any of these crates should not require changes elsewhere
in order to apply and test those changes.

Cases where new functionality is introduced may lead to creating additional changes in other
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

Implementation details behind a crate's interface boundary can have more "technical" names, but
should be oriented toward quickly describing the structure or algorithm they represent.

For example: "butterworth-filter", "audio-track-storage-chunk"

## Modules

<!-- BEGIN GENERATED INDEX: run `cargo xtask generate-architecture-index` -->

- [Audio Analysis](analysis.md)
  - [Waveform summaries](analysis.md#waveform-summaries)
  - [Spectrograms](analysis.md#spectrograms)
  - [Analysis cache files](analysis.md#analysis-cache-files)
- [Automation](automation.md)
  - [Curves](automation.md#curves)
  - [Curves in the project](automation.md#curves-in-the-project)
- [Crates](crates.md)
  - [Crate map](crates.md#crate-map)
  - [Dependency rules](crates.md#dependency-rules)
- [Delay and Reverb](delay-and-reverb.md)
  - [Delay](delay-and-reverb.md#delay)
  - [Reverb](delay-and-reverb.md#reverb)
- [Effect Displays](effect-displays.md)
  - [Effect displays](effect-displays.md#effect-displays-1)
  - [Display plots](effect-displays.md#display-plots)
  - [Equaliser displays](effect-displays.md#equaliser-displays)
  - [Dynamics display](effect-displays.md#dynamics-display)
  - [Amplifier display](effect-displays.md#amplifier-display)
  - [Impulse response display](effect-displays.md#impulse-response-display)
  - [Modulation display](effect-displays.md#modulation-display)
- [Effects](effects.md)
  - [Effect kinds](effects.md#effect-kinds)
  - [Effect presets](effects.md#effect-presets)
  - [Effect curves](effects.md#effect-curves)
  - [Filters](effects.md#filters)
  - [High-pass and low-pass filters](effects.md#high-pass-and-low-pass-filters)
  - [Parametric equaliser](effects.md#parametric-equaliser)
  - [Frequency response](effects.md#frequency-response)
  - [Dynamics](effects.md#dynamics)
  - [Amplifier](effects.md#amplifier)
- [Audio Engine](engine.md)
  - [Sample format](engine.md#sample-format)
  - [Sample rate](engine.md#sample-rate)
  - [Buffer size](engine.md#buffer-size)
  - [Audio storage](engine.md#audio-storage)
  - [Disk streaming](engine.md#disk-streaming)
  - [Real-time thread](engine.md#real-time-thread)
  - [Clip playback](engine.md#clip-playback)
  - [Transport](engine.md#transport)
  - [Offline render](engine.md#offline-render)
- [Import and Export](import-export.md)
  - [Formats](import-export.md#formats)
  - [WAV files](import-export.md#wav-files)
  - [Import](import-export.md#import)
  - [Export](import-export.md#export)
  - [Export bit depth](import-export.md#export-bit-depth)
- [Licensing](licensing.md)
  - [Project licence](licensing.md#project-licence)
  - [Dependency licences](licensing.md#dependency-licences)
  - [Bundled assets](licensing.md#bundled-assets)
- [Modulation](modulation.md)
  - [LFO](modulation.md#lfo)
  - [Modulated delay](modulation.md#modulated-delay)
  - [Chorus](modulation.md#chorus)
  - [Flanger](modulation.md#flanger)
- [Patents](patents.md)
  - [Patent policy](patents.md#patent-policy)
- [Platform Integration](platform.md)
  - [Compressed audio codecs](platform.md#compressed-audio-codecs)
  - [Folders](platform.md#folders)
  - [Audio device I/O](platform.md#audio-device-io)
- [Project Model](project.md)
  - [Editing model](project.md#editing-model)
  - [Musical time](project.md#musical-time)
  - [Range edits](project.md#range-edits)
  - [Undo history](project.md#undo-history)
  - [Named versions](project.md#named-versions)
  - [Routing](project.md#routing)
  - [Project bundle](project.md#project-bundle)
  - [Projects folder](project.md#projects-folder)
  - [Saved format](project.md#saved-format)
  - [Saving](project.md#saving)
  - [Loading](project.md#loading)
  - [Cleanup](project.md#cleanup)
  - [Application settings](project.md#application-settings)
  - [Stored audio](project.md#stored-audio)
- [Recording](recording.md)
  - [Input monitoring](recording.md#input-monitoring)
  - [Click track](recording.md#click-track)
  - [Recording alignment](recording.md#recording-alignment)
  - [Capture](recording.md#capture)
  - [Count-in](recording.md#count-in)
  - [Writer thread](recording.md#writer-thread)
  - [Take placement](recording.md#take-placement)
- [Signal Graph](signal-graph.md)
  - [Processing graph](signal-graph.md#processing-graph)
  - [Feedback loops](signal-graph.md#feedback-loops)
  - [Channel layouts](signal-graph.md#channel-layouts)
  - [Panning](signal-graph.md#panning)
  - [Mixer](signal-graph.md#mixer)
  - [Effect nodes](signal-graph.md#effect-nodes)
  - [Port types](signal-graph.md#port-types)
  - [Delay compensation](signal-graph.md#delay-compensation)
- [Startup](startup.md)
  - [Application startup](startup.md#application-startup)
- [Developer Tooling](tooling.md)
  - [Repository tasks (xtask)](tooling.md#repository-tasks-xtask)
  - [Commit gate](tooling.md#commit-gate)
- [User Interface](user-interface.md)
  - [UI toolkit](user-interface.md#ui-toolkit)
  - [Interface structure](user-interface.md#interface-structure)
  - [Layout](user-interface.md#layout)
  - [Icons](user-interface.md#icons)
  - [Patch board](user-interface.md#patch-board)
  - [Transport and seeking](user-interface.md#transport-and-seeking)
  - [Gestures](user-interface.md#gestures)
  - [Range selection and shortcuts](user-interface.md#range-selection-and-shortcuts)
  - [Recording](user-interface.md#recording)
  - [Files and devices](user-interface.md#files-and-devices)

<!-- END GENERATED INDEX -->
