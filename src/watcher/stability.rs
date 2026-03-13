//! Upload stability detection module
//!
//! Monitors a folder for new file arrivals and waits for a stability period
//! (no new files for N seconds) before considering the upload complete.
//!
//! This is critical for ensuring all videos in a multi-video capture have
//! finished uploading before processing begins.

use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::{sleep, Instant};
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

/// Waits for upload stability before processing
///
/// This function monitors a folder and waits for a specified duration with no new
/// files appearing. If new files are detected, the timeout is reset. This ensures
/// that multi-video uploads are fully complete before processing begins.
///
/// The operation is cancellable via the provided `CancellationToken`.
///
/// # Arguments
/// * `folder_path` - The folder to monitor for file stability
/// * `stability_timeout` - Duration to wait with no new files before considering upload complete
/// * `cancel` - Token that, when cancelled, aborts stability detection immediately
///
/// # Returns
/// Ok(()) when the folder is stable (no new files for the timeout duration)
///
/// # Errors
/// Returns an error if the folder doesn't exist, watching fails, or the operation is cancelled
pub async fn wait_for_stability(
    folder_path: &Path,
    stability_timeout: Duration,
    cancel: CancellationToken,
) -> Result<()> {
    info!(
        path = %folder_path.display(),
        timeout_secs = stability_timeout.as_secs(),
        "Starting upload stability detection"
    );

    // Verify folder exists
    if !folder_path.exists() {
        anyhow::bail!("Folder does not exist: {}", folder_path.display());
    }
    if !folder_path.is_dir() {
        anyhow::bail!("Path is not a directory: {}", folder_path.display());
    }

    // Try inotify-based watching for efficiency
    match wait_for_stability_with_notify(folder_path, stability_timeout, cancel.clone()).await {
        Ok(()) => Ok(()),
        Err(e) => {
            // If cancelled, propagate immediately
            if cancel.is_cancelled() {
                return Err(e);
            }

            warn!(
                error = %e,
                "Inotify-based stability detection failed, falling back to polling"
            );
            // Fall back to polling-based stability detection
            wait_for_stability_with_polling(folder_path, stability_timeout, cancel).await
        }
    }
}

/// Waits for stability using inotify-based file watching
async fn wait_for_stability_with_notify(
    folder_path: &Path,
    stability_timeout: Duration,
    cancel: CancellationToken,
) -> Result<()> {
    let watch_path = folder_path.to_path_buf();

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
        )?;

        // Store watcher in Arc to keep it alive
        if let Ok(mut guard) = watcher_clone.lock() {
            *guard = Some(watcher);

            // Now watch the directory recursively to catch files in subdirectories
            if let Some(ref mut w) = *guard {
                w.watch(&watcher_path, RecursiveMode::Recursive)?;
                debug!(path = %watcher_path.display(), "Stability watcher initialized");
            }
        }

        Ok(())
    })
    .await??;

    // Track last file event time
    let mut last_event = Instant::now();

    // Monitor for events and stability
    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Stability detection cancelled (shutdown)");
                if let Ok(mut guard) = watcher_arc.lock() {
                    *guard = None;
                }
                anyhow::bail!("Stability detection cancelled during shutdown");
            }

            // Check for new events
            Some(event_result) = rx.recv() => {
                match event_result {
                    Ok(event) => {
                        if is_file_event(&event) {
                            debug!(?event, "File event detected, resetting stability timer");
                            last_event = Instant::now();
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "Watcher error, continuing");
                    }
                }
            }

            // Check if we've reached stability
            _ = sleep(Duration::from_secs(1)) => {
                let elapsed = last_event.elapsed();
                if elapsed >= stability_timeout {
                    info!(
                        path = %watch_path.display(),
                        waited_secs = elapsed.as_secs(),
                        "Upload stability achieved"
                    );

                    // Drop the watcher to stop watching
                    if let Ok(mut guard) = watcher_arc.lock() {
                        *guard = None;
                    }

                    return Ok(());
                }
            }
        }
    }
}

/// Waits for stability using polling (fallback for network mounts)
async fn wait_for_stability_with_polling(
    folder_path: &Path,
    stability_timeout: Duration,
    cancel: CancellationToken,
) -> Result<()> {
    info!(
        path = %folder_path.display(),
        "Using polling-based stability detection"
    );

    // Get initial file list and count
    let mut last_file_count = count_files(folder_path)?;
    let mut last_change = Instant::now();

    // Poll interval (check every second)
    let poll_interval = Duration::from_secs(1);

    loop {
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Stability polling cancelled (shutdown)");
                anyhow::bail!("Stability detection cancelled during shutdown");
            }
            _ = sleep(poll_interval) => {}
        }

        // Count current files
        let current_file_count = count_files(folder_path)?;

        // Check if file count changed
        if current_file_count != last_file_count {
            debug!(
                old_count = last_file_count,
                new_count = current_file_count,
                "File count changed, resetting stability timer"
            );
            last_file_count = current_file_count;
            last_change = Instant::now();
        }

        // Check if we've reached stability
        let elapsed = last_change.elapsed();
        if elapsed >= stability_timeout {
            info!(
                path = %folder_path.display(),
                file_count = current_file_count,
                waited_secs = elapsed.as_secs(),
                "Upload stability achieved (polling)"
            );
            return Ok(());
        }
    }
}

/// Checks if an event represents a file creation or modification
fn is_file_event(event: &Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

/// Recursively counts all files in a directory
fn count_files(path: &Path) -> Result<usize> {
    let mut count = 0;

    if path.is_dir() {
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let entry_path = entry.path();

            if entry_path.is_file() {
                count += 1;
            } else if entry_path.is_dir() {
                count += count_files(&entry_path)?;
            }
        }
    }

    Ok(count)
}

/// Monitors a folder during processing to detect if new files are added
///
/// This function should be called during job processing to detect if new files
/// arrive while a job is running. If new files are detected, an error is returned
/// to signal that the job should be restarted.
///
/// # Arguments
/// * `folder_path` - The folder being processed
///
/// # Returns
/// Ok(()) if no new files detected, Err if new files arrive
///
/// # Note
/// This function is designed to be spawned as a background task and polled
/// periodically during processing.
pub async fn detect_new_files_during_processing(folder_path: &Path) -> Result<()> {
    // Get initial file count
    let initial_file_count = count_files(folder_path)?;

    debug!(
        path = %folder_path.display(),
        initial_files = initial_file_count,
        "Monitoring for new files during processing"
    );

    // Poll every 5 seconds
    let poll_interval = Duration::from_secs(5);

    loop {
        sleep(poll_interval).await;

        let current_file_count = count_files(folder_path)?;

        if current_file_count > initial_file_count {
            warn!(
                path = %folder_path.display(),
                initial_count = initial_file_count,
                current_count = current_file_count,
                "New files detected during processing"
            );
            anyhow::bail!(
                "New files added during processing. Job must restart. Initial: {}, Current: {}",
                initial_file_count,
                current_file_count
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::time::timeout;

    #[tokio::test]
    async fn test_stability_immediate_when_no_files() {
        let temp_dir = TempDir::new().unwrap();
        let folder_path = temp_dir.path();
        let cancel = CancellationToken::new();

        // Create an empty folder
        let test_folder = folder_path.join("test-folder");
        fs::create_dir(&test_folder).unwrap();

        // Should achieve stability quickly with a short timeout
        let result = timeout(
            Duration::from_secs(3),
            wait_for_stability(&test_folder, Duration::from_secs(1), cancel),
        )
        .await;

        assert!(result.is_ok(), "Timeout waiting for stability");
        assert!(result.unwrap().is_ok(), "Stability detection failed");
    }

    #[tokio::test]
    async fn test_stability_resets_on_new_file() {
        let temp_dir = TempDir::new().unwrap();
        let folder_path = temp_dir.path();
        let cancel = CancellationToken::new();

        // Create test folder
        let test_folder = folder_path.join("test-folder");
        fs::create_dir(&test_folder).unwrap();

        // Start stability detection with 2-second timeout
        let stability_task = {
            let test_folder = test_folder.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                wait_for_stability(&test_folder, Duration::from_secs(2), cancel).await
            })
        };

        // Wait 1 second, then add a file (should reset timer)
        sleep(Duration::from_millis(1000)).await;
        fs::write(test_folder.join("video1.mp4"), b"fake video").unwrap();

        // Should NOT be stable yet (timer was reset)
        // Wait another 1 second (total 2 seconds since start, but only 1 since file was added)
        sleep(Duration::from_millis(1000)).await;

        // Task should still be running (not stable yet)
        assert!(!stability_task.is_finished());

        // Wait another 1.5 seconds (now 2.5 seconds since file was added)
        sleep(Duration::from_millis(1500)).await;

        // Now it should be finished
        let result = timeout(Duration::from_millis(500), stability_task).await;
        assert!(result.is_ok(), "Task should have completed");
    }

    #[tokio::test]
    async fn test_count_files_recursive() {
        let temp_dir = TempDir::new().unwrap();
        let folder_path = temp_dir.path();

        // Create test structure
        fs::write(folder_path.join("file1.txt"), b"test").unwrap();
        fs::write(folder_path.join("file2.txt"), b"test").unwrap();

        let subdir = folder_path.join("subdir");
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("file3.txt"), b"test").unwrap();

        let count = count_files(folder_path).unwrap();
        assert_eq!(count, 3, "Should count all files recursively");
    }

    #[tokio::test]
    async fn test_detect_new_files_during_processing() {
        let temp_dir = TempDir::new().unwrap();
        let folder_path = temp_dir.path();

        // Create initial file
        fs::write(folder_path.join("initial.txt"), b"initial").unwrap();

        // Start monitoring
        let monitor_task = {
            let folder_path = folder_path.to_path_buf();
            tokio::spawn(async move { detect_new_files_during_processing(&folder_path).await })
        };

        // Add a new file after a short delay
        sleep(Duration::from_millis(100)).await;
        fs::write(folder_path.join("new.txt"), b"new file").unwrap();

        // Monitor should detect the new file and return an error
        let result = timeout(Duration::from_secs(10), monitor_task)
            .await
            .expect("Timeout waiting for detection")
            .expect("Task panicked");

        assert!(result.is_err(), "Should detect new files and return error");
        assert!(
            result.unwrap_err().to_string().contains("New files added"),
            "Error message should indicate new files"
        );
    }

    #[tokio::test]
    async fn test_stability_cancellation() {
        let temp_dir = TempDir::new().unwrap();
        let folder_path = temp_dir.path();
        let cancel = CancellationToken::new();

        let test_folder = folder_path.join("test-folder");
        fs::create_dir(&test_folder).unwrap();

        let stability_task = {
            let test_folder = test_folder.clone();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                wait_for_stability(&test_folder, Duration::from_secs(600), cancel).await
            })
        };

        // Let it start
        sleep(Duration::from_millis(200)).await;

        // Cancel
        cancel.cancel();

        // Should exit quickly despite 600s timeout
        let result = timeout(Duration::from_secs(2), stability_task)
            .await
            .expect("Timeout — cancellation didn't work")
            .expect("Task panicked");

        assert!(result.is_err());
    }
}
