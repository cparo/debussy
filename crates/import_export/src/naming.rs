//! Names for imported and recorded files in the bundle's `audio/` directory (ADR-0053,
//! ADR-0078).

use std::path::Path;

/// The longest stem kept from the imported file's name, in characters.
const MAX_STEM: usize = 64;

/// The imported file's name reduced to a safe stem, as [`safe_stem`] does.
pub(crate) fn stem(path: &Path) -> String {
    let raw = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    safe_stem(&raw)
}

/// `raw`, such as a track's name, reduced to a safe file-name stem: letters, digits, spaces, `-`
/// and `_` kept, anything else replaced by `_`, at most 64 characters, `audio` if nothing useful
/// is left.
pub fn safe_stem(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == ' ' || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .take(MAX_STEM)
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.chars().all(|c| c == '_') {
        "audio".to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// File names for `channels` channels of an import or recording from `stem`, none of which exists in `dir`:
/// `<stem> <n>.wav` for mono, `<stem> <n> L.wav` and `<stem> <n> R.wav` for stereo, with the
/// smallest `n` from 1 that is free for every channel.
pub fn unique_names(dir: &Path, stem: &str, channels: usize) -> Vec<String> {
    let suffixes: &[&str] = if channels == 2 { &[" L", " R"] } else { &[""] };
    (1u64..)
        .map(|n| {
            suffixes
                .iter()
                .map(|s| format!("{stem} {n}{s}.wav"))
                .collect::<Vec<_>>()
        })
        .find(|names| names.iter().all(|name| !dir.join(name).exists()))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stem_keeps_safe_characters_only() {
        assert_eq!(
            stem(Path::new("/x/Lead Vox_take-2.flac")),
            "Lead Vox_take-2"
        );
        assert_eq!(stem(Path::new("a/../b:c.wav")), "b_c");
        assert_eq!(stem(Path::new("..wav")), "audio");
        assert_eq!(stem(Path::new("Grüße.mp3")), "Grüße");
        assert_eq!(stem(Path::new(&"x".repeat(300))).len(), MAX_STEM);
    }
}
