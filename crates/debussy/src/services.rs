//! [`AppServices`]: `debussy_ui`'s I/O, from the bundle, disk streaming, the waveform caches,
//! file dialogs, the audio device and the app settings (ADR-0056, ADR-0059).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use audio_analysis::{BINS, Bucket, FRAME_SIZE, Spectrogram, WaveformReader};
use audio_buffers::Layout;
use audio_engine::{Streams, TrackFeed, TrackFeeds};
use debussy_ui::{DeviceChoice, ImportedTrack, Services, Spectrum, spectrum_bands};
use disk_streaming::{FrameSource, Streamer, StreamerConfig, TimelineSource, WavClipAudio};
use engine_bridge::track_renderers;
use import_export::import;
use macos_platform::MacCodecs;
use project_model::{Session, Source, State, TrackId};
use project_storage::{AppSettings, Bundle};
use signal_graph::NodeId;
use wav_files::WavReader;

use crate::audio::AudioOutput;
use crate::project::{build_waveform_caches_for, export_state, waveform_cache};

/// Streams before any project audio is loaded: nothing to move.
struct NoStreams;

impl Streams for NoStreams {
    fn seek(&mut self, _frame: u64) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        Ok(())
    }
}

/// The file types offered by the import dialog.
const IMPORT_TYPES: [&str; 6] = ["wav", "rf64", "flac", "m4a", "aac", "mp3"];

/// The application's services.
pub struct AppServices {
    bundle: Bundle,
    audio: AudioOutput,
    settings: AppSettings,
    settings_path: PathBuf,
    streamer: Option<Streamer>,
    no_streams: NoStreams,
    /// Open waveform summaries, by stored file name; `None` where there is none.
    waveforms: BTreeMap<String, Option<WaveformReader<BufReader<File>>>>,
    /// Open stored audio files for spectrograms, by name; `None` where one cannot be read.
    audio_files: BTreeMap<String, Option<WavReader<BufReader<File>>>>,
    /// The spectrogram calculator and its buffers, made on first use.
    spectrogram: Option<SpectrogramScratch>,
}

/// A spectrogram calculator with a frame's samples and levels.
struct SpectrogramScratch {
    spectrogram: Spectrogram,
    samples: Vec<f32>,
    levels: Vec<f32>,
}

impl AppServices {
    /// Services over the open `bundle`, playing through `audio`, with the app settings at
    /// `settings_path`.
    pub fn new(
        bundle: Bundle,
        audio: AudioOutput,
        settings: AppSettings,
        settings_path: PathBuf,
    ) -> Self {
        AppServices {
            bundle,
            audio,
            settings,
            settings_path,
            streamer: None,
            no_streams: NoStreams,
            waveforms: BTreeMap::new(),
            audio_files: BTreeMap::new(),
            spectrogram: None,
        }
    }
}

impl Services for AppServices {
    fn streams(&mut self) -> &mut dyn Streams {
        match &mut self.streamer {
            Some(streamer) => streamer,
            None => &mut self.no_streams,
        }
    }

    fn track_feeds(
        &mut self,
        state: &State,
        sources: &BTreeMap<TrackId, NodeId>,
    ) -> Result<Vec<TrackFeeds>, String> {
        let rate = self.bundle.sample_rate();
        let (tracks, files) = track_renderers(state, sources, rate).map_err(|e| e.to_string())?;
        let paths: Vec<_> = files
            .names()
            .iter()
            .map(|n| self.bundle.audio_dir().join(n))
            .collect();
        // One stream per track channel, in order; remember which track each belongs to.
        let mut layout = Vec::with_capacity(tracks.len());
        let mut streams: Vec<Box<dyn FrameSource>> = Vec::new();
        for track in tracks {
            layout.push((track.source, track.channels.len()));
            for renderer in track.channels {
                streams.push(Box::new(TimelineSource::new(
                    renderer,
                    WavClipAudio::new(paths.clone()),
                    rate,
                )));
            }
        }
        // The old streamer stops here; the engine is stopped, so its feeds are not being read.
        self.streamer = None;
        let (streamer, playbacks) =
            Streamer::start(streams, StreamerConfig::new(rate)).map_err(|e| e.to_string())?;
        self.streamer = Some(streamer);
        let mut playbacks = playbacks.into_iter();
        Ok(layout
            .into_iter()
            .map(|(source, channels)| TrackFeeds {
                source,
                feeds: playbacks
                    .by_ref()
                    .take(channels)
                    .map(|p| Box::new(p) as Box<dyn TrackFeed>)
                    .collect(),
            })
            .collect())
    }

    fn waveform(&mut self, file: &str, start: u64, spc: u64, out: &mut [Bucket]) -> usize {
        let bundle = &self.bundle;
        let reader = self.waveforms.entry(file.to_owned()).or_insert_with(|| {
            File::open(waveform_cache(bundle, file))
                .ok()
                .and_then(|f| WaveformReader::new(BufReader::new(f)).ok())
        });
        match reader {
            Some(reader) => reader.columns(start, spc, out).unwrap_or(0),
            None => 0,
        }
    }

    fn spectrogram(&mut self, file: &str, start: u64, spc: u64, out: &mut [Spectrum]) -> usize {
        let bundle = &self.bundle;
        let Some(reader) = self
            .audio_files
            .entry(file.to_owned())
            .or_insert_with(|| {
                File::open(bundle.audio_dir().join(file))
                    .ok()
                    .and_then(|f| WavReader::new(BufReader::new(f)).ok())
            })
            .as_mut()
        else {
            return 0;
        };
        if self.spectrogram.is_none() {
            self.spectrogram = Spectrogram::new()
                .ok()
                .map(|spectrogram| SpectrogramScratch {
                    spectrogram,
                    samples: vec![0.0; FRAME_SIZE],
                    levels: vec![0.0; BINS],
                });
        }
        let Some(scratch) = &mut self.spectrogram else {
            return 0;
        };
        // One frame from the start of each column; stored files are mono.
        let mut filled = 0;
        for (column, spectrum) in out.iter_mut().enumerate() {
            let at = start.saturating_add(column as u64 * spc);
            if at >= reader.frames() || reader.seek(at).is_err() {
                break;
            }
            let Ok(read) = reader.read(&mut [&mut scratch.samples[..]]) else {
                break;
            };
            if scratch
                .spectrogram
                .frame_db(&scratch.samples[..read], &mut scratch.levels)
                .is_err()
            {
                break;
            }
            spectrum_bands(&scratch.levels, spectrum);
            filled += 1;
        }
        filled
    }

    fn save(&mut self, session: &Session) -> Result<(), String> {
        self.bundle.save(session).map_err(|e| e.to_string())
    }

    fn import(&mut self) -> Result<Vec<ImportedTrack>, String> {
        let Some(paths) = rfd::FileDialog::new()
            .set_title("Import audio")
            .add_filter("Audio", &IMPORT_TYPES)
            .pick_files()
        else {
            return Ok(Vec::new());
        };
        let mut imported = Vec::with_capacity(paths.len());
        for path in paths {
            let result = import(
                &path,
                &self.bundle.audio_dir(),
                self.bundle.sample_rate(),
                &MacCodecs::new(),
            )
            .map_err(|e| e.to_string())?;
            if result.frames == 0 {
                continue;
            }
            let name = path.file_stem().map_or_else(
                || "Imported".to_owned(),
                |s| s.to_string_lossy().into_owned(),
            );
            let (layout, source) = match result.files.as_slice() {
                [mono] => (Layout::Mono, Source::Mono(mono.clone())),
                [left, right] => (
                    Layout::Stereo,
                    Source::Stereo {
                        left: left.clone(),
                        right: right.clone(),
                    },
                ),
                _ => return Err(format!("{} imported oddly", path.display())),
            };
            let names: Vec<&str> = result.files.iter().map(|f| f.name.as_str()).collect();
            build_waveform_caches_for(&self.bundle, &names).map_err(|e| e.to_string())?;
            imported.push(ImportedTrack {
                name,
                layout,
                source,
                frames: result.frames,
            });
        }
        Ok(imported)
    }

    fn export(&mut self, state: &State, mono: bool) -> Result<Option<String>, String> {
        let Some(path) = rfd::FileDialog::new()
            .set_title(if mono {
                "Export mono mix"
            } else {
                "Export mix"
            })
            .set_file_name("Mix.wav")
            .add_filter("WAV (24-bit)", &["wav"])
            .add_filter("FLAC (24-bit)", &["flac"])
            .add_filter("AAC", &["m4a"])
            .save_file()
        else {
            return Ok(None);
        };
        export_state(&self.bundle, state, &path, mono).map_err(|e| format!("{e:#}"))?;
        Ok(Some(path.file_name().map_or_else(String::new, |n| {
            n.to_string_lossy().into_owned()
        })))
    }

    fn devices(&mut self) -> (Vec<DeviceChoice>, Option<String>) {
        let choices = self
            .audio
            .devices()
            .into_iter()
            .filter(|d| d.output_channels > 0)
            .map(|d| DeviceChoice {
                id: d.id.0,
                name: d.name,
            })
            .collect();
        (choices, self.audio.current().map(str::to_owned))
    }

    fn audio_error(&mut self) -> Option<String> {
        self.audio.take_error()
    }

    fn set_device(&mut self, id: &str) -> Result<(), String> {
        self.audio.switch(id).map_err(|e| format!("{e:#}"))?;
        self.settings.device = Some(id.to_owned());
        self.settings
            .save(&self.settings_path)
            .map_err(|e| format!("the device is in use, but could not be remembered: {e}"))
    }
}
