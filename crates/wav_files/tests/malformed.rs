//! Untrusted input (ADR-0029): every malformed file is an error, or is read within the limits,
//! and nothing panics or allocates from a header field.

use std::error::Error;
use std::fs;
use std::io::Cursor;
use std::path::Path;

use wav_files::{Error as WavError, WavReader};

/// A `fmt ` body: tag, channels, rate, block align, bits.
fn fmt(tag: u16, channels: u16, rate: u32, block_align: u16, bits: u16) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&tag.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * u32::from(block_align)).to_le_bytes());
    b.extend_from_slice(&block_align.to_le_bytes());
    b.extend_from_slice(&bits.to_le_bytes());
    b
}

fn mono16() -> Vec<u8> {
    fmt(1, 1, 8000, 2, 16)
}

/// A `WAVE_FORMAT_EXTENSIBLE` body with sub-format `sub_tag` and the GUID tail `tail`.
fn extensible(channels: u16, bits: u16, sub_tag: u16, tail: [u8; 14]) -> Vec<u8> {
    let mut b = fmt(0xFFFE, channels, 48_000, channels * bits / 8, bits);
    b.extend_from_slice(&22_u16.to_le_bytes());
    b.extend_from_slice(&bits.to_le_bytes());
    b.extend_from_slice(&0_u32.to_le_bytes());
    b.extend_from_slice(&sub_tag.to_le_bytes());
    b.extend_from_slice(&tail);
    b
}

const GUID_TAIL: [u8; 14] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
];

/// A chunk with its true size.
fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    chunk_sized(id, u32::try_from(body.len()).unwrap_or(u32::MAX), body)
}

/// A chunk whose size field says `size`, whatever the body's length; padded if odd.
fn chunk_sized(id: &[u8; 4], size: u32, body: &[u8]) -> Vec<u8> {
    let mut c = id.to_vec();
    c.extend_from_slice(&size.to_le_bytes());
    c.extend_from_slice(body);
    if body.len() % 2 == 1 {
        c.push(0);
    }
    c
}

/// A file of `form` (`RIFF` or `RF64`) holding `chunks`.
fn file(form: &[u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = chunks.concat();
    let mut f = form.to_vec();
    f.extend_from_slice(
        &u32::try_from(body.len() + 4)
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    f.extend_from_slice(b"WAVE");
    f.extend_from_slice(&body);
    f
}

fn open(bytes: Vec<u8>) -> Result<WavReader<Cursor<Vec<u8>>>, WavError> {
    WavReader::new(Cursor::new(bytes))
}

fn valid() -> Vec<u8> {
    file(
        b"RIFF",
        &[
            chunk(b"fmt ", &mono16()),
            chunk(b"data", &[0, 0x40, 0, 0xC0]),
        ],
    )
}

#[test]
fn test_valid_baseline_reads() -> Result<(), Box<dyn Error>> {
    let mut reader = open(valid())?;
    let mut out = [0.0; 2];
    assert_eq!(reader.read(&mut [&mut out])?, 2);
    assert_eq!(out, [0.5, -0.5]);
    Ok(())
}

#[test]
fn test_rejects_bad_container() {
    assert!(matches!(open(Vec::new()), Err(WavError::Truncated)));
    let mut riff = valid();
    riff[0..4].copy_from_slice(b"RIFX");
    assert!(matches!(open(riff), Err(WavError::NotRiff)));
    let mut wave = valid();
    wave[8..12].copy_from_slice(b"AVI ");
    assert!(matches!(open(wave), Err(WavError::NotWave)));
}

#[test]
fn test_rejects_truncated_headers() {
    let full = valid();
    // Every cut inside the header, before the data chunk's body starts.
    for len in 0..44 {
        let result = open(full[..len].to_vec());
        assert!(
            matches!(
                result,
                Err(WavError::Truncated | WavError::MissingData | WavError::ChunkPastEnd { .. })
            ),
            "cut at {len}: {result:?}"
        );
    }
}

#[test]
fn test_rejects_missing_or_misplaced_chunks() {
    let data = chunk(b"data", &[0, 0]);
    assert!(matches!(
        open(file(b"RIFF", &[data.clone(), chunk(b"fmt ", &mono16())])),
        Err(WavError::MissingFmt)
    ));
    assert!(matches!(
        open(file(b"RIFF", &[chunk(b"fmt ", &mono16())])),
        Err(WavError::MissingData)
    ));
    assert!(matches!(
        open(file(b"RF64", &[chunk(b"fmt ", &mono16()), data])),
        Err(WavError::MissingDs64)
    ));
}

#[test]
fn test_rejects_chunk_sizes_past_end_of_file() {
    let big_list = chunk_sized(b"LIST", 1_000_000_000, &[0; 4]);
    assert!(matches!(
        open(file(b"RIFF", &[big_list, chunk(b"fmt ", &mono16())])),
        Err(WavError::ChunkPastEnd { .. })
    ));
    let short_fmt = chunk_sized(b"fmt ", 16, &mono16()[..10]);
    assert!(matches!(
        open(file(b"RIFF", &[short_fmt])),
        Err(WavError::ChunkPastEnd { .. })
    ));
}

#[test]
fn test_rejects_read_chunks_over_or_under_limits() {
    let mut huge = mono16();
    huge.resize(2000, 0);
    assert!(matches!(
        open(file(b"RIFF", &[chunk(b"fmt ", &huge)])),
        Err(WavError::ChunkTooLarge { size: 2000, .. })
    ));
    let max_size = chunk_sized(b"fmt ", u32::MAX, &mono16());
    assert!(matches!(
        open(file(b"RIFF", &[max_size])),
        Err(WavError::ChunkTooLarge { .. })
    ));
    assert!(matches!(
        open(file(b"RIFF", &[chunk(b"fmt ", &mono16()[..14])])),
        Err(WavError::ChunkTooSmall { size: 14, .. })
    ));
}

#[test]
fn test_rejects_bad_channel_counts() {
    for channels in [0_u16, 3, 6, 64, u16::MAX] {
        let body = fmt(1, channels, 8000, channels.wrapping_mul(2), 16);
        let result = open(file(b"RIFF", &[chunk(b"fmt ", &body), chunk(b"data", &[])]));
        assert!(
            matches!(result, Err(WavError::UnsupportedChannels(c)) if c == channels),
            "{channels}: {result:?}"
        );
    }
}

#[test]
fn test_rejects_bad_rates_formats_and_block_align() {
    let cases: [(Vec<u8>, &str); 6] = [
        (fmt(1, 1, 0, 2, 16), "rate 0"),
        (fmt(1, 1, 10_000_000, 2, 16), "rate 10 MHz"),
        (fmt(1, 1, 8000, 1, 8), "8-bit PCM"),
        (fmt(2, 1, 8000, 2, 16), "ADPCM"),
        (fmt(3, 1, 8000, 2, 16), "16-bit float"),
        (fmt(1, 2, 8000, 2, 16), "block align"),
    ];
    for (body, what) in cases {
        let result = open(file(b"RIFF", &[chunk(b"fmt ", &body), chunk(b"data", &[])]));
        assert!(
            matches!(
                result,
                Err(WavError::SampleRateOutOfRange(_)
                    | WavError::UnsupportedFormat { .. }
                    | WavError::BlockAlignMismatch { .. })
            ),
            "{what}: {result:?}"
        );
    }
}

#[test]
fn test_reads_extensible_and_rejects_bad_guids() -> Result<(), Box<dyn Error>> {
    let good = extensible(2, 32, 3, GUID_TAIL);
    let mut reader = open(file(
        b"RIFF",
        &[
            chunk(b"fmt ", &good),
            chunk(b"data", &[0, 0, 0, 0x3F, 0, 0, 0, 0xBF]),
        ],
    ))?;
    let (mut l, mut r) = ([0.0], [0.0]);
    assert_eq!(reader.read(&mut [&mut l, &mut r])?, 1);
    assert_eq!((l, r), ([0.5], [-0.5]));

    let mut bad_tail = GUID_TAIL;
    bad_tail[13] ^= 1;
    for body in [
        extensible(1, 16, 1, bad_tail),
        extensible(1, 16, 0x55, GUID_TAIL),
    ] {
        let result = open(file(b"RIFF", &[chunk(b"fmt ", &body), chunk(b"data", &[])]));
        assert!(
            matches!(result, Err(WavError::UnsupportedFormat { .. })),
            "{result:?}"
        );
    }
    let short = &extensible(1, 16, 1, GUID_TAIL)[..30];
    let result = open(file(b"RIFF", &[chunk(b"fmt ", short), chunk(b"data", &[])]));
    assert!(
        matches!(result, Err(WavError::ChunkTooSmall { .. })),
        "{result:?}"
    );
    Ok(())
}

#[test]
fn test_overlong_data_is_clamped_and_reported() -> Result<(), Box<dyn Error>> {
    // A crashed recorder's header: data claims 0xFFFFFFFF bytes; 3 bytes (1.5 frames) are present.
    let data = chunk_sized(b"data", u32::MAX, &[0, 0x40, 7]);
    let mut reader = open(file(b"RIFF", &[chunk(b"fmt ", &mono16()), data]))?;
    assert!(reader.truncated());
    assert_eq!(
        reader.frames(),
        2,
        "the pad byte makes the last frame whole"
    );
    let mut out = [0.0; 8];
    assert_eq!(reader.read(&mut [&mut out])?, 2);
    assert_eq!(out[0], 0.5);
    Ok(())
}

#[test]
fn test_overflowing_ds64_sizes_are_bounded_by_the_file() -> Result<(), Box<dyn Error>> {
    let mut ds64 = Vec::new();
    for size in [u64::MAX, u64::MAX, u64::MAX] {
        ds64.extend_from_slice(&size.to_le_bytes());
    }
    ds64.extend_from_slice(&u32::MAX.to_le_bytes());
    let data = chunk_sized(b"data", u32::MAX, &[0, 0x40, 0, 0xC0]);
    let mut reader = open(file(
        b"RF64",
        &[chunk(b"ds64", &ds64), chunk(b"fmt ", &mono16()), data],
    ))?;
    assert!(reader.truncated());
    assert_eq!(reader.frames(), 2);
    let mut out = [0.0; 2];
    assert_eq!(reader.read(&mut [&mut out])?, 2);

    // A ds64 chunk too small for its fields, and one claiming an absurd size.
    for bad in [
        chunk(b"ds64", &[0; 20]),
        chunk_sized(b"ds64", u32::MAX, &ds64),
    ] {
        let result = open(file(b"RF64", &[bad, chunk(b"fmt ", &mono16())]));
        assert!(
            matches!(
                result,
                Err(WavError::ChunkTooSmall { .. } | WavError::ChunkTooLarge { .. })
            ),
            "{result:?}"
        );
    }
    Ok(())
}

#[test]
fn test_skips_unknown_and_odd_sized_chunks() -> Result<(), Box<dyn Error>> {
    let reader = open(file(
        b"RIFF",
        &[
            chunk(b"bext", &[1, 2, 3]),
            chunk(b"fmt ", &mono16()),
            chunk(b"LIST", &[0; 9]),
            chunk(b"data", &[0, 0x40]),
        ],
    ))?;
    assert_eq!(reader.frames(), 1);
    Ok(())
}

#[test]
fn test_rejects_too_many_chunks() {
    let mut chunks = vec![chunk(b"fmt ", &mono16())];
    chunks.extend((0..1100).map(|_| chunk(b"junk", &[])));
    chunks.push(chunk(b"data", &[0, 0]));
    assert!(matches!(
        open(file(b"RIFF", &chunks)),
        Err(WavError::TooManyChunks(1024))
    ));
}

#[test]
fn test_reader_rejects_wrong_buffers_and_seeks() -> Result<(), Box<dyn Error>> {
    let mut reader = open(valid())?;
    let (mut a, mut b) = ([0.0; 2], [0.0; 2]);
    assert!(matches!(
        reader.read(&mut [&mut a, &mut b]),
        Err(WavError::ChannelCountMismatch {
            expected: 1,
            found: 2
        })
    ));
    assert!(matches!(
        reader.seek(3),
        Err(WavError::FrameOutOfRange {
            frame: 3,
            frames: 2
        })
    ));
    reader.seek(2)?;
    assert_eq!(reader.read(&mut [&mut a])?, 0);
    Ok(())
}

/// A small deterministic generator (xorshift64*), so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// Opens and fully reads `bytes`, checking the reader never claims more audio than the file
/// holds. Errors are fine; a panic fails the test.
fn exercise(bytes: Vec<u8>, rng: &mut Rng) {
    let len = bytes.len() as u64;
    let Ok(mut reader) = open(bytes) else {
        return;
    };
    let spec = reader.spec();
    assert!(reader.frames() * spec.block_align() as u64 <= len);
    let mut buffers = [[0.0_f32; 512]; 2];
    let [left, right] = &mut buffers;
    let mut slices: Vec<&mut [f32]> = vec![left, right];
    slices.truncate(spec.channels.count());
    while let Ok(n) = reader.read(&mut slices) {
        if n == 0 {
            break;
        }
    }
    let frames = reader.frames();
    let _ = reader.seek(rng.next() % (frames + 2));
    let _ = reader.read(&mut slices);
}

#[test]
fn test_mutated_files_never_panic() -> Result<(), Box<dyn Error>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut seeds = vec![
        valid(),
        file(
            b"RIFF",
            &[
                chunk(b"fmt ", &extensible(2, 24, 1, GUID_TAIL)),
                chunk(b"data", &[0; 60]),
            ],
        ),
    ];
    for name in [
        "mono_LEI16.wav",
        "stereo_LEI24.wav",
        "mono_LEF32_junk.wav",
        "mono_LEF64.wav",
    ] {
        seeds.push(fs::read(dir.join(name))?);
    }
    let mut ds64 = Vec::new();
    for size in [0_u64, 8, 4] {
        ds64.extend_from_slice(&size.to_le_bytes());
    }
    ds64.extend_from_slice(&0_u32.to_le_bytes());
    seeds.push(file(
        b"RF64",
        &[
            chunk(b"ds64", &ds64),
            chunk(b"fmt ", &mono16()),
            chunk_sized(b"data", u32::MAX, &[0; 8]),
        ],
    ));

    let boundaries = [0_u32, 1, 2, 0x7FFF_FFFF, 0x8000_0000, 0xFFFF_FFFE, u32::MAX];
    let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
    for seed in &seeds {
        for _ in 0..1500 {
            let mut bytes = seed.clone();
            let header = bytes.len().min(128);
            match rng.below(4) {
                0 => bytes.truncate(rng.below(bytes.len() + 1)),
                1 => {
                    for _ in 0..=rng.below(4) {
                        let at = rng.below(header);
                        bytes[at] ^= 1 << rng.below(8);
                    }
                }
                2 => {
                    let at = rng.below(header.saturating_sub(4));
                    let value = boundaries[rng.below(boundaries.len())];
                    if let Some(field) = bytes.get_mut(at..at + 4) {
                        field.copy_from_slice(&value.to_le_bytes());
                    }
                }
                _ => {
                    let at = rng.below(header);
                    bytes[at] = rng.next() as u8;
                }
            }
            exercise(bytes, &mut rng);
        }
    }
    Ok(())
}
