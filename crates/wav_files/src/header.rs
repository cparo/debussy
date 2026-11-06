//! Parsing the chunks before the samples, within the limits of ADR-0029.
//!
//! Every size is checked against the real file length in `u64` with checked arithmetic. Chunks
//! the reader needs (`fmt `, `ds64`) are read into fixed buffers; every other chunk is skipped by
//! seeking. Nothing is allocated.

use std::io::{self, Read, Seek, SeekFrom};

use crate::error::{ChunkId, Error};
use crate::format::{Channels, SampleFormat, Spec, TAG_EXTENSIBLE};

/// The largest `fmt ` or `ds64` chunk read.
pub(crate) const MAX_READ_CHUNK: usize = 1024;
/// The most chunks scanned before `data`.
pub(crate) const MAX_CHUNKS: usize = 1024;
/// The RF64 marker for "the real size is in `ds64`".
pub(crate) const SIZE_IN_DS64: u32 = 0xFFFF_FFFF;
/// Bytes 2..16 of every `KSDATAFORMAT_SUBTYPE_*` GUID; bytes 0..2 hold the format tag.
pub(crate) const SUBFORMAT_GUID_TAIL: [u8; 14] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
];

/// What the reader needs from the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Header {
    pub(crate) spec: Spec,
    /// Offset of the first sample, from where parsing started.
    pub(crate) data_start: u64,
    /// Whole frames present.
    pub(crate) frames: u64,
    /// Whether the `data` chunk claimed more bytes than the file holds.
    pub(crate) truncated: bool,
}

/// Parses the header of the WAV file starting at `r`'s current position, leaving `r` anywhere.
pub(crate) fn parse<R: Read + Seek>(r: &mut R) -> Result<Header, Error> {
    let base = r.stream_position()?;
    let file_len = r
        .seek(SeekFrom::End(0))?
        .checked_sub(base)
        .ok_or(Error::Truncated)?;
    r.seek(SeekFrom::Start(base))?;

    let mut riff = [0_u8; 12];
    read_header_bytes(r, &mut riff)?;
    let rf64 = match &riff[0..4] {
        b"RIFF" => false,
        b"RF64" => true,
        _ => return Err(Error::NotRiff),
    };
    if &riff[8..12] != b"WAVE" {
        return Err(Error::NotWave);
    }

    let mut pos: u64 = 12;
    let mut ds64_data_size = None;
    if rf64 {
        let (id, size) = chunk_header(r, base, pos, file_len)?;
        if id.0 != *b"ds64" {
            return Err(Error::MissingDs64);
        }
        let mut body = [0_u8; MAX_READ_CHUNK];
        let body = read_chunk(r, id, size, 28, &mut body, pos, file_len)?;
        ds64_data_size = Some(le64(body, 8));
        pos = next_chunk(pos, size)?;
    }

    let mut spec = None;
    for _ in 0..MAX_CHUNKS {
        if pos >= file_len {
            return Err(Error::MissingData);
        }
        let (id, declared) = chunk_header(r, base, pos, file_len)?;
        match &id.0 {
            b"fmt " => {
                let mut body = [0_u8; MAX_READ_CHUNK];
                let body = read_chunk(r, id, declared, 16, &mut body, pos, file_len)?;
                spec = Some(parse_fmt(body, id, declared)?);
            }
            b"data" => {
                let spec = spec.ok_or(Error::MissingFmt)?;
                let size = match ds64_data_size {
                    Some(real) if declared == u64::from(SIZE_IN_DS64) => real,
                    _ => declared,
                };
                let data_start = pos + 8;
                let available = file_len.saturating_sub(data_start);
                let bytes = size.min(available);
                // `block_align` was checked to be non-zero when the format was parsed.
                let frames = bytes / spec.block_align() as u64;
                return Ok(Header {
                    spec,
                    data_start: base.checked_add(data_start).ok_or(Error::SizeOverflow)?,
                    frames,
                    truncated: size > available,
                });
            }
            _ => {
                let end = (pos + 8).checked_add(declared).ok_or(Error::SizeOverflow)?;
                if end > file_len {
                    return Err(Error::ChunkPastEnd { id });
                }
            }
        }
        pos = next_chunk(pos, declared)?;
    }
    Err(Error::TooManyChunks(MAX_CHUNKS))
}

/// Reads the 8-byte chunk header at `pos`, leaving the reader at the chunk's body.
fn chunk_header<R: Read + Seek>(
    r: &mut R,
    base: u64,
    pos: u64,
    file_len: u64,
) -> Result<(ChunkId, u64), Error> {
    if file_len.saturating_sub(pos) < 8 {
        return Err(Error::Truncated);
    }
    seek_to(r, base, pos)?;
    let mut h = [0_u8; 8];
    read_header_bytes(r, &mut h)?;
    let id = ChunkId([h[0], h[1], h[2], h[3]]);
    Ok((id, u64::from(u32::from_le_bytes([h[4], h[5], h[6], h[7]]))))
}

/// Reads the body of a chunk the reader needs into `buf`, after checking its size is between
/// `min` and [`MAX_READ_CHUNK`] and that it lies within the file. The reader must already be at
/// the body.
fn read_chunk<'a, R: Read>(
    r: &mut R,
    id: ChunkId,
    size: u64,
    min: u64,
    buf: &'a mut [u8; MAX_READ_CHUNK],
    pos: u64,
    file_len: u64,
) -> Result<&'a [u8], Error> {
    if size > MAX_READ_CHUNK as u64 {
        return Err(Error::ChunkTooLarge {
            id,
            size,
            max: MAX_READ_CHUNK as u64,
        });
    }
    if size < min {
        return Err(Error::ChunkTooSmall { id, size });
    }
    if pos + 8 + size > file_len {
        return Err(Error::ChunkPastEnd { id });
    }
    // `size` is at most MAX_READ_CHUNK, so this fits.
    let body = &mut buf[..size as usize];
    read_header_bytes(r, body)?;
    Ok(body)
}

/// The position after the chunk at `pos` with body `size`, including the pad byte for odd sizes.
fn next_chunk(pos: u64, size: u64) -> Result<u64, Error> {
    pos.checked_add(8)
        .and_then(|p| p.checked_add(size))
        .and_then(|p| p.checked_add(size & 1))
        .ok_or(Error::SizeOverflow)
}

fn parse_fmt(b: &[u8], id: ChunkId, size: u64) -> Result<Spec, Error> {
    let mut tag = le16(b, 0);
    let channels = le16(b, 2);
    let sample_rate = le32(b, 4);
    let block_align = le16(b, 12);
    let bits = le16(b, 14);
    if tag == TAG_EXTENSIBLE {
        if b.len() < 40 || le16(b, 16) < 22 {
            return Err(Error::ChunkTooSmall { id, size });
        }
        if b[26..40] != SUBFORMAT_GUID_TAIL {
            return Err(Error::UnsupportedFormat { tag, bits });
        }
        tag = le16(b, 24);
    }
    let channels = Channels::from_count(channels)?;
    let format = SampleFormat::from_tag(tag, bits)?;
    let spec = Spec {
        channels,
        sample_rate: Spec::check_rate(sample_rate)?,
        format,
    };
    // At most 2 channels × 8 bytes, so this fits.
    let expected = spec.block_align() as u16;
    if block_align != expected {
        return Err(Error::BlockAlignMismatch {
            expected,
            found: block_align,
        });
    }
    Ok(spec)
}

/// Seeks to `pos`, an offset from `base`, where parsing began.
fn seek_to<R: Seek>(r: &mut R, base: u64, pos: u64) -> Result<(), Error> {
    let absolute = base.checked_add(pos).ok_or(Error::SizeOverflow)?;
    r.seek(SeekFrom::Start(absolute))?;
    Ok(())
}

/// `read_exact`, with a short read reported as a truncated header.
fn read_header_bytes<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<(), Error> {
    r.read_exact(buf).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => Error::Truncated,
        _ => Error::Io(e),
    })
}

fn le16(b: &[u8], at: usize) -> u16 {
    let mut a = [0_u8; 2];
    if let Some(s) = b.get(at..at + 2) {
        a.copy_from_slice(s);
    }
    u16::from_le_bytes(a)
}

fn le32(b: &[u8], at: usize) -> u32 {
    let mut a = [0_u8; 4];
    if let Some(s) = b.get(at..at + 4) {
        a.copy_from_slice(s);
    }
    u32::from_le_bytes(a)
}

fn le64(b: &[u8], at: usize) -> u64 {
    let mut a = [0_u8; 8];
    if let Some(s) = b.get(at..at + 8) {
        a.copy_from_slice(s);
    }
    u64::from_le_bytes(a)
}
