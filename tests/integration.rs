use std::{path::PathBuf, thread, time::Duration};

use file_notice::{FileMarker, FileWaitError, FileWaiter};
use tempfile::tempdir;

#[test]
fn file_marker_creates_and_removes_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("marker");

    assert!(!path.exists());

    {
        let _marker = FileMarker::new(path.clone())?;
        assert!(path.exists());
    }

    assert!(!path.exists());
    Ok(())
}

#[test]
fn file_waiter_returns_already_exists_for_existing_file() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let path = dir.path().join("marker");
    std::fs::write(&path, b"")?;

    match FileWaiter::new(&path) {
        Err(FileWaitError::AlreadyExists) => {}
        Ok(_) => panic!("expected AlreadyExists, got Ok(_)"),
        Err(err) => panic!("expected AlreadyExists, got Err({err})"),
    }

    Ok(())
}

#[test]
fn blocking_waiter_observes_file_creation() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("marker");

    let mut waiter = FileWaiter::new(&path)?;
    let marker_path = path.clone();

    let creator = thread::spawn(move || -> Result<(), std::io::Error> {
        thread::sleep(Duration::from_millis(50));
        let _marker = FileMarker::new(marker_path)?;
        thread::sleep(Duration::from_millis(50));
        Ok(())
    });

    waiter.wait_until_file_marker_blocking()?;

    creator.join().expect("marker creator thread panicked")?;
    Ok(())
}

#[test]
fn blocking_waiter_returns_immediately_when_file_already_exists_after_construction()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("marker");

    let mut waiter = FileWaiter::new(&path)?;
    let _marker = FileMarker::new(path.clone())?;

    waiter.wait_until_file_marker_blocking()?;
    Ok(())
}

#[test]
fn blocking_waiter_observes_rename_into_place() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("marker");

    let mut waiter = FileWaiter::new(&path)?;
    let marker_path = path.clone();
    let source_dir = dir.path().to_path_buf();

    let creator = thread::spawn(move || -> Result<(), std::io::Error> {
        thread::sleep(Duration::from_millis(50));
        let tmp_path = source_dir.join("marker.tmp");
        std::fs::write(&tmp_path, b"")?;
        std::fs::rename(&tmp_path, &marker_path)?;
        Ok(())
    });

    waiter.wait_until_file_marker_blocking()?;

    creator.join().expect("marker creator thread panicked")?;
    Ok(())
}

#[test]
fn waiter_accepts_relative_marker_path_without_directory() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = tempdir()?;
    let original_dir = std::env::current_dir()?;
    std::env::set_current_dir(dir.path())?;

    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let mut waiter = FileWaiter::new("marker")?;
        let _marker = FileMarker::new(PathBuf::from("marker"))?;
        waiter.wait_until_file_marker_blocking()?;
        Ok(())
    })();

    std::env::set_current_dir(original_dir)?;
    result
}

#[test]
fn waiter_rejects_invalid_marker_path() {
    let path = PathBuf::from("");

    match FileWaiter::new(&path) {
        Err(FileWaitError::InvalidMarkerPath(p)) => assert_eq!(p, path),
        Ok(_) => panic!("expected InvalidMarkerPath, got Ok(_)"),
        Err(err) => panic!("expected InvalidMarkerPath, got Err({err})"),
    }
}

#[cfg(feature = "with-tokio")]
#[tokio::test]
async fn async_waiter_observes_file_creation() -> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("marker");

    let mut waiter = file_notice::AsyncFileWaiter::new(&path)?;
    let marker_path = path.clone();

    let creator = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let _marker = FileMarker::new(marker_path)?;
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok::<(), std::io::Error>(())
    });

    waiter.wait_until_file_marker().await?;
    creator.await.expect("marker creator task panicked")?;
    Ok(())
}

#[cfg(feature = "with-tokio")]
#[tokio::test]
async fn async_waiter_returns_immediately_when_file_already_exists_after_construction()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = tempdir()?;
    let path = dir.path().join("marker");

    let mut waiter = file_notice::AsyncFileWaiter::new(&path)?;
    let _marker = FileMarker::new(path.clone())?;

    waiter.wait_until_file_marker().await?;
    Ok(())
}
