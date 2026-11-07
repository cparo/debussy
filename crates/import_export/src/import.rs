//! Import: decode a file, convert it to the project rate, and store each channel as a mono 32-bit
//! float WAV in `audio/` (`docs/architecture/import-export.md` "Import", ADR-0014, ADR-0053).

use std::fs::File;
use std::io::{BufReader, BufWriter, Read};
use std::path::{Path, PathBuf};

use dsp::{Quality, Resampler};
use platform_interfaces::{CodecFormat, Codecs, Decoder};
use project_model::AudioFile;
use wav_files::{Channels, SampleFormat, Spec, WavReader, WavWriter};

use crate::error::Error;
use crate::naming::{stem, unique_names};

/// Frames decoded at a time.
const CHUNK: usize = 16_384;

/// The format an imported file was in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportFormat {
    /// WAV or RF64, read in-house.
    Wav,
    /// A compressed format, read through the platform's codecs.
    Compressed(CodecFormat),
}

/// What an import stored.
#[derive(Debug, Clone, PartialEq)]
pub struct Imported {
    /// The stored files, one per channel, left first; each `frames` long, in `audio/`.
    pub files: Vec<AudioFile>,
    /// Frames per stored file, at the project rate.
    pub frames: u64,
    /// The file's format.
    pub format: ImportFormat,
    /// The file's own sample rate.
    pub source_rate: u32,
    /// Whether a WAV file was shorter than its header said; what was there was imported.
    pub truncated: bool,
}

/// A decoded file read in planar chunks.
trait Source {
    fn channels(&self) -> usize;
    fn rate(&self) -> u32;
    /// Fills each channel's buffer from the start; returns frames read, 0 at the end.
    fn read(&mut self, planar: &mut [Vec<f32>]) -> Result<usize, Error>;
}

struct WavSource {
    path: PathBuf,
    reader: WavReader<BufReader<File>>,
}

impl Source for WavSource {
    fn channels(&self) -> usize {
        self.reader.spec().channels.count()
    }

    fn rate(&self) -> u32 {
        self.reader.spec().sample_rate
    }

    fn read(&mut self, planar: &mut [Vec<f32>]) -> Result<usize, Error> {
        let mut views: Vec<&mut [f32]> = planar.iter_mut().map(|c| c.as_mut_slice()).collect();
        self.reader.read(&mut views).map_err(|source| Error::Wav {
            path: self.path.clone(),
            source,
        })
    }
}

struct CodecSource {
    path: PathBuf,
    decoder: Box<dyn Decoder>,
    interleaved: Vec<f32>,
}

impl Source for CodecSource {
    fn channels(&self) -> usize {
        usize::from(self.decoder.info().channels)
    }

    fn rate(&self) -> u32 {
        self.decoder.info().sample_rate
    }

    fn read(&mut self, planar: &mut [Vec<f32>]) -> Result<usize, Error> {
        let channels = planar.len();
        let frames = self
            .decoder
            .read(&mut self.interleaved)
            .map_err(|source| Error::Codec {
                path: self.path.clone(),
                source,
            })?;
        for (f, frame) in self.interleaved[..frames * channels]
            .chunks_exact(channels)
            .enumerate()
        {
            for (channel, &sample) in planar.iter_mut().zip(frame) {
                channel[f] = sample;
            }
        }
        Ok(frames)
    }
}

/// Whether `path` starts like a WAV or RF64 file. A file that cannot be read is left to the
/// codecs, which report it.
fn is_wav(path: &Path) -> bool {
    let mut head = [0u8; 12];
    File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok_and(|()| (&head[..4] == b"RIFF" || &head[..4] == b"RF64") && &head[8..12] == b"WAVE")
}

/// One channel's conversion to the project rate and its stored file.
struct Channel {
    resampler: Option<Resampler>,
    /// Output frames still to drop to undo the converter's delay.
    skip: usize,
    converted: Vec<f32>,
    writer: WavWriter<BufWriter<File>>,
    written: u64,
}

impl Channel {
    /// Converts `input` and writes up to `limit` frames in total.
    fn push(&mut self, input: &[f32], limit: u64, path: &Path) -> Result<(), Error> {
        let Channel {
            resampler,
            skip,
            converted,
            writer,
            written,
        } = self;
        let Some(resampler) = resampler else {
            return write(writer, written, input, limit, path);
        };
        let mut consumed = 0;
        while consumed < input.len() {
            let (used, produced) = resampler.process(&input[consumed..], converted);
            consumed += used;
            let dropped = (*skip).min(produced);
            *skip -= dropped;
            write(writer, written, &converted[dropped..produced], limit, path)?;
            if used == 0 && produced == 0 {
                break;
            }
        }
        Ok(())
    }
}

/// Writes up to `limit - written` of `samples` to a stored file.
fn write(
    writer: &mut WavWriter<BufWriter<File>>,
    written: &mut u64,
    samples: &[f32],
    limit: u64,
    path: &Path,
) -> Result<(), Error> {
    let n = (samples.len() as u64).min(limit.saturating_sub(*written)) as usize;
    if n > 0 {
        writer
            .write(&[&samples[..n]])
            .map_err(|source| Error::Wav {
                path: path.to_owned(),
                source,
            })?;
        *written += n as u64;
    }
    Ok(())
}

/// Imports `path` into `audio_dir` at `project_rate`: WAV and RF64 in-house, anything else
/// through `codecs`. Each channel becomes a mono 32-bit float WAV named after the file
/// (ADR-0053), converted with the import-quality resampler and aligned to the nearest sample: the
/// converter's delay is removed, rounded to whole frames. Mono and stereo files only. On failure every file it made is removed.
///
/// # Errors
///
/// [`Error::Wav`], [`Error::Codec`], [`Error::Unsupported`], [`Error::Resample`],
/// [`Error::Io`].
pub fn import(
    path: &Path,
    audio_dir: &Path,
    project_rate: u32,
    codecs: &dyn Codecs,
) -> Result<Imported, Error> {
    let (mut source, format, truncated): (Box<dyn Source>, ImportFormat, bool) = if is_wav(path) {
        let file = File::open(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?;
        let reader = WavReader::new(BufReader::new(file)).map_err(|source| Error::Wav {
            path: path.to_owned(),
            source,
        })?;
        let truncated = reader.truncated();
        let source = WavSource {
            path: path.to_owned(),
            reader,
        };
        (Box::new(source), ImportFormat::Wav, truncated)
    } else {
        let decoder = codecs.decode(path).map_err(|source| Error::Codec {
            path: path.to_owned(),
            source,
        })?;
        let info = decoder.info();
        let channels = usize::from(info.channels);
        let source = CodecSource {
            path: path.to_owned(),
            decoder,
            interleaved: vec![0.0; CHUNK * channels.max(1)],
        };
        (
            Box::new(source),
            ImportFormat::Compressed(info.format),
            false,
        )
    };
    let channels = source.channels();
    if !(1..=2).contains(&channels) {
        return Err(Error::Unsupported {
            path: path.to_owned(),
            reason: format!("{channels} channels; only mono and stereo can be imported"),
        });
    }
    let source_rate = source.rate();
    if source_rate == 0 {
        return Err(Error::Unsupported {
            path: path.to_owned(),
            reason: "a sample rate of 0".to_owned(),
        });
    }
    let names = unique_names(audio_dir, &stem(path), channels);
    let paths: Vec<PathBuf> = names.iter().map(|n| audio_dir.join(n)).collect();
    let result = store(source.as_mut(), &paths, source_rate, project_rate);
    match result {
        Ok(frames) => Ok(Imported {
            files: names
                .into_iter()
                .map(|name| AudioFile { name, frames })
                .collect(),
            frames,
            format,
            source_rate,
            truncated,
        }),
        Err(error) => {
            for path in &paths {
                let _ = std::fs::remove_file(path);
            }
            Err(error)
        }
    }
}

/// Decodes `source` into one stored file per path, returning the frames each holds.
fn store(source: &mut dyn Source, paths: &[PathBuf], from: u32, to: u32) -> Result<u64, Error> {
    let spec = Spec {
        channels: Channels::Mono,
        sample_rate: to,
        format: SampleFormat::Float32,
    };
    let mut channels = Vec::with_capacity(paths.len());
    for path in paths {
        let resampler =
            if from == to {
                None
            } else {
                Some(
                    Resampler::new(from, to, Quality::Import)
                        .map_err(|source| Error::Resample { from, to, source })?,
                )
            };
        let skip = resampler
            .as_ref()
            .map_or(0, |r| r.latency().round() as usize);
        let file = File::create(path).map_err(|source| Error::Io {
            path: path.clone(),
            source,
        })?;
        let writer = WavWriter::new(BufWriter::new(file), spec).map_err(|source| Error::Wav {
            path: path.clone(),
            source,
        })?;
        channels.push(Channel {
            resampler,
            skip,
            converted: vec![0.0; CHUNK * (to as usize / from.max(1) as usize + 2)],
            writer,
            written: 0,
        });
    }

    let mut planar = vec![vec![0.0f32; CHUNK]; paths.len()];
    let mut input_frames: u64 = 0;
    loop {
        let n = source.read(&mut planar)?;
        if n == 0 {
            break;
        }
        input_frames += n as u64;
        for ((channel, samples), path) in channels.iter_mut().zip(&planar).zip(paths) {
            channel.push(&samples[..n], u64::MAX, path)?;
        }
    }
    // The stored length: the input's duration at the project rate, rounded up.
    let frames = (u128::from(input_frames) * u128::from(to)).div_ceil(u128::from(from)) as u64;
    // Flush each converter's delay with silence until the stored length is reached.
    let silence = vec![0.0f32; CHUNK];
    for (channel, path) in channels.iter_mut().zip(paths) {
        while channel.written < frames {
            channel.push(&silence, frames, path)?;
        }
    }
    for (channel, path) in channels.into_iter().zip(paths) {
        channel.writer.finish().map_err(|source| Error::Wav {
            path: path.clone(),
            source,
        })?;
    }
    Ok(frames)
}
