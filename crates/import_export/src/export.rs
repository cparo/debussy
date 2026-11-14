//! Export: an offline render of the project, folded to mono if asked, dithered to an integer
//! depth, and written as WAV in-house, encoded through the platform's codecs, or encoded to MP3
//! by a host-installed `lame` (`docs/architecture/import-export.md` "Export", ADR-0025, ADR-0053,
//! ADR-0129).

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

use audio_engine::TrackFeeds;
use dsp::{Depth, Ditherer, fold_to_mono};
use platform_interfaces::{Codecs, EncodeSettings, Encoder, FlacDepth};
use signal_graph::{NodeId, Runner};
use wav_files::{Channels, SampleFormat, Spec, WavWriter};

use crate::error::Error;
use crate::lame::Lame;

/// The dither seeds of the left (or mono) and right channels: fixed, so an export is
/// reproducible, and different, so the channels' noise is uncorrelated.
const DITHER_SEEDS: [u64; 2] = [0xD17E_0001, 0xD17E_0002];

/// A WAV export's sample format (`docs/architecture/import-export.md` "Export bit depth").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WavDepth {
    /// 16-bit integer, dithered.
    Int16,
    /// 24-bit integer, dithered.
    Int24,
    /// 32-bit float, unchanged.
    Float32,
}

/// What to export to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// WAV (RF64 past 4 GB), written in-house.
    Wav(WavDepth),
    /// FLAC through the platform codecs, dithered to its depth first.
    Flac(FlacDepth),
    /// AAC through the platform codecs.
    Aac {
        /// Bit rate in kbit/s.
        bitrate_kbps: u32,
    },
    /// MP3 through a host-installed `lame`, dithered to 16 bits first (ADR-0129).
    Mp3 {
        /// Constant bit rate.
        bitrate: Mp3Bitrate,
    },
}

/// The constant bit rates MP3 export offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mp3Bitrate {
    /// 128 kbit/s.
    Kbps128,
    /// 256 kbit/s.
    Kbps256,
    /// 320 kbit/s.
    Kbps320,
}

impl Mp3Bitrate {
    /// Every bit rate, lowest first.
    pub const ALL: [Mp3Bitrate; 3] = [
        Mp3Bitrate::Kbps128,
        Mp3Bitrate::Kbps256,
        Mp3Bitrate::Kbps320,
    ];

    /// Kilobits per second.
    pub const fn kbps(self) -> u32 {
        match self {
            Mp3Bitrate::Kbps128 => 128,
            Mp3Bitrate::Kbps256 => 256,
            Mp3Bitrate::Kbps320 => 320,
        }
    }

    /// The bit rate of `kbps` kilobits per second, if it is one of [`Mp3Bitrate::ALL`].
    pub fn from_kbps(kbps: u32) -> Option<Mp3Bitrate> {
        Mp3Bitrate::ALL.into_iter().find(|b| b.kbps() == kbps)
    }
}

/// The exported channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportChannels {
    /// The stereo mix.
    Stereo,
    /// The mono mix, `(L + R) × 0.7071` (ADR-0030).
    Mono,
}

/// An offline render to export: the arguments of [`audio_engine::render`].
pub struct OfflineRender<'a> {
    /// The render graph's runner.
    pub runner: &'a mut Runner,
    /// The output sink.
    pub output: NodeId,
    /// The track feeds, positioned at the range's start.
    pub tracks: &'a mut [TrackFeeds],
    /// The range's first timeline frame, where curves are read from.
    pub start: u64,
    /// The range's length, in frames.
    pub frames: u64,
    /// Frames per block.
    pub block_frames: usize,
}

/// Where rendered samples go.
enum Sink {
    Wav(WavWriter<BufWriter<File>>),
    Codec(Box<dyn Encoder>),
}

/// Exports `render` to `path` as `format` with `channels`, at the project's `sample_rate`. MP3
/// needs `lame`, the host's LAME found at startup.
///
/// # Errors
///
/// [`Error::Render`], [`Error::Wav`], [`Error::Codec`], [`Error::Io`], or
/// [`Error::Mp3Unavailable`] for MP3 without `lame`.
pub fn export(
    render: OfflineRender<'_>,
    format: ExportFormat,
    channels: ExportChannels,
    sample_rate: u32,
    path: &Path,
    codecs: &dyn Codecs,
    lame: Option<&Lame>,
) -> Result<(), Error> {
    let count = match channels {
        ExportChannels::Stereo => 2,
        ExportChannels::Mono => 1,
    };
    let wav_error = |source| Error::Wav {
        path: path.to_owned(),
        source,
    };
    let codec_error = |source| Error::Codec {
        path: path.to_owned(),
        source,
    };
    let depth = match format {
        ExportFormat::Wav(WavDepth::Int16)
        | ExportFormat::Flac(FlacDepth::Int16)
        | ExportFormat::Mp3 { .. } => Some(Depth::Int16),
        ExportFormat::Wav(WavDepth::Int24) | ExportFormat::Flac(FlacDepth::Int24) => {
            Some(Depth::Int24)
        }
        ExportFormat::Wav(WavDepth::Float32) | ExportFormat::Aac { .. } => None,
    };
    let mut sink = match format {
        ExportFormat::Wav(wav_depth) => {
            let spec = Spec {
                channels: if count == 2 {
                    Channels::Stereo
                } else {
                    Channels::Mono
                },
                sample_rate,
                format: match wav_depth {
                    WavDepth::Int16 => SampleFormat::Int16,
                    WavDepth::Int24 => SampleFormat::Int24,
                    WavDepth::Float32 => SampleFormat::Float32,
                },
            };
            let file = File::create(path).map_err(|source| Error::Io {
                path: path.to_owned(),
                source,
            })?;
            Sink::Wav(WavWriter::new(BufWriter::new(file), spec).map_err(wav_error)?)
        }
        ExportFormat::Flac(depth) => Sink::Codec(
            codecs
                .encode(
                    path,
                    EncodeSettings::Flac { depth },
                    sample_rate,
                    count as u16,
                )
                .map_err(codec_error)?,
        ),
        ExportFormat::Aac { bitrate_kbps } => Sink::Codec(
            codecs
                .encode(
                    path,
                    EncodeSettings::Aac { bitrate_kbps },
                    sample_rate,
                    count as u16,
                )
                .map_err(codec_error)?,
        ),
        ExportFormat::Mp3 { bitrate } => {
            let lame = lame.ok_or(Error::Mp3Unavailable)?;
            Sink::Codec(Box::new(
                lame.encoder(path, bitrate.kbps(), sample_rate, count as u16)
                    .map_err(codec_error)?,
            ))
        }
    };
    let mut ditherers: Vec<Ditherer> = depth
        .map(|d| DITHER_SEEDS.iter().map(|&s| Ditherer::new(d, s)).collect())
        .unwrap_or_default();
    let mut left = Vec::with_capacity(render.block_frames);
    let mut right = Vec::with_capacity(render.block_frames);
    let mut interleaved = Vec::with_capacity(2 * render.block_frames);
    let mut write = |l: &[f32], r: &[f32]| -> Result<(), Error> {
        left.clear();
        right.clear();
        match channels {
            ExportChannels::Stereo => {
                left.extend_from_slice(l);
                right.extend_from_slice(r);
            }
            ExportChannels::Mono => {
                left.resize(l.len(), 0.0);
                fold_to_mono(l, r, &mut left);
            }
        }
        if let [first, rest @ ..] = ditherers.as_mut_slice() {
            first.process(&mut left);
            if let Some(second) = rest.first_mut() {
                second.process(&mut right);
            }
        }
        match &mut sink {
            Sink::Wav(writer) => {
                let result = if count == 2 {
                    writer.write(&[&left, &right])
                } else {
                    writer.write(&[&left])
                };
                result.map_err(wav_error)
            }
            Sink::Codec(encoder) => {
                interleaved.clear();
                if count == 2 {
                    interleaved.extend(left.iter().zip(&right).flat_map(|(&a, &b)| [a, b]));
                } else {
                    interleaved.extend_from_slice(&left);
                }
                encoder.write(&interleaved).map_err(codec_error)
            }
        }
    };
    let mut failure: Option<Error> = None;
    let rendered = audio_engine::render(
        render.runner,
        render.output,
        render.tracks,
        render.start,
        render.frames,
        render.block_frames,
        &mut |l, r| {
            write(l, r).map_err(|error| {
                let message = error.to_string();
                failure = Some(error);
                audio_engine::Error::Output(message.into())
            })
        },
    );
    if let Some(error) = failure {
        return Err(error);
    }
    rendered?;
    match sink {
        Sink::Wav(writer) => {
            writer.finish().map_err(wav_error)?;
        }
        Sink::Codec(encoder) => encoder.finish().map_err(codec_error)?,
    }
    Ok(())
}
