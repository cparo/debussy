//! Reading a saved file: load limits, the format version, and migration of older versions
//! (`docs/architecture/project.md` "Loading", ADR-0013, ADR-0049).

use std::path::Path;

use serde_json::Value;

use crate::error::Error;
use crate::format::{FORMAT_VERSION, SavedProject};

/// The deepest nesting of arrays and objects a file may have.
pub const MAX_DEPTH: u64 = 64;

/// A step from one format version to the next, over the parsed JSON.
pub(crate) type Migration = fn(Value) -> Result<Value, String>;

/// `MIGRATIONS[i]` turns a file of version `i + 1` into version `i + 2`.
const MIGRATIONS: &[Migration] = &[
    one_to_two,
    two_to_three,
    three_to_four,
    four_to_five,
    five_to_six,
    six_to_seven,
    seven_to_eight,
    eight_to_nine,
    nine_to_ten,
    ten_to_eleven,
    eleven_to_twelve,
];

/// Version 2 added optional `from_side` and `to_side` to connections (ADR-0058); a version 1
/// connection is a whole-port connection, which is what their absence means.
fn one_to_two(value: Value) -> Result<Value, String> {
    Ok(value)
}

/// Version 3 added an optional `position` to nodes and `output_y` to the project (ADR-0061); a
/// version 2 file leaves the patch board to the automatic layout, which is what their absence
/// means.
fn two_to_three(value: Value) -> Result<Value, String> {
    Ok(value)
}

/// Version 4 added a required `tempo` (ADR-0082); a version 3 file gets the default, 120 BPM
/// in 4/4.
fn three_to_four(mut value: Value) -> Result<Value, String> {
    let object = value
        .as_object_mut()
        .ok_or("the file is not a JSON object")?;
    object.insert(
        "tempo".to_owned(),
        serde_json::json!({"bpm": 120.0, "beats_per_bar": 4, "beat_unit": 4}),
    );
    Ok(value)
}

/// Version 5 added required automation `curves` to the project and `curve_ports` to each node
/// (ADR-0124); a version 4 file gets none of either.
fn four_to_five(mut value: Value) -> Result<Value, String> {
    let object = value
        .as_object_mut()
        .ok_or("the file is not a JSON object")?;
    object.insert("curves".to_owned(), Value::Array(Vec::new()));
    if let Some(nodes) = object.get_mut("nodes").and_then(Value::as_array_mut) {
        for node in nodes {
            node.as_object_mut()
                .ok_or("a node is not a JSON object")?
                .insert("curve_ports".to_owned(), Value::Object(Default::default()));
        }
    }
    Ok(value)
}

/// Version 6 added optional `out_handle`, `in_handle` and `points` to connections (ADR-0134); a
/// version 5 file leaves every wire on the automatic curve, which is what their absence means.
fn five_to_six(value: Value) -> Result<Value, String> {
    Ok(value)
}

/// Version 7 replaced each track's `layout` with a `kind` and added the project's MIDI
/// `sequences` (ADR-0144); every version 6 track is audio, and the file has no sequences.
fn six_to_seven(mut value: Value) -> Result<Value, String> {
    let object = value
        .as_object_mut()
        .ok_or("the file is not a JSON object")?;
    object.insert("sequences".to_owned(), Value::Array(Vec::new()));
    if let Some(tracks) = object.get_mut("tracks").and_then(Value::as_array_mut) {
        for track in tracks {
            let track = track
                .as_object_mut()
                .ok_or("a track is not a JSON object")?;
            let layout = track.remove("layout").ok_or("a track has no layout")?;
            track.insert(
                "kind".to_owned(),
                serde_json::json!({"kind": "audio", "layout": layout}),
            );
        }
    }
    Ok(value)
}

/// Version 12 added an optional `tune` to each sampler zone (ADR-0158); a version 11 zone is
/// untuned, which its absence means, so nothing changes.
fn eleven_to_twelve(value: Value) -> Result<Value, String> {
    Ok(value)
}

/// Version 11 added optional sampler `zones` to instrument nodes (ADR-0154); a version 10
/// instrument has none, so nothing changes.
fn ten_to_eleven(value: Value) -> Result<Value, String> {
    Ok(value)
}

/// Version 8 added instrument nodes (ADR-0147); no version 7 file can hold one, so nothing
/// changes.
fn seven_to_eight(value: Value) -> Result<Value, String> {
    Ok(value)
}

/// Version 9 added a required `key` and `names` to each MIDI track (ADR-0153); a version 8 MIDI
/// track gets C major and no names.
fn eight_to_nine(mut value: Value) -> Result<Value, String> {
    let object = value
        .as_object_mut()
        .ok_or("the file is not a JSON object")?;
    if let Some(tracks) = object.get_mut("tracks").and_then(Value::as_array_mut) {
        for track in tracks {
            // A track without a kind is left for parsing to refuse.
            let kind = track.get_mut("kind").and_then(Value::as_object_mut);
            if let Some(kind) = kind
                && kind.get("kind").and_then(Value::as_str) == Some("midi")
            {
                kind.insert(
                    "key".to_owned(),
                    serde_json::json!({"mode": "major", "letter": "c", "accidental": "natural"}),
                );
                kind.insert("names".to_owned(), Value::Array(Vec::new()));
            }
        }
    }
    Ok(value)
}

/// Version 10 spells a key's tonic as a `letter` and `accidental` (ADR-0153). Some version 9
/// files, saved by development builds, gave it as a pitch class `root` instead; it becomes the
/// spelling those builds showed: flats in flat-signature keys (F, Bb, Eb, Ab, Db, Gb major; D,
/// G, C, F, Bb, Eb minor), sharps otherwise. Keys already spelled are left alone.
fn nine_to_ten(mut value: Value) -> Result<Value, String> {
    const SHARPS: [(&str, &str); 12] = [
        ("c", "natural"),
        ("c", "sharp"),
        ("d", "natural"),
        ("d", "sharp"),
        ("e", "natural"),
        ("f", "natural"),
        ("f", "sharp"),
        ("g", "natural"),
        ("g", "sharp"),
        ("a", "natural"),
        ("a", "sharp"),
        ("b", "natural"),
    ];
    const FLATS: [(&str, &str); 12] = [
        ("c", "natural"),
        ("d", "flat"),
        ("d", "natural"),
        ("e", "flat"),
        ("e", "natural"),
        ("f", "natural"),
        ("g", "flat"),
        ("g", "natural"),
        ("a", "flat"),
        ("a", "natural"),
        ("b", "flat"),
        ("b", "natural"),
    ];
    let Some(tracks) = value.get_mut("tracks").and_then(Value::as_array_mut) else {
        return Ok(value);
    };
    for track in tracks {
        let key = track
            .get_mut("kind")
            .and_then(|kind| kind.get_mut("key"))
            .and_then(Value::as_object_mut);
        let Some(key) = key else {
            continue;
        };
        let Some(root) = key.get("root").and_then(Value::as_u64) else {
            continue;
        };
        let index = usize::try_from(root)
            .ok()
            .filter(|&r| r < 12)
            .ok_or_else(|| format!("a key has root {root}, not 0 to 11"))?;
        let flats = match key.get("mode").and_then(Value::as_str) {
            Some("major") => matches!(index, 5 | 10 | 3 | 8 | 1 | 6),
            Some("minor") => matches!(index, 2 | 7 | 0 | 5 | 10 | 3),
            _ => false,
        };
        let (letter, accidental) = if flats { FLATS } else { SHARPS }[index];
        key.remove("root");
        key.insert("letter".to_owned(), Value::from(letter));
        key.insert("accidental".to_owned(), Value::from(accidental));
    }
    Ok(value)
}

/// Parses `text`, read from `path`, as a saved project of this build's format version: checks
/// its nesting, refuses a newer version, and migrates an older one.
pub(crate) fn parse(path: &Path, text: &str) -> Result<SavedProject, Error> {
    parse_with(path, text, FORMAT_VERSION, MIGRATIONS)
}

/// [`parse`], for a format at `current` with `migrations` (separate so the steps can be tested).
pub(crate) fn parse_with(
    path: &Path,
    text: &str,
    current: u32,
    migrations: &[Migration],
) -> Result<SavedProject, Error> {
    if depth(text) > MAX_DEPTH {
        return Err(Error::TooLarge {
            path: path.to_owned(),
            what: "nesting depth",
            limit: MAX_DEPTH,
        });
    }
    let invalid = |reason: String| Error::Invalid {
        path: path.to_owned(),
        reason,
    };
    let json = |source| Error::Json {
        path: path.to_owned(),
        source,
    };
    let mut value: Value = serde_json::from_str(text).map_err(json)?;
    let found = value
        .get("format_version")
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .filter(|&v| v >= 1)
        .ok_or_else(|| invalid("no valid format_version".to_owned()))?;
    if found > current {
        return Err(Error::NewerFormat {
            path: path.to_owned(),
            found,
            supported: current,
        });
    }
    for version in found..current {
        let step = migrations
            .get(version as usize - 1)
            .ok_or_else(|| invalid(format!("no migration from version {version}")))?;
        value = step(value).map_err(invalid)?;
        if let Some(object) = value.as_object_mut() {
            object.insert("format_version".to_owned(), Value::from(version + 1));
        }
    }
    serde_json::from_value(value).map_err(json)
}

/// The deepest nesting of arrays and objects in `text`, ignoring brackets inside strings.
fn depth(text: &str) -> u64 {
    let (mut deepest, mut level) = (0, 0u64);
    let (mut in_string, mut escaped) = (false, false);
    for byte in text.bytes() {
        if in_string {
            match byte {
                _ if escaped => escaped = false,
                b'\\' => escaped = true,
                b'"' => in_string = false,
                _ => {}
            }
            continue;
        }
        match byte {
            b'"' => in_string = true,
            b'[' | b'{' => {
                level += 1;
                deepest = deepest.max(level);
            }
            b']' | b'}' => level = level.saturating_sub(1),
            _ => {}
        }
    }
    deepest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{SavedAccidental, SavedKey, SavedLayout, SavedLetter, SavedTrackKind};
    use std::error::Error as StdError;

    const V1: &str =
        r#"{"format_version":1,"sample_rate":48000,"tracks":[],"nodes":[],"connections":[]}"#;

    #[test]
    fn test_parse_migrates_version_one() -> Result<(), Box<dyn StdError>> {
        let saved = parse(Path::new("p"), V1)?;
        assert_eq!(saved.format_version, FORMAT_VERSION);
        assert!(saved.connections.is_empty());
        Ok(())
    }

    #[test]
    fn test_parse_migrates_version_three_to_the_default_tempo() -> Result<(), Box<dyn StdError>> {
        let v3 = V1.replace("\"format_version\":1", "\"format_version\":3");
        let saved = parse(Path::new("p"), &v3)?;
        assert_eq!(saved.format_version, FORMAT_VERSION);
        assert_eq!(
            (
                saved.tempo.bpm,
                saved.tempo.beats_per_bar,
                saved.tempo.beat_unit
            ),
            (120.0, 4, 4)
        );
        // A version 4 file must have its tempo.
        let v4 = V1.replace("\"format_version\":1", "\"format_version\":4");
        assert!(matches!(
            parse(Path::new("p"), &v4),
            Err(Error::Json { .. })
        ));
        Ok(())
    }

    #[test]
    fn test_parse_migrates_version_four_to_no_curves() -> Result<(), Box<dyn StdError>> {
        let v4 = r#"{"format_version":4,"sample_rate":48000,"tracks":[],"connections":[],
            "nodes":[{"id":1,"kind":{"kind":"bus","layout":"mono"}}],
            "tempo":{"bpm":90.0,"beats_per_bar":3,"beat_unit":4}}"#;
        let saved = parse(Path::new("p"), v4)?;
        assert_eq!(saved.format_version, FORMAT_VERSION);
        assert!(saved.curves.is_empty());
        assert!(saved.nodes[0].curve_ports.is_empty());
        // A version 5 file must have its curves and curve ports.
        let v5 = v4.replace("\"format_version\":4", "\"format_version\":5");
        assert!(matches!(
            parse(Path::new("p"), &v5),
            Err(Error::Json { .. })
        ));
        Ok(())
    }

    #[test]
    fn test_parse_migrates_version_six_tracks_to_audio_kind() -> Result<(), Box<dyn StdError>> {
        let v6 = r#"{"format_version":6,"sample_rate":48000,"connections":[],"nodes":[],
            "tracks":[{"id":1,"name":"a","layout":"stereo","clips":[]},
                      {"id":2,"name":"b","layout":"mono","clips":[]}],
            "tempo":{"bpm":90.0,"beats_per_bar":3,"beat_unit":4},"curves":[]}"#;
        let saved = parse(Path::new("p"), v6)?;
        assert_eq!(saved.format_version, FORMAT_VERSION);
        assert!(saved.sequences.is_empty());
        let kinds: Vec<_> = saved.tracks.iter().map(|t| t.kind.clone()).collect();
        assert_eq!(
            kinds,
            vec![
                SavedTrackKind::Audio {
                    layout: SavedLayout::Stereo
                },
                SavedTrackKind::Audio {
                    layout: SavedLayout::Mono
                },
            ]
        );
        // A version 7 file must have its kinds and sequences.
        let v7 = v6.replace("\"format_version\":6", "\"format_version\":7");
        assert!(matches!(
            parse(Path::new("p"), &v7),
            Err(Error::Json { .. })
        ));
        Ok(())
    }

    #[test]
    fn test_parse_migrates_version_seven_to_c_major() -> Result<(), Box<dyn StdError>> {
        let v7 = r#"{"format_version":7,"sample_rate":48000,"connections":[],
            "nodes":[{"id":1,"kind":{"kind":"effect","effect":"delay","parameters":{}},
                      "curve_ports":{}}],
            "tracks":[{"id":2,"name":"m","kind":{"kind":"midi"},"clips":[]}],
            "tempo":{"bpm":90.0,"beats_per_bar":3,"beat_unit":4},"curves":[],
            "sequences":[]}"#;
        let saved = parse(Path::new("p"), v7)?;
        assert_eq!(saved.format_version, FORMAT_VERSION);
        assert_eq!(saved.nodes.len(), 1);
        assert_eq!(saved.tracks[0].kind, c_major());
        Ok(())
    }

    fn c_major() -> SavedTrackKind {
        SavedTrackKind::Midi {
            key: SavedKey::Major {
                letter: SavedLetter::C,
                accidental: SavedAccidental::Natural,
            },
            names: Vec::new(),
        }
    }

    #[test]
    fn test_parse_migrates_version_eight_midi_tracks_to_c_major() -> Result<(), Box<dyn StdError>> {
        let v8 = r#"{"format_version":8,"sample_rate":48000,"connections":[],"nodes":[],
            "tracks":[{"id":1,"name":"a","kind":{"kind":"audio","layout":"mono"},"clips":[]},
                      {"id":2,"name":"m","kind":{"kind":"midi"},"clips":[]}],
            "tempo":{"bpm":90.0,"beats_per_bar":3,"beat_unit":4},"curves":[],
            "sequences":[]}"#;
        let saved = parse(Path::new("p"), v8)?;
        assert_eq!(saved.format_version, FORMAT_VERSION);
        assert_eq!(
            saved.tracks[0].kind,
            SavedTrackKind::Audio {
                layout: SavedLayout::Mono
            }
        );
        assert_eq!(saved.tracks[1].kind, c_major());
        // A version 9 MIDI track must have its key and names.
        let v9 = v8.replace("\"format_version\":8", "\"format_version\":9");
        assert!(matches!(
            parse(Path::new("p"), &v9),
            Err(Error::Json { .. })
        ));
        Ok(())
    }

    #[test]
    fn test_parse_migrates_version_nine_roots_to_spelled_tonics() -> Result<(), Box<dyn StdError>> {
        let track = |id: u64, key: &str| {
            format!(
                r#"{{"id":{id},"name":"m","kind":{{"kind":"midi","key":{key},"names":[]}},"clips":[]}}"#
            )
        };
        let tracks = [
            track(1, r#"{"mode":"major","root":1}"#),
            track(2, r#"{"mode":"minor","root":1}"#),
            track(3, r#"{"mode":"minor","root":10}"#),
            track(4, r#"{"mode":"major","letter":"c","accidental":"sharp"}"#),
            track(5, r#"{"mode":"indefinite"}"#),
        ]
        .join(",");
        let v9 = format!(
            r#"{{"format_version":9,"sample_rate":48000,"connections":[],"nodes":[],
            "tracks":[{tracks}],"tempo":{{"bpm":90.0,"beats_per_bar":3,"beat_unit":4}},
            "curves":[],"sequences":[]}}"#
        );
        let saved = parse(Path::new("p"), &v9)?;
        assert_eq!(saved.format_version, FORMAT_VERSION);
        let keys: Vec<_> = saved
            .tracks
            .iter()
            .map(|t| match &t.kind {
                SavedTrackKind::Midi { key, .. } => Some(*key),
                SavedTrackKind::Audio { .. } => None,
            })
            .collect();
        let (sharp, flat) = (SavedAccidental::Sharp, SavedAccidental::Flat);
        assert_eq!(
            keys,
            [
                Some(SavedKey::Major {
                    letter: SavedLetter::D,
                    accidental: flat
                }),
                Some(SavedKey::Minor {
                    letter: SavedLetter::C,
                    accidental: sharp
                }),
                Some(SavedKey::Minor {
                    letter: SavedLetter::B,
                    accidental: flat
                }),
                Some(SavedKey::Major {
                    letter: SavedLetter::C,
                    accidental: sharp
                }),
                Some(SavedKey::Indefinite {}),
            ]
        );
        let bad = v9.replace(r#""root":1}"#, r#""root":12}"#);
        assert!(matches!(
            parse(Path::new("p"), &bad),
            Err(Error::Invalid { .. })
        ));
        Ok(())
    }

    #[test]
    fn test_depth_ignores_brackets_in_strings() {
        assert_eq!(depth(r#"{"a":[[1]],"b":"[[[[{{"}"#), 3);
        assert_eq!(depth(r#"["\"[[[" ]"#), 1);
    }

    #[test]
    fn test_parse_refuses_newer_and_deep_files() {
        let newer = V1.replace("\"format_version\":1", "\"format_version\":13");
        assert!(matches!(
            parse(Path::new("p"), &newer),
            Err(Error::NewerFormat {
                found: 13,
                supported: 12,
                ..
            })
        ));
        let deep = format!("{}{}", "[".repeat(65), "]".repeat(65));
        assert!(matches!(
            parse(Path::new("p"), &deep),
            Err(Error::TooLarge {
                what: "nesting depth",
                ..
            })
        ));
        for bad in [r#"{"sample_rate":1}"#, r#"{"format_version":0}"#, "[]"] {
            assert!(matches!(
                parse(Path::new("p"), bad),
                Err(Error::Invalid { .. })
            ));
        }
    }

    #[test]
    fn test_parse_with_migrates_older_versions_in_steps() -> Result<(), Box<dyn StdError>> {
        // A pretend history: version 1 called the rate "rate"; version 2 had no "nodes".
        fn one_to_two(mut v: Value) -> Result<Value, String> {
            let object = v.as_object_mut().ok_or("not an object")?;
            let rate = object.remove("rate").ok_or("no rate")?;
            object.insert("sample_rate".into(), rate);
            object.remove("nodes");
            Ok(v)
        }
        fn two_to_three(mut v: Value) -> Result<Value, String> {
            let object = v.as_object_mut().ok_or("not an object")?;
            object.insert("nodes".into(), Value::Array(Vec::new()));
            six_to_seven(four_to_five(three_to_four(v)?)?)
        }
        let old = r#"{"format_version":1,"rate":44100,"tracks":[],"nodes":[],"connections":[]}"#;
        let saved = parse_with(Path::new("p"), old, 3, &[one_to_two, two_to_three])?;
        assert_eq!(saved.format_version, 3);
        assert_eq!(saved.sample_rate, 44_100);
        assert!(matches!(
            parse_with(Path::new("p"), old, 3, &[one_to_two]),
            Err(Error::Invalid { .. })
        ));
        Ok(())
    }
}
