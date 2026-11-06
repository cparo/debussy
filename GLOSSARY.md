# Debussy

A digital audio workstation for home-studio musicians who record and mix live performances. These
are the words the project uses for what users see and do.

## Projects and editing

**Project**:
One piece of work: its tracks, clips, mix, and named versions, at one project sample rate.
_Avoid_: session, song, document

**Project sample rate**:
The single sample rate all of a project's audio is stored and mixed at, chosen when the project is
created.

**Track**:
A lane on the timeline holding clips and feeding the mix. Mono or stereo.
_Avoid_: channel (reserved for the left/right channels of a signal)

**Clip**:
A placed section of source audio on a track: which audio, from where in it, for how long, and when
it plays.
_Avoid_: region, segment, item

**Source audio**:
Recorded or imported audio a project holds. It is never altered by editing.
_Avoid_: media, asset

**Take**:
The source audio captured in one pass of recording on a track.

**Named version**:
A saved snapshot of a project's state that a user names, and can restore or start a new project
from.
_Avoid_: snapshot, checkpoint, revision

**Undo history**:
The edits made since the project was opened, which can be undone and redone. It is not kept after
the project closes.

**Cleanup**:
The explicit action that deletes source audio no clip or named version uses.

## Recording

**Armed track**:
A track that will record from its input when recording starts.

**Monitoring**:
Hearing a track's live input through Debussy, with its effects, while recording.
_Avoid_: input echo, playthrough

**Buffer size**:
How many frames of audio Debussy processes at a time. It sets monitoring latency.

## Mixing and routing

**Mixer**:
The console view of the project's routing: track levels, pan, mute and solo.

**Patch board**:
The view in which users connect tracks, effects, buses and outputs freely.
_Avoid_: patch bay, router

**Routing**:
The connections between a project's tracks, effects, buses and outputs. The mixer and patch board
both show it.
_Avoid_: signal chain (one path through the routing, not all of it)

**Effect**:
A built-in processor placed in the routing, such as an equalizer, compressor or reverb.
_Avoid_: plugin (reserved for third-party processors, which are not supported), filter, FX

**Bus**:
A point in the routing where several signals are summed before going on.
_Avoid_: group, submix

**Aux send**:
A tap that sends a copy of a track's signal to a bus, alongside its main path.

**Feedback delay**:
The delay a user sets wherever the routing loops back on itself. Every loop needs one.

**Stereo mix**:
The project's output. The **mono mix** is derived from it.
_Avoid_: master, bounce

## Import and export

**Import**:
Bringing an audio file into a project as source audio, converted to the project sample rate.

**Export**:
Rendering the stereo or mono mix to an audio file.
_Avoid_: bounce, mixdown
