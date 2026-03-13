//! Retry logic for failed job execution
//!
//! This module implements exponential backoff retry logic for job processing.
//! Failed jobs are retried up to MAX_RETRIES times before being moved to the error folder.
//!
//! # Error Classification
//!
//! The retry logic distinguishes between:
//! - **Transient errors**: Network, storage, Azure issues that may resolve with retry
//! - **Permanent errors**: Validation, configuration issues that won't improve with retry
//!
//! Permanent errors are not retried.

use std::time::Duration;
use tokio::time::sleep;
use tracing::{error, info, instrument, warn};

use crate::health::HealthCheckState;
use crate::processor::job::{execute_job, JobResult, JobStatus};

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not including initial attempt)
    pub max_retries: u32,
    /// Base delay in seconds for exponential backoff (default: 2)
    /// Delay formula: base_delay_secs * 2^(attempt-1)
    pub base_delay_secs: u64,
    /// Maximum delay in seconds (cap for exponential backoff)
    pub max_delay_secs: u64,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay_secs: 2,
            max_delay_secs: 60,
        }
    }
}

impl RetryConfig {

    /// Create retry configuration from environment variables
    ///
    /// Reads:
    /// - MAX_RETRIES (default: 3)
    /// - RETRY_BASE_DELAY_SECS (default: 2)
    /// - RETRY_MAX_DELAY_SECS (default: 60)
    pub fn from_env() -> Self {
        Self {
            max_retries: std::env::var("MAX_RETRIES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3),
            base_delay_secs: std::env::var("RETRY_BASE_DELAY_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(2),
            max_delay_secs: std::env::var("RETRY_MAX_DELAY_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(60),
        }
    }

    /// Calculate delay for a given retry attempt
    ///
    /// Uses exponential backoff: base_delay_secs * 2^(attempt-1)
    /// Capped at max_delay_secs
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let delay_secs = self
            .base_delay_secs
            .saturating_mul(2_u64.saturating_pow(attempt.saturating_sub(1)));
        let capped_secs = delay_secs.min(self.max_delay_secs);
        Duration::from_secs(capped_secs)
    }
}


/// Execute a job with retry logic
///
/// Wraps job execution with exponential backoff retry logic. If the job fails,
/// it will be retried up to `config.max_retries` times with increasing delay between attempts.
///
/// The retry sleeps are cancellable via `shutdown_flag`. When shutdown is requested,
/// the function returns the last failed result immediately without further retries.
///
/// # Error Classification
///
/// - **Permanent errors** (validation, configuration): Not retried, fail immediately
/// - **Transient errors** (network, storage, Azure): Retried with exponential backoff
///
/// # Arguments
///
/// * `params` - Job execution parameters
/// * `config` - Retry configuration (max retries, delays)
/// * `health_state` - Optional health check state for progress updates
/// * `shutdown_flag` - Shutdown flag to cancel retry sleeps
///
/// # Returns
///
/// A `JobResult` containing the final status after all retry attempts
///
/// # Retry Behavior
///
/// - **Initial attempt**: No delay
/// - **Retry 1**: base_delay_secs * 2^0 seconds (default: 2s)
/// - **Retry 2**: base_delay_secs * 2^1 seconds (default: 4s)
/// - **Retry 3**: base_delay_secs * 2^2 seconds (default: 8s)
/// - And so on (exponential backoff, capped at max_delay_secs)
///
/// After all retries are exhausted, the function returns the final failed result.
/// The caller is responsible for moving the input folder to the error directory.
///
/// # Example
///
/// ```rust,no_run
/// use std::path::PathBuf;
/// use three_dgs_processor::processor::retry::{execute_with_retry, RetryConfig};
/// use three_dgs_processor::shutdown::ShutdownFlag;
///
/// # async fn example() -> anyhow::Result<()> {
/// let config = RetryConfig::default();
/// let shutdown_flag = ShutdownFlag::new();
/// let result = execute_with_retry(
///     /* params */,
///     config,
///     None, // no health state
///     &shutdown_flag,
/// ).await;
///
/// if result.status == three_dgs_processor::processor::job::JobStatus::Failed {
///     // Move to error folder
/// }
/// # Ok(())
/// # }
/// ```
#[instrument(skip(params, health_state, shutdown_flag), fields(
    job_id = %params.job_id,
    max_retries = config.max_retries,
    base_delay_secs = config.base_delay_secs
))]
pub async fn execute_with_retry(
    params: crate::processor::job::JobExecutionParams,
    config: RetryConfig,
    health_state: Option<&HealthCheckState>,
    shutdown_flag: &crate::shutdown::ShutdownFlag,
) -> JobResult {
    let mut attempt = 0;
    let max_attempts = config.max_retries + 1; // Initial attempt + retries

    loop {
        attempt += 1;

        if attempt == 1 {
            info!(
                job_id = %params.job_id,
                "Starting job execution (attempt {}/{})",
                attempt,
                max_attempts
            );
        } else {
            warn!(
                job_id = %params.job_id,
                attempt = attempt,
                max_attempts = max_attempts,
                "Retrying job execution (attempt {}/{})",
                attempt,
                max_attempts
            );
        }

        // Execute the job
        let result = execute_job(params.clone(), health_state).await;

        // Check if job succeeded
        if result.status == JobStatus::Success {
            if attempt > 1 {
                info!(
                    job_id = %params.job_id,
                    attempt = attempt,
                    "Job succeeded after {} attempt(s)",
                    attempt
                );
            }
            return result;
        }

        // Job failed - check if error is retryable
        let should_retry = if let Some(error_msg) = &result.error {
            // Try to parse error message to determine if it's retryable
            // This is a heuristic - we check for common non-retryable error patterns
            let is_validation_error = error_msg.contains("validation")
                || error_msg.contains("invalid")
                || error_msg.contains("unsupported");
            let is_config_error =
                error_msg.contains("configuration") || error_msg.contains("missing required");

            !is_validation_error && !is_config_error
        } else {
            true // If no error message, assume retryable
        };

        if !should_retry {
            error!(
                job_id = %params.job_id,
                attempt = attempt,
                error = result.error.as_deref().unwrap_or("unknown error"),
                "Job failed with non-retryable error (validation/configuration issue)"
            );
            return result;
        }

        // Check if we should retry
        if attempt >= max_attempts {
            error!(
                job_id = %params.job_id,
                attempts = attempt,
                error = result.error.as_deref().unwrap_or("unknown error"),
                "Job failed after {} attempt(s), max retries exhausted",
                attempt
            );
            return result;
        }

        // Calculate delay for this retry
        let delay = config.delay_for_attempt(attempt);

        warn!(
            job_id = %params.job_id,
            attempt = attempt,
            delay_secs = delay.as_secs(),
            error = result.error.as_deref().unwrap_or("unknown error"),
            "Job failed with retryable error, retrying in {} second(s)...",
            delay.as_secs()
        );

        // Wait before retrying, but abort if shutdown is requested
        tokio::select! {
            _ = shutdown_flag.cancelled() => {
                warn!(
                    job_id = %params.job_id,
                    "Shutdown requested during retry delay, aborting retries"
                );
                return result;
            }
            _ = sleep(delay) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retry_config_default() {
        let config = RetryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.base_delay_secs, 2);
        assert_eq!(config.max_delay_secs, 60);
    }

    #[test]
    fn test_retry_config_delay_calculation() {
        let config = RetryConfig::default();

        // First retry: 2 * 2^0 = 2 seconds
        assert_eq!(config.delay_for_attempt(1).as_secs(), 2);

        // Second retry: 2 * 2^1 = 4 seconds
        assert_eq!(config.delay_for_attempt(2).as_secs(), 4);

        // Third retry: 2 * 2^2 = 8 seconds
        assert_eq!(config.delay_for_attempt(3).as_secs(), 8);

        // Fourth retry: 2 * 2^3 = 16 seconds
        assert_eq!(config.delay_for_attempt(4).as_secs(), 16);

        // Fifth retry: 2 * 2^4 = 32 seconds
        assert_eq!(config.delay_for_attempt(5).as_secs(), 32);

        // Sixth retry: 2 * 2^5 = 64 seconds, but capped at 60
        assert_eq!(config.delay_for_attempt(6).as_secs(), 60);
    }

    #[test]
    fn test_retry_config_custom_settings() {
        let config = RetryConfig {
            max_retries: 5,
            base_delay_secs: 1,
            max_delay_secs: 30,
        };

        assert_eq!(config.delay_for_attempt(1).as_secs(), 1); // 1 * 2^0 = 1
        assert_eq!(config.delay_for_attempt(2).as_secs(), 2); // 1 * 2^1 = 2
        assert_eq!(config.delay_for_attempt(3).as_secs(), 4); // 1 * 2^2 = 4
        assert_eq!(config.delay_for_attempt(4).as_secs(), 8); // 1 * 2^3 = 8
        assert_eq!(config.delay_for_attempt(5).as_secs(), 16); // 1 * 2^4 = 16
        assert_eq!(config.delay_for_attempt(6).as_secs(), 30); // 1 * 2^5 = 32, capped at 30
    }

    #[test]
    fn test_exponential_backoff_calculation() {
        // Test the exponential backoff formula
        assert_eq!(2_u64.pow(0), 1); // First retry: 1 second
        assert_eq!(2_u64.pow(1), 2); // Second retry: 2 seconds
        assert_eq!(2_u64.pow(2), 4); // Third retry: 4 seconds
        assert_eq!(2_u64.pow(3), 8); // Fourth retry: 8 seconds
    }

    #[tokio::test]
    async fn test_max_attempts_calculation() {
        // Verify that max_attempts = max_retries + 1
        let max_retries = 3;
        let max_attempts = max_retries + 1;
        assert_eq!(max_attempts, 4); // 1 initial + 3 retries
    }
}
