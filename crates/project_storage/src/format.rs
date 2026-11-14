//! The saved format of `project.json` and of each file in `versions/`
//! (`docs/architecture/project.md` "Saved format", ADR-0049).
//!
//! These types are the format; `project_model`'s types are not serialized directly, so a change
//! to the model never silently changes the file. Every field is required and unknown fields are
//! refused. Conversions to the model are in `validate`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The format version this build writes. Older versions are migrated on load (`migrate`).
pub const FORMAT_VERSION: u32 = 5;

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
    /// Since version 3: the height of the project output's L on the patch board; absent for
    /// the automatic layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_y: Option<f32>,
    /// Since version 4: the project's tempo and meter.
    pub tempo: SavedTempo,
    /// Since version 5: the automation curves, in lane order.
    pub curves: Vec<SavedCurve>,
}

/// An automation curve lane (`docs/architecture/automation.md` "Curves in the project").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedCurve {
    pub id: u64,
    pub name: String,
    /// Anchor points in frame order.
    pub points: Vec<SavedPoint>,
}

/// An anchor point of a curve, with its handles as offsets from it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedPoint {
    /// The timeline frame, at the project rate.
    pub frame: i64,
    /// The normalised value, 0 to 1.
    pub value: f32,
    pub in_handle: SavedHandle,
    pub out_handle: SavedHandle,
}

/// A handle's offset from its anchor.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedHandle {
    pub d_frame: f64,
    pub d_value: f32,
}

/// The tempo and meter (`docs/architecture/project.md` "Musical time").
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedTempo {
    pub bpm: f64,
    pub beats_per_bar: u8,
    pub beat_unit: u8,
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
    /// Since version 3: where the node sits on the patch board; absent for the automatic
    /// layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<SavedPosition>,
    /// Since version 5: the curve port number, from 1, of each assigned parameter, by name.
    pub curve_ports: BTreeMap<String, u32>,
}

/// A point on the patch board, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedPosition {
    pub x: f32,
    pub y: f32,
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
    Track {
        id: u64,
    },
    Output,
    Node {
        id: u64,
    },
    /// Since version 5: an automation curve.
    Curve {
        id: u64,
    },
    /// Since version 5: curve port `port` of node `node`.
    CurvePort {
        node: u64,
        port: u32,
    },
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
