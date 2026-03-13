//! Inotify-based file system watcher for local filesystems
//!
//! Uses the notify crate's RecommendedWatcher to efficiently detect folder creation
//! events using inotify on Linux and similar mechanisms on other platforms.

use anyhow::{Context, Result};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

/// Creates an inotify-based watcher for detecting new folders
///
/// # Arguments
/// * `path` - The directory to watch for new folder creation
/// * `cancel` - Token that aborts the watch when cancelled
///
/// # Returns
/// A future that resolves to the path of the first new folder detected,
/// or an error if cancelled or the watcher fails.
pub async fn watch_for_new_folder(path: &Path, cancel: CancellationToken) -> Result<PathBuf> {
    let watch_path = path.to_path_buf();
    info!(path = %watch_path.display(), "Starting inotify watcher");

    // Create channel for watcher events
    let (tx, mut rx) = mpsc::channel::<Result<Event>>(100);

    // Keep watcher alive using Arc<Mutex>
    let watcher_arc: Arc<Mutex<Option<RecommendedWatcher>>> = Arc::new(Mutex::new(None));
    let watcher_clone = Arc::clone(&watcher_arc);

    // Create watcher in blocking thread
    let watcher_path = watch_path.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let tx_clone = tx.clone();

        let watcher = RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                let event_result = res.map_err(|e| anyhow::anyhow!("Watcher error: {}", e));
                // Send event to async channel, ignore if channel is closed
                let _ = tx_clone.blocking_send(event_result);
            },
            notify::Config::default(),
        )
        .context("Failed to create file system watcher")?;

        // Store watcher in Arc to keep it alive
        if let Ok(mut guard) = watcher_clone.lock() {
            *guard = Some(watcher);

            // Now watch the directory
            if let Some(ref mut w) = *guard {
                w.watch(&watcher_path, RecursiveMode::NonRecursive)
                    .context("Failed to start watching directory")?;

                debug!(path = %watcher_path.display(), "Watcher initialized");
            }
        }

        Ok(())
    })
    .await??;

    // Process events from the watcher, racing against cancellation
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Folder detection cancelled (shutdown)");
                // Drop the watcher
                if let Ok(mut guard) = watcher_arc.lock() {
                    *guard = None;
                }
                anyhow::bail!("Folder detection cancelled during shutdown");
            }
            event = rx.recv() => {
                match event {
                    Some(event_result) => {
                        match event_result {
                            Ok(event) => {
                                debug!(?event, "Received file system event");
                                if let Some(new_folder) = handle_event(&event, &watch_path)? {
                                    info!(folder = %new_folder.display(), "Detected new folder");
                                    if let Ok(mut guard) = watcher_arc.lock() {
                                        *guard = None;
                                    }
                                    return Ok(new_folder);
                                }
                            }
                            Err(e) => {
                                error!(error = %e, "Watcher error event");
                                warn!("Continuing to watch despite error");
                            }
                        }
                    }
                    None => {
                        anyhow::bail!("Watcher channel closed without detecting new folder");
                    }
                }
            }
        }
    }
}

/// Handles a file system event and returns the new folder path if detected
fn handle_event(event: &Event, watch_path: &Path) -> Result<Option<PathBuf>> {
    match event.kind {
        EventKind::Create(_) => {
            // Check each path in the event
            for path in &event.paths {
                // Only consider paths that are direct children of watch_path
                if let Some(parent) = path.parent() {
                    if parent == watch_path && path.is_dir() {
                        return Ok(Some(path.clone()));
                    }
                }
            }
            Ok(None)
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::time::{sleep, Duration};

    // Note: These tests are ignored on macOS because FSEvents (used by notify on macOS)
    // has timing issues in test environments. In production, the hybrid watcher
    // automatically falls back to polling when notify fails.

    #[tokio::test]
    #[cfg_attr(target_os = "macos", ignore = "FSEvents timing issues in tests")]
    async fn test_detects_new_folder() {
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path();
        let cancel = CancellationToken::new();

        // Start watcher in background
        let watcher_task = {
            let watch_path = watch_path.to_path_buf();
            let cancel = cancel.clone();
            tokio::spawn(async move { watch_for_new_folder(&watch_path, cancel).await })
        };

        // Wait a bit for watcher to initialize
        sleep(Duration::from_millis(100)).await;

        // Create a new folder
        let new_folder = watch_path.join("test-folder");
        fs::create_dir(&new_folder).unwrap();

        // Wait for detection
        let result = tokio::time::timeout(Duration::from_secs(5), watcher_task)
            .await
            .expect("Timeout waiting for folder detection")
            .expect("Watcher task failed");

        assert!(result.is_ok());
        let detected_path = result.unwrap();
        assert_eq!(detected_path, new_folder);
    }

    #[tokio::test]
    #[cfg_attr(target_os = "macos", ignore = "FSEvents timing issues in tests")]
    async fn test_ignores_files() {
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path();
        let cancel = CancellationToken::new();

        // Start watcher
        let watcher_task = {
            let watch_path = watch_path.to_path_buf();
            let cancel = cancel.clone();
            tokio::spawn(async move { watch_for_new_folder(&watch_path, cancel).await })
        };

        // Wait for watcher to initialize
        sleep(Duration::from_millis(100)).await;

        // Create a file (should be ignored)
        fs::write(watch_path.join("test.txt"), "content").unwrap();

        // Wait a bit
        sleep(Duration::from_millis(500)).await;

        // Create a folder (should be detected)
        let new_folder = watch_path.join("test-folder");
        fs::create_dir(&new_folder).unwrap();

        // Wait for detection
        let result = tokio::time::timeout(Duration::from_secs(5), watcher_task)
            .await
            .expect("Timeout waiting for folder detection")
            .expect("Watcher task failed");

        assert!(result.is_ok());
        let detected_path = result.unwrap();
        assert_eq!(detected_path, new_folder);
    }

    #[tokio::test]
    #[cfg_attr(target_os = "macos", ignore = "FSEvents timing issues in tests")]
    async fn test_cancellation_stops_watcher() {
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path();
        let cancel = CancellationToken::new();

        let watcher_task = {
            let watch_path = watch_path.to_path_buf();
            let cancel = cancel.clone();
            tokio::spawn(async move { watch_for_new_folder(&watch_path, cancel).await })
        };

        // Let watcher start
        sleep(Duration::from_millis(200)).await;

        // Cancel
        cancel.cancel();

        // Should exit quickly
        let result = tokio::time::timeout(Duration::from_secs(2), watcher_task)
            .await
            .expect("Timeout — cancellation didn't work")
            .expect("Task panicked");

        assert!(result.is_err());
    }
}
