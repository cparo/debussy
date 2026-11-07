//! The little-endian fields of the cache files' headers (`docs/architecture/analysis.md`
//! "Analysis cache files").

use std::io::{Read, Seek, SeekFrom, Write};

use crate::error::Error;

/// Writes `value` as four little-endian bytes.
pub(crate) fn write_u32<W: Write>(out: &mut W, value: u32) -> Result<(), Error> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}

/// Writes `value` as eight little-endian bytes.
pub(crate) fn write_u64<W: Write>(out: &mut W, value: u64) -> Result<(), Error> {
    out.write_all(&value.to_le_bytes())?;
    Ok(())
}

/// Reads four little-endian bytes.
pub(crate) fn read_u32<R: Read>(input: &mut R) -> Result<u32, Error> {
    let mut bytes = [0; 4];
    input.read_exact(&mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}

/// Reads eight little-endian bytes.
pub(crate) fn read_u64<R: Read>(input: &mut R) -> Result<u64, Error> {
    let mut bytes = [0; 8];
    input.read_exact(&mut bytes)?;
    Ok(u64::from_le_bytes(bytes))
}

/// Checks that `input` holds at least a `header_len`-byte header that starts with `magic` and
/// `version`, leaving it positioned just after the version. Returns the file's length.
pub(crate) fn read_start<R: Read + Seek>(
    input: &mut R,
    magic: [u8; 4],
    version: u32,
    kind: &'static str,
    header_len: u64,
) -> Result<u64, Error> {
    let length = input.seek(SeekFrom::End(0))?;
    if length < header_len {
        return Err(if length < 4 {
            Error::WrongMagic(kind)
        } else {
            Error::WrongLength {
                expected: header_len,
                found: length,
            }
        });
    }
    input.seek(SeekFrom::Start(0))?;
    let mut found = [0; 4];
    input.read_exact(&mut found)?;
    if found != magic {
        return Err(Error::WrongMagic(kind));
    }
    let found = read_u32(input)?;
    if found != version {
        return Err(Error::UnsupportedVersion(found));
    }
    Ok(length)
}
