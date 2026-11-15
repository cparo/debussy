//! The recording marker, `recording.json`, and recovering a pass a crash cut off
//! (`docs/architecture/recording.md` "Take recovery", `docs/architecture/project.md` "Loading").
//!
//! The marker is untrusted input: it is read within fixed limits, and a marker that breaks any
//! of them is discarded, never refusing the bundle.

use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Read};
use std::path::Path;

use project_model::{Session, TrackId, TrackKind};
use serde::{Deserialize, Serialize};
use wav_files::{Channels, WavReader, repair_sizes};

use crate::validate::{MAX_TRACKS, is_plain_file_name};

/// The marker's file name in the bundle.
pub(crate) const RECORDING_FILE: &str = "recording.json";
/// The largest marker read, in bytes.
pub const MAX_RECORDING_BYTES: u64 = 1024 * 1024;
/// The marker format version this build writes and reads.
const RECORDING_VERSION: u32 = 1;

/// The saved marker: the format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedRecording {
    pub format_version: u32,
    /// The device's input plus output latency when the pass started, in frames.
    pub device_latency: u64,
    /// Where capture began; absent during the count-in.
    pub start: Option<SavedStart>,
    pub tracks: Vec<SavedRecordingTrack>,
}

/// Where capture began.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedStart {
    pub start_frame: u64,
    pub plan_latency: u64,
}

/// One armed track and its take files, left first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SavedRecordingTrack {
    pub track: u64,
    pub files: Vec<String>,
}

/// A recording pass in progress, as the marker records it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recording {
    /// The device's input plus output latency when the pass started, in frames.
    pub device_latency: u64,
    /// Where capture began, once it has.
    pub start: Option<RecordingStart>,
    /// Each armed track and its files in `audio/`: one for mono, left then right for stereo.
    pub tracks: Vec<(TrackId, Vec<String>)>,
}

/// Where a pass's capture began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordingStart {
    /// The timeline frame of the takes' first frame.
    pub start_frame: u64,
    /// The plan's output latency then, in frames.
    pub plan_latency: u64,
}

/// A pass recovered on open, its files repaired and kept until the interface places them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecoveredPass {
    /// Each track still in the project, with its files, left first.
    pub tracks: Vec<(TrackId, Vec<String>)>,
    /// The shortest file's length, in frames: the length every take is placed with.
    pub frames: u64,
    /// Where capture began.
    pub start: RecordingStart,
    /// The device latency when the pass started, in frames.
    pub device_latency: u64,
}

/// What [`Bundle::open`](crate::Bundle::open) found in a recording marker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recovery {
    /// Takes to place; the marker stays until [`Bundle::clear_recording`](crate::Bundle::clear_recording).
    Takes(RecoveredPass),
    /// Nothing could be recovered; the marker is gone and cleanup deletes the files.
    Discarded {
        /// Why, for logs.
        reason: String,
    },
}

impl From<&Recording> for SavedRecording {
    fn from(recording: &Recording) -> Self {
        SavedRecording {
            format_version: RECORDING_VERSION,
            device_latency: recording.device_latency,
            start: recording.start.map(|s| SavedStart {
                start_frame: s.start_frame,
                plan_latency: s.plan_latency,
            }),
            tracks: recording
                .tracks
                .iter()
                .map(|(id, files)| SavedRecordingTrack {
                    track: id.0,
                    files: files.clone(),
                })
                .collect(),
        }
    }
}

/// Reads the marker in `bundle`, if any, and recovers what it describes against `session`.
/// Returns what was found and the files cleanup must keep. A marker with nothing to recover is
/// removed.
pub(crate) fn recover(
    bundle: &Path,
    audio: &Path,
    session: &Session,
    sample_rate: u32,
) -> (Option<Recovery>, BTreeSet<String>) {
    let marker = bundle.join(RECORDING_FILE);
    if !marker.exists() {
        return (None, BTreeSet::new());
    }
    let found = read(&marker).and_then(|saved| place(&saved, audio, session, sample_rate));
    match found {
        Ok(Some(pass)) => {
            let keep = pass.tracks.iter().flat_map(|(_, f)| f.clone()).collect();
            (Some(Recovery::Takes(pass)), keep)
        }
        Ok(None) => {
            // Already placed, or nothing was captured: nothing is lost.
            let _ = fs::remove_file(&marker);
            (None, BTreeSet::new())
        }
        Err(reason) => {
            // A marker that cannot be removed is discarded again on the next open.
            let _ = fs::remove_file(&marker);
            (Some(Recovery::Discarded { reason }), BTreeSet::new())
        }
    }
}

/// Reads and checks the marker's shape and limits.
fn read(path: &Path) -> Result<SavedRecording, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut text = String::new();
    file.take(MAX_RECORDING_BYTES + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() as u64 > MAX_RECORDING_BYTES {
        return Err(format!("the marker is over {MAX_RECORDING_BYTES} bytes"));
    }
    let saved: SavedRecording = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    if saved.format_version != RECORDING_VERSION {
        return Err(format!("marker format version {}", saved.format_version));
    }
    if saved.tracks.len() > MAX_TRACKS {
        return Err(format!("the marker names over {MAX_TRACKS} tracks"));
    }
    let mut names = BTreeSet::new();
    let mut tracks = BTreeSet::new();
    for track in &saved.tracks {
        if !(1..=2).contains(&track.files.len()) {
            return Err("a track in the marker does not have one or two files".into());
        }
        if !tracks.insert(track.track) {
            return Err("the marker names a track twice".into());
        }
        for name in &track.files {
            if !is_plain_file_name(name) || !name.to_ascii_lowercase().ends_with(".wav") {
                return Err(format!("{name:?} is not a plain WAV file name"));
            }
            if !names.insert(name.as_str()) {
                return Err(format!("the marker names {name:?} twice"));
            }
        }
    }
    Ok(saved)
}

/// Repairs the marker's files and returns the pass to place: `None` if every track's takes are
/// already in the project or nothing was captured.
fn place(
    saved: &SavedRecording,
    audio: &Path,
    session: &Session,
    sample_rate: u32,
) -> Result<Option<RecoveredPass>, String> {
    let Some(start) = saved.start else {
        return Err("the crash came before capture began".into());
    };
    let referenced = session.referenced_files();
    let mut placed = false;
    let mut tracks = Vec::new();
    let mut frames = u64::MAX;
    for track in &saved.tracks {
        let id = TrackId(track.track);
        let Some(kind) = session.state().track(id).map(|t| t.kind) else {
            continue;
        };
        let TrackKind::Audio(layout) = kind else {
            return Err(format!("the marker names {id}, which is a MIDI track"));
        };
        if layout.channels() != track.files.len() {
            continue;
        }
        if track.files.iter().any(|f| referenced.contains(f.as_str())) {
            placed = true;
            continue;
        }
        let lengths: Result<Vec<u64>, String> = track
            .files
            .iter()
            .map(|name| repair(&audio.join(name), sample_rate))
            .collect();
        if let Ok(lengths) = lengths {
            frames = lengths.into_iter().fold(frames, u64::min);
            tracks.push((id, track.files.clone()));
        }
    }
    if tracks.is_empty() {
        return if placed {
            Ok(None)
        } else {
            Err("none of the marker's takes could be repaired for a track in the project".into())
        };
    }
    if frames == 0 {
        return Ok(None);
    }
    Ok(Some(RecoveredPass {
        tracks,
        frames,
        start: RecordingStart {
            start_frame: start.start_frame,
            plan_latency: start.plan_latency,
        },
        device_latency: saved.device_latency,
    }))
}

/// Repairs one take file, a regular file in `audio/`, and checks it is a mono file at the
/// project rate. Returns its frame count.
fn repair(path: &Path, sample_rate: u32) -> Result<u64, String> {
    let is_file = fs::symlink_metadata(path).is_ok_and(|m| m.is_file());
    if !is_file {
        return Err(format!("{} is not a file", path.display()));
    }
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|e| e.to_string())?;
    let frames = repair_sizes(&mut file).map_err(|e| e.to_string())?;
    let file = File::open(path).map_err(|e| e.to_string())?;
    let spec = WavReader::new(BufReader::new(file))
        .map_err(|e| e.to_string())?
        .spec();
    if spec.channels != Channels::Mono || spec.sample_rate != sample_rate {
        return Err(format!(
            "{} is not a mono take at the project rate",
            path.display()
        ));
    }
    Ok(frames)
}
