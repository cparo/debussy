//! Where a drum kit's files go: each file's name and folders matched to a General MIDI drum key
//! (`docs/architecture/node-dialogs.md` "Sampler zones").

use project_model::{MAX_KEY, SampleZone, ZoneMode};

use crate::services::ImportedTrack;

/// The first key for files that match no drum, or whose drum already has a file: above the
/// General MIDI drum map, which ends at 81.
pub(crate) const EXTRA_FIRST_KEY: u8 = 82;
/// The choke group the hi-hats share when a kit has more than one of them.
pub(crate) const HAT_CHOKE: u8 = 1;
/// The closed, pedal and open hi-hat keys.
const HATS: [u8; 3] = [42, 44, 46];
/// The tom keys, in the order toms with no high, mid, low or floor in their name take them.
const TOMS: [u8; 6] = [50, 47, 45, 48, 43, 41];

/// The words of a name, lowercase: split at anything not a letter or digit, and between letters
/// and digits, so `"Kick_01"` and `"kick01"` are both `kick`, `01`.
fn words(name: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        let joins = word
            .chars()
            .last()
            .is_none_or(|last| last.is_ascii_digit() == c.is_ascii_digit());
        if (!c.is_alphanumeric() || !joins) && !word.is_empty() {
            words.push(std::mem::take(&mut word));
        }
        if c.is_alphanumeric() {
            word.push(c);
        }
    }
    if !word.is_empty() {
        words.push(word);
    }
    words
}

/// What drum a name is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Drum {
    /// A General MIDI key, then a second for a second file of the same drum, if any.
    Key(u8, Option<u8>),
    /// A tom, with its key if its name says which.
    Tom(Option<u8>),
}

/// The drum `name` is, if any, from its words; the first rule that matches wins, so an open hat
/// is not taken for a closed one.
fn drum(name: &str) -> Option<Drum> {
    let words = words(name);
    let has = |list: &[&str]| words.iter().any(|w| list.contains(&w.as_str()));
    let hat =
        has(&["hat", "hats", "hh", "hihat", "hihats", "hi-hat"]) || (has(&["hi"]) && has(&["hat"]));
    let key = |k| Some(Drum::Key(k, None));
    if has(&["ohh", "oh", "openhat", "ohat"]) || (has(&["open"]) && hat) {
        return key(46);
    }
    if has(&["phh", "pedalhat", "foothat"]) || (has(&["pedal", "foot"]) && hat) {
        return key(44);
    }
    if hat || has(&["chh", "ch", "closedhat", "chat"]) {
        return key(42);
    }
    if has(&["kick", "kicks", "kik", "kck", "bd", "bassdrum"]) || (has(&["bass"]) && has(&["drum"]))
    {
        return key(36);
    }
    if has(&["rim", "rimshot", "sidestick", "xstick", "rs"]) || (has(&["side"]) && has(&["stick"]))
    {
        return key(37);
    }
    if has(&["snare", "snares", "snr", "sd", "sn"]) {
        return key(38);
    }
    if has(&["clap", "claps", "cp", "handclap"]) {
        return key(39);
    }
    if words.iter().any(|w| w.contains("tom")) {
        let which = if words.iter().any(|w| w.contains("floor")) {
            Some(41)
        } else if has(&["low", "lo", "lotom", "lowtom"]) {
            Some(45)
        } else if has(&["mid", "md", "midtom"]) {
            Some(47)
        } else if has(&["high", "hi", "hitom", "hightom"]) {
            Some(50)
        } else {
            None
        };
        return Some(Drum::Tom(which));
    }
    if has(&["ride"]) && has(&["bell"]) {
        return key(53);
    }
    if has(&["cowbell"]) || (has(&["cow"]) && has(&["bell"])) {
        return key(56);
    }
    let fixed: [(&[&str], u8, Option<u8>); 12] = [
        (&["crash", "crashes"], 49, Some(57)),
        (&["ride", "rides"], 51, Some(59)),
        (&["china"], 52, None),
        (&["splash"], 55, None),
        (&["tambourine", "tamb", "tambo", "tamborine"], 54, None),
        (&["cabasa"], 69, None),
        (
            &["shaker", "shakers", "shake", "maraca", "maracas"],
            70,
            None,
        ),
        (&["clave", "claves"], 75, None),
        (&["woodblock", "block"], 76, None),
        (&["triangle", "tri"], 81, None),
        (&["conga", "congas"], 63, None),
        (&["bongo", "bongos"], 60, None),
    ];
    fixed
        .iter()
        .find(|(names, ..)| has(names))
        .map(|&(_, first, second)| Drum::Key(first, second))
}

/// The one-shot zones a kit's `files` become, and the names of any left out:
///
/// - A file whose name or folders name a drum plays on its General MIDI key (kick 36, snare 38,
///   closed hi-hat 42, open hi-hat 46 and so on); a second crash or ride takes the second crash
///   or ride key.
/// - A file matching no drum, or a drum that already has a file, goes on the next free key from
///   [`EXTRA_FIRST_KEY`]; files past the top key are left out.
/// - When the kit has more than one hi-hat, they share choke group [`HAT_CHOKE`].
pub(crate) fn kit_zones(files: Vec<ImportedTrack>) -> (Vec<SampleZone>, Vec<String>) {
    let mut taken = [false; 128];
    let mut placed: Vec<(u8, ImportedTrack)> = Vec::new();
    let mut extras = Vec::new();
    let mut toms = Vec::new();
    for file in files {
        let key = match drum(&file.name) {
            Some(Drum::Key(first, second)) => [Some(first), second]
                .into_iter()
                .flatten()
                .find(|&k| !taken[usize::from(k)]),
            Some(Drum::Tom(Some(k))) if !taken[usize::from(k)] => Some(k),
            Some(Drum::Tom(_)) => {
                toms.push(file);
                continue;
            }
            None => None,
        };
        match key {
            Some(k) => {
                taken[usize::from(k)] = true;
                placed.push((k, file));
            }
            None => extras.push(file),
        }
    }
    // Toms that did not say which, in the order found, take the toms left.
    for file in toms {
        match TOMS.into_iter().find(|&k| !taken[usize::from(k)]) {
            Some(k) => {
                taken[usize::from(k)] = true;
                placed.push((k, file));
            }
            None => extras.push(file),
        }
    }
    let mut left_out = Vec::new();
    let mut next = EXTRA_FIRST_KEY;
    for file in extras {
        while next <= MAX_KEY && taken[usize::from(next)] {
            next += 1;
        }
        if next > MAX_KEY {
            left_out.push(file.name);
            continue;
        }
        taken[usize::from(next)] = true;
        placed.push((next, file));
    }
    let hats = HATS.iter().filter(|&&k| taken[usize::from(k)]).count();
    let zones = placed
        .into_iter()
        .map(|(key, file)| SampleZone {
            file: file.source,
            low: key,
            high: key,
            root: key,
            mode: ZoneMode::OneShot,
            choke: (hats > 1 && HATS.contains(&key)).then_some(HAT_CHOKE),
            tune: 0,
        })
        .collect();
    (zones, left_out)
}

#[cfg(test)]
mod tests {
    use audio_buffers::Layout;
    use project_model::{AudioFile, Source};

    use super::*;

    fn file(name: &str) -> ImportedTrack {
        ImportedTrack {
            name: name.to_owned(),
            layout: Layout::Mono,
            source: Source::Mono(AudioFile {
                name: format!("{}.wav", name.replace('/', "_")),
                frames: 10,
            }),
            frames: 10,
        }
    }

    fn keys(names: &[&str]) -> Vec<(u8, Option<u8>)> {
        let files = names.iter().map(|n| file(n)).collect();
        kit_zones(files)
            .0
            .iter()
            .map(|z| (z.low, z.choke))
            .collect()
    }

    #[test]
    fn test_words_split_at_separators_case_and_digits() {
        assert_eq!(words("Kick_01"), ["kick", "01"]);
        assert_eq!(words("kick01"), ["kick", "01"]);
        assert_eq!(words("Hats/Open Hat-2"), ["hats", "open", "hat", "2"]);
    }

    #[test]
    fn test_drum_names_find_their_general_midi_keys() {
        let cases = [
            ("BD 808", 36),
            ("Kicks/Punchy", 36),
            ("Snare Tight", 38),
            ("SD_02", 38),
            ("Rimshot", 37),
            ("Side Stick", 37),
            ("Clap", 39),
            ("Closed Hat", 42),
            ("HH", 42),
            ("Hats/Pedal Hat", 44),
            ("Open Hat", 46),
            ("OHH 1", 46),
            ("Crash", 49),
            ("Ride Bell", 53),
            ("Ride", 51),
            ("Cowbell", 56),
            ("Tambourine", 54),
            ("Shaker", 70),
        ];
        for (name, key) in cases {
            assert_eq!(drum(name), Some(Drum::Key(key, drum_second(key))), "{name}");
        }
        assert_eq!(drum("Floor Tom"), Some(Drum::Tom(Some(41))));
        assert_eq!(drum("Hi Tom"), Some(Drum::Tom(Some(50))));
        assert_eq!(drum("Tom 2"), Some(Drum::Tom(None)));
        assert_eq!(drum("Vox Chop"), None);
    }

    fn drum_second(key: u8) -> Option<u8> {
        match key {
            49 => Some(57),
            51 => Some(59),
            _ => None,
        }
    }

    #[test]
    fn test_kit_zones_place_drums_and_choke_the_hats() {
        assert_eq!(
            keys(&["Kick", "Snare", "Closed Hat", "Open Hat"]),
            [(36, None), (38, None), (42, Some(1)), (46, Some(1))]
        );
        // A lone hat has no one to choke.
        assert_eq!(keys(&["Closed Hat"]), [(42, None)]);
    }

    #[test]
    fn test_kit_zones_put_extras_and_unknowns_above_the_drum_map() {
        assert_eq!(
            keys(&["Kick 1", "Kick 2", "Vox", "Crash 1", "Crash 2"]),
            [(36, None), (49, None), (57, None), (82, None), (83, None)]
        );
    }

    #[test]
    fn test_kit_zones_give_unnamed_toms_the_free_tom_keys() {
        assert_eq!(
            keys(&["Tom 1", "Low Tom", "Tom 2"]),
            [(45, None), (50, None), (47, None)]
        );
    }

    #[test]
    fn test_kit_zones_leave_out_what_does_not_fit() {
        let many: Vec<String> = (0..50).map(|n| format!("Perc {n}")).collect();
        let files = many.iter().map(|n| file(n)).collect();
        let (zones, left_out) = kit_zones(files);
        assert_eq!(zones.len(), 46);
        assert_eq!(left_out, ["Perc 46", "Perc 47", "Perc 48", "Perc 49"]);
    }
}
