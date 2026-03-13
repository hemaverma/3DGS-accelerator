//! Disk space monitoring and checking utilities
//!
//! This module provides functionality to check available disk space before
//! starting jobs and at key pipeline stages. It ensures minimum space requirements
//! are met and can pause/resume processing based on available space.

use anyhow::{Context, Result};
use std::path::Path;
use sysinfo::Disks;
use tracing::{info, instrument, warn};

/// Disk space status information
#[derive(Debug, Clone)]
pub struct DiskSpaceStatus {
    /// Total space on disk in bytes
    pub total_bytes: u64,
    /// Available space on disk in bytes
    pub available_bytes: u64,
    /// Used space on disk in bytes
    pub used_bytes: u64,
    /// Mount point of the disk
    pub mount_point: String,
}

impl DiskSpaceStatus {
    /// Get available space in GB
    pub fn available_gb(&self) -> f64 {
        self.available_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    }

    /// Get total space in GB
    pub fn total_gb(&self) -> f64 {
        self.total_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    }

    /// Get used space in GB
    pub fn used_gb(&self) -> f64 {
        self.used_bytes as f64 / 1024.0 / 1024.0 / 1024.0
    }

    /// Get percentage of space used
    pub fn used_percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.used_bytes as f64 / self.total_bytes as f64) * 100.0
        }
    }

    /// Get percentage of space available
    pub fn available_percentage(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            (self.available_bytes as f64 / self.total_bytes as f64) * 100.0
        }
    }

    /// Check if available space meets requirement
    pub fn meets_requirement(&self, required_gb: f64) -> bool {
        self.available_gb() >= required_gb
    }

    /// Check if we're at warning threshold (20% free space)
    pub fn is_at_warning_threshold(&self) -> bool {
        self.available_percentage() <= 20.0
    }

    /// Check if we're at critical threshold (10% free space)
    pub fn is_at_critical_threshold(&self) -> bool {
        self.available_percentage() <= 10.0
    }
}

/// Space requirements for different pipeline stages
#[derive(Debug, Clone)]
pub struct SpaceRequirements {
    /// Minimum space required to start a job (GB)
    pub job_start: f64,
    /// Minimum space required before COLMAP (GB)
    pub before_colmap: f64,
    /// Minimum space required before training (GB)
    pub before_training: f64,
}

impl Default for SpaceRequirements {
    fn default() -> Self {
        Self {
            job_start: 10.0,       // 10 GB minimum to start
            before_colmap: 10.0,   // 10 GB minimum for COLMAP
            before_training: 10.0, // 10 GB minimum for training
        }
    }
}

/// Disk space checker with configurable requirements
pub struct DiskSpaceChecker {
    /// Space requirements configuration
    requirements: SpaceRequirements,
}

impl DiskSpaceChecker {
    /// Create a new disk space checker with default requirements
    pub fn new() -> Self {
        Self {
            requirements: SpaceRequirements::default(),
        }
    }

    /// Create a new disk space checker with custom requirements
    pub fn with_requirements(requirements: SpaceRequirements) -> Self {
        Self { requirements }
    }

    /// Check disk space for a specific path
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check disk space for
    ///
    /// # Returns
    ///
    /// DiskSpaceStatus containing space information
    #[instrument(skip(self), fields(path = %path.display()))]
    pub fn check_space(&self, path: &Path) -> Result<DiskSpaceStatus> {
        let disks = Disks::new_with_refreshed_list();

        // Find the disk containing the path
        let disk = disks
            .iter()
            .filter(|d| path.starts_with(d.mount_point()))
            .max_by_key(|d| d.mount_point().as_os_str().len())
            .context("No disk found for path")?;

        let status = DiskSpaceStatus {
            total_bytes: disk.total_space(),
            available_bytes: disk.available_space(),
            used_bytes: disk.total_space() - disk.available_space(),
            mount_point: disk.mount_point().to_string_lossy().to_string(),
        };

        info!(
            path = %path.display(),
            mount_point = %status.mount_point,
            available_gb = format!("{:.2}", status.available_gb()),
            total_gb = format!("{:.2}", status.total_gb()),
            used_percentage = format!("{:.2}", status.used_percentage()),
            "Disk space checked"
        );

        Ok(status)
    }

    /// Check if there's sufficient space to start a job
    ///
    /// # Arguments
    ///
    /// * `path` - Path where job will be processed
    ///
    /// # Returns
    ///
    /// Ok if sufficient space, Err otherwise
    #[instrument(skip(self), fields(path = %path.display()))]
    pub fn check_job_start(&self, path: &Path) -> Result<()> {
        let status = self.check_space(path)?;

        if !status.meets_requirement(self.requirements.job_start) {
            warn!(
                available_gb = format!("{:.2}", status.available_gb()),
                required_gb = format!("{:.2}", self.requirements.job_start),
                "Insufficient disk space to start job"
            );
            anyhow::bail!(
                "Insufficient disk space: {:.2} GB available, {:.2} GB required",
                status.available_gb(),
                self.requirements.job_start
            );
        }

        info!(
            available_gb = format!("{:.2}", status.available_gb()),
            required_gb = format!("{:.2}", self.requirements.job_start),
            "Sufficient disk space to start job"
        );

        Ok(())
    }

    /// Check if there's sufficient space before COLMAP processing
    ///
    /// # Arguments
    ///
    /// * `path` - Path where COLMAP will run
    ///
    /// # Returns
    ///
    /// Ok if sufficient space, Err otherwise
    #[instrument(skip(self), fields(path = %path.display()))]
    pub fn check_before_colmap(&self, path: &Path) -> Result<()> {
        let status = self.check_space(path)?;

        if !status.meets_requirement(self.requirements.before_colmap) {
            warn!(
                available_gb = format!("{:.2}", status.available_gb()),
                required_gb = format!("{:.2}", self.requirements.before_colmap),
                "Insufficient disk space for COLMAP"
            );
            anyhow::bail!(
                "Insufficient disk space for COLMAP: {:.2} GB available, {:.2} GB required",
                status.available_gb(),
                self.requirements.before_colmap
            );
        }

        info!(
            available_gb = format!("{:.2}", status.available_gb()),
            required_gb = format!("{:.2}", self.requirements.before_colmap),
            "Sufficient disk space for COLMAP"
        );

        Ok(())
    }

    /// Check if there's sufficient space before 3DGS training
    ///
    /// # Arguments
    ///
    /// * `path` - Path where training will run
    ///
    /// # Returns
    ///
    /// Ok if sufficient space, Err otherwise
    #[instrument(skip(self), fields(path = %path.display()))]
    pub fn check_before_training(&self, path: &Path) -> Result<()> {
        let status = self.check_space(path)?;

        if !status.meets_requirement(self.requirements.before_training) {
            warn!(
                available_gb = format!("{:.2}", status.available_gb()),
                required_gb = format!("{:.2}", self.requirements.before_training),
                "Insufficient disk space for training"
            );
            anyhow::bail!(
                "Insufficient disk space for training: {:.2} GB available, {:.2} GB required",
                status.available_gb(),
                self.requirements.before_training
            );
        }

        info!(
            available_gb = format!("{:.2}", status.available_gb()),
            required_gb = format!("{:.2}", self.requirements.before_training),
            "Sufficient disk space for training"
        );

        Ok(())
    }

    /// Check disk space and log warnings if thresholds are exceeded
    ///
    /// This function checks disk space and logs warnings when:
    /// - Available space < 20% (warning threshold)
    /// - Available space < 10% (critical threshold)
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check disk space for
    ///
    /// # Returns
    ///
    /// Ok with DiskSpaceStatus if successful
    #[instrument(skip(self), fields(path = %path.display()))]
    pub fn check_and_warn_thresholds(&self, path: &Path) -> Result<DiskSpaceStatus> {
        let status = self.check_space(path)?;

        if status.is_at_critical_threshold() {
            warn!(
                available_percentage = format!("{:.2}%", status.available_percentage()),
                available_gb = format!("{:.2} GB", status.available_gb()),
                "CRITICAL: Less than 10% disk space remaining! Processing may pause."
            );
        } else if status.is_at_warning_threshold() {
            warn!(
                available_percentage = format!("{:.2}%", status.available_percentage()),
                available_gb = format!("{:.2} GB", status.available_gb()),
                "WARNING: Less than 20% disk space remaining. Consider cleanup."
            );
        }

        Ok(status)
    }

    /// Clean up oldest folders from processed/error directories to free space
    ///
    /// This function removes the oldest folders from the processed and error
    /// directories until sufficient space is available or there are no more
    /// folders to delete.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check disk space for
    /// * `processed_path` - Path to processed directory
    /// * `error_path` - Path to error directory
    /// * `target_free_gb` - Target amount of free space in GB
    ///
    /// # Returns
    ///
    /// Number of folders deleted
    ///
    /// # Behavior
    ///
    /// - Scans processed_path and error_path for folders
    /// - Sorts by modification time (oldest first)
    /// - Deletes folders one by one until target_free_gb is reached
    /// - Stops if no more folders to delete or target is reached
    #[instrument(skip(self), fields(
        path = %path.display(),
        processed_path = %processed_path.display(),
        error_path = %error_path.display(),
        target_free_gb = target_free_gb
    ))]
    pub async fn cleanup_for_space(
        &self,
        path: &Path,
        processed_path: &Path,
        error_path: &Path,
        target_free_gb: f64,
    ) -> Result<usize> {
        use tokio::fs;

        info!(
            target_free_gb = target_free_gb,
            "Starting automatic cleanup to free disk space"
        );

        let mut deleted_count = 0;

        // Collect all folders from both directories with their metadata
        let mut folders = Vec::new();

        // Collect from processed directory
        if processed_path.exists() {
            let mut entries = fs::read_dir(processed_path).await?;
            while let Some(entry) = entries.next_entry().await? {
                if let Ok(metadata) = entry.metadata().await {
                    if metadata.is_dir() {
                        if let Ok(modified) = metadata.modified() {
                            folders.push((entry.path(), modified));
                        }
                    }
                }
            }
        }

        // Collect from error directory
        if error_path.exists() {
            let mut entries = fs::read_dir(error_path).await?;
            while let Some(entry) = entries.next_entry().await? {
                if let Ok(metadata) = entry.metadata().await {
                    if metadata.is_dir() {
                        if let Ok(modified) = metadata.modified() {
                            folders.push((entry.path(), modified));
                        }
                    }
                }
            }
        }

        // Sort by modification time (oldest first)
        folders.sort_by_key(|(_, modified)| *modified);

        info!(
            total_folders = folders.len(),
            "Found {} folders for potential cleanup",
            folders.len()
        );

        // Delete folders until we reach target space
        for (folder_path, _) in folders {
            // Check current space
            let status = self.check_space(path)?;
            if status.available_gb() >= target_free_gb {
                info!(
                    available_gb = format!("{:.2}", status.available_gb()),
                    target_gb = target_free_gb,
                    deleted_count = deleted_count,
                    "Target space reached, stopping cleanup"
                );
                break;
            }

            // Delete the folder
            info!(
                folder = %folder_path.display(),
                "Deleting old folder to free space"
            );

            if let Err(e) = fs::remove_dir_all(&folder_path).await {
                warn!(
                    folder = %folder_path.display(),
                    error = %e,
                    "Failed to delete folder, continuing with next"
                );
            } else {
                deleted_count += 1;
                info!(
                    folder = %folder_path.display(),
                    deleted_count = deleted_count,
                    "Successfully deleted folder"
                );
            }
        }

        let final_status = self.check_space(path)?;
        info!(
            deleted_count = deleted_count,
            available_gb = format!("{:.2}", final_status.available_gb()),
            "Automatic cleanup complete"
        );

        Ok(deleted_count)
    }

    /// Check space and auto-cleanup if at critical threshold
    ///
    /// This is a convenience method that checks disk space and automatically
    /// triggers cleanup if available space is below 10% (critical threshold).
    /// Attempts to free space up to 20% available.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to check disk space for
    /// * `processed_path` - Path to processed directory
    /// * `error_path` - Path to error directory
    ///
    /// # Returns
    ///
    /// Ok with number of folders deleted (0 if no cleanup needed)
    #[instrument(skip(self), fields(
        path = %path.display(),
        processed_path = %processed_path.display(),
        error_path = %error_path.display()
    ))]
    pub async fn check_and_auto_cleanup(
        &self,
        path: &Path,
        processed_path: &Path,
        error_path: &Path,
    ) -> Result<usize> {
        let status = self.check_and_warn_thresholds(path)?;

        if status.is_at_critical_threshold() {
            warn!(
                available_percentage = format!("{:.2}%", status.available_percentage()),
                "Critical disk space threshold reached, triggering automatic cleanup"
            );

            // Calculate target: 20% of total disk space
            let target_free_gb = status.total_gb() * 0.20;

            self.cleanup_for_space(path, processed_path, error_path, target_free_gb)
                .await
        } else {
            Ok(0)
        }
    }

    /// Wait for sufficient disk space to become available
    ///
    /// This function checks disk space periodically and returns when sufficient
    /// space is available. Use this to pause processing when space is low.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to monitor
    /// * `required_gb` - Required space in GB
    /// * `check_interval_secs` - How often to check (in seconds)
    ///
    /// # Behavior
    ///
    /// Loops indefinitely, checking space every `check_interval_secs` seconds
    /// until sufficient space is available.
    #[instrument(skip(self), fields(path = %path.display()))]
    pub async fn wait_for_space(
        &self,
        path: &Path,
        required_gb: f64,
        check_interval_secs: u64,
    ) -> Result<()> {
        loop {
            let status = self.check_space(path)?;

            if status.meets_requirement(required_gb) {
                info!(
                    available_gb = format!("{:.2}", status.available_gb()),
                    required_gb = format!("{:.2}", required_gb),
                    "Sufficient disk space now available"
                );
                return Ok(());
            }

            warn!(
                available_gb = format!("{:.2}", status.available_gb()),
                required_gb = format!("{:.2}", required_gb),
                check_interval_secs = check_interval_secs,
                "Insufficient disk space, waiting for space to become available"
            );

            tokio::time::sleep(tokio::time::Duration::from_secs(check_interval_secs)).await;
        }
    }
}

impl Default for DiskSpaceChecker {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function to check disk space for a path
///
/// # Arguments
///
/// * `path` - Path to check disk space for
///
/// # Returns
///
/// DiskSpaceStatus containing space information
///
/// # Example
///
/// ```no_run
/// # use std::path::PathBuf;
/// # fn example() -> anyhow::Result<()> {
/// use three_dgs_processor::storage::check_disk_space;
///
/// let status = check_disk_space(&PathBuf::from("/data"))?;
/// println!("Available: {:.2} GB", status.available_gb());
/// # Ok(())
/// # }
/// ```
pub fn check_disk_space(path: &Path) -> Result<DiskSpaceStatus> {
    DiskSpaceChecker::new().check_space(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_disk_space_status_calculations() {
        let status = DiskSpaceStatus {
            total_bytes: 100 * 1024 * 1024 * 1024,    // 100 GB
            available_bytes: 50 * 1024 * 1024 * 1024, // 50 GB
            used_bytes: 50 * 1024 * 1024 * 1024,      // 50 GB
            mount_point: "/".to_string(),
        };

        assert!((status.total_gb() - 100.0).abs() < 1.0);
        assert!((status.available_gb() - 50.0).abs() < 1.0);
        assert!((status.used_gb() - 50.0).abs() < 1.0);
        assert!((status.used_percentage() - 50.0).abs() < 0.1);
        assert!((status.available_percentage() - 50.0).abs() < 0.1);
        assert!(status.meets_requirement(40.0));
        assert!(!status.meets_requirement(60.0));
        assert!(!status.is_at_warning_threshold()); // 50% available > 20%
        assert!(!status.is_at_critical_threshold()); // 50% available > 10%
    }

    #[test]
    fn test_disk_space_thresholds() {
        // Test warning threshold (20% available)
        let warning_status = DiskSpaceStatus {
            total_bytes: 100 * 1024 * 1024 * 1024,    // 100 GB
            available_bytes: 20 * 1024 * 1024 * 1024, // 20 GB (20%)
            used_bytes: 80 * 1024 * 1024 * 1024,      // 80 GB
            mount_point: "/".to_string(),
        };
        assert!(warning_status.is_at_warning_threshold());
        assert!(!warning_status.is_at_critical_threshold());

        // Test critical threshold (10% available)
        let critical_status = DiskSpaceStatus {
            total_bytes: 100 * 1024 * 1024 * 1024,    // 100 GB
            available_bytes: 10 * 1024 * 1024 * 1024, // 10 GB (10%)
            used_bytes: 90 * 1024 * 1024 * 1024,      // 90 GB
            mount_point: "/".to_string(),
        };
        assert!(critical_status.is_at_warning_threshold());
        assert!(critical_status.is_at_critical_threshold());

        // Test below critical threshold (5% available)
        let very_low_status = DiskSpaceStatus {
            total_bytes: 100 * 1024 * 1024 * 1024,   // 100 GB
            available_bytes: 5 * 1024 * 1024 * 1024, // 5 GB (5%)
            used_bytes: 95 * 1024 * 1024 * 1024,     // 95 GB
            mount_point: "/".to_string(),
        };
        assert!(very_low_status.is_at_warning_threshold());
        assert!(very_low_status.is_at_critical_threshold());

        // Test plenty of space (70% available)
        let good_status = DiskSpaceStatus {
            total_bytes: 100 * 1024 * 1024 * 1024,    // 100 GB
            available_bytes: 70 * 1024 * 1024 * 1024, // 70 GB (70%)
            used_bytes: 30 * 1024 * 1024 * 1024,      // 30 GB
            mount_point: "/".to_string(),
        };
        assert!(!good_status.is_at_warning_threshold());
        assert!(!good_status.is_at_critical_threshold());
    }

    #[test]
    fn test_space_requirements_default() {
        let reqs = SpaceRequirements::default();
        assert_eq!(reqs.job_start, 10.0);
        assert_eq!(reqs.before_colmap, 10.0);
        assert_eq!(reqs.before_training, 10.0);
    }

    #[test]
    fn test_disk_space_checker_creation() {
        let checker = DiskSpaceChecker::new();
        assert_eq!(checker.requirements.job_start, 10.0);

        let custom_reqs = SpaceRequirements {
            job_start: 20.0,
            before_colmap: 15.0,
            before_training: 15.0,
        };
        let custom_checker = DiskSpaceChecker::with_requirements(custom_reqs);
        assert_eq!(custom_checker.requirements.job_start, 20.0);
    }

    #[test]
    fn test_check_disk_space() {
        // Use current directory for test
        let current_dir = env::current_dir().unwrap();
        let result = check_disk_space(&current_dir);

        // Should succeed on any system with disk space
        assert!(result.is_ok());
        let status = result.unwrap();
        assert!(status.total_bytes > 0);
    }
}
