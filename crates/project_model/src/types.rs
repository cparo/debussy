//! Identifiers and references to stored audio.

use std::fmt;

use audio_buffers::Layout;

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

/// The audio a clip plays: one mono file, or a left and a right mono file read together.
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
}

impl Source {
    /// The channel layout this source plays as.
    pub fn layout(&self) -> Layout {
        match self {
            Source::Mono(_) => Layout::Mono,
            Source::Stereo { .. } => Layout::Stereo,
        }
    }

    /// The frames every channel has; for a stereo pair, the shorter file's length.
    pub fn frames(&self) -> u64 {
        match self {
            Source::Mono(file) => file.frames,
            Source::Stereo { left, right } => left.frames.min(right.frames),
        }
    }

    /// The files the source reads.
    pub fn files(&self) -> impl Iterator<Item = &AudioFile> {
        let (first, second) = match self {
            Source::Mono(file) => (file, None),
            Source::Stereo { left, right } => (left, Some(right)),
        };
        std::iter::once(first).chain(second)
    }
}
