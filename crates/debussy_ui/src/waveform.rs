//! Track lanes: one per channel, each the visible stretch summarised from the stored files —
//! a [`Bucket`] per column from the waveform summaries (`docs/architecture/analysis.md`
//! "Waveform summaries"), or a spectrum per coarser column computed from the audio
//! ("Spectrograms") — and the canvas that draws a lane, seeks on a click and pans on a
//! sideways scroll.

use std::cell::Cell;

use audio_analysis::Bucket;
use iced::widget::canvas::{self, Cache, Event, Frame, Geometry, Program};
use iced::{Point, Rectangle, Renderer, Size, Theme, mouse};
use project_model::{Clip, Source, Track};

use crate::app::Message;
use crate::services::Services;
use crate::theme::{AUDIO, CURSOR, mix, tokens, tone_of};

/// Waveform columns per lane; the canvas scales them to its width.
pub const LANE_COLUMNS: usize = 1024;

/// Spectrogram columns per lane: each spans `LANE_COLUMNS / SPECTRUM_COLUMNS` waveform columns.
pub const SPECTRUM_COLUMNS: usize = 256;

/// Frequency bands per spectrogram column, log-spaced from the lowest bin to the highest.
pub const SPECTRUM_BANDS: usize = 64;

/// The finest zoom: frames per waveform column. Below the summaries' smallest bucket, columns
/// are summarised from the audio itself.
pub const MIN_SAMPLES_PER_COLUMN: u64 = 2;

/// The level drawn as the lane's background colour, in dB.
const SPECTRUM_FLOOR_DB: f32 = -96.0;

/// One band's level in dB per spectrogram column.
pub type Spectrum = [f32; SPECTRUM_BANDS];

/// What a lane shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Display {
    /// Peak and RMS outlines.
    #[default]
    Waveform,
    /// Level by frequency over time.
    Spectrogram,
}

/// One channel's visible stretch, a column at a time; `None` where no clip plays.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lane {
    /// The waveform columns, left to right; empty in spectrogram display.
    pub columns: Vec<Option<Bucket>>,
    /// The spectrogram columns, left to right; empty in waveform display.
    pub spectrum: Vec<Option<Spectrum>>,
}

/// Reduces one spectrogram frame's bins (in dB, 0 Hz first) to [`SPECTRUM_BANDS`] log-spaced
/// bands, each the loudest bin it covers. Bin 0 (DC) is left out.
pub fn spectrum_bands(bins_db: &[f32], out: &mut Spectrum) {
    let top = bins_db.len().saturating_sub(1).max(1) as f32;
    let mut lo = 1usize;
    for (band, level) in out.iter_mut().enumerate() {
        let edge = top.powf((band + 1) as f32 / SPECTRUM_BANDS as f32).round() as usize;
        // Band `band` covers the bins after the previous band's, up to and including `edge`.
        let hi = (edge + 1).max(lo + 1).min(bins_db.len());
        *level = bins_db
            .get(lo..hi)
            .and_then(|b| b.iter().copied().reduce(f32::max))
            .unwrap_or(f32::NEG_INFINITY);
        lo = hi;
    }
}

/// The stored file behind channel `channel` of `source`.
fn channel_file(source: &Source, channel: usize) -> Option<&str> {
    match (source, channel) {
        (Source::Mono(file), 0) => Some(&file.name),
        (Source::Stereo { left, .. }, 0) => Some(&left.name),
        (Source::Stereo { right, .. }, 1) => Some(&right.name),
        _ => None,
    }
}

/// Paints `out` (columns of `spc` frames from timeline frame `start`, given as `(start, spc)`) with the clips' values:
/// bottom layer first, so the top clip shows where they overlap. `read` fills a slice from a
/// file at a frame and returns how many columns hold audio; `gain` applies a clip's gain.
fn paint<T: Copy>(
    clips: &[(project_model::ClipId, Clip)],
    channel: usize,
    (start, spc): (u64, u64),
    out: &mut [Option<T>],
    blank: T,
    mut read: impl FnMut(&str, u64, &mut [T]) -> usize,
    gain: impl Fn(T, f32) -> T,
) {
    let count = out.len();
    let end = start.saturating_add(spc.saturating_mul(count as u64));
    let mut scratch = vec![blank; count];
    for (_, clip) in clips {
        if clip.end() <= start || clip.position >= end {
            continue;
        }
        let Some(file) = channel_file(&clip.source, channel) else {
            continue;
        };
        // The column holding the clip's first visible frame, and that frame's place in the
        // file. Columns are read from there, so they may sit up to one column's width off the
        // timeline grid: close enough to draw.
        let visible_from = clip.position.max(start);
        let first = ((visible_from - start) / spc) as usize;
        let last = ((clip.end() - start).div_ceil(spc) as usize).min(count);
        if first >= last {
            continue;
        }
        let file_start = clip.offset + (visible_from - clip.position);
        let n = read(file, file_start, &mut scratch[..last - first]);
        for (column, value) in out[first..first + n].iter_mut().zip(&scratch[..n]) {
            *column = Some(gain(*value, clip.gain_db));
        }
    }
}

/// The lanes of `track`, one per channel (left above right), from timeline frame `start` at
/// `samples_per_column` frames per waveform column, showing `display`.
pub fn lanes(
    track: &Track,
    start: u64,
    samples_per_column: u64,
    display: Display,
    services: &mut dyn Services,
) -> Vec<Lane> {
    let spc = samples_per_column.max(MIN_SAMPLES_PER_COLUMN);
    let channels = match track.layout {
        audio_buffers::Layout::Mono => 1,
        audio_buffers::Layout::Stereo => 2,
    };
    (0..channels)
        .map(|channel| match display {
            Display::Waveform => {
                let mut columns = vec![None; LANE_COLUMNS];
                let blank = Bucket {
                    min: 0.0,
                    max: 0.0,
                    rms: 0.0,
                };
                paint(
                    &track.clips,
                    channel,
                    (start, spc),
                    &mut columns,
                    blank,
                    |file, from, out| services.waveform(file, from, spc, out),
                    |b, gain_db| {
                        let gain = 10f32.powf(gain_db / 20.0);
                        Bucket {
                            min: b.min * gain,
                            max: b.max * gain,
                            rms: b.rms * gain,
                        }
                    },
                );
                Lane {
                    columns,
                    spectrum: Vec::new(),
                }
            }
            Display::Spectrogram => {
                let spc = spc * (LANE_COLUMNS / SPECTRUM_COLUMNS) as u64;
                let mut spectrum = vec![None; SPECTRUM_COLUMNS];
                paint(
                    &track.clips,
                    channel,
                    (start, spc),
                    &mut spectrum,
                    [f32::NEG_INFINITY; SPECTRUM_BANDS],
                    |file, from, out| services.spectrogram(file, from, spc, out),
                    |mut s, gain_db| {
                        s.iter_mut().for_each(|level| *level += gain_db);
                        s
                    },
                );
                Lane {
                    columns: Vec::new(),
                    spectrum,
                }
            }
        })
        .collect()
}

/// The canvas state of a lane: its drawing, kept until the lanes change.
#[derive(Default)]
pub struct LaneState {
    cache: Cache,
    generation: Cell<Option<u64>>,
}

/// Draws a lane with the cursor over it; a click seeks there and a sideways scroll pans.
pub struct LaneView<'a> {
    /// The lane.
    pub lane: &'a Lane,
    /// Changes whenever any lane's contents or the tone change, to redraw.
    pub generation: u64,
    /// Where the cursor is, as a fraction of the lane's width, if visible.
    pub cursor: Option<f32>,
    /// The first timeline frame shown.
    pub start: u64,
    /// The frames shown across the lane's width.
    pub span: u64,
}

impl Program<Message> for LaneView<'_> {
    type State = LaneState;

    fn update(
        &self,
        _state: &mut LaneState,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        let at = cursor.position_in(bounds)?;
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let fraction = f64::from(at.x / bounds.width.max(1.0));
                let frame = self.start + (fraction * self.span as f64) as u64;
                Some(canvas::Action::publish(Message::Seek(frame)).and_capture())
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let (x, y) = match delta {
                    mouse::ScrollDelta::Pixels { x, y } => (*x, *y),
                    // A line is about 1/40 of the lane.
                    mouse::ScrollDelta::Lines { x, y } => {
                        (x * bounds.width / 40.0, y * bounds.width / 40.0)
                    }
                };
                // Mostly sideways: pan. Otherwise the pane scrolls.
                (x.abs() > y.abs()).then(|| {
                    canvas::Action::publish(Message::Pan(-x / bounds.width.max(1.0))).and_capture()
                })
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &LaneState,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        if state.generation.get() != Some(self.generation) {
            state.cache.clear();
            state.generation.set(Some(self.generation));
        }
        let background = tokens(tone_of(theme)).lane;
        let lane = state.cache.draw(renderer, bounds.size(), |frame| {
            frame.fill_rectangle(Point::ORIGIN, bounds.size(), background);
            if self.lane.spectrum.is_empty() {
                draw_waveform(frame, self.lane, bounds.size());
            } else {
                draw_spectrum(frame, self.lane, bounds.size(), background);
            }
        });
        let mut geometry = vec![lane];
        if let Some(at) = self.cursor {
            let mut frame = Frame::new(renderer, bounds.size());
            frame.fill_rectangle(
                Point::new(at * bounds.width - 0.75, 0.0),
                Size::new(1.5, bounds.height),
                CURSOR,
            );
            geometry.push(frame.into_geometry());
        }
        geometry
    }

    fn mouse_interaction(
        &self,
        _state: &LaneState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor.is_over(bounds) {
            mouse::Interaction::Crosshair
        } else {
            mouse::Interaction::default()
        }
    }
}

/// The peak outline in the audio colour, the RMS body darker, centred.
fn draw_waveform(frame: &mut Frame, lane: &Lane, size: Size) {
    let mid = size.height / 2.0;
    let width = size.width / LANE_COLUMNS as f32;
    let body = mix(AUDIO, iced::Color::BLACK, 0.35);
    // The centre line, where there is no audio.
    frame.fill_rectangle(
        Point::new(0.0, mid - 0.5),
        Size::new(size.width, 1.0),
        AUDIO,
    );
    for (i, bucket) in lane.columns.iter().enumerate() {
        let Some(b) = bucket else {
            continue;
        };
        let x = i as f32 * width;
        let top = mid - b.max.clamp(-1.0, 1.0) * mid;
        let bottom = mid - b.min.clamp(-1.0, 1.0) * mid;
        frame.fill_rectangle(
            Point::new(x, top),
            Size::new(width.max(1.0), (bottom - top).max(1.0)),
            AUDIO,
        );
        let rms = b.rms.clamp(0.0, 1.0) * mid;
        frame.fill_rectangle(
            Point::new(x, mid - rms),
            Size::new(width.max(1.0), (2.0 * rms).max(1.0)),
            body,
        );
    }
}

/// Bands from the bottom (lowest) up, from the background at the floor through the audio
/// colour to the far end from the background (near white on dark, near black on light) at 0 dB.
fn draw_spectrum(frame: &mut Frame, lane: &Lane, size: Size, background: iced::Color) {
    let width = size.width / lane.spectrum.len().max(1) as f32;
    let height = size.height / SPECTRUM_BANDS as f32;
    let light = background.r + background.g + background.b > 1.5;
    let hot = if light {
        mix(AUDIO, iced::Color::BLACK, 0.6)
    } else {
        mix(AUDIO, iced::Color::WHITE, 0.75)
    };
    for (i, column) in lane.spectrum.iter().enumerate() {
        let Some(bands) = column else {
            continue;
        };
        for (band, level) in bands.iter().enumerate() {
            let t = ((level - SPECTRUM_FLOOR_DB) / -SPECTRUM_FLOOR_DB).clamp(0.0, 1.0);
            if t <= 0.0 {
                continue;
            }
            let color = if t < 0.7 {
                mix(background, AUDIO, t / 0.7)
            } else {
                mix(AUDIO, hot, (t - 0.7) / 0.3)
            };
            frame.fill_rectangle(
                Point::new(i as f32 * width, size.height - (band + 1) as f32 * height),
                Size::new(width + 0.5, height + 0.5),
                color,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spectrum_bands_takes_the_loudest_bin_of_each_band() {
        let mut bins = vec![-120.0; 1025];
        bins[0] = 0.0; // DC is left out.
        bins[1] = -10.0;
        bins[1024] = -20.0;
        let mut out = [0.0; SPECTRUM_BANDS];
        spectrum_bands(&bins, &mut out);
        assert_eq!(out[0], -10.0);
        assert_eq!(out[SPECTRUM_BANDS - 1], -20.0);
        assert!(out[1..SPECTRUM_BANDS - 1].iter().all(|&l| l == -120.0));
    }

    #[test]
    fn test_spectrum_bands_survives_short_input() {
        let mut out = [0.0; SPECTRUM_BANDS];
        spectrum_bands(&[-3.0, -6.0], &mut out);
        assert_eq!(out[0], -6.0);
        assert!(out[1..].iter().all(|l| l.is_infinite()));
        spectrum_bands(&[], &mut out);
        assert!(out.iter().all(|l| l.is_infinite()));
    }
}
