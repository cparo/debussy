//! Waveform lanes: each track's visible stretch summarised into one [`Bucket`] per column from
//! the stored files' waveform summaries, and drawn on a canvas (`docs/architecture/analysis.md`
//! "Waveform summaries").

use audio_analysis::{BASE_BUCKET, Bucket};
use iced::widget::canvas::{self, Frame, Geometry, Program};
use iced::{Color, Point, Rectangle, Renderer, Size, Theme, mouse};
use project_model::{Source, Track};

use crate::services::Services;

/// Columns per lane; the canvas scales them to its width.
pub const LANE_COLUMNS: usize = 1024;

/// The coarsest zoom: no finer than the summaries' smallest bucket.
pub const MIN_SAMPLES_PER_COLUMN: u64 = BASE_BUCKET;

/// One track's visible waveform, a column at a time; `None` where no clip plays.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Lane {
    /// The columns, left to right.
    pub columns: Vec<Option<Bucket>>,
}

/// The lane of `track`'s first channel from timeline frame `start`, `samples_per_column`
/// frames per column. Clips are painted bottom layer first, so the top clip shows where they
/// overlap, scaled by their gain.
pub fn lane(
    track: &Track,
    start: u64,
    samples_per_column: u64,
    services: &mut dyn Services,
) -> Lane {
    let spc = samples_per_column.max(MIN_SAMPLES_PER_COLUMN);
    let mut columns = vec![None; LANE_COLUMNS];
    let mut scratch = vec![
        Bucket {
            min: 0.0,
            max: 0.0,
            rms: 0.0
        };
        LANE_COLUMNS
    ];
    let end = start.saturating_add(spc.saturating_mul(LANE_COLUMNS as u64));
    for (_, clip) in &track.clips {
        if clip.end() <= start || clip.position >= end {
            continue;
        }
        let file = match &clip.source {
            Source::Mono(file) => &file.name,
            Source::Stereo { left, .. } => &left.name,
        };
        // The column holding the clip's first visible frame, and that frame's place in the
        // file. Columns are read from there, so they may sit up to one column's width off the
        // timeline grid: close enough to draw.
        let visible_from = clip.position.max(start);
        let first = ((visible_from - start) / spc) as usize;
        let last = ((clip.end() - start).div_ceil(spc) as usize).min(LANE_COLUMNS);
        if first >= last {
            continue;
        }
        let file_start = clip.offset + (visible_from - clip.position);
        let n = services.waveform(file, file_start, spc, &mut scratch[..last - first]);
        let gain = 10f32.powf(clip.gain_db / 20.0);
        for (column, bucket) in columns[first..first + n].iter_mut().zip(&scratch[..n]) {
            *column = Some(Bucket {
                min: bucket.min * gain,
                max: bucket.max * gain,
                rms: bucket.rms * gain,
            });
        }
    }
    Lane { columns }
}

/// Draws a lane: the peak outline in a light tone, the RMS body darker, centred.
pub struct LaneView<'a> {
    /// The lane.
    pub lane: &'a Lane,
    /// Where the playhead is, as a fraction of the lane's width, if visible.
    pub playhead: Option<f32>,
}

impl<Message> Program<Message> for LaneView<'_> {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let palette = theme.extended_palette();
        let mut frame = Frame::new(renderer, bounds.size());
        frame.fill_rectangle(Point::ORIGIN, bounds.size(), palette.background.weak.color);
        let mid = bounds.height / 2.0;
        let width = bounds.width / LANE_COLUMNS as f32;
        let peak = palette.primary.weak.color;
        let body = palette.primary.strong.color;
        for (i, bucket) in self.lane.columns.iter().enumerate() {
            let Some(b) = bucket else {
                continue;
            };
            let x = i as f32 * width;
            let top = mid - b.max.clamp(-1.0, 1.0) * mid;
            let bottom = mid - b.min.clamp(-1.0, 1.0) * mid;
            frame.fill_rectangle(
                Point::new(x, top),
                Size::new(width.max(1.0), (bottom - top).max(1.0)),
                peak,
            );
            let rms = b.rms.clamp(0.0, 1.0) * mid;
            frame.fill_rectangle(
                Point::new(x, mid - rms),
                Size::new(width.max(1.0), (2.0 * rms).max(1.0)),
                body,
            );
        }
        if let Some(at) = self.playhead {
            frame.fill_rectangle(
                Point::new(at * bounds.width, 0.0),
                Size::new(1.5, bounds.height),
                Color::from_rgb(0.9, 0.2, 0.2),
            );
        }
        vec![frame.into_geometry()]
    }
}

/// A canvas element drawing `view`.
pub fn canvas<'a, Message: 'a>(view: LaneView<'a>) -> canvas::Canvas<LaneView<'a>, Message> {
    canvas::Canvas::new(view)
}
