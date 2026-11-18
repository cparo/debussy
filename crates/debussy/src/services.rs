//! [`AppServices`]: `debussy_ui`'s I/O, from the bundle, disk streaming, the waveform caches,
//! file dialogs, the audio device, the take recorder, the MIDI output sender and the app settings
//! (ADR-0056, ADR-0059, ADR-0078, ADR-0162).

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use audio_analysis::{BINS, Bucket, FRAME_SIZE, Spectrogram, WaveformReader};
use audio_buffers::Layout;
use audio_engine::{NoteSink, Processor, TakeInfo, TakeSink};
use audio_engine::{Streams, TrackFeed, TrackFeeds};
use debussy_ui::{
    ClickPreferences, DeviceChoice, ExportKind, ExportSettings, ImportedTrack, MidiInputChoice,
    MidiOutRoute, MidiOutputChoice, MidiTakeFile, PassMarker, Recovered, RecoveredMidi,
    RecoveredPass, SampleImport, Services, Spectrum, TakeFiles, TakeStatus, Takes, spectrum_bands,
};
use disk_streaming::{
    FrameSource, Recorder, RecorderConfig, Streamer, StreamerConfig, TakeEvent, TimelineSource,
    WavClipAudio,
};
use engine_bridge::{ClickSound, track_renderers};
use import_export::{Lame, import, import_sample, safe_stem, unique_names};
use macos_platform::MacCodecs;
use midi::{LIVE_QUEUE_CAPACITY, LiveInput, OutProducer};
use platform_interfaces::{MidiDevices, MidiDirection, MidiEndpointId, MidiInput};
use project_model::{MAX_ZONES, Session, Source, State, TrackId};
use project_storage::{
    ARCHIVE_SUFFIX, AppSettings, Bundle, ClickSound as SavedClickSound, MAX_MIDI_LATENCY_MS,
    MAX_MIDI_OUTPUTS, MidiOutSettings, Recording, RecordingStart, Recovery, write_archive,
};
use signal_graph::NodeId;
use signal_graph::NodeId as GraphNodeId;
use wav_files::WavReader;

use crate::audio::AudioOutput;
use crate::export_settings;
use crate::midi_out::{self, MidiOuts};
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

/// Every file under `dir`, in it and its subfolders, whose extension is one of
/// [`IMPORT_TYPES`], added to `out`. Hidden files and folders are left out, and symbolic links are
/// not followed, so a link cannot loop the walk.
fn audio_files_in(dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let path = entry.path();
        let hidden = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden {
            continue;
        }
        if kind.is_dir() {
            audio_files_in(&path, out)?;
        } else if kind.is_file()
            && path.extension().is_some_and(|e| {
                IMPORT_TYPES
                    .iter()
                    .any(|t| e.to_string_lossy().eq_ignore_ascii_case(t))
            })
        {
            out.push(path);
        }
    }
    Ok(())
}

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
    /// The pass in the crash marker, while there is one.
    marker: Option<Recording>,
    /// The host's LAME, found at startup; MP3 export is offered only with it (ADR-0129).
    lame: Option<Lame>,
    /// The MIDI endpoints, if the platform's MIDI service could be reached.
    midi: Option<Arc<dyn MidiDevices>>,
    /// The MIDI output sender, started with the first route.
    midi_outs: Option<MidiOuts>,
    /// Whether the MIDI endpoints have changed since the interface last asked.
    midi_changed: Arc<AtomicBool>,
    /// Each armed MIDI track's open input.
    midi_open: BTreeMap<TrackId, Box<dyn MidiInput>>,
}

/// A spectrogram calculator with a frame's samples and levels.
struct SpectrogramScratch {
    spectrogram: Spectrogram,
    samples: Vec<f32>,
    levels: Vec<f32>,
}

impl AppServices {
    /// Services over the open `bundle`, playing through `audio`, with the app settings at
    /// `settings_path`, and MIDI input from `midi` if given.
    pub fn new(
        bundle: Bundle,
        audio: AudioOutput,
        settings: AppSettings,
        settings_path: PathBuf,
        lame: Option<Lame>,
        midi: Option<Box<dyn MidiDevices>>,
    ) -> Self {
        let midi: Option<Arc<dyn MidiDevices>> = midi.map(Arc::from);
        let midi_changed = Arc::new(AtomicBool::new(true));
        if let Some(midi) = &midi {
            let changed = Arc::clone(&midi_changed);
            midi.on_change(Box::new(move || changed.store(true, Ordering::Release)));
        }
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
            marker: None,
            lame,
            midi,
            midi_outs: None,
            midi_changed,
            midi_open: BTreeMap::new(),
        }
    }

    /// Gives the MIDI output sender the device's output delay and each port's latency from the
    /// settings (`docs/architecture/engine.md` "MIDI output").
    fn push_midi_offsets(&self) {
        let Some(outs) = &self.midi_outs else {
            return;
        };
        let rate = u64::from(self.audio.sample_rate().max(1));
        let output_nanos = self.audio.midi_latency().saturating_mul(1_000_000_000) / rate;
        let ports = self
            .settings
            .midi_outputs
            .iter()
            .map(|(port, s)| (port.clone(), midi_out::latency_nanos(s.latency_ms)))
            .collect();
        outs.set_offsets(output_nanos, ports);
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

/// `count` names for new MIDI take journals in `dir`, `take-<n>.midilog` with the smallest `n`
/// from 1 not in use.
fn journal_names(dir: &std::path::Path, count: usize) -> Vec<String> {
    (1u64..)
        .map(|n| format!("take-{n}.midilog"))
        .filter(|name| !dir.join(name).exists())
        .take(count)
        .collect()
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

    fn archive(&mut self, session: &Session) -> Result<Option<String>, String> {
        let stem = self.bundle.path().file_stem().map_or_else(
            || "Untitled".to_owned(),
            |s| s.to_string_lossy().into_owned(),
        );
        let Some(path) = rfd::FileDialog::new()
            .set_title("Share/Archive project")
            .set_file_name(format!("{stem}{ARCHIVE_SUFFIX}"))
            .add_filter("Zip archive", &["zip"])
            .save_file()
        else {
            return Ok(None);
        };
        self.bundle.save(session).map_err(|e| e.to_string())?;
        write_archive(self.bundle.path(), &path).map_err(|e| format!("Archive failed: {e}"))?;
        Ok(Some(path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        )))
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

    fn import_samples(&mut self, kit: bool) -> Result<SampleImport, String> {
        let mut found: Vec<(PathBuf, String)> = if kit {
            let Some(folder) = rfd::FileDialog::new().set_title("Add kit").pick_folder() else {
                return Ok(SampleImport::default());
            };
            let mut files = Vec::new();
            audio_files_in(&folder, &mut files)
                .map_err(|e| format!("{}: {e}", folder.display()))?;
            files.sort();
            files
                .into_iter()
                .map(|path| {
                    let name = path
                        .strip_prefix(&folder)
                        .unwrap_or(&path)
                        .with_extension("")
                        .to_string_lossy()
                        .into_owned();
                    (path, name)
                })
                .collect()
        } else {
            let Some(path) = rfd::FileDialog::new()
                .set_title("Add sample")
                .add_filter("Audio", &IMPORT_TYPES)
                .pick_file()
            else {
                return Ok(SampleImport::default());
            };
            let name = path
                .file_stem()
                .map_or_else(|| "Sample".to_owned(), |s| s.to_string_lossy().into_owned());
            vec![(path, name)]
        };
        if kit && found.is_empty() {
            return Err(format!(
                "No audio files ({}) were found in the folder or its subfolders.",
                IMPORT_TYPES.join(", ")
            ));
        }
        if found.len() > MAX_ZONES {
            return Err(format!(
                "The folder and its subfolders hold {} audio files, but a sampler holds at most \
                 {MAX_ZONES}. Choose a folder with one kit in it, such as a single kit from a \
                 larger pack.",
                found.len()
            ));
        }
        let mut result = SampleImport::default();
        for (path, name) in found.drain(..) {
            let stored = import_sample(
                &path,
                &self.bundle.audio_dir(),
                self.bundle.sample_rate(),
                &MacCodecs::new(),
            );
            let stored = match stored {
                Ok(stored) if stored.frames > 0 => stored,
                Ok(_) => {
                    result.skipped.push(format!("{name}: the file is empty"));
                    continue;
                }
                Err(error) => {
                    result.skipped.push(format!("{name}: {error}"));
                    continue;
                }
            };
            let (layout, source) = match stored.files.as_slice() {
                [mono] => (Layout::Mono, Source::Mono(mono.clone())),
                [left, right] => (
                    Layout::Stereo,
                    Source::Stereo {
                        left: left.clone(),
                        right: right.clone(),
                    },
                ),
                _ => {
                    result.skipped.push(format!("{name}: imported oddly"));
                    continue;
                }
            };
            result.imported.push(ImportedTrack {
                name,
                layout,
                source,
                frames: stored.frames,
            });
        }
        Ok(result)
    }

    fn export_settings(&mut self) -> ExportSettings {
        export_settings::from_saved(self.settings.export)
    }

    fn mp3_available(&mut self) -> bool {
        self.lame.is_some()
    }

    fn pick_export_path(&mut self, settings: &ExportSettings) -> Option<PathBuf> {
        let kind = settings.format;
        let filter = match kind {
            ExportKind::Wav => "WAV",
            ExportKind::Flac => "FLAC",
            ExportKind::Aac => "AAC",
            ExportKind::Mp3 => "MP3",
        };
        rfd::FileDialog::new()
            .set_title(if settings.mono {
                "Export mono mix"
            } else {
                "Export mix"
            })
            .set_file_name(format!("Mix.{}", kind.extension()))
            .add_filter(filter, &[kind.extension()])
            .save_file()
    }

    fn export(
        &mut self,
        state: &State,
        path: &Path,
        settings: &ExportSettings,
    ) -> Result<String, String> {
        let format = export_settings::to_format(settings);
        export_state(
            &self.bundle,
            state,
            path,
            format,
            settings.mono,
            self.lame.as_ref(),
        )
        .map_err(|e| format!("{e:#}"))?;
        let name = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        self.settings.export = export_settings::to_saved(settings);
        Ok(match self.settings.save(&self.settings_path) {
            Ok(()) => name,
            Err(e) => format!("{name} (the export settings could not be remembered: {e})"),
        })
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
        let result = self.audio.set_input(input).map_err(|e| format!("{e:#}"));
        self.push_midi_offsets();
        result
    }

    fn input_channels(&mut self) -> usize {
        usize::from(self.audio.input_channels())
    }

    fn device_latency(&mut self) -> u64 {
        self.audio.latency()
    }

    fn midi_latency(&mut self) -> u64 {
        self.audio.midi_latency()
    }

    fn midi_inputs(&mut self) -> Vec<MidiInputChoice> {
        let Some(midi) = &self.midi else {
            return Vec::new();
        };
        match midi.endpoints() {
            Ok(endpoints) => endpoints
                .into_iter()
                .filter(|e| e.direction == MidiDirection::Input && e.online)
                .map(|e| MidiInputChoice {
                    id: e.id.0,
                    name: e.name,
                })
                .collect(),
            Err(error) => {
                eprintln!("debussy: listing MIDI inputs: {error}");
                Vec::new()
            }
        }
    }

    fn midi_inputs_changed(&mut self) -> bool {
        self.midi_changed.swap(false, Ordering::AcqRel)
    }

    fn open_midi_input(&mut self, track: TrackId, id: &str) -> Result<LiveInput, String> {
        self.midi_open.remove(&track);
        let midi = self.midi.as_ref().ok_or("MIDI is not available")?;
        let (producer, input) = midi::channel(LIVE_QUEUE_CAPACITY).map_err(|e| e.to_string())?;
        let opened = midi
            .open_input(&MidiEndpointId(id.to_owned()), Box::new(producer))
            .map_err(|e| e.to_string())?;
        self.midi_open.insert(track, opened);
        Ok(input)
    }

    fn close_midi_input(&mut self, track: TrackId) {
        self.midi_open.remove(&track);
    }

    fn midi_outputs(&mut self) -> Vec<MidiOutputChoice> {
        let Some(midi) = &self.midi else {
            return Vec::new();
        };
        match midi.endpoints() {
            Ok(endpoints) => endpoints
                .into_iter()
                .filter(|e| e.direction == MidiDirection::Output && e.online)
                .map(|e| MidiOutputChoice {
                    id: e.id.0,
                    name: e.name,
                })
                .collect(),
            Err(error) => {
                eprintln!("debussy: listing MIDI outputs: {error}");
                Vec::new()
            }
        }
    }

    fn sync_midi_outputs(&mut self, routes: &[MidiOutRoute]) -> Vec<(GraphNodeId, OutProducer)> {
        if self.midi_outs.is_none() {
            if routes.is_empty() {
                return Vec::new();
            }
            let Some(midi) = &self.midi else {
                return Vec::new();
            };
            self.midi_outs = Some(MidiOuts::start(Arc::clone(midi)));
        }
        self.push_midi_offsets();
        self.midi_outs
            .as_mut()
            .map(|outs| outs.sync(routes))
            .unwrap_or_default()
    }

    fn midi_output_lost(&mut self, node: project_model::NodeId) -> bool {
        self.midi_outs.as_ref().is_some_and(|outs| outs.lost(node))
    }

    fn midi_output_latency(&mut self, port: &str) -> f32 {
        self.settings
            .midi_outputs
            .get(port)
            .map_or(0.0, |s| s.latency_ms)
    }

    fn set_midi_output_latency(&mut self, port: &str, ms: f32) -> Result<(), String> {
        if !(ms.is_finite() && (0.0..=MAX_MIDI_LATENCY_MS).contains(&ms)) {
            return Err(format!(
                "{ms} ms is not a latency from 0 to {MAX_MIDI_LATENCY_MS} ms"
            ));
        }
        if self.settings.midi_outputs.len() >= MAX_MIDI_OUTPUTS
            && !self.settings.midi_outputs.contains_key(port)
        {
            return Err(format!(
                "the latencies of at most {MAX_MIDI_OUTPUTS} MIDI outputs are remembered"
            ));
        }
        self.settings
            .midi_outputs
            .insert(port.to_owned(), MidiOutSettings { latency_ms: ms });
        self.push_midi_offsets();
        self.settings
            .save(&self.settings_path)
            .map_err(|e| format!("the MIDI output latency could not be remembered: {e}"))
    }

    fn start_takes(&mut self, tracks: &[(String, usize)], midi: usize) -> Result<Takes, String> {
        let dir = self.bundle.audio_dir();
        let names = take_names(&dir, tracks);
        let paths: Vec<PathBuf> = names.iter().flatten().map(|n| dir.join(n)).collect();
        let journals = journal_names(&dir, midi);
        let journal_paths: Vec<PathBuf> = journals.iter().map(|n| dir.join(n)).collect();
        // A pass still held is finished first, keeping its files on disk.
        self.recorder = None;
        let config = RecorderConfig::new(self.bundle.sample_rate());
        let (recorder, takes, midi_takes) =
            Recorder::start_with_midi(&paths, &journal_paths, config).map_err(|e| e.to_string())?;
        self.recorder = Some(recorder);
        let mut takes = takes.into_iter();
        let audio = names
            .into_iter()
            .map(|files| TakeFiles {
                sinks: takes
                    .by_ref()
                    .take(files.len())
                    .map(|t| Box::new(t) as Box<dyn TakeSink>)
                    .collect(),
                files,
            })
            .collect();
        let midi = journals
            .into_iter()
            .zip(midi_takes)
            .map(|(file, t)| MidiTakeFile {
                file,
                sink: Box::new(t) as Box<dyn NoteSink>,
            })
            .collect();
        Ok(Takes { audio, midi })
    }

    fn take_messages(&mut self, take: usize) -> Vec<(u64, [u8; 3])> {
        self.recorder
            .as_mut()
            .and_then(|r| r.take_messages(take))
            .unwrap_or_default()
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

    fn mark_pass(&mut self, marker: &PassMarker) -> Result<(), String> {
        let recording = Recording {
            device_latency: marker.device_latency,
            midi_latency: marker.midi_latency,
            start: None,
            tracks: marker.tracks.clone(),
        };
        let result = self.bundle.mark_recording(&recording);
        self.marker = Some(recording);
        result.map_err(|e| e.to_string())
    }

    fn mark_pass_started(&mut self, info: TakeInfo) -> Result<(), String> {
        let Some(recording) = &mut self.marker else {
            return Ok(());
        };
        recording.start = Some(RecordingStart {
            start_frame: info.start_frame,
            plan_latency: info.latency,
        });
        self.bundle
            .mark_recording(recording)
            .map_err(|e| e.to_string())
    }

    fn clear_pass_marker(&mut self) {
        self.marker = None;
        if let Err(error) = self.bundle.clear_recording() {
            eprintln!("debussy: {error}");
        }
    }

    fn take_recovered(&mut self) -> Option<Recovered> {
        match self.bundle.take_recovery()? {
            Recovery::Takes(pass) => Some(Recovered::Takes(RecoveredPass {
                tracks: pass.tracks,
                frames: pass.frames,
                midi: pass
                    .midi
                    .into_iter()
                    .map(|m| RecoveredMidi {
                        track: m.track,
                        file: m.file,
                        messages: m.messages,
                        frames: m.frames,
                    })
                    .collect(),
                info: TakeInfo {
                    start_frame: pass.start.start_frame,
                    latency: pass.start.plan_latency,
                },
                device_latency: pass.device_latency,
                midi_latency: pass.midi_latency,
            })),
            Recovery::Discarded { reason } => {
                eprintln!("debussy: a take interrupted by a crash was discarded: {reason}");
                Some(Recovered::Discarded)
            }
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
        self.push_midi_offsets();
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
        self.push_midi_offsets();
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

#[cfg(test)]
mod tests {
    use std::error::Error;

    use super::*;

    #[test]
    fn test_audio_files_in_walks_subfolders_and_skips_hidden_and_other_files()
    -> Result<(), Box<dyn Error>> {
        let dir = std::env::temp_dir().join(format!("debussy-kit-{}", std::process::id()));
        if dir.exists() {
            std::fs::remove_dir_all(&dir)?;
        }
        std::fs::create_dir_all(dir.join("Hats"))?;
        std::fs::create_dir_all(dir.join(".cache"))?;
        for name in [
            "Kick.wav",
            "Hats/Open.WAV",
            "Hats/notes.txt",
            ".hidden.wav",
            ".cache/x.wav",
        ] {
            std::fs::write(dir.join(name), b"")?;
        }
        std::os::unix::fs::symlink(&dir, dir.join("Hats/loop"))?;
        let mut found = Vec::new();
        audio_files_in(&dir, &mut found)?;
        found.sort();
        assert_eq!(found, [dir.join("Hats/Open.WAV"), dir.join("Kick.wav")]);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
