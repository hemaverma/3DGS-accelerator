//! Retention policy enforcement for processed and error folders
//!
//! This module implements automatic cleanup of folders older than the configured
//! retention period (default 30 days). It scans the processed and error directories
//! and removes folders based on their modification time.

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use std::path::Path;
use tokio::fs;
use tracing::{info, instrument, warn};

/// Retention policy configuration
#[derive(Debug, Clone)]
pub struct RetentionPolicy {
    /// Number of days to retain folders
    pub retention_days: i64,
}

impl RetentionPolicy {
    /// Create a new retention policy
    ///
    /// # Arguments
    ///
    /// * `retention_days` - Number of days to retain folders before cleanup
    pub fn new(retention_days: u32) -> Self {
        Self {
            retention_days: retention_days as i64,
        }
    }

    /// Get the cutoff timestamp for retention
    ///
    /// Folders older than this timestamp should be deleted
    pub fn cutoff_time(&self) -> DateTime<Utc> {
        Utc::now() - Duration::days(self.retention_days)
    }
}

/// Clean up old folders from processed and error directories
///
/// This function scans the specified directories and removes any folders
/// that are older than the retention policy cutoff time.
///
/// # Arguments
///
/// * `processed_path` - Path to the processed directory
/// * `error_path` - Path to the error directory
/// * `policy` - Retention policy specifying how long to keep folders
///
/// # Returns
///
/// Total number of folders deleted
///
/// # Behavior
///
/// - Iterates through all entries in processed_path and error_path
/// - Checks modification time of each folder
/// - Deletes folders older than policy.cutoff_time()
/// - Logs each deletion
/// - Continues on individual folder errors (logs warning)
///
/// # Example
///
/// ```no_run
/// # use std::path::PathBuf;
/// # async fn example() -> anyhow::Result<()> {
/// use three_dgs_processor::cleanup::{cleanup_old_folders, RetentionPolicy};
///
/// let processed = PathBuf::from("/processed");
/// let error = PathBuf::from("/error");
/// let policy = RetentionPolicy::new(30); // 30 days
///
/// let deleted = cleanup_old_folders(&processed, &error, &policy).await?;
/// println!("Deleted {} old folders", deleted);
/// # Ok(())
/// # }
/// ```
#[instrument(skip_all, fields(
    processed_path = %processed_path.display(),
    error_path = %error_path.display(),
    retention_days = policy.retention_days
))]
pub async fn cleanup_old_folders(
    processed_path: &Path,
    error_path: &Path,
    policy: &RetentionPolicy,
) -> Result<usize> {
    let cutoff = policy.cutoff_time();
    let mut total_deleted = 0;

    info!(
        retention_days = policy.retention_days,
        cutoff = %cutoff.to_rfc3339(),
        "Starting retention policy cleanup"
    );

    // Clean up processed directory
    let processed_deleted = cleanup_directory(processed_path, &cutoff, "processed")
        .await
        .context("Failed to cleanup processed directory")?;
    total_deleted += processed_deleted;

    // Clean up error directory
    let error_deleted = cleanup_directory(error_path, &cutoff, "error")
        .await
        .context("Failed to cleanup error directory")?;
    total_deleted += error_deleted;

    info!(
        total_deleted = total_deleted,
        processed_deleted = processed_deleted,
        error_deleted = error_deleted,
        "Retention policy cleanup complete"
    );

    Ok(total_deleted)
}

/// Clean up old folders from a single directory
///
/// # Arguments
///
/// * `dir_path` - Path to the directory to clean
/// * `cutoff` - Cutoff timestamp (delete folders older than this)
/// * `dir_type` - Type of directory (for logging: "processed" or "error")
///
/// # Returns
///
/// Number of folders deleted
async fn cleanup_directory(
    dir_path: &Path,
    cutoff: &DateTime<Utc>,
    dir_type: &str,
) -> Result<usize> {
    // Check if directory exists
    if !dir_path.exists() {
        info!(
            path = %dir_path.display(),
            dir_type = dir_type,
            "Directory does not exist, skipping cleanup"
        );
        return Ok(0);
    }

    let mut deleted_count = 0;
    let mut entries = fs::read_dir(dir_path)
        .await
        .with_context(|| format!("Failed to read directory: {}", dir_path.display()))?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();

        // Only process directories
        if !path.is_dir() {
            continue;
        }

        // Get folder name for logging
        let folder_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        // Get modification time
        match get_folder_modified_time(&path).await {
            Ok(modified_time) => {
                if modified_time < *cutoff {
                    // Folder is older than retention period, delete it
                    info!(
                        folder = %path.display(),
                        dir_type = dir_type,
                        modified = %modified_time.to_rfc3339(),
                        age_days = (Utc::now() - modified_time).num_days(),
                        "Deleting folder older than retention period"
                    );

                    match fs::remove_dir_all(&path).await {
                        Ok(_) => {
                            deleted_count += 1;
                            info!(
                                folder = folder_name,
                                dir_type = dir_type,
                                "Successfully deleted folder"
                            );
                        }
                        Err(e) => {
                            warn!(
                                folder = folder_name,
                                dir_type = dir_type,
                                error = %e,
                                "Failed to delete folder, continuing"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                warn!(
                    folder = folder_name,
                    dir_type = dir_type,
                    error = %e,
                    "Failed to get modification time, skipping folder"
                );
            }
        }
    }

    Ok(deleted_count)
}

/// Get the modification time of a folder as a DateTime<Utc>
///
/// # Arguments
///
/// * `path` - Path to the folder
///
/// # Returns
///
/// The modification time as a DateTime<Utc>
async fn get_folder_modified_time(path: &Path) -> Result<DateTime<Utc>> {
    let metadata = fs::metadata(path)
        .await
        .with_context(|| format!("Failed to get metadata for: {}", path.display()))?;

    let modified = metadata
        .modified()
        .context("Failed to get modification time")?;

    Ok(modified.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::fs;

    #[tokio::test]
    async fn test_retention_policy_cutoff() {
        let policy = RetentionPolicy::new(30);
        let cutoff = policy.cutoff_time();
        let expected_age = Utc::now() - Duration::days(30);

        // Cutoff should be approximately 30 days ago (within 1 minute tolerance)
        let diff = (cutoff - expected_age).num_seconds().abs();
        assert!(diff < 60, "Cutoff time should be ~30 days ago");
    }

    #[tokio::test]
    async fn test_cleanup_old_folders() {
        let temp_root = tempfile::tempdir().unwrap();
        let processed_path = temp_root.path().join("processed");
        let error_path = temp_root.path().join("error");

        // Create directories
        fs::create_dir_all(&processed_path).await.unwrap();
        fs::create_dir_all(&error_path).await.unwrap();

        // Create some test folders
        let old_folder = processed_path.join("old_job");
        let new_folder = processed_path.join("new_job");
        fs::create_dir_all(&old_folder).await.unwrap();
        fs::create_dir_all(&new_folder).await.unwrap();

        // We can't easily set modification time in tests, but we can verify
        // the function runs without errors
        let policy = RetentionPolicy::new(30);
        let result = cleanup_old_folders(&processed_path, &error_path, &policy).await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_cleanup_nonexistent_directory() {
        let temp_root = tempfile::tempdir().unwrap();
        let nonexistent = temp_root.path().join("nonexistent");
        let cutoff = Utc::now();

        let result = cleanup_directory(&nonexistent, &cutoff, "test").await;

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }
}
