//! [`MacCodecs`]: MP3 decoding and AAC and FLAC encoding and decoding through AudioToolbox
//! (`docs/architecture/platform.md` "Compressed audio codecs", ADR-0024, ADR-0051).
//!
//! An imported file is untrusted. AudioToolbox parses it; this module refuses any file whose
//! format is not MP3, AAC or FLAC, or that reports more than [`MAX_CHANNELS`] channels, a sample
//! rate outside [`MIN_SAMPLE_RATE`]..=[`MAX_SAMPLE_RATE`], or more than [`MAX_SECONDS`] of audio,
//! and stops a decode that runs past that length. Every status is checked; nothing panics on bad
//! input.

use std::path::Path;

use objc2_audio_toolbox::{kAudioFileFLACType, kAudioFileM4AType};
use objc2_core_audio_types::{
    AudioStreamBasicDescription, kAppleLosslessFormatFlag_16BitSourceData,
    kAppleLosslessFormatFlag_24BitSourceData, kAudioFormatFLAC, kAudioFormatMPEG4AAC,
    kAudioFormatMPEGLayer3,
};
use platform_interfaces::{
    CodecFormat, Codecs, Decoder, EncodeSettings, Encoder, Error, FlacDepth, SourceInfo,
};

use crate::sys::{ExtAudioFile, Status, float_format, format_info};

/// The most channels a decoded file may have; stored audio is mono or stereo (ADR-0029).
pub const MAX_CHANNELS: u16 = 2;
/// The lowest sample rate accepted, in hertz.
pub const MIN_SAMPLE_RATE: u32 = 8_000;
/// The highest sample rate accepted, in hertz.
pub const MAX_SAMPLE_RATE: u32 = 384_000;
/// The longest file accepted, in seconds: 24 hours.
pub const MAX_SECONDS: u64 = 24 * 60 * 60;

/// The macOS codecs.
#[derive(Debug, Default, Clone, Copy)]
pub struct MacCodecs;

impl MacCodecs {
    /// The codecs.
    pub fn new() -> Self {
        MacCodecs
    }
}

fn platform(status: Status) -> Error {
    Error::Platform(status.to_string())
}

/// The codec of an AudioToolbox format id, if it is one Debussy decodes this way.
fn codec_of(format_id: u32) -> Option<CodecFormat> {
    match format_id {
        id if id == kAudioFormatMPEGLayer3 => Some(CodecFormat::Mp3),
        id if id == kAudioFormatMPEG4AAC => Some(CodecFormat::Aac),
        id if id == kAudioFormatFLAC => Some(CodecFormat::Flac),
        _ => None,
    }
}

/// Checks what an untrusted file reports, returning its rate, channels and frame limit.
fn check_source(format: &AudioStreamBasicDescription) -> Result<(u32, u16, u64), Error> {
    let rate = format.mSampleRate;
    if !rate.is_finite()
        || rate.fract() != 0.0
        || rate < f64::from(MIN_SAMPLE_RATE)
        || rate > f64::from(MAX_SAMPLE_RATE)
    {
        return Err(Error::Undecodable(format!("sample rate {rate} Hz")));
    }
    let channels = format.mChannelsPerFrame;
    if channels == 0 || channels > u32::from(MAX_CHANNELS) {
        return Err(Error::Undecodable(format!(
            "{channels} channels; at most {MAX_CHANNELS} are supported"
        )));
    }
    let rate = rate as u32;
    Ok((rate, channels as u16, u64::from(rate) * MAX_SECONDS))
}

/// A decoder of one file, reading interleaved `f32` at the file's rate and channels.
struct MacDecoder {
    file: ExtAudioFile,
    info: SourceInfo,
    read: u64,
    limit: u64,
}

impl Decoder for MacDecoder {
    fn info(&self) -> SourceInfo {
        self.info
    }

    fn read(&mut self, interleaved: &mut [f32]) -> Result<usize, Error> {
        let channels = usize::from(self.info.channels);
        let frames = interleaved.len() / channels;
        if frames == 0 {
            return Ok(0);
        }
        let n = self
            .file
            .read(&mut interleaved[..frames * channels])
            .map_err(|s| Error::Undecodable(s.to_string()))?;
        self.read += n as u64;
        if self.read > self.limit {
            return Err(Error::Undecodable(format!(
                "longer than {MAX_SECONDS} seconds"
            )));
        }
        Ok(n)
    }
}

/// An encoder writing interleaved `f32` frames into a file.
struct MacEncoder {
    file: ExtAudioFile,
    channels: usize,
}

impl Encoder for MacEncoder {
    fn write(&mut self, interleaved: &[f32]) -> Result<(), Error> {
        let whole = interleaved.len() / self.channels * self.channels;
        self.file.write(&interleaved[..whole]).map_err(platform)
    }

    fn finish(self: Box<Self>) -> Result<(), Error> {
        self.file.close().map_err(platform)
    }
}

impl Codecs for MacCodecs {
    fn can_decode(&self, _format: CodecFormat) -> bool {
        true
    }

    fn can_encode(&self, format: CodecFormat) -> bool {
        format != CodecFormat::Mp3
    }

    fn decode(&self, path: &Path) -> Result<Box<dyn Decoder>, Error> {
        // Report a missing or unreadable file as I/O, as the contract asks, before
        // AudioToolbox sees it.
        std::fs::File::open(path)?;
        let mut file = ExtAudioFile::open(path).map_err(|s| Error::Undecodable(s.to_string()))?;
        let source = file
            .file_format()
            .map_err(|s| Error::Undecodable(s.to_string()))?;
        let format = codec_of(source.mFormatID)
            .ok_or_else(|| Error::Undecodable("not an MP3, AAC or FLAC file".to_owned()))?;
        let (rate, channels, limit) = check_source(&source)?;
        let frames = file
            .length_frames()
            .ok()
            .and_then(|n| u64::try_from(n).ok())
            .filter(|&n| n > 0);
        if frames.is_some_and(|n| n > limit) {
            return Err(Error::Undecodable(format!(
                "longer than {MAX_SECONDS} seconds"
            )));
        }
        file.set_client_format(float_format(f64::from(rate), u32::from(channels)))
            .map_err(|s| Error::Undecodable(s.to_string()))?;
        Ok(Box::new(MacDecoder {
            file,
            info: SourceInfo {
                format,
                sample_rate: rate,
                channels,
                frames,
            },
            read: 0,
            limit,
        }))
    }

    fn encode(
        &self,
        path: &Path,
        settings: EncodeSettings,
        sample_rate: u32,
        channels: u16,
    ) -> Result<Box<dyn Encoder>, Error> {
        if channels == 0 || channels > MAX_CHANNELS {
            return Err(Error::Platform(format!(
                "cannot encode {channels} channels"
            )));
        }
        let (format_id, flags, file_type) = match settings {
            EncodeSettings::Aac { .. } => (kAudioFormatMPEG4AAC, 0, kAudioFileM4AType),
            EncodeSettings::Flac { depth } => (
                kAudioFormatFLAC,
                match depth {
                    FlacDepth::Int16 => kAppleLosslessFormatFlag_16BitSourceData,
                    FlacDepth::Int24 => kAppleLosslessFormatFlag_24BitSourceData,
                },
                kAudioFileFLACType,
            ),
        };
        let mut target = float_format(f64::from(sample_rate), u32::from(channels));
        target.mFormatID = format_id;
        target.mFormatFlags = flags;
        target.mBytesPerPacket = 0;
        target.mFramesPerPacket = 0;
        target.mBytesPerFrame = 0;
        target.mBitsPerChannel = 0;
        let target = format_info(target).map_err(platform)?;
        let mut file = ExtAudioFile::create(path, file_type, target).map_err(platform)?;
        file.set_client_format(float_format(f64::from(sample_rate), u32::from(channels)))
            .map_err(platform)?;
        if let EncodeSettings::Aac { bitrate_kbps } = settings {
            file.set_bit_rate(bitrate_kbps.saturating_mul(1000))
                .map_err(platform)?;
        }
        Ok(Box::new(MacEncoder {
            file,
            channels: usize::from(channels),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(rate: f64, channels: u32) -> AudioStreamBasicDescription {
        float_format(rate, channels)
    }

    #[test]
    fn test_check_source_limits_rate_and_channels() {
        assert_eq!(
            check_source(&source(48_000.0, 2)).ok(),
            Some((48_000, 2, 48_000 * MAX_SECONDS))
        );
        for (rate, channels) in [
            (7_999.0, 1),
            (384_001.0, 1),
            (44_100.5, 1),
            (f64::NAN, 1),
            (48_000.0, 0),
            (48_000.0, 6),
        ] {
            assert!(matches!(
                check_source(&source(rate, channels)),
                Err(Error::Undecodable(_))
            ));
        }
    }

    #[test]
    fn test_codec_of_maps_only_the_three_formats() {
        assert_eq!(codec_of(kAudioFormatMPEGLayer3), Some(CodecFormat::Mp3));
        assert_eq!(codec_of(kAudioFormatMPEG4AAC), Some(CodecFormat::Aac));
        assert_eq!(codec_of(kAudioFormatFLAC), Some(CodecFormat::Flac));
        assert_eq!(
            codec_of(objc2_core_audio_types::kAudioFormatLinearPCM),
            None
        );
    }
}
