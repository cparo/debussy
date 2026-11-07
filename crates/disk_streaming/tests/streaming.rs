//! Streaming through the reader thread, with every real-time call made under the guard.
// Tests wait for the reader thread with `sleep` and write fixture files; neither runs on the
// real-time side, which is what `clippy.toml` guards.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::error::Error;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{Duration, Instant};

use disk_streaming::{
    Error as StreamError, Event, FrameSource, Plan, Playback, Streamer, StreamerConfig, open_wav,
};
use realtime_guard::{GuardAllocator, assert_realtime};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

const RATE: u32 = 48_000;

/// A source whose sample at frame `n` is `n % 65536`, exactly representable in `f32`.
struct Ramp {
    frames: u64,
    position: u64,
    delay: Duration,
    fail: Arc<AtomicBool>,
}

impl Ramp {
    fn new(frames: u64) -> Self {
        Self {
            frames,
            position: 0,
            delay: Duration::ZERO,
            fail: Arc::new(AtomicBool::new(false)),
        }
    }
}

fn value(frame: u64) -> f32 {
    (frame % 65_536) as f32
}

impl FrameSource for Ramp {
    fn sample_rate(&self) -> u32 {
        RATE
    }

    fn frames(&self) -> u64 {
        self.frames
    }

    fn seek(&mut self, frame: u64) -> Result<(), StreamError> {
        self.position = frame.min(self.frames);
        Ok(())
    }

    fn read(&mut self, out: &mut [f32]) -> Result<usize, StreamError> {
        if self.fail.load(Ordering::Relaxed) {
            return Err(StreamError::Io(std::io::Error::other("disk unplugged")));
        }
        sleep(self.delay);
        let n =
            usize::try_from(self.frames - self.position).map_or(out.len(), |r| r.min(out.len()));
        for (i, sample) in out[..n].iter_mut().enumerate() {
            *sample = value(self.position + i as u64);
        }
        self.position += n as u64;
        Ok(n)
    }
}

fn config() -> StreamerConfig {
    StreamerConfig::new(RATE)
}

/// Waits (off the real-time thread) until `playback` is primed with `frames`.
fn wait_primed(playback: &Playback, frames: u64) -> Result<(), Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !playback.is_primed(frames) {
        if Instant::now() > deadline {
            return Err("stream never primed".into());
        }
        sleep(Duration::from_millis(1));
    }
    Ok(())
}

/// Reads `frames` frames in 128-frame blocks, each read under the real-time guard and only once
/// primed, so a correct streamer never underruns.
fn play(playback: &mut Playback, frames: usize) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut out = vec![0.0; frames];
    for block in out.chunks_mut(128) {
        wait_primed(playback, block.len() as u64)?;
        assert_realtime(|| playback.read(block));
    }
    Ok(out)
}

#[test]
fn test_plays_a_source_in_order_to_the_end() -> Result<(), Box<dyn Error>> {
    let frames = 3 * u64::from(RATE) + 77;
    let (streamer, mut playbacks) = Streamer::start(vec![Box::new(Ramp::new(frames))], config())?;
    let playback = &mut playbacks[0];
    let out = play(playback, usize::try_from(frames)?)?;
    for (n, &s) in out.iter().enumerate() {
        assert_eq!(s, value(n as u64), "frame {n}");
    }
    // At the end: silence, and no underrun because the source has ended.
    let mut tail = [1.0_f32; 64];
    assert_eq!(assert_realtime(|| playback.read(&mut tail)), 0);
    assert_eq!(tail, [0.0; 64]);
    assert_eq!(playback.underruns(), 0);
    assert_eq!(streamer.underruns(0), Some(0));
    drop(streamer);
    Ok(())
}

#[test]
fn test_seek_discards_old_blocks_and_plays_from_new_position() -> Result<(), Box<dyn Error>> {
    let (mut streamer, mut playbacks) =
        Streamer::start(vec![Box::new(Ramp::new(500_000))], config())?;
    let playback = &mut playbacks[0];
    play(playback, 5000)?;
    streamer.seek(0, Plan::from(300_000))?;
    let out = play(playback, 10_000)?;
    for (n, &s) in out.iter().enumerate() {
        assert_eq!(s, value(300_000 + n as u64), "frame {n}");
    }
    assert_eq!(playback.underruns(), 0);
    Ok(())
}

#[test]
fn test_loop_region_wraps_without_a_gap() -> Result<(), Box<dyn Error>> {
    let (mut streamer, mut playbacks) =
        Streamer::start(vec![Box::new(Ramp::new(100_000))], config())?;
    streamer.seek(
        0,
        Plan {
            start: 1500,
            loop_region: Some((1000, 4000)),
        },
    )?;
    let playback = &mut playbacks[0];
    let out = play(playback, 12_000)?;
    for (n, &s) in out.iter().enumerate() {
        // 1500..4000, then 1000..4000 repeatedly.
        let n = n as u64;
        let expected = if n < 2500 {
            1500 + n
        } else {
            1000 + (n - 2500) % 3000
        };
        assert_eq!(s, value(expected), "frame {n}");
    }
    assert_eq!(playback.underruns(), 0);
    Ok(())
}

#[test]
fn test_many_streams_play_together() -> Result<(), Box<dyn Error>> {
    let sources: Vec<Box<dyn FrameSource>> = (0..32)
        .map(|_| Box::new(Ramp::new(100_000)) as Box<dyn FrameSource>)
        .collect();
    let (_streamer, mut playbacks) = Streamer::start(sources, config())?;
    for playback in &mut playbacks {
        let out = play(playback, 20_000)?;
        assert_eq!(out[19_999], value(19_999));
        assert_eq!(playback.underruns(), 0);
    }
    Ok(())
}

#[test]
fn test_slow_source_underruns_with_silence_not_a_stall() -> Result<(), Box<dyn Error>> {
    let mut slow = Ramp::new(1_000_000);
    slow.delay = Duration::from_millis(200);
    let (_streamer, mut playbacks) = Streamer::start(vec![Box::new(slow)], config())?;
    let playback = &mut playbacks[0];
    // Read at once, without waiting to be primed: the reader cannot have caught up.
    let mut block = [1.0_f32; 128];
    let started = Instant::now();
    let read = assert_realtime(|| playback.read(&mut block));
    assert!(
        started.elapsed() < Duration::from_millis(50),
        "read must not wait"
    );
    assert_eq!(read, 0);
    assert_eq!(block, [0.0; 128]);
    assert_eq!(playback.underruns(), 1);
    Ok(())
}

#[test]
fn test_read_failure_is_reported_and_plays_silence() -> Result<(), Box<dyn Error>> {
    let source = Ramp::new(1_000_000);
    let fail = Arc::clone(&source.fail);
    let (mut streamer, mut playbacks) = Streamer::start(vec![Box::new(source)], config())?;
    let playback = &mut playbacks[0];
    play(playback, 1000)?;
    fail.store(true, Ordering::Relaxed);
    streamer.seek(0, Plan::from(500_000))?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let event = loop {
        if let Some(event) = streamer.poll_event() {
            break event;
        }
        if Instant::now() > deadline {
            return Err("no failure event".into());
        }
        sleep(Duration::from_millis(1));
    };
    assert!(
        matches!(event, Event::ReadFailed { stream: 0, ref message } if message.contains("unplugged"))
    );
    let mut block = [1.0_f32; 128];
    assert_eq!(assert_realtime(|| playback.read(&mut block)), 0);
    assert_eq!(block, [0.0; 128]);
    Ok(())
}

#[test]
fn test_start_and_seek_reject_bad_requests() -> Result<(), Box<dyn Error>> {
    let result = Streamer::start(vec![Box::new(Ramp::new(10))], StreamerConfig::new(44_100));
    assert!(matches!(
        result,
        Err(StreamError::SampleRateMismatch {
            expected: 44_100,
            found: RATE
        })
    ));
    let (mut streamer, _playbacks) = Streamer::start(vec![Box::new(Ramp::new(1000))], config())?;
    assert!(matches!(
        streamer.seek(1, Plan::from(0)),
        Err(StreamError::UnknownStream(1))
    ));
    assert!(matches!(
        streamer.seek(0, Plan::from(1001)),
        Err(StreamError::PlanOutOfRange { .. })
    ));
    for region in [(500, 500), (600, 500), (0, 1001)] {
        let plan = Plan {
            start: 0,
            loop_region: Some(region),
        };
        assert!(
            matches!(
                streamer.seek(0, plan),
                Err(StreamError::PlanOutOfRange { .. })
            ),
            "{region:?}"
        );
    }
    assert_eq!(streamer.underruns(1), None);
    Ok(())
}

#[test]
fn test_open_wav_streams_stored_audio() -> Result<(), Box<dyn Error>> {
    use wav_files::{Channels, SampleFormat, Spec, WavWriter};
    let dir = std::env::temp_dir().join(format!("disk-streaming-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    let write = |name: &str, channels: Channels| -> Result<std::path::PathBuf, Box<dyn Error>> {
        let path = dir.join(name);
        let spec = Spec {
            channels,
            sample_rate: RATE,
            format: SampleFormat::Float32,
        };
        let file = std::io::BufWriter::new(std::fs::File::create(&path)?);
        let mut writer = WavWriter::new(file, spec)?;
        let samples: Vec<f32> = (0..20_000).map(value).collect();
        let views: Vec<&[f32]> = (0..channels.count()).map(|_| samples.as_slice()).collect();
        writer.write(&views)?;
        writer.finish()?;
        Ok(path)
    };
    let mono = write("mono.wav", Channels::Mono)?;
    let stereo = write("stereo.wav", Channels::Stereo)?;
    assert!(matches!(open_wav(&stereo), Err(StreamError::NotMono)));

    let (_streamer, mut playbacks) = Streamer::start(vec![open_wav(&mono)?], config())?;
    let out = play(&mut playbacks[0], 20_000)?;
    for (n, &s) in out.iter().enumerate() {
        assert_eq!(s, value(n as u64), "frame {n}");
    }
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
