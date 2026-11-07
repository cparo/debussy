//! The crate's error type, shared by every platform implementation.

use crate::codec::CodecFormat;

/// Why a platform operation failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A file could not be read or written.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// No device has this identifier; it may have been unplugged.
    #[error("no audio device {0:?}")]
    DeviceNotFound(String),
    /// The device cannot run this configuration.
    #[error("the device does not support {0}")]
    UnsupportedConfig(String),
    /// A running stream stopped, for example because its device was unplugged.
    #[error("the audio stream stopped: {0}")]
    StreamStopped(String),
    /// The platform cannot encode or decode this format.
    #[error("{0:?} is not supported here")]
    UnsupportedFormat(CodecFormat),
    /// A file is not in a format the platform can decode.
    #[error("the file could not be decoded: {0}")]
    Undecodable(String),
    /// Any other failure reported by the platform, as its message.
    #[error("platform error: {0}")]
    Platform(String),
}
