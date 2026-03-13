//! File watching module for detecting new video uploads
//!
//! Implements a hybrid watching approach:
//! - Uses inotify (via notify crate) for local filesystems
//! - Falls back to polling for network mounts and blobfuse2
//!
//! The watcher detects new folders in the INPUT_PATH and returns them
//! for processing.

mod notify_watcher;
mod poll_watcher;
mod stability;

pub use stability::{detect_new_files_during_processing, wait_for_stability};

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

/// Detects a new folder in the specified watch path
///
/// This function uses a hybrid approach:
/// - Attempts to use inotify-based watching for efficiency
/// - Falls back to polling if inotify fails (network mounts, blobfuse)
///
/// The operation is cancellable via the provided `CancellationToken`.
///
/// # Arguments
/// * `watch_path` - The directory to monitor for new folders
/// * `poll_interval` - Interval for polling fallback (if needed)
/// * `cancel` - Token that, when cancelled, aborts the detection immediately
///
/// # Returns
/// Path to the newly detected folder, or an error if cancelled/failed
pub async fn detect_new_folder(
    watch_path: &Path,
    poll_interval: Duration,
    cancel: CancellationToken,
) -> Result<PathBuf> {
    info!(path = %watch_path.display(), "Starting folder detection");

    // Verify watch path exists and is a directory
    if !watch_path.exists() {
        anyhow::bail!("Watch path does not exist: {}", watch_path.display());
    }
    if !watch_path.is_dir() {
        anyhow::bail!("Watch path is not a directory: {}", watch_path.display());
    }

    // Try inotify-based watcher first
    match notify_watcher::watch_for_new_folder(watch_path, cancel.clone()).await {
        Ok(folder) => {
            info!(folder = %folder.display(), "Folder detected via inotify");
            Ok(folder)
        }
        Err(e) => {
            // If cancelled, propagate immediately
            if cancel.is_cancelled() {
                anyhow::bail!("Folder detection cancelled during shutdown");
            }

            warn!(
                error = %e,
                "Inotify watcher failed, falling back to polling"
            );

            // Fall back to polling
            poll_watcher::watch_for_new_folder(watch_path, poll_interval, cancel)
                .await
                .context("Polling watcher also failed")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::time::{sleep, timeout};

    #[tokio::test]
    #[cfg_attr(target_os = "macos", ignore = "FSEvents timing issues in tests")]
    async fn test_detect_new_folder_inotify() {
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path();
        let cancel = CancellationToken::new();

        // Start detection in background
        let detection_task = {
            let watch_path = watch_path.to_path_buf();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                detect_new_folder(&watch_path, Duration::from_millis(100), cancel).await
            })
        };

        // Wait for watcher to initialize
        sleep(Duration::from_millis(100)).await;

        // Create a new folder
        let new_folder = watch_path.join("test-job-folder");
        fs::create_dir(&new_folder).unwrap();

        // Wait for detection
        let result = timeout(Duration::from_secs(5), detection_task)
            .await
            .expect("Timeout waiting for detection")
            .expect("Detection task failed");

        assert!(result.is_ok());
        let detected = result.unwrap();
        assert_eq!(detected, new_folder);
    }

    #[tokio::test]
    async fn test_detect_new_folder_validates_path() {
        let non_existent = PathBuf::from("/non/existent/path");
        let cancel = CancellationToken::new();
        let result = detect_new_folder(&non_existent, Duration::from_secs(1), cancel).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("does not exist"));
    }

    #[tokio::test]
    async fn test_detect_new_folder_validates_is_directory() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("not-a-dir.txt");
        fs::write(&file_path, "content").unwrap();

        let cancel = CancellationToken::new();
        let result = detect_new_folder(&file_path, Duration::from_secs(1), cancel).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not a directory"));
    }

    #[tokio::test]
    async fn test_detect_new_folder_cancellation() {
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path().to_path_buf();
        let cancel = CancellationToken::new();

        let detection_task = {
            let cancel = cancel.clone();
            let wp = watch_path.clone();
            tokio::spawn(async move {
                detect_new_folder(&wp, Duration::from_millis(100), cancel).await
            })
        };

        // Let it start
        sleep(Duration::from_millis(200)).await;

        // Cancel
        cancel.cancel();

        // Should return quickly with an error
        let result = timeout(Duration::from_secs(2), detection_task)
            .await
            .expect("Timeout — cancellation did not work")
            .expect("Task panicked");

        assert!(result.is_err());
    }
}
