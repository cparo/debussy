//! Sample blocks: one buffer's worth of `f32` audio, mono or stereo
//! (`docs/architecture/engine.md` "Sample format" and "Buffer size",
//! `docs/architecture/signal-graph.md` "Channel layouts").

use crate::error::Error;

/// The smallest block capacity, in frames.
pub const MIN_FRAMES: usize = 32;
/// The largest block capacity, in frames.
pub const MAX_FRAMES: usize = 1024;

/// Whether a block holds one channel or two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// One channel.
    Mono,
    /// Left and right channels.
    Stereo,
}

impl Layout {
    /// The number of channels.
    pub const fn channels(self) -> usize {
        match self {
            Layout::Mono => 1,
            Layout::Stereo => 2,
        }
    }
}

/// A buffer of `f32` samples with a fixed capacity and layout, stored one channel after another.
///
/// The number of frames in use can change up to the capacity without allocating, so one block can
/// serve every buffer size up to the one it was made for.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// Every channel's samples, `capacity` per channel, channel by channel.
    samples: Box<[f32]>,
    layout: Layout,
    capacity: usize,
    frames: usize,
}

impl Block {
    /// A silent block of `capacity` frames, all in use. Allocates; call off the real-time thread.
    ///
    /// # Errors
    ///
    /// [`Error::CapacityOutOfRange`] if `capacity` is outside [`MIN_FRAMES`]`..=`[`MAX_FRAMES`].
    pub fn new(layout: Layout, capacity: usize) -> Result<Self, Error> {
        if !(MIN_FRAMES..=MAX_FRAMES).contains(&capacity) {
            return Err(Error::CapacityOutOfRange(capacity));
        }
        Ok(Self {
            samples: vec![0.0; capacity * layout.channels()].into_boxed_slice(),
            layout,
            capacity,
            frames: capacity,
        })
    }

    /// The block's layout.
    pub fn layout(&self) -> Layout {
        self.layout
    }

    /// The number of channels.
    pub fn channels(&self) -> usize {
        self.layout.channels()
    }

    /// The most frames the block can hold.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// The number of frames in use.
    pub fn frames(&self) -> usize {
        self.frames
    }

    /// Sets the number of frames in use. Samples beyond it are kept, not cleared.
    ///
    /// # Errors
    ///
    /// [`Error::FramesOverCapacity`] if `frames` is more than the capacity.
    pub fn set_frames(&mut self, frames: usize) -> Result<(), Error> {
        if frames > self.capacity {
            return Err(Error::FramesOverCapacity {
                frames,
                capacity: self.capacity,
            });
        }
        self.frames = frames;
        Ok(())
    }

    /// The frames in use of channel `index` (0 is mono or left, 1 is right), or `None` if the
    /// block has no such channel.
    pub fn channel(&self, index: usize) -> Option<&[f32]> {
        let start = self.channel_start(index)?;
        self.samples.get(start..start + self.frames)
    }

    /// The frames in use of channel `index`, mutably; `None` if the block has no such channel.
    pub fn channel_mut(&mut self, index: usize) -> Option<&mut [f32]> {
        let start = self.channel_start(index)?;
        self.samples.get_mut(start..start + self.frames)
    }

    /// Both channels of a stereo block, mutably and at once; `None` for a mono block.
    pub fn stereo_mut(&mut self) -> Option<(&mut [f32], &mut [f32])> {
        if self.layout != Layout::Stereo {
            return None;
        }
        let (left, right) = self.samples.split_at_mut(self.capacity);
        Some((&mut left[..self.frames], &mut right[..self.frames]))
    }

    /// Sets every sample of every channel, in use or not, to silence.
    pub fn clear(&mut self) {
        self.samples.fill(0.0);
    }

    fn channel_start(&self, index: usize) -> Option<usize> {
        (index < self.channels()).then_some(index * self.capacity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error as StdError;

    #[test]
    fn test_new_rejects_capacity_out_of_range() {
        assert_eq!(
            Block::new(Layout::Mono, MIN_FRAMES - 1),
            Err(Error::CapacityOutOfRange(31))
        );
        assert_eq!(
            Block::new(Layout::Stereo, MAX_FRAMES + 1),
            Err(Error::CapacityOutOfRange(1025))
        );
        assert!(Block::new(Layout::Mono, MIN_FRAMES).is_ok());
        assert!(Block::new(Layout::Stereo, MAX_FRAMES).is_ok());
    }

    #[test]
    fn test_new_is_silent_and_full() -> Result<(), Box<dyn StdError>> {
        let block = Block::new(Layout::Stereo, 128)?;
        assert_eq!(
            (block.channels(), block.capacity(), block.frames()),
            (2, 128, 128)
        );
        for index in 0..2 {
            let channel = block.channel(index).ok_or("missing channel")?;
            assert!(channel.iter().all(|&s| s == 0.0));
        }
        Ok(())
    }

    #[test]
    fn test_channel_out_of_range_is_none() -> Result<(), Box<dyn StdError>> {
        let mut block = Block::new(Layout::Mono, 64)?;
        assert!(block.channel(1).is_none());
        assert!(block.channel_mut(1).is_none());
        assert!(block.stereo_mut().is_none());
        Ok(())
    }

    #[test]
    fn test_channels_are_independent() -> Result<(), Box<dyn StdError>> {
        let mut block = Block::new(Layout::Stereo, 32)?;
        let (left, right) = block.stereo_mut().ok_or("not stereo")?;
        left.fill(1.0);
        right.fill(-1.0);
        assert!(block.channel(0).ok_or("left")?.iter().all(|&s| s == 1.0));
        assert!(block.channel(1).ok_or("right")?.iter().all(|&s| s == -1.0));
        Ok(())
    }

    #[test]
    fn test_set_frames_limits_views_and_keeps_samples() -> Result<(), Box<dyn StdError>> {
        let mut block = Block::new(Layout::Stereo, 64)?;
        block.channel_mut(1).ok_or("right")?.fill(0.5);
        block.set_frames(32)?;
        assert_eq!(block.channel(1).ok_or("right")?.len(), 32);
        let (left, right) = block.stereo_mut().ok_or("not stereo")?;
        assert_eq!((left.len(), right.len()), (32, 32));
        block.set_frames(64)?;
        assert!(block.channel(1).ok_or("right")?.iter().all(|&s| s == 0.5));
        assert_eq!(
            block.set_frames(65),
            Err(Error::FramesOverCapacity {
                frames: 65,
                capacity: 64
            })
        );
        block.set_frames(0)?;
        assert_eq!(block.channel(0).ok_or("left")?.len(), 0);
        Ok(())
    }

    #[test]
    fn test_clear_silences_unused_frames_too() -> Result<(), Box<dyn StdError>> {
        let mut block = Block::new(Layout::Mono, 32)?;
        block.channel_mut(0).ok_or("mono")?.fill(1.0);
        block.set_frames(8)?;
        block.clear();
        block.set_frames(32)?;
        assert!(block.channel(0).ok_or("mono")?.iter().all(|&s| s == 0.0));
        Ok(())
    }
}
