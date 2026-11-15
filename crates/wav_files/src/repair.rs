//! Repairing the sizes of a file [`WavWriter`](crate::WavWriter) never finished, such as a take
//! cut off by a crash (`docs/architecture/recording.md` "Take recovery").
//!
//! Only the writer's own layout is accepted: `RIFF` (or `RF64`), the 28-byte reserve at 12
//! (`JUNK`, or `ds64` once finished as RF64), `fmt `, `fact` for float files, then `data`. The
//! sizes are rebuilt from the file's length, so a file from elsewhere is refused rather than
//! guessed at.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};

use crate::error::{ChunkId, Error};
use crate::format::SampleFormat;
use crate::header::parse_fmt;
use crate::reader::WavReader;
use crate::writer::{DS64_BODY, Positions, RESERVE_AT, write_sizes};

/// Fills in the sizes of the WAV file `file` from its length, truncating a partial last frame,
/// syncs it, and returns its frame count. A file whose sizes already match its length is left
/// alone.
///
/// # Errors
///
/// [`Error::UnrepairableLayout`] for a file not laid out as [`WavWriter`](crate::WavWriter)
/// writes, any format error in its `fmt ` chunk, [`Error::Truncated`] if it ends inside its
/// header, or [`Error::Io`].
pub fn repair_sizes(file: &mut File) -> Result<u64, Error> {
    file.seek(SeekFrom::Start(0))?;
    let frames = repair(file, u64::from(u32::MAX), |f, len| f.set_len(len))?;
    file.sync_data()?;
    Ok(frames)
}

/// [`repair_sizes`] over any stream starting at offset 0, cutting it to a length with
/// `set_len`, and turning it into RF64 above `rf64_threshold`.
pub(crate) fn repair<F: Read + Write + Seek>(
    f: &mut F,
    rf64_threshold: u64,
    set_len: impl FnOnce(&mut F, u64) -> io::Result<()>,
) -> Result<u64, Error> {
    let len = f.seek(SeekFrom::End(0))?;
    f.seek(SeekFrom::Start(0))?;
    let (at, block_align) = layout(f)?;
    let block_align = block_align as u64;
    let available = len.checked_sub(at.data_start).ok_or(Error::Truncated)?;
    let frames = available / block_align;
    let data_bytes = frames * block_align;

    f.seek(SeekFrom::Start(0))?;
    if let Ok(reader) = WavReader::new(&mut *f)
        && !reader.truncated()
        && reader.frames() == frames
        && len == at.data_start + data_bytes + (data_bytes & 1)
    {
        return Ok(frames);
    }
    set_len(f, at.data_start + data_bytes)?;
    let end = write_sizes(f, at, data_bytes, frames, rf64_threshold)?;
    f.seek(SeekFrom::Start(end))?;
    f.flush()?;
    Ok(frames)
}

/// Checks the header has the writer's layout, ignoring the size fields, and returns where those
/// are and the block align.
fn layout<R: Read>(r: &mut R) -> Result<(Positions, usize), Error> {
    let refuse = |what: &'static str| Error::UnrepairableLayout(what);
    let mut head = [0_u8; 12 + 8 + DS64_BODY as usize + 8];
    read(r, &mut head)?;
    if !matches!(&head[0..4], b"RIFF" | b"RF64") || &head[8..12] != b"WAVE" {
        return Err(refuse("it does not start as a RIFF WAVE file"));
    }
    let reserve = RESERVE_AT as usize;
    if !matches!(&head[reserve..reserve + 4], b"JUNK" | b"ds64")
        || le32(&head[reserve + 4..]) != DS64_BODY
    {
        return Err(refuse("its second chunk is not the 28-byte reserve"));
    }
    let fmt_at = reserve + 8 + DS64_BODY as usize;
    let fmt_size = le32(&head[fmt_at + 4..]);
    if &head[fmt_at..fmt_at + 4] != b"fmt " || !matches!(fmt_size, 16 | 18) {
        return Err(refuse("its third chunk is not a plain fmt chunk"));
    }
    let mut body = [0_u8; 18];
    let body = &mut body[..fmt_size as usize];
    read(r, body)?;
    let spec = parse_fmt(body, ChunkId(*b"fmt "), u64::from(fmt_size))?;
    let float = spec.format == SampleFormat::Float32;
    let writable = matches!(
        spec.format,
        SampleFormat::Int16 | SampleFormat::Int24 | SampleFormat::Float32
    );
    if !writable || float != (fmt_size == 18) {
        return Err(refuse("its format is not one the writer produces"));
    }
    let mut pos = (fmt_at + 8) as u64 + u64::from(fmt_size);
    let mut fact_at = None;
    let mut chunk = [0_u8; 8];
    if float {
        let mut fact = [0_u8; 12];
        read(r, &mut fact)?;
        if &fact[0..4] != b"fact" || le32(&fact[4..]) != 4 {
            return Err(refuse("a float file has no fact chunk before its data"));
        }
        fact_at = Some(pos + 8);
        pos += 12;
    }
    read(r, &mut chunk)?;
    if &chunk[0..4] != b"data" {
        return Err(refuse("the data chunk does not follow the format"));
    }
    let at = Positions {
        base: 0,
        fact_at,
        data_size_at: pos + 4,
        data_start: pos + 8,
    };
    Ok((at, spec.block_align()))
}

/// `read_exact`, with a short read reported as a truncated header.
fn read<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<(), Error> {
    r.read_exact(buf).map_err(|e| match e.kind() {
        io::ErrorKind::UnexpectedEof => Error::Truncated,
        _ => Error::Io(e),
    })
}

fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::{Channels, Spec};
    use crate::writer::WavWriter;
    use std::error::Error as StdError;
    use std::io::Cursor;

    fn spec(channels: Channels, format: SampleFormat) -> Spec {
        Spec {
            channels,
            sample_rate: 48_000,
            format,
        }
    }

    /// The bytes a writer leaves when dropped without finishing, after `samples` per channel.
    fn unfinished(spec: Spec, samples: &[&[f32]]) -> Result<Vec<u8>, Box<dyn StdError>> {
        let mut writer = WavWriter::new(Cursor::new(Vec::new()), spec)?;
        writer.write(samples)?;
        Ok(std::mem::take(writer.get_mut().get_mut()))
    }

    fn repair_bytes(bytes: Vec<u8>, threshold: u64) -> Result<(u64, Vec<u8>), Box<dyn StdError>> {
        let mut cursor = Cursor::new(bytes);
        let frames = repair(&mut cursor, threshold, |c, len| {
            c.get_mut().truncate(len as usize);
            Ok(())
        })?;
        Ok((frames, cursor.into_inner()))
    }

    fn read_all(bytes: Vec<u8>, channels: usize) -> Result<Vec<Vec<f32>>, Box<dyn StdError>> {
        let mut reader = WavReader::new(Cursor::new(bytes))?;
        assert!(!reader.truncated());
        let mut out = vec![vec![0.0; reader.frames() as usize]; channels];
        let mut slices: Vec<&mut [f32]> = out.iter_mut().map(Vec::as_mut_slice).collect();
        reader.read(&mut slices)?;
        Ok(out)
    }

    #[test]
    fn test_repair_fills_in_zero_sizes() -> Result<(), Box<dyn StdError>> {
        let samples: Vec<f32> = (0..1000).map(|i| i as f32 / 1000.0).collect();
        let bytes = unfinished(spec(Channels::Mono, SampleFormat::Float32), &[&samples])?;
        assert_eq!(WavReader::new(Cursor::new(bytes.clone()))?.frames(), 0);
        let (frames, repaired) = repair_bytes(bytes, u64::from(u32::MAX))?;
        assert_eq!(frames, 1000);
        assert_eq!(&repaired[0..4], b"RIFF");
        assert_eq!(read_all(repaired, 1)?, [samples]);
        Ok(())
    }

    #[test]
    fn test_repair_truncates_a_partial_frame() -> Result<(), Box<dyn StdError>> {
        let left = [0.5, -0.5, 0.25];
        let right = [-0.25, 0.75, -0.75];
        let mut bytes = unfinished(
            spec(Channels::Stereo, SampleFormat::Int16),
            &[&left, &right],
        )?;
        let whole = bytes.len();
        bytes.extend_from_slice(&[1, 2, 3]);
        let (frames, repaired) = repair_bytes(bytes, u64::from(u32::MAX))?;
        assert_eq!((frames, repaired.len()), (3, whole));
        let back = read_all(repaired, 2)?;
        assert!(back[0].iter().zip(left).all(|(a, b)| (a - b).abs() < 1e-4));
        assert!(back[1].iter().zip(right).all(|(a, b)| (a - b).abs() < 1e-4));
        Ok(())
    }

    #[test]
    fn test_repair_pads_an_odd_data_size() -> Result<(), Box<dyn StdError>> {
        let bytes = unfinished(spec(Channels::Mono, SampleFormat::Int24), &[&[0.5]])?;
        let (frames, repaired) = repair_bytes(bytes, u64::from(u32::MAX))?;
        assert_eq!(frames, 1);
        assert_eq!(repaired.len() % 2, 0);
        assert_eq!(WavReader::new(Cursor::new(repaired))?.frames(), 1);
        Ok(())
    }

    #[test]
    fn test_repair_writes_rf64_past_threshold() -> Result<(), Box<dyn StdError>> {
        let samples: Vec<f32> = (0..1000).map(|i| i as f32).collect();
        let bytes = unfinished(spec(Channels::Mono, SampleFormat::Float32), &[&samples])?;
        let (frames, repaired) = repair_bytes(bytes, 1024)?;
        assert_eq!(frames, 1000);
        assert_eq!(
            (&repaired[0..4], &repaired[12..16]),
            (&b"RF64"[..], &b"ds64"[..])
        );
        assert_eq!(read_all(repaired.clone(), 1)?, [samples]);
        // Repairing it again changes nothing.
        let (again, same) = repair_bytes(repaired.clone(), 1024)?;
        assert_eq!((again, same), (1000, repaired));
        Ok(())
    }

    #[test]
    fn test_repair_leaves_a_finished_file_alone() -> Result<(), Box<dyn StdError>> {
        let mut writer = WavWriter::new(
            Cursor::new(Vec::new()),
            spec(Channels::Mono, SampleFormat::Int24),
        )?;
        writer.write(&[&[0.5, 0.25, -0.5]])?;
        let finished = writer.finish()?.into_inner();
        let mut cursor = Cursor::new(finished.clone());
        let frames = repair(&mut cursor, u64::from(u32::MAX), |_, _| {
            Err(io::Error::other("a finished file was truncated"))
        })?;
        assert_eq!((frames, cursor.into_inner()), (3, finished));
        Ok(())
    }

    #[test]
    fn test_repair_refuses_a_foreign_layout() -> Result<(), Box<dyn StdError>> {
        let mut plain = Vec::new();
        plain.extend_from_slice(b"RIFF\0\0\0\0WAVEfmt ");
        plain.extend_from_slice(&16_u32.to_le_bytes());
        plain.extend_from_slice(&[1, 0, 1, 0, 0x80, 0xBB, 0, 0, 0, 0x77, 1, 0, 2, 0, 16, 0]);
        plain.extend_from_slice(b"data\0\0\0\0");
        plain.extend_from_slice(&[0; 64]);
        assert!(matches!(
            repair_bytes(plain, u64::from(u32::MAX)),
            Err(e) if matches!(e.downcast_ref(), Some(Error::UnrepairableLayout(_)))
        ));
        let mut short = unfinished(spec(Channels::Mono, SampleFormat::Float32), &[&[]])?;
        short.truncate(40);
        assert!(repair_bytes(short, u64::from(u32::MAX)).is_err());
        Ok(())
    }
}
