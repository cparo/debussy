//! An owned AudioToolbox `ExtAudioFile`: open, create, properties, read, write, dispose.

use std::ffi::c_void;
use std::path::Path;
use std::ptr::{self, NonNull};

use objc2_audio_toolbox::{
    AudioConverterRef, AudioConverterSetProperty, AudioFileTypeID, AudioFormatGetProperty,
    ExtAudioFileCreateWithURL, ExtAudioFileDispose, ExtAudioFileGetProperty, ExtAudioFileOpenURL,
    ExtAudioFilePropertyID, ExtAudioFileRead, ExtAudioFileRef, ExtAudioFileSetProperty,
    ExtAudioFileWrite, kAudioConverterEncodeBitRate, kAudioFormatProperty_FormatInfo,
    kExtAudioFileProperty_AudioConverter, kExtAudioFileProperty_ClientDataFormat,
    kExtAudioFileProperty_ConverterConfig, kExtAudioFileProperty_FileDataFormat,
    kExtAudioFileProperty_FileLengthFrames,
};
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription, kAudioFormatFlagIsFloat,
    kAudioFormatFlagIsPacked, kAudioFormatLinearPCM,
};
use objc2_core_foundation::CFURL;

/// A failed AudioToolbox call: its name and `OSStatus`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Status {
    /// The function that failed.
    pub call: &'static str,
    /// Its result code.
    pub code: i32,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Many codes are four printable characters.
        let bytes = self.code.to_be_bytes();
        if bytes.iter().all(|b| b.is_ascii_graphic() || *b == b' ') {
            let text: String = bytes.iter().map(|&b| char::from(b)).collect();
            write!(f, "{} failed: '{text}' ({})", self.call, self.code)
        } else {
            write!(f, "{} failed: {}", self.call, self.code)
        }
    }
}

/// `Ok` for `noErr`, else the status.
fn check(call: &'static str, code: i32) -> Result<(), Status> {
    if code == 0 {
        Ok(())
    } else {
        Err(Status { call, code })
    }
}

/// Packed, interleaved, native-endian `f32` PCM at `sample_rate` with `channels`.
pub(crate) fn float_format(sample_rate: f64, channels: u32) -> AudioStreamBasicDescription {
    AudioStreamBasicDescription {
        mSampleRate: sample_rate,
        mFormatID: kAudioFormatLinearPCM,
        mFormatFlags: kAudioFormatFlagIsFloat | kAudioFormatFlagIsPacked,
        mBytesPerPacket: 4 * channels,
        mFramesPerPacket: 1,
        mBytesPerFrame: 4 * channels,
        mChannelsPerFrame: channels,
        mBitsPerChannel: 32,
        mReserved: 0,
    }
}

/// Fills in the fields of `format` that AudioToolbox can derive (packet sizes for a compressed
/// format), as `kAudioFormatProperty_FormatInfo` does.
pub(crate) fn format_info(
    mut format: AudioStreamBasicDescription,
) -> Result<AudioStreamBasicDescription, Status> {
    let mut size = size_of::<AudioStreamBasicDescription>() as u32;
    // SAFETY: `format` is a valid, initialised ASBD that outlives the call; `size` is its size,
    // which is what this property reads and writes; no specifier is passed.
    let code = unsafe {
        AudioFormatGetProperty(
            kAudioFormatProperty_FormatInfo,
            0,
            ptr::null(),
            &mut size,
            (&raw mut format).cast::<c_void>(),
        )
    };
    check("AudioFormatGetProperty(FormatInfo)", code)?;
    Ok(format)
}

/// An open `ExtAudioFile`, disposed on drop.
pub(crate) struct ExtAudioFile {
    file: NonNull<c_void>,
    /// Channels of the client format, set by [`ExtAudioFile::set_client_format`].
    channels: u32,
}

// SAFETY: an `ExtAudioFileRef` may be used from any thread as long as calls on it are not
// concurrent; this type owns the ref and every call takes `&mut self` or `&self` without
// interior sharing, so moving it to another thread is sound. It is not `Sync`.
unsafe impl Send for ExtAudioFile {}

impl ExtAudioFile {
    fn raw(&self) -> ExtAudioFileRef {
        self.file.as_ptr().cast()
    }

    /// Opens `path` for reading.
    pub(crate) fn open(path: &Path) -> Result<Self, Status> {
        let url = CFURL::from_file_path(path).ok_or(Status {
            call: "CFURL::from_file_path",
            code: -1,
        })?;
        let mut file: ExtAudioFileRef = ptr::null_mut();
        // SAFETY: `url` is a valid CFURL kept alive across the call; `file` is a valid place for
        // the out-pointer.
        let code = unsafe { ExtAudioFileOpenURL(&url, NonNull::from(&mut file)) };
        check("ExtAudioFileOpenURL", code)?;
        Self::wrap(file, "ExtAudioFileOpenURL")
    }

    /// Creates `path` (replacing any file there) of `file_type` holding `format`.
    pub(crate) fn create(
        path: &Path,
        file_type: AudioFileTypeID,
        mut format: AudioStreamBasicDescription,
    ) -> Result<Self, Status> {
        let url = CFURL::from_file_path(path).ok_or(Status {
            call: "CFURL::from_file_path",
            code: -1,
        })?;
        let mut file: ExtAudioFileRef = ptr::null_mut();
        // kAudioFileFlags_EraseFile.
        let erase = 1;
        // SAFETY: `url` and `format` are valid and outlive the call; a null channel layout is
        // allowed; `file` is a valid place for the out-pointer.
        let code = unsafe {
            ExtAudioFileCreateWithURL(
                &url,
                file_type,
                NonNull::from(&mut format),
                ptr::null(),
                erase,
                NonNull::from(&mut file),
            )
        };
        check("ExtAudioFileCreateWithURL", code)?;
        Self::wrap(file, "ExtAudioFileCreateWithURL")
    }

    fn wrap(file: ExtAudioFileRef, call: &'static str) -> Result<Self, Status> {
        let file = NonNull::new(file.cast::<c_void>()).ok_or(Status { call, code: -1 })?;
        Ok(ExtAudioFile { file, channels: 0 })
    }

    /// Reads a fixed-size property into a `T`.
    ///
    /// # Safety
    ///
    /// `T` must be the type AudioToolbox writes for `property`.
    unsafe fn get<T: Copy>(
        &self,
        property: ExtAudioFilePropertyID,
        mut value: T,
        call: &'static str,
    ) -> Result<T, Status> {
        let mut size = size_of::<T>() as u32;
        // SAFETY: the file is open; `value` is a valid `T` of `size` bytes, the type the caller
        // guarantees this property has.
        let code = unsafe {
            ExtAudioFileGetProperty(
                self.raw(),
                property,
                NonNull::from(&mut size),
                NonNull::from(&mut value).cast(),
            )
        };
        check(call, code)?;
        Ok(value)
    }

    /// The file's own data format.
    pub(crate) fn file_format(&self) -> Result<AudioStreamBasicDescription, Status> {
        // SAFETY: FileDataFormat is an AudioStreamBasicDescription.
        unsafe {
            self.get(
                kExtAudioFileProperty_FileDataFormat,
                float_format(0.0, 0),
                "ExtAudioFileGetProperty(FileDataFormat)",
            )
        }
    }

    /// The file's length in its own sample frames, as the file reports it.
    pub(crate) fn length_frames(&self) -> Result<i64, Status> {
        // SAFETY: FileLengthFrames is an SInt64.
        unsafe {
            self.get(
                kExtAudioFileProperty_FileLengthFrames,
                0i64,
                "ExtAudioFileGetProperty(FileLengthFrames)",
            )
        }
    }

    /// Sets the format reads return and writes take, converting to and from the file's.
    pub(crate) fn set_client_format(
        &mut self,
        mut format: AudioStreamBasicDescription,
    ) -> Result<(), Status> {
        // SAFETY: the file is open; ClientDataFormat takes an AudioStreamBasicDescription of
        // exactly this size, read during the call.
        let code = unsafe {
            ExtAudioFileSetProperty(
                self.raw(),
                kExtAudioFileProperty_ClientDataFormat,
                size_of::<AudioStreamBasicDescription>() as u32,
                NonNull::from(&mut format).cast(),
            )
        };
        check("ExtAudioFileSetProperty(ClientDataFormat)", code)?;
        self.channels = format.mChannelsPerFrame;
        Ok(())
    }

    /// Sets the encoder's bit rate, in bits per second, on a file being written.
    pub(crate) fn set_bit_rate(&mut self, bits_per_second: u32) -> Result<(), Status> {
        // SAFETY: AudioConverter is an AudioConverterRef owned by the file.
        let converter: AudioConverterRef = unsafe {
            self.get(
                kExtAudioFileProperty_AudioConverter,
                ptr::null_mut(),
                "ExtAudioFileGetProperty(AudioConverter)",
            )?
        };
        if converter.is_null() {
            return Err(Status {
                call: "ExtAudioFileGetProperty(AudioConverter)",
                code: -1,
            });
        }
        let mut rate = bits_per_second;
        // SAFETY: `converter` is the file's live converter; EncodeBitRate takes a UInt32.
        let code = unsafe {
            AudioConverterSetProperty(
                converter,
                kAudioConverterEncodeBitRate,
                size_of::<u32>() as u32,
                NonNull::from(&mut rate).cast(),
            )
        };
        check("AudioConverterSetProperty(EncodeBitRate)", code)?;
        // Setting a null converter config makes the file pick up the converter's new settings.
        let mut config: *const c_void = ptr::null();
        // SAFETY: ConverterConfig takes a CFArrayRef; a null one of pointer size is documented
        // as "apply the converter's current settings".
        let code = unsafe {
            ExtAudioFileSetProperty(
                self.raw(),
                kExtAudioFileProperty_ConverterConfig,
                size_of::<*const c_void>() as u32,
                NonNull::from(&mut config).cast(),
            )
        };
        check("ExtAudioFileSetProperty(ConverterConfig)", code)
    }

    /// Reads up to `out.len() / channels` interleaved frames in the client format into `out`,
    /// returning the frames read; 0 at the end.
    pub(crate) fn read(&mut self, out: &mut [f32]) -> Result<usize, Status> {
        let channels = self.channels.max(1) as usize;
        let frames = (out.len() / channels).min(u32::MAX as usize / (4 * channels));
        if frames == 0 {
            return Ok(0);
        }
        let mut count = frames as u32;
        let mut list = AudioBufferList {
            mNumberBuffers: 1,
            mBuffers: [AudioBuffer {
                mNumberChannels: channels as u32,
                mDataByteSize: (frames * channels * 4) as u32,
                mData: out.as_mut_ptr().cast(),
            }],
        };
        // SAFETY: the client format is packed interleaved f32 of `channels` (set before any
        // read); the buffer points at `out`, which holds at least `frames × channels` f32 and
        // stays borrowed for the call; AudioToolbox writes at most `mDataByteSize` bytes.
        let code = unsafe {
            ExtAudioFileRead(
                self.raw(),
                NonNull::from(&mut count),
                NonNull::from(&mut list),
            )
        };
        check("ExtAudioFileRead", code)?;
        Ok((count as usize).min(frames))
    }

    /// Writes `input`'s whole interleaved frames in the client format.
    pub(crate) fn write(&mut self, input: &[f32]) -> Result<(), Status> {
        let channels = self.channels.max(1) as usize;
        for chunk in input.chunks(channels * 65_536) {
            let frames = chunk.len() / channels;
            if frames == 0 {
                continue;
            }
            let mut list = AudioBufferList {
                mNumberBuffers: 1,
                mBuffers: [AudioBuffer {
                    mNumberChannels: channels as u32,
                    mDataByteSize: (frames * channels * 4) as u32,
                    // AudioToolbox only reads from it when writing.
                    mData: chunk.as_ptr().cast_mut().cast(),
                }],
            };
            // SAFETY: the client format is packed interleaved f32 of `channels`; the buffer
            // points at `chunk`, at least `frames × channels` f32, borrowed for the call and
            // only read.
            let code =
                unsafe { ExtAudioFileWrite(self.raw(), frames as u32, NonNull::from(&mut list)) };
            check("ExtAudioFileWrite", code)?;
        }
        Ok(())
    }

    /// Finishes the file (writing any trailing packets and headers) and closes it.
    pub(crate) fn close(self) -> Result<(), Status> {
        let raw = self.raw();
        std::mem::forget(self);
        // SAFETY: `raw` is the open file, disposed exactly once: `self` was forgotten, so drop
        // will not dispose it again.
        let code = unsafe { ExtAudioFileDispose(raw) };
        check("ExtAudioFileDispose", code)
    }
}

impl Drop for ExtAudioFile {
    fn drop(&mut self) {
        // SAFETY: the file is open and disposed only here (or in `close`, which forgets it).
        unsafe {
            ExtAudioFileDispose(self.raw());
        }
    }
}
