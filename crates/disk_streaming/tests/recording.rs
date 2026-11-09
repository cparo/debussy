//! Recording through the writer thread, with every real-time call made under the guard.
// Tests wait for the writer thread with `sleep` and read the files back; neither runs on the
// real-time side, which is what `clippy.toml` guards.
#![allow(clippy::disallowed_methods, clippy::disallowed_types)]

use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::thread::sleep;
use std::time::{Duration, Instant};

use disk_streaming::{Recorder, RecorderConfig, Take, TakeEvent};
use realtime_guard::{GuardAllocator, assert_realtime};
use wav_files::{Channels, SampleFormat, WavReader};

#[global_allocator]
static ALLOCATOR: GuardAllocator = GuardAllocator;

const RATE: u32 = 48_000;
const BLOCK: usize = 128;

/// A sample for take frame `n`, exactly representable and never 0.
fn value(n: usize) -> f32 {
    (n % 65_536) as f32 + 1.0
}

/// A fresh directory for one test.
fn scratch(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let dir = std::env::temp_dir().join(format!("disk-recording-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Writes `samples` to `take` in device-sized blocks under the guard, returning the frames kept.
fn write(take: &mut Take, samples: &[f32]) -> Result<usize, Box<dyn Error>> {
    let kept: usize = assert_realtime(|| samples.chunks(BLOCK).map(|c| take.write(c)).sum());
    Ok(kept)
}

/// Waits for `count` events.
fn events(recorder: &mut Recorder, count: usize) -> Result<Vec<TakeEvent>, Box<dyn Error>> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut events = Vec::new();
    while events.len() < count {
        match recorder.poll_event() {
            Some(event) => events.push(event),
            None if Instant::now() > deadline => return Err("no event from the writer".into()),
            None => sleep(Duration::from_millis(1)),
        }
    }
    Ok(events)
}

/// The samples of the mono float file at `path`.
fn read(path: &Path) -> Result<Vec<f32>, Box<dyn Error>> {
    let mut reader = WavReader::new(BufReader::new(File::open(path)?))?;
    let spec = reader.spec();
    assert_eq!(
        (spec.channels, spec.sample_rate, spec.format),
        (Channels::Mono, RATE, SampleFormat::Float32)
    );
    let mut samples = vec![0.0; reader.frames() as usize];
    let mut done = 0;
    while done < samples.len() {
        let n = reader.read(&mut [&mut samples[done..]])?;
        if n == 0 {
            break;
        }
        done += n;
    }
    Ok(samples)
}

#[test]
fn test_take_is_written_and_read_back() -> Result<(), Box<dyn Error>> {
    let dir = scratch("written")?;
    let paths = [dir.join("a.wav"), dir.join("b.wav")];
    let (mut recorder, mut takes) = Recorder::start(&paths, RecorderConfig::new(RATE))?;
    let a: Vec<f32> = (0..30_000).map(value).collect();
    let b: Vec<f32> = (0..30_000).map(|n| -value(n)).collect();
    // Written a little at a time, as the device runs, so the writer keeps up.
    for (ca, cb) in a.chunks(4096).zip(b.chunks(4096)) {
        assert_eq!(write(&mut takes[0], ca)?, ca.len());
        assert_eq!(write(&mut takes[1], cb)?, cb.len());
        sleep(Duration::from_millis(5));
    }
    assert_realtime(|| takes.iter_mut().for_each(Take::end));
    let mut got = events(&mut recorder, 2)?;
    got.sort_by_key(|e| match e {
        TakeEvent::Finished { take, .. } | TakeEvent::Failed { take, .. } => *take,
    });
    assert_eq!(
        got,
        [
            TakeEvent::Finished {
                take: 0,
                frames: 30_000
            },
            TakeEvent::Finished {
                take: 1,
                frames: 30_000
            }
        ]
    );
    assert_eq!(read(&paths[0])?, a);
    assert_eq!(read(&paths[1])?, b);
    assert_eq!(recorder.overruns(0), Some(0));
    drop(takes);
    drop(recorder);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_overrun_drops_frames_counts_and_pads_with_silence() -> Result<(), Box<dyn Error>> {
    let dir = scratch("overrun")?;
    let paths = [dir.join("a.wav")];
    // The smallest buffer: one block plus the two spare, 3 × 1024 frames.
    let config = RecorderConfig {
        sample_rate: RATE,
        buffer: Duration::from_millis(1),
    };
    let (mut recorder, mut takes) = Recorder::start(&paths, config)?;
    let take = &mut takes[0];
    // One write larger than the pool: blocks come back only between writes, so it overruns.
    let first: Vec<f32> = (0..5000).map(value).collect();
    let kept = assert_realtime(|| take.write(&first));
    assert_eq!(kept, 3 * 1024);
    assert_eq!(take.overruns(), 1);
    // Once the writer has caught up, writes are kept again.
    let second: Vec<f32> = (5000..5300).map(value).collect();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut kept = 0;
    while kept == 0 {
        if Instant::now() > deadline {
            return Err("the writer never returned a block".into());
        }
        sleep(Duration::from_millis(5));
        kept = assert_realtime(|| take.write(&second[..1]));
    }
    // Each failed attempt above dropped one more frame and counted one more overrun.
    let failed = take.overruns() as usize - 1;
    assert_eq!(write(take, &second[1..])?, 299);
    assert_realtime(|| take.end());
    assert_eq!(
        events(&mut recorder, 1)?,
        [TakeEvent::Finished {
            take: 0,
            frames: (5300 + failed) as u64
        }]
    );
    let samples = read(&paths[0])?;
    let gap_end = 5000 + failed;
    assert_eq!(&samples[..3072], &first[..3072]);
    assert!(samples[3072..gap_end].iter().all(|&s| s == 0.0));
    assert_eq!(&samples[gap_end..], &second[..]);
    assert_eq!(recorder.overruns(0), Some(take.overruns()));
    drop(takes);
    drop(recorder);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_empty_take_finishes_with_no_frames() -> Result<(), Box<dyn Error>> {
    let dir = scratch("empty")?;
    let paths = [dir.join("a.wav")];
    let (mut recorder, takes) = Recorder::start(&paths, RecorderConfig::new(RATE))?;
    // Dropping a take ends it.
    drop(takes);
    assert_eq!(
        events(&mut recorder, 1)?,
        [TakeEvent::Finished { take: 0, frames: 0 }]
    );
    assert!(read(&paths[0])?.is_empty());
    drop(recorder);
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}

#[test]
fn test_start_refuses_an_existing_file_and_removes_those_created() -> Result<(), Box<dyn Error>> {
    let dir = scratch("exists")?;
    let paths = [dir.join("new.wav"), dir.join("old.wav")];
    std::fs::write(&paths[1], b"keep")?;
    assert!(Recorder::start(&paths, RecorderConfig::new(RATE)).is_err());
    assert!(!paths[0].exists());
    assert_eq!(std::fs::read(&paths[1])?, b"keep");
    let _ = std::fs::remove_dir_all(&dir);
    Ok(())
}
