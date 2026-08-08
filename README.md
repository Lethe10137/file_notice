# `file_notice`

> **Linux only.** This crate will fail to compile on other platforms (see [Platform support](#platform-support)).

A small Rust crate for waiting on a file to appear as a lightweight notification mechanism.

This crate is built on Linux `inotify` and is intended for simple file-based coordination between processes or tasks. A common pattern is:

- one side creates a marker file to signal readiness or completion
- another side waits until that marker file appears

## Features

- blocking API with `FileWaiter`
- Tokio-based async API with `AsyncFileWaiter`
- RAII-style marker file helper with `FileMarker`
- small, focused API surface

## Platform support

`file_notice` currently supports **Linux only** because it relies on `inotify`. Building this crate on any other target (including macOS and Windows) fails at compile time with a clear error message rather than a confusing dependency error.

macOS support is planned but not implemented yet, and is not part of the initial release.

If you need cross-platform filesystem notifications, this crate is probably not the right abstraction.

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
file_notice = "0.1"
```

If you want the Tokio-based async API, enable the `with-tokio` feature if it is not already enabled in your setup:

```toml
[dependencies]
file_notice = { version = "0.1", features = ["with-tokio"] }
```

## Usage

### Blocking example

```rust
use file_notice::{FileMarker, FileWaitError, FileWaiter};
use std::path::PathBuf;

fn main() -> Result<(), FileWaitError> {
    let path = PathBuf::from("/tmp/example.marker");

    let mut waiter = FileWaiter::new(&path)?;

    let _marker = FileMarker::new(path)?;
    waiter.wait_until_file_marker_blocking()?;

    Ok(())
}
```

### Async example with Tokio

```rust
use file_notice::{AsyncFileWaiter, FileMarker, FileWaitError};
use std::path::PathBuf;

async fn demo() -> Result<(), FileWaitError> {
    let path = PathBuf::from("/tmp/example.marker");

    let mut waiter = AsyncFileWaiter::new(&path)?;

    let _marker = FileMarker::new(path)?;
    waiter.wait_until_file_marker().await?;

    Ok(())
}
```

## API overview

### `FileMarker`

`FileMarker` creates a file when constructed and removes it when dropped.

This is useful when you want marker-file lifetime to follow Rust scope automatically.

```rust
use file_notice::FileMarker;
use std::path::PathBuf;

fn create_marker() -> std::io::Result<()> {
    let path = PathBuf::from("/tmp/example.marker");
    let _marker = FileMarker::new(path)?;

    // the file exists while `_marker` is alive

    Ok(())
    // the file is removed here when `_marker` is dropped
}
```

### `FileWaiter`

`FileWaiter` watches the parent directory and blocks until the target file appears.

```rust
use file_notice::{FileWaitError, FileWaiter};
use std::path::PathBuf;

fn wait_for_marker() -> Result<(), FileWaitError> {
    let path = PathBuf::from("/tmp/example.marker");
    let mut waiter = FileWaiter::new(&path)?;
    waiter.wait_until_file_marker_blocking()?;
    Ok(())
}
```

### `AsyncFileWaiter`

When the `with-tokio` feature is enabled, `AsyncFileWaiter` provides the same waiting behavior for async Tokio code.

```rust
use file_notice::{AsyncFileWaiter, FileWaitError};
use std::path::PathBuf;

async fn wait_for_marker() -> Result<(), FileWaitError> {
    let path = PathBuf::from("/tmp/example.marker");
    let mut waiter = AsyncFileWaiter::new(&path)?;
    waiter.wait_until_file_marker().await?;
    Ok(())
}
```

## Error behavior

The crate uses `FileWaitError` for waiter construction and waiting operations.

Possible cases include:

- invalid marker path
- I/O errors from filesystem or `inotify`
- `AlreadyExists` when constructing a waiter for a path that already exists

## Notes and caveats

- `FileMarker::new` currently creates the file immediately and truncates an existing file at the same path.
- `FileMarker` attempts to remove the marker file on drop, ignoring removal errors.
- `FileWaiter::new` is intended for waiting on a **future** creation event, so it returns `AlreadyExists` if the target file already exists at construction time.
- You can still call the wait methods after the file appears, and they return immediately if the file already exists at that point.
- The marker file can appear either by direct creation or by being renamed into place (a common atomic-write pattern); both are detected.
- A relative marker path with no directory component (e.g. `"marker"`) is watched relative to the current directory.
- `FileWaiter` watches the marker's **parent directory**, not the file itself, so it wakes up for every filesystem event in that directory, not just the marker's own creation. Prefer a dedicated, low-traffic directory for the marker (e.g. `/tmp/uid-my-app-<random>/`) over a shared, busy one (e.g. `/tmp` directly) to avoid unnecessary wakeups.

## Testing

This crate includes integration tests covering:

- marker creation and cleanup
- blocking waiter behavior
- async waiter behavior
- already-existing marker handling
- invalid path handling
- rename-into-place marker creation
- relative marker paths without a directory component

## License

Licensed under either of:

- MIT license
- Apache License, Version 2.0

at your option.