//! [`AppServices`]: `debussy_ui`'s I/O, from the bundle, disk streaming, the waveform caches,
//! file dialogs, the audio device, the take recorder and the app settings (ADR-0056, ADR-0059,
//! ADR-0078).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use audio_analysis::{BINS, Bucket, FRAME_SIZE, Spectrogram, WaveformReader};
use audio_buffers::Layout;
use audio_engine::{Processor, TakeSink};
use audio_engine::{Streams, TrackFeed, TrackFeeds};
use debussy_ui::{
    ClickPreferences, DeviceChoice, ImportedTrack, Services, Spectrum, TakeFiles, TakeStatus,
    spectrum_bands,
};
use disk_streaming::{
    FrameSource, Recorder, RecorderConfig, Streamer, StreamerConfig, TakeEvent, TimelineSource,
    WavClipAudio,
};
use engine_bridge::{ClickSound, track_renderers};
use import_export::{Lame, Mp3Bitrate, import, safe_stem, unique_names};
use macos_platform::MacCodecs;
use project_model::{Session, Source, State, TrackId};
use project_storage::{AppSettings, Bundle, ClickSound as SavedClickSound};
use signal_graph::NodeId;
use wav_files::WavReader;

use crate::audio::AudioOutput;
use crate::project::{build_waveform_caches_for, export_state, waveform_cache};

/// The bundled sound a remembered click sound names.
fn click_sound(saved: SavedClickSound) -> ClickSound {
    match saved {
        SavedClickSound::Block => ClickSound::Block,
        SavedClickSound::Click => ClickSound::Click,
        SavedClickSound::Stick => ClickSound::Stick,
    }
}

/// How a click sound is remembered.
fn saved_click_sound(sound: ClickSound) -> SavedClickSound {
    match sound {
        ClickSound::Block => SavedClickSound::Block,
        ClickSound::Click => SavedClickSound::Click,
        ClickSound::Stick => SavedClickSound::Stick,
    }
}

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
    /// Open stored audio files for spectrograms and the finest zooms, by name; `None` where one
    /// cannot be read.
    audio_files: BTreeMap<String, Option<WavReader<BufReader<File>>>>,
    /// The spectrogram calculator and its buffers, made on first use.
    spectrogram: Option<SpectrogramScratch>,
    /// The recording pass's writer, until its takes are kept or discarded.
    recorder: Option<Recorder>,
    /// The host's LAME, found at startup; MP3 export is offered only with it (ADR-0129).
    lame: Option<Lame>,
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
        lame: Option<Lame>,
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
            recorder: None,
            lame,
        }
    }
}

/// New file names in `dir` for each of `tracks`' takes: `<track name> <n>[ L| R].wav`, none
/// existing and none repeated across the tracks.
fn take_names(dir: &std::path::Path, tracks: &[(String, usize)]) -> Vec<Vec<String>> {
    let mut chosen: Vec<Vec<String>> = Vec::with_capacity(tracks.len());
    for (name, channels) in tracks {
        let stem = safe_stem(name);
        let names = (1u64..)
            .map(|k| {
                let stem = if k == 1 {
                    stem.clone()
                } else {
                    format!("{stem}-{k}")
                };
                unique_names(dir, &stem, *channels)
            })
            .find(|names| {
                !names
                    .iter()
                    .any(|n| chosen.iter().flatten().any(|c| c == n))
            })
            .unwrap_or_default();
        chosen.push(names);
    }
    chosen
}

/// Stored audio file `file` from `bundle`, opened on first use and kept in `audio_files`; `None`
/// if it cannot be read.
fn audio_file<'a>(
    audio_files: &'a mut BTreeMap<String, Option<WavReader<BufReader<File>>>>,
    bundle: &Bundle,
    file: &str,
) -> Option<&'a mut WavReader<BufReader<File>>> {
    audio_files
        .entry(file.to_owned())
        .or_insert_with(|| {
            File::open(bundle.audio_dir().join(file))
                .ok()
                .and_then(|f| WavReader::new(BufReader::new(f)).ok())
        })
        .as_mut()
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

    fn samples(&mut self, file: &str, start: u64, out: &mut [f32]) -> usize {
        // Stored files are mono.
        let Some(reader) = audio_file(&mut self.audio_files, &self.bundle, file) else {
            return 0;
        };
        if start >= reader.frames() || reader.seek(start).is_err() {
            return 0;
        }
        reader.read(&mut [out]).unwrap_or(0)
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
        let Some(reader) = audio_file(&mut self.audio_files, &self.bundle, file) else {
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
        // One frame centred on each column, zero outside the file; stored files are mono.
        let half = FRAME_SIZE as u64 / 2;
        let mut filled = 0;
        for (column, spectrum) in out.iter_mut().enumerate() {
            let at = start.saturating_add(column as u64 * spc);
            if at >= reader.frames() {
                break;
            }
            let centre = at.saturating_add(spc / 2);
            // Samples before frame 0 are silence.
            let skip = half.saturating_sub(centre) as usize;
            let from = centre.saturating_sub(half);
            scratch.samples[..skip].fill(0.0);
            let read = if from < reader.frames() {
                if reader.seek(from).is_err() {
                    break;
                }
                let Ok(read) = reader.read(&mut [&mut scratch.samples[skip..]]) else {
                    break;
                };
                read
            } else {
                0
            };
            scratch.samples[skip + read..].fill(0.0);
            if scratch
                .spectrogram
                .frame_db(&scratch.samples, &mut scratch.levels)
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

    fn pick_export_path(&mut self, mono: bool) -> Option<PathBuf> {
        let mut dialog = rfd::FileDialog::new()
            .set_title(if mono {
                "Export mono mix"
            } else {
                "Export mix"
            })
            .set_file_name("Mix.wav")
            .add_filter("WAV (24-bit)", &["wav"])
            .add_filter("FLAC (24-bit)", &["flac"])
            .add_filter("AAC", &["m4a"]);
        if self.lame.is_some() {
            dialog = dialog.add_filter("MP3", &["mp3"]);
        }
        dialog.save_file()
    }

    fn export(
        &mut self,
        state: &State,
        path: &Path,
        mono: bool,
        mp3_kbps: Option<u32>,
    ) -> Result<String, String> {
        let mp3 = mp3_kbps.and_then(Mp3Bitrate::from_kbps);
        export_state(&self.bundle, state, path, mono, mp3, self.lame.as_ref())
            .map_err(|e| format!("{e:#}"))?;
        Ok(path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()))
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

    fn set_input(&mut self, input: bool) -> Result<(), String> {
        self.audio.set_input(input).map_err(|e| format!("{e:#}"))
    }

    fn input_channels(&mut self) -> usize {
        usize::from(self.audio.input_channels())
    }

    fn device_latency(&mut self) -> u64 {
        self.audio.latency()
    }

    fn start_takes(&mut self, tracks: &[(String, usize)]) -> Result<Vec<TakeFiles>, String> {
        let dir = self.bundle.audio_dir();
        let names = take_names(&dir, tracks);
        let paths: Vec<PathBuf> = names.iter().flatten().map(|n| dir.join(n)).collect();
        // A pass still held is finished first, keeping its files on disk.
        self.recorder = None;
        let config = RecorderConfig::new(self.bundle.sample_rate());
        let (recorder, takes) = Recorder::start(&paths, config).map_err(|e| e.to_string())?;
        self.recorder = Some(recorder);
        let mut takes = takes.into_iter();
        Ok(names
            .into_iter()
            .map(|files| TakeFiles {
                sinks: takes
                    .by_ref()
                    .take(files.len())
                    .map(|t| Box::new(t) as Box<dyn TakeSink>)
                    .collect(),
                files,
            })
            .collect())
    }

    fn take_status(&mut self) -> Vec<TakeStatus> {
        let Some(recorder) = &mut self.recorder else {
            return Vec::new();
        };
        std::iter::from_fn(|| recorder.poll_event())
            .map(|event| match event {
                TakeEvent::Finished { take, frames } => TakeStatus::Finished { take, frames },
                TakeEvent::Failed { take, message } => TakeStatus::Failed { take, message },
            })
            .collect()
    }

    fn take_overruns(&mut self) -> u64 {
        self.recorder
            .as_ref()
            .map_or(0, |r| (0..r.takes()).filter_map(|t| r.overruns(t)).sum())
    }

    fn keep_takes(&mut self, files: &[String]) -> Result<(), String> {
        self.recorder = None;
        let names: Vec<&str> = files.iter().map(String::as_str).collect();
        build_waveform_caches_for(&self.bundle, &names).map_err(|e| format!("{e:#}"))
    }

    fn discard_takes(&mut self, files: &[String]) {
        self.recorder = None;
        let dir = self.bundle.audio_dir();
        for file in files {
            let _ = std::fs::remove_file(dir.join(file));
        }
    }

    fn audio_lost(&mut self) -> bool {
        self.audio.is_lost()
    }

    fn adopt_engine(&mut self, processor: Processor, device: Option<&str>) -> Result<(), String> {
        let preferred = device
            .or(self.settings.device.as_deref())
            .map(str::to_owned);
        self.audio
            .adopt(processor, preferred.as_deref())
            .map_err(|e| format!("{e:#}"))?;
        if let Some(id) = device {
            self.settings.device = Some(id.to_owned());
            self.settings
                .save(&self.settings_path)
                .map_err(|e| format!("the device is in use, but could not be remembered: {e}"))?;
        }
        Ok(())
    }

    fn set_device(&mut self, id: &str) -> Result<(), String> {
        self.audio.switch(id).map_err(|e| format!("{e:#}"))?;
        self.settings.device = Some(id.to_owned());
        self.settings
            .save(&self.settings_path)
            .map_err(|e| format!("the device is in use, but could not be remembered: {e}"))
    }

    fn click_preferences(&mut self) -> ClickPreferences {
        ClickPreferences {
            sound: click_sound(self.settings.click_sound),
            level_db: self.settings.click_level_db,
        }
    }

    fn set_click_preferences(&mut self, preferences: ClickPreferences) -> Result<(), String> {
        self.settings.click_sound = saved_click_sound(preferences.sound);
        self.settings.click_level_db = preferences.level_db;
        self.settings
            .save(&self.settings_path)
            .map_err(|e| format!("the click settings could not be remembered: {e}"))
    }
}
