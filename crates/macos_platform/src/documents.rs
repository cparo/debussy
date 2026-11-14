//! Projects opened from the Finder (`docs/architecture/platform.md` "Documents opened from the
//! Finder", ADR-0132).

use std::path::PathBuf;

use objc2::MainThreadMarker;
use thiserror::Error;

use crate::sys;

/// Why [`on_open_documents`] could not listen.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum DocumentsError {
    /// It was called off the main thread.
    #[error("documents opened from the Finder can only be received on the main thread")]
    NotMainThread,
}

/// Calls `on_open` with the path of each document the Finder asks Debussy to open, including
/// the one a launch was asked for. Call it once, on the main thread, before the window's event
/// loop starts; the paths are untrusted and arrive on the main thread.
///
/// # Errors
///
/// [`DocumentsError::NotMainThread`] off the main thread.
pub fn on_open_documents(on_open: impl Fn(PathBuf) + 'static) -> Result<(), DocumentsError> {
    let mtm = MainThreadMarker::new().ok_or(DocumentsError::NotMainThread)?;
    sys::install_open_documents(mtm, Box::new(on_open));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_on_open_documents_refuses_a_worker_thread() -> Result<(), Box<dyn std::error::Error>> {
        // Test threads are not the main thread.
        let result = std::thread::spawn(|| on_open_documents(|_| {}))
            .join()
            .map_err(|_| "the thread panicked")?;
        assert_eq!(result, Err(DocumentsError::NotMainThread));
        Ok(())
    }
}
