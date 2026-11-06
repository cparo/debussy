//! What a WAV file holds, and converting its samples to and from `f32`.

use crate::error::Error;

/// The highest sample rate accepted, in hertz (ADR-0029).
pub const MAX_SAMPLE_RATE: u32 = 768_000;

/// `WAVE_FORMAT_PCM`.
pub(crate) const TAG_PCM: u16 = 0x0001;
/// `WAVE_FORMAT_IEEE_FLOAT`.
pub(crate) const TAG_FLOAT: u16 = 0x0003;
/// `WAVE_FORMAT_EXTENSIBLE`.
pub(crate) const TAG_EXTENSIBLE: u16 = 0xFFFE;

/// Mono or stereo: the only channel counts supported (ADR-0029).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    /// One channel.
    Mono,
    /// Left and right, interleaved in the file.
    Stereo,
}

impl Channels {
    /// The number of channels.
    pub const fn count(self) -> usize {
        match self {
            Channels::Mono => 1,
            Channels::Stereo => 2,
        }
    }

    /// From a header's channel count.
    pub(crate) fn from_count(count: u16) -> Result<Self, Error> {
        match count {
            1 => Ok(Channels::Mono),
            2 => Ok(Channels::Stereo),
            other => Err(Error::UnsupportedChannels(other)),
        }
    }
}

/// How each sample is encoded in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SampleFormat {
    /// 16-bit signed integer.
    Int16,
    /// 24-bit signed integer, packed in three bytes.
    Int24,
    /// 32-bit signed integer.
    Int32,
    /// 32-bit IEEE float.
    Float32,
    /// 64-bit IEEE float.
    Float64,
}

impl SampleFormat {
    /// Bytes per sample.
    pub const fn bytes(self) -> usize {
        match self {
            SampleFormat::Int16 => 2,
            SampleFormat::Int24 => 3,
            SampleFormat::Int32 | SampleFormat::Float32 => 4,
            SampleFormat::Float64 => 8,
        }
    }

    /// The plain format tag for this encoding.
    pub(crate) const fn tag(self) -> u16 {
        match self {
            SampleFormat::Int16 | SampleFormat::Int24 | SampleFormat::Int32 => TAG_PCM,
            SampleFormat::Float32 | SampleFormat::Float64 => TAG_FLOAT,
        }
    }

    /// From a (plain) format tag and container bit depth.
    pub(crate) fn from_tag(tag: u16, bits: u16) -> Result<Self, Error> {
        match (tag, bits) {
            (TAG_PCM, 16) => Ok(SampleFormat::Int16),
            (TAG_PCM, 24) => Ok(SampleFormat::Int24),
            (TAG_PCM, 32) => Ok(SampleFormat::Int32),
            (TAG_FLOAT, 32) => Ok(SampleFormat::Float32),
            (TAG_FLOAT, 64) => Ok(SampleFormat::Float64),
            _ => Err(Error::UnsupportedFormat { tag, bits }),
        }
    }

    /// Decodes one sample from the start of `bytes`, which holds at least [`Self::bytes`].
    /// Integers map to -1.0..1.0.
    pub(crate) fn decode(self, bytes: &[u8]) -> f32 {
        let mut b = [0_u8; 8];
        let n = self.bytes().min(bytes.len());
        b[..n].copy_from_slice(&bytes[..n]);
        match self {
            SampleFormat::Int16 => f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0,
            // Placing the three bytes in the top of an i32 sign-extends them.
            SampleFormat::Int24 => {
                (i32::from_le_bytes([0, b[0], b[1], b[2]]) >> 8) as f32 / 8_388_608.0
            }
            SampleFormat::Int32 => {
                (f64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])) / 2_147_483_648.0) as f32
            }
            SampleFormat::Float32 => f32::from_le_bytes([b[0], b[1], b[2], b[3]]),
            SampleFormat::Float64 => f64::from_le_bytes(b) as f32,
        }
    }

    /// Encodes `sample` into the start of `out`, which holds at least [`Self::bytes`]. Integer
    /// encodings round and clip to the format's range, without dither; NaN becomes 0.
    pub(crate) fn encode(self, sample: f32, out: &mut [u8]) {
        let scaled = |full_scale: f64, min: f64, max: f64| -> i64 {
            // `as` saturates and maps NaN to 0, so this cannot overflow.
            (f64::from(sample) * full_scale).round().clamp(min, max) as i64
        };
        match self {
            SampleFormat::Int16 => {
                let v = scaled(32_768.0, -32_768.0, 32_767.0) as i16;
                out[..2].copy_from_slice(&v.to_le_bytes());
            }
            SampleFormat::Int24 => {
                let v = scaled(8_388_608.0, -8_388_608.0, 8_388_607.0) as i32;
                out[..3].copy_from_slice(&v.to_le_bytes()[..3]);
            }
            SampleFormat::Int32 => {
                let v = scaled(2_147_483_648.0, -2_147_483_648.0, 2_147_483_647.0) as i32;
                out[..4].copy_from_slice(&v.to_le_bytes());
            }
            SampleFormat::Float32 => out[..4].copy_from_slice(&sample.to_le_bytes()),
            SampleFormat::Float64 => out[..8].copy_from_slice(&f64::from(sample).to_le_bytes()),
        }
    }
}

/// What a WAV file holds: its channels, sample rate and sample encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Spec {
    /// Mono or stereo.
    pub channels: Channels,
    /// Frames per second.
    pub sample_rate: u32,
    /// The sample encoding.
    pub format: SampleFormat,
}

impl Spec {
    /// Bytes per frame: channels × bytes per sample.
    pub fn block_align(&self) -> usize {
        self.channels.count() * self.format.bytes()
    }

    /// Checks the sample rate is within 1 Hz to [`MAX_SAMPLE_RATE`].
    pub(crate) fn check_rate(sample_rate: u32) -> Result<u32, Error> {
        if (1..=MAX_SAMPLE_RATE).contains(&sample_rate) {
            Ok(sample_rate)
        } else {
            Err(Error::SampleRateOutOfRange(sample_rate))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [SampleFormat; 5] = [
        SampleFormat::Int16,
        SampleFormat::Int24,
        SampleFormat::Int32,
        SampleFormat::Float32,
        SampleFormat::Float64,
    ];

    #[test]
    fn test_encode_decode_round_trips_exact_values() {
        let samples = [0.0, 0.5, -0.5, -1.0, 0.25, -0.125];
        for format in ALL {
            let mut buf = [0_u8; 8];
            for s in samples {
                format.encode(s, &mut buf);
                assert_eq!(format.decode(&buf), s, "{format:?} {s}");
            }
        }
    }

    #[test]
    fn test_encode_clips_and_maps_nan_to_zero() {
        let mut buf = [0_u8; 8];
        for format in [
            SampleFormat::Int16,
            SampleFormat::Int24,
            SampleFormat::Int32,
        ] {
            format.encode(2.0, &mut buf);
            // Full scale rounds to exactly 1.0 in f32 for 32-bit integers.
            let top = format.decode(&buf);
            assert!(top <= 1.0 && top > 0.999, "{format:?}");
            format.encode(-2.0, &mut buf);
            assert_eq!(format.decode(&buf), -1.0, "{format:?}");
            format.encode(f32::NAN, &mut buf);
            assert_eq!(format.decode(&buf), 0.0, "{format:?}");
        }
    }

    #[test]
    fn test_decode_int24_sign_extends() {
        assert_eq!(
            SampleFormat::Int24.decode(&[0xFF, 0xFF, 0xFF]),
            -1.0 / 8_388_608.0
        );
        assert_eq!(SampleFormat::Int24.decode(&[0x00, 0x00, 0x80]), -1.0);
        assert_eq!(
            SampleFormat::Int24.decode(&[0x00]),
            0.0,
            "short input is zero-padded"
        );
    }

    #[test]
    fn test_from_tag_and_channels_reject_unsupported() {
        assert!(SampleFormat::from_tag(TAG_PCM, 8).is_err());
        assert!(SampleFormat::from_tag(TAG_FLOAT, 16).is_err());
        assert!(SampleFormat::from_tag(0x0055, 16).is_err());
        assert!(matches!(
            Channels::from_count(0),
            Err(Error::UnsupportedChannels(0))
        ));
        assert!(matches!(
            Channels::from_count(3),
            Err(Error::UnsupportedChannels(3))
        ));
        assert!(Spec::check_rate(0).is_err());
        assert!(Spec::check_rate(MAX_SAMPLE_RATE + 1).is_err());
        assert!(Spec::check_rate(44_100).is_ok());
    }
}
