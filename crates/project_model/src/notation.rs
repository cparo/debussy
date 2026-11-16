//! A MIDI track's key and pitch names (`docs/architecture/track-pane.md` "Key and pitch
//! names"): how its lane labels pitches. Display only; nothing here changes what plays.

use std::collections::BTreeMap;

use crate::error::Error;

/// The highest MIDI pitch.
pub const MAX_PITCH: u8 = 127;

/// The most characters a pitch name may have.
pub const MAX_NAME_CHARS: usize = 32;

/// Major or minor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// A major key.
    Major,
    /// A minor key.
    Minor,
}

/// A note letter, the natural pitch a tonic is spelled from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Letter {
    /// C.
    C,
    /// D.
    D,
    /// E.
    E,
    /// F.
    F,
    /// G.
    G,
    /// A.
    A,
    /// B.
    B,
}

impl Letter {
    /// Every letter, A first, as the key dialog lists them.
    pub const ALL: [Letter; 7] = [
        Letter::A,
        Letter::B,
        Letter::C,
        Letter::D,
        Letter::E,
        Letter::F,
        Letter::G,
    ];

    /// The letter's pitch class, 0 (C) to 11.
    pub fn pitch_class(self) -> u8 {
        match self {
            Letter::C => 0,
            Letter::D => 2,
            Letter::E => 4,
            Letter::F => 5,
            Letter::G => 7,
            Letter::A => 9,
            Letter::B => 11,
        }
    }

    /// Its place on the circle of fifths from C: the sharps (positive) or flats (negative) in
    /// its major key.
    fn fifths(self) -> i8 {
        match self {
            Letter::F => -1,
            Letter::C => 0,
            Letter::G => 1,
            Letter::D => 2,
            Letter::A => 3,
            Letter::E => 4,
            Letter::B => 5,
        }
    }

    /// The letter as written: "C".
    pub fn name(self) -> &'static str {
        match self {
            Letter::C => "C",
            Letter::D => "D",
            Letter::E => "E",
            Letter::F => "F",
            Letter::G => "G",
            Letter::A => "A",
            Letter::B => "B",
        }
    }
}

/// A tonic's accidental: none, a sharp or a flat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Accidental {
    /// No accidental.
    #[default]
    Natural,
    /// A semitone up: "#".
    Sharp,
    /// A semitone down: "b".
    Flat,
}

impl Accidental {
    /// The accidental as written, in ASCII: "", "#" or "b".
    pub fn name(self) -> &'static str {
        match self {
            Accidental::Natural => "",
            Accidental::Sharp => "#",
            Accidental::Flat => "b",
        }
    }
}

/// A track's key: a tonic, spelled as a letter and accidental, and a mode; or none for a track
/// whose pitches stand for sounds rather than notes (drums, samples). Enharmonic tonics, such as
/// C# and Db, are different keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A key with a tonic and a mode.
    Tonal {
        /// The tonic's letter.
        letter: Letter,
        /// The tonic's accidental.
        accidental: Accidental,
        /// Major or minor.
        mode: Mode,
    },
    /// No key: pitches may carry names instead.
    Indefinite,
}

impl Default for Key {
    /// C major.
    fn default() -> Self {
        Key::Tonal {
            letter: Letter::C,
            accidental: Accidental::Natural,
            mode: Mode::Major,
        }
    }
}

impl Key {
    /// The tonic's pitch class, 0 (C) to 11; `None` for [`Key::Indefinite`].
    pub fn root(&self) -> Option<u8> {
        match *self {
            Key::Tonal {
                letter, accidental, ..
            } => Some(match accidental {
                Accidental::Natural => letter.pitch_class(),
                Accidental::Sharp => (letter.pitch_class() + 1) % 12,
                Accidental::Flat => (letter.pitch_class() + 11) % 12,
            }),
            Key::Indefinite => None,
        }
    }

    /// Whether the key's signature has flats, so that its notes are spelled with flats: Db
    /// major and Bb minor do, C# major and A# minor do not.
    pub fn uses_flats(&self) -> bool {
        match *self {
            Key::Tonal {
                letter,
                accidental,
                mode,
            } => {
                let shift = match accidental {
                    Accidental::Natural => 0,
                    Accidental::Sharp => 7,
                    Accidental::Flat => -7,
                };
                // A minor key has the signature of the major key a minor third above.
                let relative = if mode == Mode::Minor { -3 } else { 0 };
                letter.fifths() + shift + relative < 0
            }
            Key::Indefinite => false,
        }
    }

    /// The key's name: "C major", "Db major", "A# minor", or "Indefinite".
    pub fn name(&self) -> String {
        match *self {
            Key::Tonal {
                letter,
                accidental,
                mode,
            } => {
                let mode = match mode {
                    Mode::Major => "major",
                    Mode::Minor => "minor",
                };
                format!("{}{} {mode}", letter.name(), accidental.name())
            }
            Key::Indefinite => "Indefinite".to_owned(),
        }
    }
}

/// A MIDI track's key and pitch names.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Notation {
    /// The key.
    pub key: Key,
    /// Names of pitches, such as "kick". Kept under a tonal key but shown only while the key is
    /// [`Key::Indefinite`].
    pub names: BTreeMap<u8, String>,
}

impl Notation {
    /// An indefinite key with the General MIDI drum names ([`GM_DRUMS`]).
    pub fn gm_drums() -> Self {
        Notation {
            key: Key::Indefinite,
            names: gm_drum_names(),
        }
    }

    /// Checks the notation is within the limits: pitches up to
    /// [`MAX_PITCH`], and names that are not blank, have no control characters and have at most
    /// [`MAX_NAME_CHARS`] characters.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidNotation`], saying which limit.
    pub fn check(&self) -> Result<(), Error> {
        for (&pitch, name) in &self.names {
            if pitch > MAX_PITCH {
                return Err(Error::InvalidNotation(format!(
                    "pitch {pitch} is above {MAX_PITCH}"
                )));
            }
            check_name(name)?;
        }
        Ok(())
    }
}

/// Checks a pitch name: not blank, no control characters, at most [`MAX_NAME_CHARS`]
/// characters.
///
/// # Errors
///
/// [`Error::InvalidNotation`], saying which limit.
pub fn check_name(name: &str) -> Result<(), Error> {
    if name.trim().is_empty() {
        return Err(Error::InvalidNotation("a pitch name is blank".to_owned()));
    }
    if name.chars().any(char::is_control) {
        return Err(Error::InvalidNotation(
            "a pitch name has a control character".to_owned(),
        ));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(Error::InvalidNotation(format!(
            "a pitch name is longer than {MAX_NAME_CHARS} characters"
        )));
    }
    Ok(())
}

/// The name of pitch class `class` (0 = C), with a flat or a sharp where it needs one, in
/// ASCII: "Bb", "C#".
pub fn pitch_class_name(class: u8, flats: bool) -> &'static str {
    const SHARPS: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    const FLATS: [&str; 12] = [
        "C", "Db", "D", "Eb", "E", "F", "Gb", "G", "Ab", "A", "Bb", "B",
    ];
    let names = if flats { &FLATS } else { &SHARPS };
    names[usize::from(class % 12)]
}

/// The standard name of MIDI pitch `pitch` in `key`: middle C, 60, is "C4", 59 is "B3" and 0 is
/// "C-1". Flat-signature keys spell with flats, the others and [`Key::Indefinite`] with sharps.
pub fn pitch_name(pitch: u8, key: &Key) -> String {
    let octave = i32::from(pitch / 12) - 1;
    format!("{}{octave}", pitch_class_name(pitch % 12, key.uses_flats()))
}

/// The label of `pitch` on a lane: its name under an indefinite key if it has one, otherwise
/// [`pitch_name`].
pub fn note_label(pitch: u8, notation: &Notation) -> String {
    match (notation.key, notation.names.get(&pitch)) {
        (Key::Indefinite, Some(name)) => name.clone(),
        (key, _) => pitch_name(pitch, &key),
    }
}

/// The pitches from `low` to `high`, inclusive, whose class is `key`'s root, lowest first; none
/// for [`Key::Indefinite`].
pub fn root_pitches(key: &Key, low: u8, high: u8) -> impl Iterator<Item = u8> + use<> {
    let root = key.root();
    (low..=high).filter(move |pitch| root.is_some_and(|root| pitch % 12 == root))
}

/// The General MIDI percussion map (channel 10, pitches 35 to 81), with short names.
pub const GM_DRUMS: [(u8, &str); 47] = [
    (35, "kick-2"),
    (36, "kick"),
    (37, "side-stick"),
    (38, "snare"),
    (39, "clap"),
    (40, "snare-2"),
    (41, "low-floor-tom"),
    (42, "closed-hat"),
    (43, "high-floor-tom"),
    (44, "pedal-hat"),
    (45, "low-tom"),
    (46, "open-hat"),
    (47, "low-mid-tom"),
    (48, "high-mid-tom"),
    (49, "crash"),
    (50, "high-tom"),
    (51, "ride"),
    (52, "china"),
    (53, "ride-bell"),
    (54, "tambourine"),
    (55, "splash"),
    (56, "cowbell"),
    (57, "crash-2"),
    (58, "vibraslap"),
    (59, "ride-2"),
    (60, "high-bongo"),
    (61, "low-bongo"),
    (62, "mute-high-conga"),
    (63, "open-high-conga"),
    (64, "low-conga"),
    (65, "high-timbale"),
    (66, "low-timbale"),
    (67, "high-agogo"),
    (68, "low-agogo"),
    (69, "cabasa"),
    (70, "maracas"),
    (71, "short-whistle"),
    (72, "long-whistle"),
    (73, "short-guiro"),
    (74, "long-guiro"),
    (75, "claves"),
    (76, "high-wood-block"),
    (77, "low-wood-block"),
    (78, "mute-cuica"),
    (79, "open-cuica"),
    (80, "mute-triangle"),
    (81, "open-triangle"),
];

/// [`GM_DRUMS`] as a map of names.
pub fn gm_drum_names() -> BTreeMap<u8, String> {
    GM_DRUMS
        .iter()
        .map(|&(pitch, name)| (pitch, name.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const C_MAJOR: Key = key(Letter::C, Accidental::Natural, Mode::Major);

    const fn key(letter: Letter, accidental: Accidental, mode: Mode) -> Key {
        Key::Tonal {
            letter,
            accidental,
            mode,
        }
    }

    #[test]
    fn test_pitch_name_octaves_follow_middle_c_as_c4() {
        assert_eq!(pitch_name(60, &C_MAJOR), "C4");
        assert_eq!(pitch_name(59, &C_MAJOR), "B3");
        assert_eq!(pitch_name(0, &C_MAJOR), "C-1");
        assert_eq!(pitch_name(127, &C_MAJOR), "G9");
        assert_eq!(pitch_name(61, &Key::Indefinite), "C#4");
    }

    #[test]
    fn test_pitch_name_spells_with_flats_in_flat_keys() {
        let f_major = key(Letter::F, Accidental::Natural, Mode::Major);
        assert_eq!(pitch_name(70, &f_major), "Bb4");
        assert_eq!(pitch_name(70, &C_MAJOR), "A#4");
        let d_minor = key(Letter::D, Accidental::Natural, Mode::Minor);
        assert_eq!(pitch_name(70, &d_minor), "Bb4");
        let e_minor = key(Letter::E, Accidental::Natural, Mode::Minor);
        assert_eq!(pitch_name(66, &e_minor), "F#4");
    }

    #[test]
    fn test_key_name_spells_the_root_for_the_key() {
        assert_eq!(C_MAJOR.name(), "C major");
        let b_flat = key(Letter::B, Accidental::Flat, Mode::Minor);
        assert_eq!(b_flat.name(), "Bb minor");
        let f_sharp = key(Letter::F, Accidental::Sharp, Mode::Minor);
        assert_eq!(f_sharp.name(), "F# minor");
        assert_eq!(Key::Indefinite.name(), "Indefinite");
    }

    #[test]
    fn test_enharmonic_keys_share_a_root_but_not_a_spelling() {
        let c_sharp = key(Letter::C, Accidental::Sharp, Mode::Major);
        let d_flat = key(Letter::D, Accidental::Flat, Mode::Major);
        assert_eq!((c_sharp.root(), d_flat.root()), (Some(1), Some(1)));
        assert_eq!(
            (c_sharp.name(), d_flat.name()),
            ("C# major".into(), "Db major".into())
        );
        assert_eq!(pitch_name(61, &c_sharp), "C#4");
        assert_eq!(pitch_name(61, &d_flat), "Db4");
        // A# minor has sharps, Bb minor flats; B# wraps to C, Cb to B.
        assert!(!key(Letter::A, Accidental::Sharp, Mode::Minor).uses_flats());
        assert!(key(Letter::B, Accidental::Flat, Mode::Minor).uses_flats());
        assert_eq!(
            key(Letter::B, Accidental::Sharp, Mode::Major).root(),
            Some(0)
        );
        assert_eq!(
            key(Letter::C, Accidental::Flat, Mode::Major).root(),
            Some(11)
        );
        assert_eq!(Key::Indefinite.root(), None);
    }

    #[test]
    fn test_note_label_uses_names_only_while_indefinite() {
        let mut notation = Notation::gm_drums();
        assert_eq!(note_label(36, &notation), "kick");
        assert_eq!(note_label(30, &notation), "F#1");
        notation.key = C_MAJOR;
        assert_eq!(note_label(36, &notation), "C2");
    }

    #[test]
    fn test_root_pitches_lists_each_root_in_range() {
        assert_eq!(
            root_pitches(&C_MAJOR, 48, 72).collect::<Vec<_>>(),
            [48, 60, 72]
        );
        let a_minor = key(Letter::A, Accidental::Natural, Mode::Minor);
        assert_eq!(root_pitches(&a_minor, 50, 70).collect::<Vec<_>>(), [57, 69]);
        assert_eq!(root_pitches(&C_MAJOR, 0, 127).count(), 11);
        assert_eq!(root_pitches(&Key::Indefinite, 0, 127).count(), 0);
    }

    #[test]
    fn test_gm_drums_are_sorted_unique_and_valid() -> Result<(), Error> {
        assert!(GM_DRUMS.windows(2).all(|w| w[0].0 < w[1].0));
        assert_eq!(GM_DRUMS.first().map(|d| d.0), Some(35));
        assert_eq!(GM_DRUMS.last().map(|d| d.0), Some(81));
        Notation::gm_drums().check()
    }

    #[test]
    fn test_check_refuses_out_of_range_notation() {
        for (pitch, name) in [
            (128, "kick".to_owned()),
            (36, "  ".to_owned()),
            (36, "a\nb".to_owned()),
            (36, "x".repeat(MAX_NAME_CHARS + 1)),
        ] {
            let notation = Notation {
                key: Key::Indefinite,
                names: BTreeMap::from([(pitch, name)]),
            };
            assert!(notation.check().is_err(), "{notation:?}");
        }
        assert!(check_name(&"é".repeat(MAX_NAME_CHARS)).is_ok());
    }
}
