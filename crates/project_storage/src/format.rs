//! The saved format of `project.json` and of each file in `versions/`
//! (`docs/architecture/project.md` "Saved format", ADR-0049).
//!
//! These types are the format; `project_model`'s types are not serialized directly, so a change
//! to the model never silently changes the file. Every field is required and unknown fields are
//! refused. Conversions to the model are in `validate`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The format version this build writes. Older versions are migrated on load (`migrate`).
pub const FORMAT_VERSION: u32 = 2;

/// A whole project file: `project.json`, or a named version with `name` and `saved_at` set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedProject {
    pub format_version: u32,
    /// For a named version: its name. Absent in `project.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// For a named version: when it was first saved, as an ISO 8601 basic UTC stamp; also its
    /// file name. Absent in `project.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<String>,
    /// The project sample rate, in hertz.
    pub sample_rate: u32,
    /// Tracks, top of the arrangement first.
    pub tracks: Vec<SavedTrack>,
    pub nodes: Vec<SavedNode>,
    pub connections: Vec<SavedConnection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SavedLayout {
    Mono,
    Stereo,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedTrack {
    pub id: u64,
    pub name: String,
    pub layout: SavedLayout,
    /// Clips from the bottom layer up.
    pub clips: Vec<SavedClip>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedFile {
    /// The file's name within `audio/`.
    pub name: String,
    pub frames: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SavedSource {
    Mono { file: SavedFile },
    Stereo { left: SavedFile, right: SavedFile },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedClip {
    pub id: u64,
    pub source: SavedSource,
    /// All times are frames at the project rate.
    pub offset: u64,
    pub length: u64,
    pub position: u64,
    pub gain_db: f32,
    pub fade_in: u64,
    pub fade_out: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedNode {
    pub id: u64,
    pub kind: SavedNodeKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SavedNodeKind {
    /// A built-in effect by stable id, with parameter values by name.
    Effect {
        effect: String,
        parameters: BTreeMap<String, f32>,
    },
    Mixer {
        channels: usize,
        parameters: BTreeMap<String, f32>,
    },
    Bus {
        layout: SavedLayout,
    },
    FeedbackDelay {
        layout: SavedLayout,
        seconds: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum SavedEndpoint {
    Track { id: u64 },
    Output,
    Node { id: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SavedSide {
    Left,
    Right,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedConnection {
    pub id: u64,
    pub from: SavedEndpoint,
    pub output: usize,
    pub to: SavedEndpoint,
    pub input: usize,
    pub pan: f32,
    /// Since version 2: the side of a stereo output taken; absent for the whole port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_side: Option<SavedSide>,
    /// Since version 2: the side of a stereo input fed; absent for the whole port.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_side: Option<SavedSide>,
}
