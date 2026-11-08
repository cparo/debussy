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
const MIGRATIONS: &[Migration] = &[one_to_two, two_to_three];

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
    fn test_depth_ignores_brackets_in_strings() {
        assert_eq!(depth(r#"{"a":[[1]],"b":"[[[[{{"}"#), 3);
        assert_eq!(depth(r#"["\"[[[" ]"#), 1);
    }

    #[test]
    fn test_parse_refuses_newer_and_deep_files() {
        let newer = V1.replace("\"format_version\":1", "\"format_version\":4");
        assert!(matches!(
            parse(Path::new("p"), &newer),
            Err(Error::NewerFormat {
                found: 4,
                supported: 3,
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
            Ok(v)
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
