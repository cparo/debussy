//! The MIDI take journal's byte format, `take-N.midilog` (`docs/architecture/project.md`
//! "Recording marker", ADR-0151): a 16-byte header, then one 16-byte record per message, so a
//! file cut short by a crash loses at most its partial last record.
//!
//! This module only encodes and decodes bytes; the writer and the recovery code do the I/O.
//! Decoding treats its input as untrusted.

use crate::Message;

/// The header's first eight bytes.
pub const JOURNAL_MAGIC: [u8; 8] = *b"DBMIDILG";
/// The journal format version this build writes and reads.
pub const JOURNAL_VERSION: u32 = 1;
/// The header's length in bytes.
pub const JOURNAL_HEADER_BYTES: usize = 16;
/// Each record's length in bytes.
pub const JOURNAL_RECORD_BYTES: usize = 16;

/// Why journal bytes were refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JournalError {
    /// The magic is not [`JOURNAL_MAGIC`].
    #[error("not a MIDI take journal")]
    Magic,
    /// The format version is not [`JOURNAL_VERSION`].
    #[error("MIDI take journal version {0}")]
    Version(u32),
    /// A record's bytes are not one channel message, or its padding is not zero.
    #[error("a MIDI take journal record is not a channel message")]
    Record,
}

/// The header for a journal at `sample_rate`: the magic, the version and the rate, little-endian.
pub fn journal_header(sample_rate: u32) -> [u8; JOURNAL_HEADER_BYTES] {
    let mut out = [0; JOURNAL_HEADER_BYTES];
    out[..8].copy_from_slice(&JOURNAL_MAGIC);
    out[8..12].copy_from_slice(&JOURNAL_VERSION.to_le_bytes());
    out[12..].copy_from_slice(&sample_rate.to_le_bytes());
    out
}

/// The sample rate a header names.
///
/// # Errors
///
/// [`JournalError::Magic`] or [`JournalError::Version`].
pub fn parse_journal_header(bytes: &[u8; JOURNAL_HEADER_BYTES]) -> Result<u32, JournalError> {
    if bytes[..8] != JOURNAL_MAGIC {
        return Err(JournalError::Magic);
    }
    let version = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if version != JOURNAL_VERSION {
        return Err(JournalError::Version(version));
    }
    Ok(u32::from_le_bytes([
        bytes[12], bytes[13], bytes[14], bytes[15],
    ]))
}

/// The record for a message at take frame `frame`: the frame, little-endian, then the status
/// and data bytes (an unused data byte is 0), then five zero bytes.
pub fn journal_record(frame: u64, bytes: [u8; 3]) -> [u8; JOURNAL_RECORD_BYTES] {
    let mut out = [0; JOURNAL_RECORD_BYTES];
    out[..8].copy_from_slice(&frame.to_le_bytes());
    out[8..11].copy_from_slice(&bytes);
    out
}

/// A record's take frame and message bytes.
///
/// # Errors
///
/// [`JournalError::Record`] unless the bytes are one channel message, padded with zeros.
pub fn parse_journal_record(
    bytes: &[u8; JOURNAL_RECORD_BYTES],
) -> Result<(u64, [u8; 3]), JournalError> {
    let mut frame = [0; 8];
    frame.copy_from_slice(&bytes[..8]);
    let message = [bytes[8], bytes[9], bytes[10]];
    let padded = bytes[11..].iter().all(|&b| b == 0);
    let valid = Message::from_bytes(&message).is_some()
        || (message[2] == 0 && Message::from_bytes(&message[..2]).is_some());
    if !padded || !valid {
        return Err(JournalError::Record);
    }
    Ok((u64::from_le_bytes(frame), message))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_header_round_trips_the_rate() {
        let header = journal_header(48_000);
        assert_eq!(&header[..8], b"DBMIDILG");
        assert_eq!(parse_journal_header(&header), Ok(48_000));
    }

    #[test]
    fn test_parse_journal_header_refuses_another_magic_or_version() {
        let mut header = journal_header(48_000);
        header[8] = 2;
        assert_eq!(parse_journal_header(&header), Err(JournalError::Version(2)));
        header[0] = b'X';
        assert_eq!(parse_journal_header(&header), Err(JournalError::Magic));
    }

    #[test]
    fn test_journal_record_round_trips_two_and_three_byte_messages() {
        let note = journal_record(1 << 40, [0x90, 60, 100]);
        assert_eq!(parse_journal_record(&note), Ok((1 << 40, [0x90, 60, 100])));
        let program = journal_record(7, [0xC0, 5, 0]);
        assert_eq!(parse_journal_record(&program), Ok((7, [0xC0, 5, 0])));
    }

    #[test]
    fn test_parse_journal_record_refuses_bad_bytes_or_padding() {
        for record in [
            journal_record(0, [0xF8, 0, 0]),
            journal_record(0, [0x90, 0x80, 1]),
            journal_record(0, [0xC0, 5, 9]),
            journal_record(0, [0x40, 1, 1]),
        ] {
            assert_eq!(parse_journal_record(&record), Err(JournalError::Record));
        }
        let mut record = journal_record(0, [0x90, 60, 100]);
        record[15] = 1;
        assert_eq!(parse_journal_record(&record), Err(JournalError::Record));
    }
}
