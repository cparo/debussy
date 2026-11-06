# Product Brief: Debussy

A digital audio workstation designed as a basic-but-powerful tool for amateur musicians.

## Whos is this for?

Our first and primary user is a "home studio" musician who will use this software to record and
process audio from live band performances and solo tracking sessions.

They are _not_ producers of purely-electronic "programmed" music, nor artists who deal primarily
in sample-based arrangements.

While AI is being used to aid in the development of this product, we expect its end users to center
human-driven composition and performance in their own creative process. ML-powered features may be
introduced down the line - but only after a core baseline of what would have been seen as critical
to a 1980s recording studio has been covered, and only in forms which center the useful purpose of
the feature (ex: "recovery of clipped audio" or "isolation of individual instruments") vs. its use
of AI or ML as an underlying implementation detail.

## How will they use it?

We aim to build this application following an "MVP" approach where we quickly get to _something_
useful and then horizontally build out its functionality from there to cover a growing range of
users and use-cases.

### Initial use case

Our initial use case is the straightforward capture and mixing of multi-track audio for export.

At this stage, a user should be able to:

* import from WAV, FLAC, mp3, and aac
* record new (mono and stereo) tracks, selecting from available input devices
* see recorded audio channels as waveforms or spectrograms
* configure mixing across recorded channels
* "mute" or "solo" channels in the mixer
* play back the mixed results (in a mono or stereo mix), selecting from available output devices
* export to WAV, FLAC, mp3, or aac

### Improved support for studio tracking

To better support "tracking" of individual parts for a project, we'll want to provide support for:

* live playback of already-recorded audio while recording
* playing a click track while recording

### Expanded audio processing feature set

The value of this application can be enhanced by introducing the following core set of effects
filters:

* high-pass, low-pass, and fully-parametric equalizers
* compressor/limiter/gate
* amplifier simulation (gain and tone modification)
* delay/reverb
* chorus/flanger

These filters should become configurable on a "patch board" which can be used to arrange them and
the connections between them to form flexible "effects chains" between the recorded audio tracks
and the project's stereo output channels for playback or export.

### Non-linear editing functionality

More advanced production can be unlocked for our users by giving them the following editing powers:

* time-shifting of tracks
* cut/copy/paste of selected ranges of one or more tracks
* trimming of audio tracks

### Expression curves for effects parameters

Editable Bezier curves for the value of effects parameters (ex: gain, EQ cutoff threshold) over
time will give our users the ability to vary the application of an effect over time rather than
having it fixed for the entire recorded piece.

### MIDI support, synthesis, and sampling

These features will add greatly to the flexibility of this application's use, but are less core to
our initial user's workflow than the recording and processing of audio from microphones and line
outputs driven by physical instruments.

## What will they run it on?

This software will be designed to run on MacOS devices with Apple Silicon and access to Apple's
"Core Audio" frameworks.

It should be able to record from and play back to built-in devices (ex: a laptop's microphone and
headphone port), or external audio interfaces (ex: a MOTU M6 or Focusrite Scarlett Solo interface).

Storage of project data and metadata will be local to the filesystem of the machine running the
application.

Clear seams should be maintained between OS-provided and hardware-specific modules and the
core logic of this application such that future extension to support additional platforms (possibly
with different available features due to technical or licensing limitations) is possible without
modification to our core logic - but no such support for additional platforms is explicitly planned
at this time.
