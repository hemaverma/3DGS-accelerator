//! Temporary directory management with RAII cleanup
//!
//! This module provides automatic cleanup of temporary files using the Drop trait.
//! Temporary directories are created per-job and automatically removed when the
//! JobTempDir goes out of scope, ensuring no temp files are left behind even on
//! error paths.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tracing::{info, instrument, warn};

/// RAII wrapper for job-specific temporary directory
///
/// This struct creates a temporary directory for a job and automatically
/// cleans it up when dropped, regardless of whether the job succeeds or fails.
///
/// # Example
///
/// ```no_run
/// # use std::path::PathBuf;
/// # async fn example() -> anyhow::Result<()> {
/// let temp_root = PathBuf::from("/tmp/3dgs-work");
/// let job_temp = JobTempDir::new(&temp_root, "job_123")?;
///
/// // Use job_temp.path() for temporary operations
/// let work_file = job_temp.path().join("intermediate.dat");
///
/// // Automatic cleanup happens here when job_temp goes out of scope
/// # Ok(())
/// # }
/// ```
pub struct JobTempDir {
    /// The underlying TempDir handle (provides RAII cleanup)
    temp_dir: TempDir,
    /// Job identifier for logging
    job_id: String,
}

impl JobTempDir {
    /// Create a new job-specific temporary directory
    ///
    /// # Arguments
    ///
    /// * `temp_root` - Root path for temporary directories (from TEMP_PATH env var)
    /// * `job_id` - Unique identifier for this job
    ///
    /// # Returns
    ///
    /// A JobTempDir that will automatically clean up when dropped
    ///
    /// # Errors
    ///
    /// Returns an error if the temporary directory cannot be created
    #[instrument(skip(temp_root), fields(temp_root = %temp_root.display()))]
    pub fn new(temp_root: &Path, job_id: &str) -> Result<Self> {
        // Ensure temp root exists
        std::fs::create_dir_all(temp_root).with_context(|| {
            format!(
                "Failed to create temp root directory: {}",
                temp_root.display()
            )
        })?;

        // Create job-specific temporary directory
        let temp_dir = TempDir::new_in(temp_root).with_context(|| {
            format!(
                "Failed to create temporary directory in {}",
                temp_root.display()
            )
        })?;

        info!(
            job_id = %job_id,
            temp_path = %temp_dir.path().display(),
            "Created job temporary directory"
        );

        Ok(JobTempDir {
            temp_dir,
            job_id: job_id.to_string(),
        })
    }

    /// Get the path to the temporary directory
    ///
    /// This path is valid until the JobTempDir is dropped
    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Create a subdirectory within the job temporary directory
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the subdirectory to create
    ///
    /// # Returns
    ///
    /// The path to the created subdirectory
    ///
    /// # Errors
    ///
    /// Returns an error if the subdirectory cannot be created
    pub fn create_subdir(&self, name: &str) -> Result<PathBuf> {
        let subdir = self.path().join(name);
        std::fs::create_dir_all(&subdir)
            .with_context(|| format!("Failed to create subdirectory: {}", subdir.display()))?;
        Ok(subdir)
    }

    /// Explicitly clean up the temporary directory
    ///
    /// Normally cleanup happens automatically on Drop, but this method allows
    /// explicit early cleanup if desired. Consumes self since directory is removed.
    ///
    /// Note: This method takes ownership to ensure the JobTempDir cannot be used
    /// after cleanup. The TempDir's Drop implementation will handle the actual cleanup.
    #[instrument(skip(self))]
    pub fn into_path(self) -> PathBuf {
        let path = self.temp_dir.path().to_path_buf();
        info!(
            job_id = %self.job_id,
            temp_path = %path.display(),
            "Returning owned path, cleanup will happen on drop"
        );
        path
    }
}

impl Drop for JobTempDir {
    fn drop(&mut self) {
        // TempDir automatically cleans up in its own Drop implementation
        // We just log that it's happening
        info!(
            job_id = %self.job_id,
            temp_path = %self.temp_dir.path().display(),
            "JobTempDir dropped - automatic cleanup initiated"
        );
    }
}

/// Legacy helper for creating subdirectories in a temporary path
///
/// This function is provided for compatibility with existing code that
/// manually manages temporary directories. New code should use JobTempDir.
///
/// # Arguments
///
/// * `temp_path` - Base temporary directory path
/// * `subdir_name` - Name of the subdirectory to create
///
/// # Returns
///
/// Path to the created subdirectory
///
/// # Errors
///
/// Returns an error if the subdirectory cannot be created
pub fn create_temp_subdir(temp_path: &Path, subdir_name: &str) -> Result<PathBuf> {
    let subdir = temp_path.join(subdir_name);
    std::fs::create_dir_all(&subdir).with_context(|| {
        format!(
            "Failed to create temporary subdirectory: {}",
            subdir.display()
        )
    })?;
    Ok(subdir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_job_temp_dir_creation() {
        let temp_root = tempfile::tempdir().unwrap();
        let job_temp = JobTempDir::new(temp_root.path(), "test_job").unwrap();

        assert!(job_temp.path().exists());
        assert!(job_temp.path().starts_with(temp_root.path()));
    }

    #[test]
    fn test_job_temp_dir_cleanup_on_drop() {
        let temp_root = tempfile::tempdir().unwrap();
        let temp_path: PathBuf;

        {
            let job_temp = JobTempDir::new(temp_root.path(), "test_job").unwrap();
            temp_path = job_temp.path().to_path_buf();
            assert!(temp_path.exists());

            // Create a file in the temp directory
            fs::write(temp_path.join("test.txt"), "test").unwrap();
        }

        // After drop, temp directory should be cleaned up
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_create_subdir() {
        let temp_root = tempfile::tempdir().unwrap();
        let job_temp = JobTempDir::new(temp_root.path(), "test_job").unwrap();

        let subdir = job_temp.create_subdir("frames").unwrap();
        assert!(subdir.exists());
        assert_eq!(subdir, job_temp.path().join("frames"));
    }

    #[test]
    fn test_into_path() {
        let temp_root = tempfile::tempdir().unwrap();
        let job_temp = JobTempDir::new(temp_root.path(), "test_job").unwrap();
        let temp_path = job_temp.path().to_path_buf();

        assert!(temp_path.exists());

        // Get path ownership
        let _owned_path = job_temp.into_path();

        // Path still exists until drop (we don't actually have a way to test
        // after drop since we'd need to keep temp_root alive)
    }

    #[test]
    fn test_create_temp_subdir_helper() {
        let temp_root = tempfile::tempdir().unwrap();
        let subdir = create_temp_subdir(temp_root.path(), "colmap").unwrap();

        assert!(subdir.exists());
        assert_eq!(subdir, temp_root.path().join("colmap"));
    }
}
