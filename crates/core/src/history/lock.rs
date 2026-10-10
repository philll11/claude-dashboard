//! The ingest lock: one process ingests at a time while the others read.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::Path;

use super::HistoryError;

/// Held by the one process that is ingesting.
///
/// Dropping it releases the lock, and so does the operating system when the process ends, even
/// if it crashes.
#[derive(Debug)]
pub struct IngestLock {
    _file: File,
}

impl IngestLock {
    pub(super) fn try_acquire(path: &Path) -> Result<Option<Self>, HistoryError> {
        let file_error = |source| HistoryError::File {
            path: path.to_owned(),
            source,
        };
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(file_error)?;
        match file.try_lock() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(source)) => Err(file_error(source)),
        }
    }
}
