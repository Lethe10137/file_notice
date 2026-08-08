//! Wait for a file to appear as a lightweight notification mechanism.
//!
//! This crate provides a small Linux-only API built on `inotify` for waiting
//! until a marker file is created.
//!
//! # Platform support
//!
//! This crate currently supports Linux only, because it uses `inotify`.
//!
//! # Synchronous and asynchronous APIs
//!
//! Use [`FileWaiter`] for blocking code, or [`AsyncFileWaiter`] when the
//! `with-tokio` feature is enabled.
//!
//! # Examples
//!
//! Blocking usage:
//!
//! ```no_run
//! use file_notice::{FileMarker, FileWaiter};
//!
//! let path = std::path::PathBuf::from("/tmp/example.marker");
//! let mut waiter = FileWaiter::new(&path)?;
//! let _marker = FileMarker::new(path)?;
//! waiter.wait_until_file_marker_blocking()?;
//! # Ok::<(), file_notice::FileWaitError>(())
//! ```
//!
//! Async usage with Tokio:
//!
//! ```no_run
//! # #[cfg(feature = "with-tokio")]
//! # async fn demo() -> Result<(), file_notice::FileWaitError> {
//! use file_notice::{AsyncFileWaiter, FileMarker};
//!
//! let path = std::path::PathBuf::from("/tmp/example.marker");
//! let mut waiter = AsyncFileWaiter::new(&path)?;
//! let _marker = FileMarker::new(path)?;
//! waiter.wait_until_file_marker().await?;
//! # Ok(())
//! # }
//! ```
#[cfg(not(target_os = "linux"))]
compile_error!(
    "file_notice currently supports Linux only (it is built on inotify). \
     macOS support is planned but not yet implemented."
);

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

use inotify::{Inotify, WatchMask};

#[cfg(feature = "with-tokio")]
use tokio::io::unix::AsyncFd;

/// Errors returned when creating or waiting on a file marker.
///
/// This type covers invalid marker paths, I/O failures from filesystem
/// operations or inotify, and attempts to wait on a marker that already
/// exists.
#[derive(Debug)]
pub enum FileWaitError {
    /// The provided marker path did not include a valid file name or parent
    /// directory.
    InvalidMarkerPath(PathBuf),
    /// An I/O error occurred while interacting with the filesystem or
    /// inotify.
    IoError(std::io::Error),
    /// The marker file already exists when constructing a waiter.
    ///
    /// A waiter expects to observe the file being created after it starts
    /// watching the parent directory.
    AlreadyExists,
}

impl From<std::io::Error> for FileWaitError {
    fn from(e: std::io::Error) -> Self {
        FileWaitError::IoError(e)
    }
}

impl std::fmt::Display for FileWaitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileWaitError::InvalidMarkerPath(path) => {
                write!(f, "Invalid marker path: {:?}", path)
            }
            FileWaitError::IoError(e) => {
                write!(f, "I/O error: {}", e)
            }
            FileWaitError::AlreadyExists => {
                write!(f, "File already exists")
            }
        }
    }
}

impl std::error::Error for FileWaitError {}

/// A marker file whose existence signals readiness or completion.
///
/// `FileMarker` creates the file when constructed and removes it when dropped,
/// making it useful as a simple RAII-style notification primitive between
/// processes or tasks.
pub struct FileMarker {
    path: PathBuf,
}

impl FileMarker {
    /// Creates a new marker file at `path`.
    ///
    /// The file is created immediately. When the returned value is dropped, the
    /// file is removed. Any existing file at the same path is truncated.
    pub fn new(path: PathBuf) -> std::io::Result<Self> {
        // Create an empty file
        std::fs::File::create(&path)?;
        Ok(Self { path })
    }
}

impl Drop for FileMarker {
    fn drop(&mut self) {
        // Try to delete the marker file. Errors are ignored.
        std::fs::remove_file(&self.path).ok();
    }
}

/// Waits synchronously for a marker file to be created in a directory.
///
/// A `FileWaiter` watches the parent directory of the target marker and
/// returns once the file appears.
pub struct FileWaiter {
    dir: PathBuf,
    file_name: OsString,
    inotify: Inotify,
}

fn check_file_path(marker: impl AsRef<Path>) -> Result<(OsString, PathBuf), FileWaitError> {
    let marker = marker.as_ref();
    let Some(file_name) = marker.file_name().map(|n| n.to_os_string()) else {
        return Err(FileWaitError::InvalidMarkerPath(marker.to_path_buf()));
    };
    let Some(dir) = marker.parent() else {
        return Err(FileWaitError::InvalidMarkerPath(marker.to_path_buf()));
    };
    // `Path::parent` returns `Some("")` for a bare relative file name (e.g.
    // "marker.txt"), which is not a usable watch target. Treat it as the
    // current directory instead.
    let dir = if dir.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        dir.to_path_buf()
    };
    Ok((file_name, dir))
}

impl FileWaiter {
    /// Creates a new synchronous waiter for `marker`.
    ///
    /// The marker path must include both a file name and a parent directory.
    /// Construction fails with [`FileWaitError::AlreadyExists`] if the file is
    /// already present, because the waiter is intended to observe a future
    /// creation event.
    ///
    /// This watches the marker's *parent directory*, so it wakes up for
    /// every filesystem event in that directory, not just the marker's own
    /// creation. Prefer a dedicated, low-traffic directory for the marker
    /// (e.g. `/tmp/uid-my-app-<random>/`) over a shared, busy one (e.g.
    /// `/tmp` directly) to avoid unnecessary wakeups.
    pub fn new(marker: impl AsRef<Path>) -> Result<Self, FileWaitError> {
        let marker = marker.as_ref();
        let (file_name, dir) = check_file_path(marker)?;

        // The watch is registered before checking whether the marker already
        // exists, so that a file created concurrently with this call is
        // always either caught by the `try_exists` check below or observed
        // as an event afterwards. Checking existence first would leave a
        // window in which a marker created (and possibly removed) between
        // the check and the watch registration is missed entirely, causing
        // later waits to block forever.
        let inotify = Inotify::init().map_err(FileWaitError::IoError)?;
        inotify
            .watches()
            .add(&dir, WatchMask::CREATE | WatchMask::MOVED_TO)
            .map_err(FileWaitError::IoError)?;

        if marker.try_exists().map_err(FileWaitError::IoError)? {
            return Err(FileWaitError::AlreadyExists);
        }

        Ok(Self {
            dir,
            file_name,
            inotify,
        })
    }

    /// Blocks until the marker file exists.
    ///
    /// If the file already exists when this method is called, it returns
    /// immediately. Otherwise it waits for a matching creation/moved-to event in the
    /// watched directory.
    pub fn wait_until_file_marker_blocking(&mut self) -> Result<(), FileWaitError> {
        let marker = self.dir.join(&self.file_name);
        if marker.exists() {
            return Ok(());
        }

        let mut buffer = [0u8; 1024];
        while !marker.exists() {
            match self.inotify.read_events_blocking(&mut buffer) {
                Ok(mut events) => {
                    if events.any(|event| event.name.is_some_and(|name| name == self.file_name)) {
                        break;
                    }
                }
                Err(e) => {
                    return Err(FileWaitError::IoError(e));
                }
            }
        }
        Ok(())
    }
}

#[cfg(feature = "with-tokio")]
/// Waits asynchronously for a marker file to be created.
///
/// This type provides the same behavior as [`FileWaiter`], but integrates with
/// Tokio through [`tokio::io::unix::AsyncFd`].
pub struct AsyncFileWaiter {
    inotify: AsyncFd<Inotify>,
    dir: PathBuf,
    file_name: OsString,
}

#[cfg(feature = "with-tokio")]
impl AsyncFileWaiter {
    /// Creates a new asynchronous waiter for `marker`.
    ///
    /// This has the same validation and error behavior as [`FileWaiter::new`].
    pub fn new(marker: impl AsRef<Path>) -> Result<Self, FileWaitError> {
        let FileWaiter {
            dir,
            file_name,
            inotify,
        } = FileWaiter::new(marker)?;
        Ok(Self {
            dir,
            file_name,
            inotify: AsyncFd::new(inotify)?,
        })
    }

    /// Asynchronously waits until the marker file exists.
    ///
    /// If the file already exists when this method is called, it returns
    /// immediately. Otherwise it waits for the watched inotify file descriptor
    /// to become readable and scans for a matching creation/moved-to event.
    pub async fn wait_until_file_marker(&mut self) -> Result<(), FileWaitError> {
        let marker = self.dir.join(&self.file_name);
        if marker.exists() {
            return Ok(());
        }

        let mut buffer = [0u8; 1024];
        loop {
            let _guard = self.inotify.readable().await?;
            let events = match self.inotify.get_mut().read_events(&mut buffer) {
                Ok(events) => events,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => continue,
                Err(e) => return Err(FileWaitError::IoError(e)),
            };
            for item in events {
                if item.name.is_some_and(|name| name == self.file_name) {
                    return Ok(());
                }
            }
        }
    }
}
