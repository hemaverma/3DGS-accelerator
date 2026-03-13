//! Polling-based file system watcher for network mounts and blobfuse
//!
//! This watcher periodically checks for new folders using directory listing.
//! It's designed as a fallback for filesystems that don't support inotify
//! (network mounts, FUSE filesystems like blobfuse2).

use anyhow::{Context, Result};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info};

/// Polls a directory for new folders at a specified interval
///
/// # Arguments
/// * `path` - The directory to watch for new folder creation
/// * `poll_interval` - How often to check for new folders
/// * `cancel` - Token that aborts the polling when cancelled
///
/// # Returns
/// A future that resolves to the path of the first new folder detected
pub async fn watch_for_new_folder(
    path: &Path,
    poll_interval: Duration,
    cancel: CancellationToken,
) -> Result<PathBuf> {
    let watch_path = path.to_path_buf();
    info!(
        path = %watch_path.display(),
        interval_secs = poll_interval.as_secs(),
        "Starting polling watcher"
    );

    // Get initial set of folders
    let mut known_folders = list_folders(&watch_path).await?;
    debug!(count = known_folders.len(), "Initial folder count");

    loop {
        // Wait for the poll interval, but abort immediately on cancellation
        tokio::select! {
            _ = cancel.cancelled() => {
                info!("Polling watcher cancelled (shutdown)");
                anyhow::bail!("Polling watcher cancelled during shutdown");
            }
            _ = sleep(poll_interval) => {}
        }

        // Get current set of folders
        let current_folders = list_folders(&watch_path).await?;

        // Find new folders (in current but not in known)
        let new_folders: Vec<_> = current_folders
            .difference(&known_folders)
            .cloned()
            .collect();

        if !new_folders.is_empty() {
            // Return the first new folder found
            let new_folder = new_folders[0].clone();
            info!(
                folder = %new_folder.display(),
                total_new = new_folders.len(),
                "Detected new folder via polling"
            );
            return Ok(new_folder);
        }

        // Update known folders for next iteration
        known_folders = current_folders;
        debug!(count = known_folders.len(), "No new folders detected");
    }
}

/// Lists all folders (directories) in a given path
async fn list_folders(path: &Path) -> Result<HashSet<PathBuf>> {
    let path_buf = path.to_path_buf();

    // Run blocking I/O in a separate task
    tokio::task::spawn_blocking(move || {
        let mut folders = HashSet::new();

        let entries = std::fs::read_dir(&path_buf)
            .with_context(|| format!("Failed to read directory: {}", path_buf.display()))?;

        for entry in entries {
            let entry = entry.context("Failed to read directory entry")?;
            let path = entry.path();

            // Only include directories
            if path.is_dir() {
                folders.insert(path);
            }
        }

        Ok::<HashSet<PathBuf>, anyhow::Error>(folders)
    })
    .await
    .context("Blocking task panicked")?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;
    use tokio::time::{timeout, Duration};

    #[tokio::test]
    async fn test_detects_new_folder() {
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path();
        let cancel = CancellationToken::new();

        // Start polling watcher
        let watcher_task = {
            let watch_path = watch_path.to_path_buf();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                watch_for_new_folder(&watch_path, Duration::from_millis(100), cancel).await
            })
        };

        // Wait for first poll cycle
        sleep(Duration::from_millis(150)).await;

        // Create a new folder
        let new_folder = watch_path.join("test-folder");
        fs::create_dir(&new_folder).unwrap();

        // Wait for detection (should happen on next poll)
        let result = timeout(Duration::from_secs(5), watcher_task)
            .await
            .expect("Timeout waiting for folder detection")
            .expect("Watcher task failed");

        assert!(result.is_ok());
        let detected_path = result.unwrap();
        assert_eq!(detected_path, new_folder);
    }

    #[tokio::test]
    async fn test_ignores_files() {
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path();
        let cancel = CancellationToken::new();

        // Start polling watcher
        let watcher_task = {
            let watch_path = watch_path.to_path_buf();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                watch_for_new_folder(&watch_path, Duration::from_millis(100), cancel).await
            })
        };

        // Wait for first poll cycle
        sleep(Duration::from_millis(150)).await;

        // Create a file (should be ignored)
        fs::write(watch_path.join("test.txt"), "content").unwrap();

        // Wait for a poll cycle
        sleep(Duration::from_millis(150)).await;

        // Create a folder (should be detected)
        let new_folder = watch_path.join("test-folder");
        fs::create_dir(&new_folder).unwrap();

        // Wait for detection
        let result = timeout(Duration::from_secs(5), watcher_task)
            .await
            .expect("Timeout waiting for folder detection")
            .expect("Watcher task failed");

        assert!(result.is_ok());
        let detected_path = result.unwrap();
        assert_eq!(detected_path, new_folder);
    }

    #[tokio::test]
    async fn test_list_folders() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path();

        // Create some folders and files
        fs::create_dir(path.join("folder1")).unwrap();
        fs::create_dir(path.join("folder2")).unwrap();
        fs::write(path.join("file.txt"), "content").unwrap();

        let folders = list_folders(path).await.unwrap();

        assert_eq!(folders.len(), 2);
        assert!(folders.contains(&path.join("folder1")));
        assert!(folders.contains(&path.join("folder2")));
        assert!(!folders.contains(&path.join("file.txt")));
    }

    #[tokio::test]
    async fn test_detects_multiple_new_folders() {
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path();
        let cancel = CancellationToken::new();

        // Create an initial folder
        fs::create_dir(watch_path.join("existing")).unwrap();

        // Start polling watcher
        let watcher_task = {
            let watch_path = watch_path.to_path_buf();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                watch_for_new_folder(&watch_path, Duration::from_millis(100), cancel).await
            })
        };

        // Wait for first poll cycle (to capture existing folder)
        sleep(Duration::from_millis(150)).await;

        // Create two new folders
        let folder1 = watch_path.join("new-folder-1");
        let folder2 = watch_path.join("new-folder-2");
        fs::create_dir(&folder1).unwrap();
        fs::create_dir(&folder2).unwrap();

        // Wait for detection (should return first one found)
        let result = timeout(Duration::from_secs(5), watcher_task)
            .await
            .expect("Timeout waiting for folder detection")
            .expect("Watcher task failed");

        assert!(result.is_ok());
        let detected_path = result.unwrap();

        // Should detect one of the new folders (not the existing one)
        assert!(detected_path == folder1 || detected_path == folder2);
        assert_ne!(detected_path, watch_path.join("existing"));
    }

    #[tokio::test]
    async fn test_cancellation_stops_polling() {
        let temp_dir = TempDir::new().unwrap();
        let watch_path = temp_dir.path();
        let cancel = CancellationToken::new();

        let watcher_task = {
            let watch_path = watch_path.to_path_buf();
            let cancel = cancel.clone();
            tokio::spawn(async move {
                watch_for_new_folder(&watch_path, Duration::from_secs(60), cancel).await
            })
        };

        // Let the poll start
        sleep(Duration::from_millis(100)).await;

        // Cancel
        cancel.cancel();

        // Should exit quickly despite 60s poll interval
        let result = timeout(Duration::from_secs(2), watcher_task)
            .await
            .expect("Timeout — cancellation didn't work")
            .expect("Task panicked");

        assert!(result.is_err());
    }
}
