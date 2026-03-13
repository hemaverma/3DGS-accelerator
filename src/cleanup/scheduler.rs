//! Daily retention policy scheduler
//!
//! This module implements a background task that runs retention policy cleanup
//! daily at midnight UTC. It continues running until the service is shut down.

use chrono::{Duration, Timelike, Utc};
use std::path::PathBuf;
use tokio::time::{sleep, Duration as TokioDuration};
use tracing::{error, info, instrument};

use super::retention::{cleanup_old_folders, RetentionPolicy};

/// Retention policy scheduler
///
/// This struct manages the background task that runs retention policy cleanup
/// on a daily schedule.
pub struct RetentionScheduler {
    /// Path to the processed directory
    processed_path: PathBuf,
    /// Path to the error directory
    error_path: PathBuf,
    /// Retention policy to enforce
    policy: RetentionPolicy,
}

impl RetentionScheduler {
    /// Create a new retention scheduler
    ///
    /// # Arguments
    ///
    /// * `processed_path` - Path to the processed directory
    /// * `error_path` - Path to the error directory
    /// * `retention_days` - Number of days to retain folders
    pub fn new(processed_path: PathBuf, error_path: PathBuf, retention_days: u32) -> Self {
        Self {
            processed_path,
            error_path,
            policy: RetentionPolicy::new(retention_days),
        }
    }

    /// Start the retention scheduler as a background task
    ///
    /// This function spawns a Tokio task that runs cleanup daily at midnight UTC.
    /// The task continues until the runtime is shut down.
    ///
    /// # Returns
    ///
    /// A JoinHandle to the background task
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.run().await;
        })
    }

    /// Run the retention scheduler loop
    ///
    /// This is the main loop that calculates the next midnight UTC and waits
    /// until that time to run cleanup.
    #[instrument(skip(self), fields(
        processed_path = %self.processed_path.display(),
        error_path = %self.error_path.display(),
        retention_days = self.policy.retention_days
    ))]
    async fn run(self) {
        info!("Starting retention policy scheduler (runs daily at midnight UTC)");

        loop {
            // Calculate time until next midnight UTC
            let now = Utc::now();
            let next_midnight = (now + Duration::days(1))
                .with_hour(0)
                .and_then(|t| t.with_minute(0))
                .and_then(|t| t.with_second(0))
                .and_then(|t| t.with_nanosecond(0))
                .unwrap_or_else(|| now + Duration::days(1));

            let sleep_duration = (next_midnight - now)
                .to_std()
                .unwrap_or(TokioDuration::from_secs(86400)); // Default to 24h if calculation fails

            info!(
                next_run = %next_midnight.to_rfc3339(),
                sleep_hours = sleep_duration.as_secs() / 3600,
                "Scheduled next retention cleanup"
            );

            // Wait until midnight
            sleep(sleep_duration).await;

            // Run cleanup
            info!("Running scheduled retention policy cleanup");
            match cleanup_old_folders(&self.processed_path, &self.error_path, &self.policy).await {
                Ok(deleted_count) => {
                    info!(
                        deleted_count = deleted_count,
                        "Scheduled retention cleanup completed successfully"
                    );
                }
                Err(e) => {
                    error!(
                        error = %e,
                        "Scheduled retention cleanup failed"
                    );
                }
            }
        }
    }
}

/// Start the retention policy scheduler
///
/// This is a convenience function that creates and starts a RetentionScheduler
/// in a single call.
///
/// # Arguments
///
/// * `processed_path` - Path to the processed directory
/// * `error_path` - Path to the error directory
/// * `retention_days` - Number of days to retain folders
///
/// # Returns
///
/// A JoinHandle to the background task
///
/// # Example
///
/// ```no_run
/// # use std::path::PathBuf;
/// # async fn example() {
/// use three_dgs_processor::cleanup::start_retention_scheduler;
///
/// let processed = PathBuf::from("/processed");
/// let error = PathBuf::from("/error");
///
/// let handle = start_retention_scheduler(processed, error, 30);
///
/// // Scheduler runs in background...
/// // To wait for it (though it runs forever):
/// // handle.await.unwrap();
/// # }
/// ```
pub fn start_retention_scheduler(
    processed_path: PathBuf,
    error_path: PathBuf,
    retention_days: u32,
) -> tokio::task::JoinHandle<()> {
    let scheduler = RetentionScheduler::new(processed_path, error_path, retention_days);
    scheduler.start()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_creation() {
        let processed = PathBuf::from("/processed");
        let error = PathBuf::from("/error");
        let scheduler = RetentionScheduler::new(processed.clone(), error.clone(), 30);

        assert_eq!(scheduler.processed_path, processed);
        assert_eq!(scheduler.error_path, error);
        assert_eq!(scheduler.policy.retention_days, 30);
    }
}
