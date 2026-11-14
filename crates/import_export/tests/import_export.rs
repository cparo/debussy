//! Import into stored mono files and export from an offline render, with WAV on disk and
//! compressed formats through `platform_interfaces`' test doubles.

use std::error::Error;
use std::f64::consts::TAU;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use audio_buffers::Layout;
use audio_engine::{ChannelClip, ClipAudio, RenderFeed, TrackFeed, TrackFeeds, TrackRenderer};
use import_export::{
    Error as IoError, ExportChannels, ExportFormat, ImportFormat, OfflineRender, WavDepth, export,
    import,
};
use platform_interfaces::test_doubles::{FakeCodecs, FakeFile};
use platform_interfaces::{CodecFormat, FlacDepth, SourceInfo};
use signal_graph::{Config, Graph};
use wav_files::{Channels, SampleFormat, Spec, WavReader, WavWriter};

fn scratch(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("import-export-{}-{name}", std::process::id()));
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(dir.join("audio"))?;
    Ok(dir)
}

fn sine(frames: usize, rate: u32, hz: f64, phase: f64) -> Vec<f32> {
    (0..frames)
        .map(|n| (0.5 * (TAU * hz * n as f64 / f64::from(rate) + phase).sin()) as f32)
        .collect()
}

fn write_wav(
    path: &Path,
    rate: u32,
    format: SampleFormat,
    channels: &[&[f32]],
) -> Result<(), Box<dyn Error>> {
    let spec = Spec {
        channels: if channels.len() == 2 {
            Channels::Stereo
        } else {
            Channels::Mono
        },
        sample_rate: rate,
        format,
    };
    let mut writer = WavWriter::new(std::fs::File::create(path)?, spec)?;
    writer.write(channels)?;
    writer.finish()?;
    Ok(())
}

/// A WAV file's spec and its channels.
type Wav = (Spec, Vec<Vec<f32>>);

/// Every channel of the WAV at `path`, and its spec.
fn read_wav(path: &Path) -> Result<Wav, Box<dyn Error>> {
    let mut reader = WavReader::new(std::fs::File::open(path)?)?;
    let spec = reader.spec();
    let frames = reader.frames() as usize;
    let mut channels = vec![vec![0.0; frames]; spec.channels.count()];
    let mut views: Vec<&mut [f32]> = channels.iter_mut().map(|c| c.as_mut_slice()).collect();
    assert_eq!(reader.read(&mut views)?, frames);
    Ok((spec, channels))
}

#[test]
fn test_import_wav_at_project_rate_stores_exact_mono_files() -> Result<(), Box<dyn Error>> {
    let dir = scratch("same-rate")?;
    let audio = dir.join("audio");
    let left = sine(10_000, 48_000, 440.0, 0.0);
    let right = sine(10_000, 48_000, 220.0, 1.0);
    let path = dir.join("Take: one.wav");
    write_wav(&path, 48_000, SampleFormat::Float32, &[&left, &right])?;
    let imported = import(&path, &audio, 48_000, &FakeCodecs::new())?;
    assert_eq!(imported.format, ImportFormat::Wav);
    assert_eq!((imported.frames, imported.source_rate), (10_000, 48_000));
    let names: Vec<&str> = imported.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Take_ one 1 L.wav", "Take_ one 1 R.wav"]);
    for (file, want) in imported.files.iter().zip([&left, &right]) {
        let (spec, channels) = read_wav(&audio.join(&file.name))?;
        assert_eq!(
            (spec.channels, spec.format, spec.sample_rate),
            (Channels::Mono, SampleFormat::Float32, 48_000)
        );
        assert_eq!(&channels[0], want);
        assert_eq!(file.frames, 10_000);
    }
    // A second import of the same name takes the next number.
    let again = import(&path, &audio, 48_000, &FakeCodecs::new())?;
    assert_eq!(again.files[0].name, "Take_ one 2 L.wav");
    Ok(())
}

#[test]
fn test_import_resamples_and_keeps_alignment() -> Result<(), Box<dyn Error>> {
    let dir = scratch("resample")?;
    let audio = dir.join("audio");
    let path = dir.join("tone.wav");
    let frames = 44_100;
    write_wav(
        &path,
        44_100,
        SampleFormat::Int24,
        &[&sine(frames, 44_100, 1000.0, 0.3)],
    )?;
    let imported = import(&path, &audio, 48_000, &FakeCodecs::new())?;
    assert_eq!(imported.frames, 48_000);
    assert_eq!(imported.source_rate, 44_100);
    let (_, channels) = read_wav(&audio.join(&imported.files[0].name))?;
    assert_eq!(channels[0].len(), 48_000);
    // Away from the edges the stored signal is the same tone at 48 kHz, in phase to within the
    // converter's fractional delay, which is rounded to the nearest sample (under half a sample).
    let latency = dsp::Resampler::new(44_100, 48_000, dsp::Quality::Import)?.latency();
    let residual = latency - latency.round();
    assert!(residual.abs() <= 0.5);
    let want: Vec<f32> = (0..48_000)
        .map(|n| (0.5 * (TAU * 1000.0 * (n as f64 - residual) / 48_000.0 + 0.3).sin()) as f32)
        .collect();
    let worst = (1000..47_000)
        .map(|n| (channels[0][n] - want[n]).abs())
        .fold(0.0f32, f32::max);
    assert!(worst < 1e-3, "worst error {worst}");
    Ok(())
}

#[test]
fn test_import_compressed_through_codecs() -> Result<(), Box<dyn Error>> {
    let dir = scratch("codecs")?;
    let audio = dir.join("audio");
    let codecs = FakeCodecs::new();
    let stereo: Vec<f32> = (0..2000).map(|i| i as f32 / 4000.0).collect();
    codecs.add_file(
        dir.join("song.flac"),
        FakeFile {
            info: SourceInfo {
                format: CodecFormat::Flac,
                sample_rate: 48_000,
                channels: 2,
                frames: Some(1000),
            },
            samples: stereo.clone(),
        },
    );
    let imported = import(&dir.join("song.flac"), &audio, 48_000, &codecs)?;
    assert_eq!(imported.format, ImportFormat::Compressed(CodecFormat::Flac));
    assert_eq!(imported.frames, 1000);
    let (_, left) = read_wav(&audio.join(&imported.files[0].name))?;
    let (_, right) = read_wav(&audio.join(&imported.files[1].name))?;
    let want_left: Vec<f32> = stereo.iter().step_by(2).copied().collect();
    let want_right: Vec<f32> = stereo.iter().skip(1).step_by(2).copied().collect();
    assert_eq!(left[0], want_left);
    assert_eq!(right[0], want_right);

    codecs.add_file(
        dir.join("voice.mp3"),
        FakeFile {
            info: SourceInfo {
                format: CodecFormat::Mp3,
                sample_rate: 44_100,
                channels: 1,
                frames: None,
            },
            samples: sine(4410, 44_100, 300.0, 0.0),
        },
    );
    let imported = import(&dir.join("voice.mp3"), &audio, 96_000, &codecs)?;
    assert_eq!(imported.frames, 9600);
    assert_eq!(imported.files.len(), 1);
    Ok(())
}

#[test]
fn test_failed_import_leaves_nothing_behind() -> Result<(), Box<dyn Error>> {
    let dir = scratch("failed")?;
    let audio = dir.join("audio");
    let codecs = FakeCodecs::new();
    codecs.add_file(
        dir.join("surround.m4a"),
        FakeFile {
            info: SourceInfo {
                format: CodecFormat::Aac,
                sample_rate: 48_000,
                channels: 6,
                frames: Some(10),
            },
            samples: vec![0.0; 60],
        },
    );
    assert!(matches!(
        import(&dir.join("surround.m4a"), &audio, 48_000, &codecs),
        Err(IoError::Unsupported { .. })
    ));
    assert!(matches!(
        import(&dir.join("missing.mp3"), &audio, 48_000, &codecs),
        Err(IoError::Codec { .. })
    ));
    // A WAV that claims a rate the resampler cannot reach.
    let odd = dir.join("odd.wav");
    write_wav(&odd, 99_991, SampleFormat::Float32, &[&[0.25; 100]])?;
    assert!(matches!(
        import(&odd, &audio, 48_000, &codecs),
        Err(IoError::Resample { .. })
    ));
    assert_eq!(std::fs::read_dir(&audio)?.count(), 0);
    Ok(())
}

/// Stored audio for the render, in memory.
struct Memory(Arc<Vec<Vec<f32>>>);

impl ClipAudio for Memory {
    fn read(
        &mut self,
        file: usize,
        frame: u64,
        out: &mut [f32],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let samples = self.0.get(file).ok_or("no file")?;
        let start = frame as usize;
        out.copy_from_slice(samples.get(start..start + out.len()).ok_or("past end")?);
        Ok(())
    }
}

/// Exports a one-clip stereo project of `frames` frames, returning the left and right signals
/// the render produces (for comparison).
fn export_project(
    format: ExportFormat,
    channels: ExportChannels,
    path: &Path,
    codecs: &FakeCodecs,
) -> Result<(Vec<f32>, Vec<f32>), Box<dyn Error>> {
    let frames = 6000;
    let left = sine(frames, 48_000, 500.0, 0.0);
    let right = sine(frames, 48_000, 700.0, 0.5);
    let audio = Arc::new(vec![left.clone(), right.clone()]);
    let (mut graph, mut runner) = Graph::new(Config::new(48_000, 512))?;
    let source = graph.add_source(Layout::Stereo);
    let output = graph.add_sink(Layout::Stereo);
    graph.connect(source, 0, output, 0)?;
    graph.commit()?;
    let feed = |file: usize| -> Result<Box<dyn TrackFeed>, Box<dyn Error>> {
        let clip = ChannelClip {
            file,
            offset: 0,
            length: frames as u64,
            position: 0,
            gain: 1.0,
            fade_in: 0,
            fade_out: 0,
        };
        let renderer = TrackRenderer::new(vec![clip], 48_000)?;
        Ok(Box::new(RenderFeed::new(
            renderer,
            Memory(Arc::clone(&audio)),
            0,
        )))
    };
    let mut tracks = vec![TrackFeeds {
        source,
        feeds: vec![feed(0)?, feed(1)?],
    }];
    export(
        OfflineRender {
            runner: &mut runner,
            output,
            tracks: &mut tracks,
            start: 0,
            frames: frames as u64,
            block_frames: 256,
        },
        format,
        channels,
        48_000,
        path,
        codecs,
    )?;
    Ok((left, right))
}

#[test]
fn test_export_wav_float_is_the_render_exactly() -> Result<(), Box<dyn Error>> {
    let dir = scratch("export-float")?;
    let path = dir.join("mix.wav");
    let (left, right) = export_project(
        ExportFormat::Wav(WavDepth::Float32),
        ExportChannels::Stereo,
        &path,
        &FakeCodecs::new(),
    )?;
    let (spec, channels) = read_wav(&path)?;
    assert_eq!(
        (spec.channels, spec.format),
        (Channels::Stereo, SampleFormat::Float32)
    );
    assert_eq!(channels, vec![left, right]);
    Ok(())
}

#[test]
fn test_export_integer_wav_is_dithered_onto_the_grid() -> Result<(), Box<dyn Error>> {
    let dir = scratch("export-int")?;
    for (depth, format, bits) in [
        (WavDepth::Int16, SampleFormat::Int16, 16),
        (WavDepth::Int24, SampleFormat::Int24, 24),
    ] {
        let path = dir.join(format!("mix{bits}.wav"));
        let (left, right) = export_project(
            ExportFormat::Wav(depth),
            ExportChannels::Stereo,
            &path,
            &FakeCodecs::new(),
        )?;
        let (spec, channels) = read_wav(&path)?;
        assert_eq!(spec.format, format);
        let lsb = 2.0f32.powi(1 - bits);
        for (got, want) in channels.iter().zip([&left, &right]) {
            let worst = got
                .iter()
                .zip(want)
                .map(|(g, w)| (g - w).abs())
                .fold(0.0, f32::max);
            // TPDF dither of ±1 LSB, then rounding: within 1.5 LSB.
            assert!(worst <= 1.5 * lsb, "{bits}-bit error {worst}");
        }
        // Dithered, not merely rounded: some samples differ from plain rounding.
        let scale = 2.0f32.powi(bits - 1);
        assert!(
            channels[0]
                .iter()
                .zip(&left)
                .any(|(g, w)| (g * scale).round() != (w * scale).round())
        );
    }
    Ok(())
}

#[test]
fn test_export_mono_folds_down() -> Result<(), Box<dyn Error>> {
    let dir = scratch("export-mono")?;
    let path = dir.join("mono.wav");
    let (left, right) = export_project(
        ExportFormat::Wav(WavDepth::Float32),
        ExportChannels::Mono,
        &path,
        &FakeCodecs::new(),
    )?;
    let (spec, channels) = read_wav(&path)?;
    assert_eq!(spec.channels, Channels::Mono);
    let want: Vec<f32> = left
        .iter()
        .zip(&right)
        .map(|(l, r)| (l + r) * dsp::FOLD_DOWN_GAIN)
        .collect();
    assert_eq!(channels[0], want);
    Ok(())
}

#[test]
fn test_export_compressed_through_codecs() -> Result<(), Box<dyn Error>> {
    let dir = scratch("export-codecs")?;
    let codecs = FakeCodecs::new();
    let flac = dir.join("mix.flac");
    let (left, _) = export_project(
        ExportFormat::Flac(FlacDepth::Int16),
        ExportChannels::Stereo,
        &flac,
        &codecs,
    )?;
    let encoded = codecs.file(&flac).ok_or("nothing encoded")?;
    assert_eq!(encoded.info.format, CodecFormat::Flac);
    assert_eq!(
        (encoded.info.channels, encoded.info.sample_rate),
        (2, 48_000)
    );
    assert_eq!(encoded.info.frames, Some(left.len() as u64));
    // FLAC gets 16-bit-grid samples: dithered here, not by the encoder.
    assert!(
        encoded
            .samples
            .iter()
            .all(|s| (s * 32_768.0).fract() == 0.0)
    );

    let aac = dir.join("mix.m4a");
    export_project(
        ExportFormat::Aac { bitrate_kbps: 192 },
        ExportChannels::Mono,
        &aac,
        &codecs,
    )?;
    let encoded = codecs.file(&aac).ok_or("nothing encoded")?;
    assert_eq!(
        (encoded.info.format, encoded.info.channels),
        (CodecFormat::Aac, 1)
    );
    Ok(())
}

#[test]
fn test_export_to_a_missing_directory_fails_cleanly() -> Result<(), Box<dyn Error>> {
    let dir = scratch("export-missing")?;
    let result = export_project(
        ExportFormat::Wav(WavDepth::Int24),
        ExportChannels::Stereo,
        &dir.join("no such dir").join("mix.wav"),
        &FakeCodecs::new(),
    );
    assert!(result.is_err());
    Ok(())
}
