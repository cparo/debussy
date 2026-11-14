//! Opening or creating the bundle, importing files as tracks, building waveform caches, and
//! exporting the mix.

use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::Path;

use anyhow::{Context, Result, bail};
use audio_analysis::WaveformWriter;
use audio_buffers::Layout;
use audio_engine::{RenderFeed, TrackFeed, TrackFeeds};
use disk_streaming::WavClipAudio;
use engine_bridge::{render_graph, track_renderers};
use import_export::{
    ExportChannels, ExportFormat, Lame, Mp3Bitrate, OfflineRender, WavDepth, export, import,
};
use macos_platform::MacCodecs;
use platform_interfaces::FlacDepth;
use project_model::{Clip, Session, Source, State};
use project_storage::Bundle;
use signal_graph::Config;
use wav_files::WavReader;

/// The largest block the engine processes, in frames.
pub const MAX_BLOCK: usize = 1024;

/// Opens `path`, creating it at `sample_rate` if it does not exist.
///
/// # Errors
///
/// If the bundle cannot be created or read.
pub fn open_or_create(path: &Path, sample_rate: u32) -> Result<(Bundle, Session)> {
    if !path.exists() {
        Bundle::create(path, sample_rate)
            .with_context(|| format!("creating {}", path.display()))?;
    }
    Bundle::open(path).with_context(|| format!("opening {}", path.display()))
}

/// Imports each file as a new track named after it, holding one clip of the whole file at the
/// start of the timeline, then saves.
///
/// # Errors
///
/// If a file cannot be imported or the project saved.
pub fn import_files(
    bundle: &mut Bundle,
    session: &mut Session,
    files: &[impl AsRef<Path>],
) -> Result<()> {
    for file in files {
        let file = file.as_ref();
        let imported = import(
            file,
            &bundle.audio_dir(),
            bundle.sample_rate(),
            &MacCodecs::new(),
        )
        .with_context(|| format!("importing {}", file.display()))?;
        let name = file.file_stem().map_or_else(
            || "Imported".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let (layout, source) = match imported.files.as_slice() {
            [mono] => (Layout::Mono, Source::Mono(mono.clone())),
            [left, right] => (
                Layout::Stereo,
                Source::Stereo {
                    left: left.clone(),
                    right: right.clone(),
                },
            ),
            _ => bail!(
                "{} imported as {} files",
                file.display(),
                imported.files.len()
            ),
        };
        if imported.frames == 0 {
            eprintln!("{} holds no audio; skipped", file.display());
            continue;
        }
        session.add_track_with_clip(&name, layout, Clip::new(source, 0, imported.frames, 0))?;
        if imported.truncated {
            eprintln!(
                "{} was shorter than its header said; imported what was there",
                file.display()
            );
        }
    }
    bundle.save(session).context("saving the project")?;
    Ok(())
}

/// The waveform cache file for stored audio file `name`.
pub fn waveform_cache(bundle: &Bundle, name: &str) -> std::path::PathBuf {
    bundle.cache_dir().join(format!("{name}.waveform"))
}

/// Builds the waveform summary of every stored file the project uses that has no valid one yet
/// (`docs/architecture/analysis.md` "Analysis cache files").
///
/// # Errors
///
/// If a stored file cannot be read or a cache file written.
pub fn build_waveform_caches(bundle: &Bundle, session: &Session) -> Result<()> {
    let names: Vec<&str> = session.referenced_files().into_iter().collect();
    build_waveform_caches_for(bundle, &names)
}

/// Builds the waveform summary of each stored file in `names` that has no valid one yet.
///
/// # Errors
///
/// If a stored file cannot be read or a cache file written.
pub fn build_waveform_caches_for(bundle: &Bundle, names: &[&str]) -> Result<()> {
    std::fs::create_dir_all(bundle.cache_dir())?;
    for &name in names {
        let cache = waveform_cache(bundle, name);
        let valid = File::open(&cache)
            .ok()
            .and_then(|f| audio_analysis::WaveformReader::new(BufReader::new(f)).ok())
            .is_some();
        if valid {
            continue;
        }
        let audio = bundle.audio_dir().join(name);
        let mut reader = WavReader::new(BufReader::new(
            File::open(&audio).with_context(|| format!("reading {}", audio.display()))?,
        ))
        .with_context(|| format!("reading {}", audio.display()))?;
        let mut writer =
            WaveformWriter::new(BufWriter::new(File::create(&cache)?), reader.frames())?;
        let mut buffer = vec![0.0f32; 65_536];
        loop {
            let n = reader.read(&mut [&mut buffer])?;
            if n == 0 {
                break;
            }
            writer.push(&buffer[..n])?;
        }
        writer.finish()?;
    }
    Ok(())
}

/// The frame just after the last clip ends.
fn project_end(state: &State) -> u64 {
    state
        .tracks
        .iter()
        .flat_map(|t| t.clips.iter().map(|(_, c)| c.end()))
        .max()
        .unwrap_or(0)
}

/// The format the command line's `--export` path names by its extension: `.wav` (24-bit),
/// `.flac` (24-bit), `.m4a` (AAC, 256 kbit/s) or `.mp3` (at `mp3`, default 320 kbit/s).
///
/// # Errors
///
/// For an unknown extension.
pub fn format_for_path(path: &Path, mp3: Option<Mp3Bitrate>) -> Result<ExportFormat> {
    let extension = path
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    Ok(match extension.as_str() {
        "wav" => ExportFormat::Wav(WavDepth::Int24),
        "flac" => ExportFormat::Flac(FlacDepth::Int24),
        "m4a" | "aac" => ExportFormat::Aac { bitrate_kbps: 256 },
        "mp3" => ExportFormat::Mp3 {
            bitrate: mp3.unwrap_or(Mp3Bitrate::Kbps320),
        },
        other => bail!("cannot export to .{other}; use .wav, .flac, .m4a or .mp3"),
    })
}

/// Renders the whole of `state` to `path` as `format`, the mono mix if `mono`; MP3 through
/// `lame`.
///
/// # Errors
///
/// For MP3 without `lame`, or if rendering or writing fails.
pub fn export_state(
    bundle: &Bundle,
    state: &State,
    path: &Path,
    format: ExportFormat,
    mono: bool,
    lame: Option<&Lame>,
) -> Result<()> {
    let rate = bundle.sample_rate();
    let mut graph = render_graph(state, Config::new(rate, MAX_BLOCK))?;
    let (renderers, files) = track_renderers(state, &graph.sources, rate)?;
    let paths: Vec<_> = files
        .names()
        .iter()
        .map(|n| bundle.audio_dir().join(n))
        .collect();
    let mut tracks: Vec<TrackFeeds> = renderers
        .into_iter()
        .map(|track| TrackFeeds {
            source: track.source,
            feeds: track
                .channels
                .into_iter()
                .map(|r| {
                    Box::new(RenderFeed::new(r, WavClipAudio::new(paths.clone()), 0))
                        as Box<dyn TrackFeed>
                })
                .collect(),
        })
        .collect();
    export(
        OfflineRender {
            runner: &mut graph.runner,
            output: graph.output,
            tracks: &mut tracks,
            start: 0,
            frames: project_end(state),
            block_frames: MAX_BLOCK,
        },
        format,
        if mono {
            ExportChannels::Mono
        } else {
            ExportChannels::Stereo
        },
        rate,
        path,
        &MacCodecs::new(),
        lame,
    )
    .with_context(|| format!("exporting {}", path.display()))?;
    Ok(())
}
