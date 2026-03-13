//! Duplicate job detection
//!
//! Prevents reprocessing of jobs that have already been completed or failed.
//! Checks the processed/ and error/ directories to avoid duplicate work,
//! especially important after container restarts.

use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Result of duplicate detection check
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DuplicateStatus {
    /// Job is new and should be processed
    New,
    /// Job was already processed successfully
    AlreadyProcessed { processed_path: PathBuf },
    /// Job previously failed and is in error directory
    AlreadyFailed { error_path: PathBuf },
}

/// Duplicate job detector
#[derive(Clone)]
pub struct DuplicateDetector {
    processed_dir: PathBuf,
    error_dir: PathBuf,
}

impl DuplicateDetector {
    /// Create a new duplicate detector
    ///
    /// # Arguments
    /// * `processed_dir` - Path to the processed/ directory
    /// * `error_dir` - Path to the error/ directory
    pub fn new(processed_dir: PathBuf, error_dir: PathBuf) -> Self {
        Self {
            processed_dir,
            error_dir,
        }
    }

    /// Create a detector from environment configuration
    ///
    /// Uses OUTPUT_PATH environment variable as the base path.
    pub fn from_env() -> anyhow::Result<Self> {
        let output_path =
            std::env::var("OUTPUT_PATH").unwrap_or_else(|_| "/mnt/output".to_string());
        let output_path = PathBuf::from(output_path);

        Ok(Self {
            processed_dir: output_path.join("processed"),
            error_dir: output_path.join("error"),
        })
    }

    /// Check if a job is a duplicate
    ///
    /// # Arguments
    /// * `job_dir` - The input job directory to check
    ///
    /// # Returns
    /// * `DuplicateStatus` indicating whether the job is new, already processed, or already failed
    pub fn check(&self, job_dir: &Path) -> DuplicateStatus {
        let job_name = match job_dir.file_name().and_then(|n| n.to_str()) {
            Some(name) => name,
            None => {
                warn!(path = %job_dir.display(), "Invalid job directory name");
                return DuplicateStatus::New;
            }
        };

        debug!(job_name = job_name, "Checking for duplicate job");

        // Check processed directory
        let processed_path = self.processed_dir.join(job_name);
        if processed_path.exists() {
            info!(
                job_name = job_name,
                processed_path = %processed_path.display(),
                "Job already processed, skipping"
            );
            return DuplicateStatus::AlreadyProcessed { processed_path };
        }

        // Check error directory
        let error_path = self.error_dir.join(job_name);
        if error_path.exists() {
            info!(
                job_name = job_name,
                error_path = %error_path.display(),
                "Job previously failed, skipping"
            );
            return DuplicateStatus::AlreadyFailed { error_path };
        }

        debug!(job_name = job_name, "Job is new, ready for processing");
        DuplicateStatus::New
    }

    /// Batch check multiple job directories
    ///
    /// # Arguments
    /// * `job_dirs` - List of job directories to check
    ///
    /// # Returns
    /// * Vector of tuples (job_dir, status) for each input
    pub fn check_batch(&self, job_dirs: &[PathBuf]) -> Vec<(PathBuf, DuplicateStatus)> {
        job_dirs
            .iter()
            .map(|dir| {
                let status = self.check(dir);
                (dir.clone(), status)
            })
            .collect()
    }

    /// Filter out duplicate jobs from a list, returning only new jobs
    ///
    /// # Arguments
    /// * `job_dirs` - List of job directories to filter
    ///
    /// # Returns
    /// * Vector of job directories that are new (not duplicates)
    pub fn filter_new(&self, job_dirs: &[PathBuf]) -> Vec<PathBuf> {
        job_dirs
            .iter()
            .filter(|dir| matches!(self.check(dir), DuplicateStatus::New))
            .cloned()
            .collect()
    }

    /// Get the processed directory path
    pub fn processed_dir(&self) -> &Path {
        &self.processed_dir
    }

    /// Get the error directory path
    pub fn error_dir(&self) -> &Path {
        &self.error_dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_new_job() {
        let temp = TempDir::new().unwrap();
        let processed_dir = temp.path().join("processed");
        let error_dir = temp.path().join("error");
        fs::create_dir_all(&processed_dir).unwrap();
        fs::create_dir_all(&error_dir).unwrap();

        let detector = DuplicateDetector::new(processed_dir, error_dir);

        let job_dir = temp.path().join("input").join("new_job");
        fs::create_dir_all(&job_dir).unwrap();

        let status = detector.check(&job_dir);
        assert_eq!(status, DuplicateStatus::New);
    }

    #[test]
    fn test_already_processed() {
        let temp = TempDir::new().unwrap();
        let processed_dir = temp.path().join("processed");
        let error_dir = temp.path().join("error");
        fs::create_dir_all(&processed_dir).unwrap();
        fs::create_dir_all(&error_dir).unwrap();

        // Create a processed job
        let processed_job = processed_dir.join("test_job");
        fs::create_dir_all(&processed_job).unwrap();

        let detector = DuplicateDetector::new(processed_dir.clone(), error_dir);

        let job_dir = temp.path().join("input").join("test_job");
        fs::create_dir_all(&job_dir).unwrap();

        let status = detector.check(&job_dir);
        assert!(matches!(status, DuplicateStatus::AlreadyProcessed { .. }));

        if let DuplicateStatus::AlreadyProcessed { processed_path } = status {
            assert_eq!(processed_path, processed_job);
        }
    }

    #[test]
    fn test_already_failed() {
        let temp = TempDir::new().unwrap();
        let processed_dir = temp.path().join("processed");
        let error_dir = temp.path().join("error");
        fs::create_dir_all(&processed_dir).unwrap();
        fs::create_dir_all(&error_dir).unwrap();

        // Create a failed job
        let error_job = error_dir.join("failed_job");
        fs::create_dir_all(&error_job).unwrap();

        let detector = DuplicateDetector::new(processed_dir, error_dir.clone());

        let job_dir = temp.path().join("input").join("failed_job");
        fs::create_dir_all(&job_dir).unwrap();

        let status = detector.check(&job_dir);
        assert!(matches!(status, DuplicateStatus::AlreadyFailed { .. }));

        if let DuplicateStatus::AlreadyFailed { error_path } = status {
            assert_eq!(error_path, error_job);
        }
    }

    #[test]
    fn test_filter_new() {
        let temp = TempDir::new().unwrap();
        let processed_dir = temp.path().join("processed");
        let error_dir = temp.path().join("error");
        fs::create_dir_all(&processed_dir).unwrap();
        fs::create_dir_all(&error_dir).unwrap();

        // Create some existing jobs
        fs::create_dir_all(processed_dir.join("job1")).unwrap();
        fs::create_dir_all(error_dir.join("job2")).unwrap();

        let detector = DuplicateDetector::new(processed_dir, error_dir);

        let input_dir = temp.path().join("input");
        fs::create_dir_all(&input_dir).unwrap();

        let job_dirs = vec![
            input_dir.join("job1"), // Processed
            input_dir.join("job2"), // Error
            input_dir.join("job3"), // New
            input_dir.join("job4"), // New
        ];

        // Create the input directories
        for dir in &job_dirs {
            fs::create_dir_all(dir).unwrap();
        }

        let new_jobs = detector.filter_new(&job_dirs);
        assert_eq!(new_jobs.len(), 2);
        assert!(new_jobs.contains(&input_dir.join("job3")));
        assert!(new_jobs.contains(&input_dir.join("job4")));
    }

    #[test]
    fn test_check_batch() {
        let temp = TempDir::new().unwrap();
        let processed_dir = temp.path().join("processed");
        let error_dir = temp.path().join("error");
        fs::create_dir_all(&processed_dir).unwrap();
        fs::create_dir_all(&error_dir).unwrap();

        fs::create_dir_all(processed_dir.join("job1")).unwrap();

        let detector = DuplicateDetector::new(processed_dir, error_dir);

        let input_dir = temp.path().join("input");
        fs::create_dir_all(&input_dir).unwrap();

        let job_dirs = vec![input_dir.join("job1"), input_dir.join("job2")];

        for dir in &job_dirs {
            fs::create_dir_all(dir).unwrap();
        }

        let results = detector.check_batch(&job_dirs);
        assert_eq!(results.len(), 2);

        // First job should be already processed
        assert!(matches!(
            results[0].1,
            DuplicateStatus::AlreadyProcessed { .. }
        ));

        // Second job should be new
        assert_eq!(results[1].1, DuplicateStatus::New);
    }
}
