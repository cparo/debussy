# Architectural Decision Records

This directory records **why** a concrete architectural decision was made: the alternatives
considered, the trade-offs accepted, and the findings that forced a choice. It complements, and is
deliberately separate from, the [architectural specs](../architecture/README.md) which answer
**how** the system is built and link back to these ADRs rather than restating their reasoning.

## Conventions

- **One file per decision**, at `NNNN-slug.md`, numbered zero-padded and monotonically increasing.
  Numbers are **never reused**. New records append to the end of the sequence regardless of which
  architecture section they support.
- Start new records with `cargo xtask new-adr "<decision>" --decided-by <value>`, which fills
  [`0000-template.md`](0000-template.md), takes the next number, and adds the index row.
- **Decided by** is the name of the person who decided, `agent, ratified by <who>`,
  or `agent, unratified`: so it is clear who signed off on each decision.
  `cargo xtask new-adr --decided-by human` fills in the name from `git config user.name`.
- **Status** is `Accepted` or `Superseded by ADR-NNNN`. A record is never deleted or edited to
  reverse its own decision — write a new record and mark the old one superseded.
- Link related records with `[ADR-NNNN](NNNN-slug.md)`; link back to the section under
  [`docs/architecture/`](../architecture/README.md) that the decision supports.

## Index

| ADR | Title | Status | Architecture section |
|---|---|---|---|
| [ADR-0001](0001-debussy-is-licensed-mit-or-apache-2-0-and-accepts.md) | Debussy is licensed MIT OR Apache-2.0 and accepts only permissively licensed dependencies | Accepted | [Project licence](../architecture/licensing.md#project-licence) |
| [ADR-0002](0002-repository-tasks-are-built-in-house-on-std-and.md) | Repository tasks are built in-house on std and anyhow | Accepted | [Repository tasks (xtask)](../architecture/tooling.md#repository-tasks-xtask) |
| [ADR-0003](0003-the-user-interface-is-built-with-iced.md) | The user interface is built with iced | Accepted | [UI toolkit](../architecture/user-interface.md#ui-toolkit) |
| [ADR-0004](0004-mp3-and-aac-go-through-apple-audiotoolbox.md) | MP3 and AAC go through Apple AudioToolbox | Superseded by ADR-0024 | [Compressed audio codecs](../architecture/platform.md#compressed-audio-codecs) |
| [ADR-0005](0005-the-engine-processes-audio-as-32-bit-float.md) | The engine processes audio as 32-bit float throughout | Accepted | [Sample format](../architecture/engine.md#sample-format) |
| [ADR-0006](0006-each-project-has-one-fixed-sample-rate.md) | Each project has one fixed sample rate | Accepted | [Sample rate](../architecture/engine.md#sample-rate) |
| [ADR-0007](0007-the-engine-targets-a-128-frame-default-buffer.md) | The engine targets a 128-frame default buffer | Accepted | [Buffer size](../architecture/engine.md#buffer-size) |
| [ADR-0008](0008-project-audio-is-streamed-from-disk.md) | Project audio is streamed from disk | Accepted | [Audio storage](../architecture/engine.md#audio-storage) |
| [ADR-0009](0009-audio-device-i-o-uses-cpal-behind-our-own.md) | Audio device I/O uses cpal behind our own interface | Accepted | [Audio device I/O](../architecture/platform.md#audio-device-io) |
| [ADR-0010](0010-real-time-code-is-checked-by-an-allocator-guard.md) | Real-time code is checked by an allocator guard and clippy bans | Accepted | [Real-time thread](../architecture/engine.md#real-time-thread) |
| [ADR-0011](0011-editing-is-non-destructive-over-immutable-audio.md) | Editing is non-destructive over immutable audio files | Accepted | [Editing model](../architecture/project.md#editing-model) |
| [ADR-0012](0012-undo-is-unlimited-per-session-and-named-versions.md) | Undo is unlimited per session and named versions persist earlier states | Accepted | [Undo history](../architecture/project.md#undo-history) |
| [ADR-0013](0013-a-project-is-a-bundle-directory-with-versioned.md) | A project is a bundle directory with versioned JSON metadata | Accepted | [Project bundle](../architecture/project.md#project-bundle) |
| [ADR-0014](0014-project-audio-is-stored-as-mono-32-bit-float-wav.md) | Project audio is stored as mono 32-bit float WAV | Accepted | [Stored audio](../architecture/project.md#stored-audio) |
| [ADR-0015](0015-routing-is-one-general-processing-graph-per.md) | Routing is one general processing graph per project | Accepted | [Processing graph](../architecture/signal-graph.md#processing-graph) |
| [ADR-0016](0016-feedback-loops-must-pass-through-a-fixed-delay.md) | Feedback loops must pass through a fixed-delay node | Accepted | [Feedback loops](../architecture/signal-graph.md#feedback-loops) |
| [ADR-0017](0017-signals-are-mono-or-stereo-only.md) | Signals are mono or stereo only | Superseded by ADR-0030 | [Channel layouts](../architecture/signal-graph.md#channel-layouts) |
| [ADR-0018](0018-panning-uses-a-3-db-constant-power-law.md) | Panning uses a -3 dB constant-power law | Accepted | [Panning](../architecture/signal-graph.md#panning) |
| [ADR-0019](0019-only-built-in-effects-for-now-behind-a-plugin.md) | Only built-in effects for now, behind a plugin-capable node interface | Accepted | [Effect nodes](../architecture/signal-graph.md#effect-nodes) |
| [ADR-0020](0020-ports-and-track-contents-are-typed-so-midi-can-be.md) | Ports and track contents are typed so MIDI can be added later | Accepted | [Port types](../architecture/signal-graph.md#port-types) |
| [ADR-0021](0021-inputs-can-be-monitored-through-the-software-per.md) | Inputs can be monitored through the software per track | Accepted | [Input monitoring](../architecture/recording.md#input-monitoring) |
| [ADR-0022](0022-the-engine-compensates-recording-and-effect.md) | The engine compensates recording and effect latency automatically | Accepted | [Recording alignment](../architecture/recording.md#recording-alignment) |
| [ADR-0023](0023-expired-or-royalty-free-patents-may-be-implemented.md) | Expired or royalty-free patents may be implemented; others need a licensing SDK or review | Accepted | [Patent policy](../architecture/patents.md#patent-policy) |
| [ADR-0024](0024-compressed-codecs-go-through-audiotoolbox-and-mp3.md) | Compressed codecs go through AudioToolbox and MP3 export is deferred | Accepted | [Compressed audio codecs](../architecture/platform.md#compressed-audio-codecs) |
| [ADR-0025](0025-wav-is-built-in-house-and-integer-exports-are.md) | WAV is built in-house and integer exports are dithered | Accepted | [WAV files](../architecture/import-export.md#wav-files) |
| [ADR-0026](0026-the-workspace-is-split-into-seventeen-crates-in.md) | The workspace is split into seventeen crates in five dependency tiers | Accepted | [Crate map](../architecture/crates.md#crate-map) |
| [ADR-0027](0027-the-commit-guard-parses-shell-commands-with-python.md) | The commit guard parses shell commands with Python's standard library | Accepted | [Commit gate](../architecture/tooling.md#commit-gate) |
| [ADR-0028](0028-unsafe-is-allowed-in-audio-buffers-queue-module.md) | `unsafe` is allowed in audio_buffers' queue module only | Accepted | [Dependency rules](../architecture/crates.md#dependency-rules) |
| [ADR-0029](0029-the-wav-reader-accepts-only-mono-and-stereo-files.md) | The WAV reader accepts only mono and stereo files and bounds every size it reads | Accepted | [WAV files](../architecture/import-export.md#wav-files) |
| [ADR-0030](0030-signals-are-mono-or-stereo-and-the-mono-mix-is-the.md) | Signals are mono or stereo, and the mono mix is the channel sum scaled by -3 dB | Accepted | [Channel layouts](../architecture/signal-graph.md#channel-layouts) |
| [ADR-0031](0031-feedback-delays-are-at-least-1-ms.md) | Feedback delays are at least 1 ms | Accepted | [Feedback loops](../architecture/signal-graph.md#feedback-loops) |
| [ADR-0032](0032-monitored-input-is-not-delayed-for-delay.md) | Monitored input is not delayed for delay compensation | Accepted | [Input monitoring](../architecture/recording.md#input-monitoring) |
| [ADR-0033](0033-github-actions-are-pinned-to-commit-shas-and.md) | GitHub Actions are pinned to commit SHAs and updated by Dependabot | Accepted | [Commit gate](../architecture/tooling.md#commit-gate) |
| [ADR-0034](0034-disk-streaming-reads-2-s-ahead-on-one-thread.md) | Disk streaming reads 2 s ahead on one thread, primes before playing, and reports underruns | Accepted | [Disk streaming](../architecture/engine.md#disk-streaming) |
| [ADR-0035](0035-the-dither-noise-source-is-xorshift64-seeded-by.md) | The dither noise source is xorshift64* seeded by SplitMix64 | Accepted | [Patent policy](../architecture/patents.md#patent-policy) |
| [ADR-0036](0036-sample-rate-conversion-is-exact-rational-polyphase.md) | Sample-rate conversion is exact rational polyphase filtering without drift correction | Accepted | [Sample rate](../architecture/engine.md#sample-rate) |
| [ADR-0037](0037-overlapping-clips-the-later-covers-with-a-short.md) | Overlapping clips: the later covers with a short crossfade; clips have gain and fades; undo is per gesture; time is in frames | Accepted | [Editing model](../architecture/project.md#editing-model) |
| [ADR-0038](0038-processing-plans-are-swapped-over-queues.md) | Processing plans are swapped over queues, parameters are smoothed atomics, and only cycles run in sub-blocks | Accepted | [Processing graph](../architecture/signal-graph.md#processing-graph) |
| [ADR-0039](0039-the-fft-is-radix-2-cooley-tukey-with-real-input.md) | The FFT is radix-2 Cooley-Tukey with real-input packing and a Hann-windowed STFT | Accepted | [Patent policy](../architecture/patents.md#patent-policy) |
| [ADR-0040](0040-sources-and-sinks-are-graph-nodes-declarations-are.md) | Sources and sinks are graph nodes, declarations are fixed when a node is added, and loop-closing connections are not compensated | Accepted | [Delay compensation](../architecture/signal-graph.md#delay-compensation) |
| [ADR-0041](0041-replacing-a-node-keeps-its-id-compatible.md) | Replacing a node keeps its id, compatible connections and parameter values by name | Accepted | [Processing graph](../architecture/signal-graph.md#processing-graph) |
| [ADR-0042](0042-tracks-have-a-fixed-layout-with-paired-mono-files.md) | Tracks have a fixed layout with paired mono files for stereo, clips stack in layers, and edits return their inverse | Accepted | [Editing model](../architecture/project.md#editing-model) |
| [ADR-0043](0043-routing-is-undoable-project-state-tracks-are-bare.md) | Routing is undoable project state: tracks are bare sources, one fixed output, effects by kind id with parameters by name | Accepted | [Routing](../architecture/project.md#routing) |
| [ADR-0044](0044-mixing-is-a-mixer-node-with-per-channel-gain-pan.md) | Mixing is a mixer node with per-channel gain, pan and a mute/solo mode | Accepted | [Mixer](../architecture/signal-graph.md#mixer) |
| [ADR-0045](0045-waveform-summaries-and-spectrogram-overviews-are.md) | Waveform summaries and spectrogram overviews are cached per audio file in disposable binary formats | Accepted | [Analysis cache files](../architecture/analysis.md#analysis-cache-files) |
| [ADR-0046](0046-the-engine-renders-clips-on-the-reader-thread-and.md) | The engine renders clips on the reader thread and accepts clip edits only while stopped | Accepted | [Clip playback](../architecture/engine.md#clip-playback) |
| [ADR-0047](0047-audio-engine-defines-its-feeds-and-disk-streaming.md) | audio_engine defines its feeds, and disk_streaming implements them | Accepted | [Clip playback](../architecture/engine.md#clip-playback) |
| [ADR-0048](0048-project-files-are-serialized-with-serde-and-serde.md) | Project files are serialized with serde and serde_json | Accepted | [Dependency rules](../architecture/crates.md#dependency-rules) |
| [ADR-0049](0049-saved-projects-use-their-own-format-types-crash.md) | Saved projects use their own format types, crash-safe renames, timestamped versions and checked loads | Accepted | [Saved format](../architecture/project.md#saved-format) |
| [ADR-0050](0050-cpal-is-added-and-dependency-audits-cover-apple.md) | cpal is added and dependency audits cover Apple targets only | Accepted | [Dependency licences](../architecture/licensing.md#dependency-licences) |
| [ADR-0051](0051-audiotoolbox-codecs-use-objc2-bindings-unsafe.md) | AudioToolbox codecs use objc2 bindings, unsafe confined to sys, and checked in-process decoding | Accepted | [Compressed audio codecs](../architecture/platform.md#compressed-audio-codecs) |
| [ADR-0052](0052-a-duplex-stream-is-two-cpal-streams-joined-by-a.md) | A duplex stream is two cpal streams joined by a sample ring | Accepted | [Audio device I/O](../architecture/platform.md#audio-device-io) |
| [ADR-0053](0053-imports-are-named-after-their-file-and-exports-use.md) | Imports are named after their file and exports use a reproducible dither | Accepted | [Import](../architecture/import-export.md#import) |
| [ADR-0054](0054-a-new-engine-bridge-crate-keeps-the-running-graph.md) | A new engine_bridge crate keeps the running graph in step by diffing checked states | Accepted | [Routing](../architecture/project.md#routing) |
| [ADR-0055](0055-iced-0-14-is-added-the-msrv-rises-to-1-90-and-cc0.md) | iced 0.14 is added, the MSRV rises to 1.90, and CC0 is allowed | Accepted | [UI toolkit](../architecture/user-interface.md#ui-toolkit) |
| [ADR-0056](0056-the-interface-owns-the-session-and-reaches-i-o.md) | The interface owns the session and reaches I/O through a Services trait | Accepted | [Interface structure](../architecture/user-interface.md#interface-structure) |
| [ADR-0057](0057-the-first-binary-imports-and-exports-from-the.md) | The first binary imports and exports from the command line and plays through the default output | Accepted | [Application startup](../architecture/user-interface.md#application-startup) |
| [ADR-0058](0058-stereo-ports-are-connected-by-side-and-new-tracks.md) | Stereo ports are connected by side, and new tracks join a project mixer | Accepted | [Routing](../architecture/project.md#routing) |
| [ADR-0059](0059-the-window-imports-exports-and-picks-one-audio.md) | The window imports, exports and picks one audio device, remembered in app settings | Accepted | [Files and devices](../architecture/user-interface.md#files-and-devices) |
| [ADR-0060](0060-the-window-is-laid-out-as-top-bar-track-pane.md) | The window is laid out as top bar, track pane beside the patch board, and bottom bar, with seeking that sets the return point | Accepted | [Layout](../architecture/user-interface.md#layout) |
| [ADR-0061](0061-node-positions-and-the-outputs-height-are-saved-as.md) | Node positions and the outputs' height are saved as optional fields, and the patch board edits the routing | Accepted | [Saved format](../architecture/project.md#saved-format) |
| [ADR-0062](0062-zooms-finer-than-the-waveform-summaries-are-drawn.md) | Zooms finer than the waveform summaries are drawn from the audio on every refresh, with no tile cache | Accepted | [Transport and seeking](../architecture/user-interface.md#transport-and-seeking) |
| [ADR-0063](0063-while-playing-the-view-pages-along-with-a-visible.md) | While playing, the view pages along with a visible playhead, from 80 % of the lane's width to 20 % | Accepted | [Transport and seeking](../architecture/user-interface.md#transport-and-seeking) |
| [ADR-0064](0064-the-intensity-graded-trace-may-be-built-in-house.md) | The intensity-graded trace may be built in-house: no patent in force was found to read on it | Accepted | [Patent policy](../architecture/patents.md#patent-policy) |
| [ADR-0065](0065-zooms-finer-than-the-waveform-summaries-are-drawn.md) | Zooms finer than the waveform summaries are drawn as a time-in-level trace, normalised per column through a square root | Accepted | [Waveform summaries](../architecture/analysis.md#waveform-summaries) |
| [ADR-0066](0066-zoom-out-stops-at-an-exact-fit-of-the-project.md) | Zoom out stops at an exact fit of the project | Accepted | [Transport and seeking](../architecture/user-interface.md#transport-and-seeking) |
| [ADR-0067](0067-the-project-is-saved-after-every-edit.md) | The project is saved after every edit | Accepted | [Saving](../architecture/project.md#saving) |
| [ADR-0068](0068-lanes-are-rasterised-in-memory-and-blurred-with-a.md) | Lanes are rasterised in memory and blurred with a Gaussian, drawn through iced's image support | Accepted | [Layout](../architecture/user-interface.md#layout) |
| [ADR-0069](0069-panning-stops-at-the-later-of-the-project-s-end.md) | Panning stops at the later of the project's end and the playhead | Superseded by ADR-0077 | [Transport and seeking](../architecture/user-interface.md#transport-and-seeking) |
| [ADR-0070](0070-icons-are-svg-files-drawn-with-iced-s-svg-feature.md) | Icons are SVG files drawn with iced's svg feature | Accepted | [Icons](../architecture/user-interface.md#icons) |
| [ADR-0071](0071-the-audio-colour-is-orange-on-dark-chrome-and-blue.md) | The audio colour is orange on dark chrome and blue on light | Accepted | [Layout](../architecture/user-interface.md#layout) |
| [ADR-0072](0072-toolbar-controls-show-icons-named-in-tooltips.md) | Toolbar controls show icons named in tooltips | Accepted | [Icons](../architecture/user-interface.md#icons) |
| [ADR-0073](0073-range-edits-split-clips-without-new-fades-never.md) | Range edits split clips without new fades, never ripple, and paste as an overlay | Accepted | [Range edits](../architecture/project.md#range-edits) |
| [ADR-0074](0074-a-drag-in-the-lanes-selects-a-range-and-edit.md) | A drag in the lanes selects a range, and edit shortcuts act on it | Accepted | [Range selection and shortcuts](../architecture/user-interface.md#range-selection-and-shortcuts) |
| [ADR-0075](0075-the-playhead-time-reads-hh-mm-ss-000-dropping.md) | The playhead time reads HH:MM:SS.000, dropping leading units a short project does not need | Accepted | [Transport and seeking](../architecture/user-interface.md#transport-and-seeking) |
| [ADR-0076](0076-playback-stops-by-itself-at-the-project-s-end.md) | Playback stops by itself at the project's end | Accepted | [Transport and seeking](../architecture/user-interface.md#transport-and-seeking) |
| [ADR-0077](0077-every-view-move-stops-at-the-scroll-bound.md) | Every view move stops at the scroll bound | Accepted | [Transport and seeking](../architecture/user-interface.md#transport-and-seeking) |
| [ADR-0078](0078-recording-captures-armed-inputs-on-the-real-time.md) | Recording captures armed inputs on the real-time thread and writes takes on a separate thread | Accepted | [Capture](../architecture/recording.md#capture) |
| [ADR-0079](0079-arm-state-is-session-only-and-inputs-open-only.md) | Arm state is session-only and inputs open only while a track is armed | Accepted | [Recording](../architecture/user-interface.md#recording) |
| [ADR-0080](0080-record-and-arm-are-lit-in-a-fixed-recording-red.md) | Record and arm are lit in a fixed recording red and the audio device is picked in the bottom bar | Accepted | [Layout](../architecture/user-interface.md#layout) |
| [ADR-0081](0081-a-lost-audio-device-s-engine-is-rebuilt-from-the.md) | A lost audio device's engine is rebuilt from the project | Accepted | [Files and devices](../architecture/user-interface.md#files-and-devices) |
| [ADR-0082](0082-the-project-has-one-tempo-and-meter-saved-in.md) | The project has one tempo and meter, saved in project format 4 | Accepted | [Musical time](../architecture/project.md#musical-time) |
| [ADR-0083](0083-the-click-is-mixed-at-the-output-stage-from.md) | The click is mixed at the output stage from bundled CC0 samples | Accepted | [Click track](../architecture/recording.md#click-track) |
| [ADR-0084](0084-recording-s-count-in-splits-the-block-it-ends-in.md) | Recording's count-in splits the block it ends in | Accepted | [Count-in](../architecture/recording.md#count-in) |
| [ADR-0085](0085-the-bars-and-beats-grid-is-a-sharp-overlay-thinned.md) | The bars and beats grid is a sharp overlay thinned by zoom | Accepted | [Layout](../architecture/user-interface.md#layout) |
| [ADR-0086](0086-the-bottom-bar-opens-the-audio-device-list-and-the.md) | The bottom bar opens the audio device list and the click dialog from icon buttons | Accepted | [Layout](../architecture/user-interface.md#layout) |
| [ADR-0087](0087-a-parameter-declares-its-unit-scale-and-group-so-a.md) | A parameter declares its unit, scale and group so a generic control can be drawn for it | Accepted | [Effect nodes](../architecture/signal-graph.md#effect-nodes) |
| [ADR-0088](0088-equalisers-are-built-on-a-trapezoidal-state.md) | Equalisers are built on a trapezoidal state-variable filter, and a filter's order changes only while stopped | Accepted | [Filters](../architecture/effects.md#filters) |
| [ADR-0089](0089-the-mixer-dialog-shows-narrow-numbered-strips-with.md) | The mixer dialog shows narrow numbered strips with vertical gain and pan and a channel-count field | Accepted | [Layout](../architecture/user-interface.md#layout) |
| [ADR-0090](0090-the-interface-opens-in-the-dark-tone-and-the-light.md) | The interface opens in the dark tone and the light-tone logo is sage green | Accepted | [Layout](../architecture/user-interface.md#layout) |
| [ADR-0091](0091-effect-displays-are-registered-per-effect-kind-and.md) | Effect displays are registered per effect kind and draw RGBA pixels from parameter values and the tone | Accepted | [Effect displays](../architecture/user-interface.md#effect-displays) |
| [ADR-0092](0092-equaliser-displays-plot-the-exact-digital.md) | Equaliser displays plot the exact digital magnitude response at the project sample rate on fixed axes, labelled in an in-house stroke font behind a Typeface seam | Accepted | [Equaliser displays](../architecture/user-interface.md#equaliser-displays) |
| [ADR-0093](0093-effect-controls-show-slider-values-as-hover.md) | Effect controls show slider values as hover tooltips and two-valued parameters as unlabelled two-button strips | Accepted | [Layout](../architecture/user-interface.md#layout) |

