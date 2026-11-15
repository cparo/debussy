//! Identifiers, track kinds, and what clips play.

use std::fmt;
use std::sync::Arc;

use audio_buffers::Layout;

use crate::midi::Sequence;

/// Identifies a track. Ids are never reused within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TrackId(pub u64);

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "track {}", self.0)
    }
}

/// Identifies a clip. Ids are never reused within a session, and a clip keeps its id when it
/// moves between tracks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClipId(pub u64);

impl fmt::Display for ClipId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "clip {}", self.0)
    }
}

/// Identifies an automation curve. Ids are never reused within a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CurveId(pub u64);

impl fmt::Display for CurveId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "curve {}", self.0)
    }
}

/// One stored mono audio file of the project (`docs/architecture/project.md` "Stored audio").
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AudioFile {
    /// The file's name within the bundle's `audio/` directory.
    pub name: String,
    /// Its length, in frames.
    pub frames: u64,
}

/// What a track holds: audio clips of one channel layout, or MIDI clips (ADR-0144).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackKind {
    /// Audio clips, mono or stereo.
    Audio(Layout),
    /// MIDI clips; the track's output is an event port.
    Midi,
}

/// What a clip plays: one mono file, a left and a right mono file read together, or a shared
/// MIDI sequence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// One channel.
    Mono(AudioFile),
    /// Two channels, read at the same offset.
    Stereo {
        /// The left channel.
        left: AudioFile,
        /// The right channel.
        right: AudioFile,
    },
    /// A MIDI sequence, shared by every clip cut from the same one.
    Midi(Arc<Sequence>),
}

impl Source {
    /// The kind of track this source plays on.
    pub fn kind(&self) -> TrackKind {
        match self {
            Source::Mono(_) => TrackKind::Audio(Layout::Mono),
            Source::Stereo { .. } => TrackKind::Audio(Layout::Stereo),
            Source::Midi(_) => TrackKind::Midi,
        }
    }

    /// The frames every channel has; for a stereo pair, the shorter file's length; for MIDI, the
    /// sequence's length.
    pub fn frames(&self) -> u64 {
        match self {
            Source::Mono(file) => file.frames,
            Source::Stereo { left, right } => left.frames.min(right.frames),
            Source::Midi(sequence) => sequence.frames(),
        }
    }

    /// The audio files the source reads; none for MIDI.
    pub fn files(&self) -> impl Iterator<Item = &AudioFile> {
        let (first, second) = match self {
            Source::Mono(file) => (Some(file), None),
            Source::Stereo { left, right } => (Some(left), Some(right)),
            Source::Midi(_) => (None, None),
        };
        first.into_iter().chain(second)
    }
}
