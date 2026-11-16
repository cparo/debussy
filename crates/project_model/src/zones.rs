//! A sampler's zones: which audio file each key range plays, and how
//! (`docs/architecture/instruments.md` "Sampler").

use crate::error::Error;
use crate::types::Source;

/// The highest MIDI key.
pub const MAX_KEY: u8 = 127;

/// The highest choke group number; groups are 1 to this.
pub const MAX_CHOKE_GROUP: u8 = 16;

/// The longest zone sample, in seconds at the project rate: longer imports are refused, and a
/// longer file found in the bundle is cut to this length (ADR-0154).
pub const MAX_ZONE_SECONDS: u32 = 60;

/// The most zones a sampler may have.
pub const MAX_ZONES: usize = 128;

/// The instrument kind id of the sampler, the only instrument that has zones.
pub const SAMPLER_KIND: &str = "sampler";

/// How a zone's sample plays once its note starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZoneMode {
    /// Plays to the end of the sample; note-off is ignored.
    OneShot,
    /// Releases through the amplitude envelope on note-off.
    Gated,
}

/// One zone of a sampler: an audio file played across a key range, pitched from its root key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SampleZone {
    /// The sample: [`Source::Mono`] or [`Source::Stereo`], never MIDI.
    pub file: Source,
    /// The lowest key the zone plays, 0–127.
    pub low: u8,
    /// The highest key the zone plays, `low`–127.
    pub high: u8,
    /// The key at which the sample plays at its own pitch, 0–127; it may lie outside the range.
    pub root: u8,
    /// One-shot or gated.
    pub mode: ZoneMode,
    /// The choke group, 1–16: a note in the group cuts every other voice in it.
    pub choke: Option<u8>,
}

impl SampleZone {
    /// Whether key `note` falls in the zone's range.
    pub fn contains(&self, note: u8) -> bool {
        (self.low..=self.high).contains(&note)
    }

    /// Checks the zone's limits.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidZone`] for a MIDI source, a key above 127, `low` above `high`, or a choke
    /// group outside 1–16.
    pub fn check(&self) -> Result<(), Error> {
        if matches!(self.file, Source::Midi(_)) {
            return Err(Error::InvalidZone("a zone plays an audio file, not MIDI"));
        }
        if self.low > MAX_KEY || self.high > MAX_KEY || self.root > MAX_KEY {
            return Err(Error::InvalidZone("keys are 0 to 127"));
        }
        if self.low > self.high {
            return Err(Error::InvalidZone("the low key is above the high key"));
        }
        if self.choke.is_some_and(|g| g == 0 || g > MAX_CHOKE_GROUP) {
            return Err(Error::InvalidZone("choke groups are 1 to 16"));
        }
        Ok(())
    }
}

/// Checks the zones of an instrument of kind `kind`: only a sampler may have any, at most
/// [`MAX_ZONES`], each within [`SampleZone::check`]'s limits.
///
/// # Errors
///
/// [`Error::InvalidZone`].
pub fn check_zones(kind: &str, zones: &[SampleZone]) -> Result<(), Error> {
    if !zones.is_empty() && kind != SAMPLER_KIND {
        return Err(Error::InvalidZone("only a sampler has zones"));
    }
    if zones.len() > MAX_ZONES {
        return Err(Error::InvalidZone("a sampler has at most 128 zones"));
    }
    zones.iter().try_for_each(SampleZone::check)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::AudioFile;

    fn zone() -> SampleZone {
        SampleZone {
            file: Source::Mono(AudioFile {
                name: "kick.wav".into(),
                frames: 100,
            }),
            low: 36,
            high: 36,
            root: 36,
            mode: ZoneMode::OneShot,
            choke: None,
        }
    }

    #[test]
    fn test_check_accepts_root_outside_range() {
        let z = SampleZone { root: 0, ..zone() };
        assert_eq!(z.check(), Ok(()));
    }

    #[test]
    fn test_check_refuses_bad_limits() {
        for bad in [
            SampleZone { low: 40, ..zone() },
            SampleZone {
                high: 128,
                ..zone()
            },
            SampleZone {
                root: 128,
                ..zone()
            },
            SampleZone {
                choke: Some(0),
                ..zone()
            },
            SampleZone {
                choke: Some(17),
                ..zone()
            },
        ] {
            assert!(bad.check().is_err(), "{bad:?}");
        }
    }

    #[test]
    fn test_check_zones_only_sampler_has_zones() {
        assert!(check_zones("synth", &[zone()]).is_err());
        assert_eq!(check_zones("synth", &[]), Ok(()));
        assert_eq!(check_zones(SAMPLER_KIND, &[zone()]), Ok(()));
    }
}
