//! Health status types

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Current state of the processor
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProcessorState {
    /// Watching for new input folders
    Watching,
    /// Currently processing a job
    Processing,
    /// Idle (not watching, not processing)
    Idle,
    /// Error state
    Error,
}

impl Default for ProcessorState {
    fn default() -> Self {
        Self::Watching
    }
}

/// Job progress information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobProgress {
    /// Job identifier
    pub job_id: String,
    /// Current processing stage
    pub stage: String,
    /// Progress percentage (0-100)
    pub progress_percentage: f64,
    /// Number of videos being processed
    #[serde(skip_serializing_if = "Option::is_none")]
    pub video_count: Option<usize>,
    /// Total frames extracted
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_frames: Option<usize>,
    /// Number of Gaussians in model
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gaussian_count: Option<usize>,
    /// Job start timestamp
    pub started_at: DateTime<Utc>,
}

/// Health status response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Current processor state
    pub state: ProcessorState,
    /// Last update timestamp
    pub last_update: DateTime<Utc>,
    /// Optional error message if in error state
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    /// Current job progress (if processing)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_job: Option<JobProgress>,
}

impl Default for HealthStatus {
    fn default() -> Self {
        Self {
            state: ProcessorState::Watching,
            last_update: Utc::now(),
            error_message: None,
            current_job: None,
        }
    }
}

impl HealthStatus {
    /// Create a new health status with watching state
    pub fn watching() -> Self {
        Self {
            state: ProcessorState::Watching,
            last_update: Utc::now(),
            error_message: None,
            current_job: None,
        }
    }

    /// Create a new health status with processing state
    pub fn processing() -> Self {
        Self {
            state: ProcessorState::Processing,
            last_update: Utc::now(),
            error_message: None,
            current_job: None,
        }
    }

    /// Create a new health status with processing state and job info
    pub fn processing_with_job(job_progress: JobProgress) -> Self {
        Self {
            state: ProcessorState::Processing,
            last_update: Utc::now(),
            error_message: None,
            current_job: Some(job_progress),
        }
    }

    /// Create a new health status with idle state
    pub fn idle() -> Self {
        Self {
            state: ProcessorState::Idle,
            last_update: Utc::now(),
            error_message: None,
            current_job: None,
        }
    }

    /// Create a new health status with error state
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            state: ProcessorState::Error,
            last_update: Utc::now(),
            error_message: Some(message.into()),
            current_job: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_processor_state_default() {
        let state = ProcessorState::default();
        assert_eq!(state, ProcessorState::Watching);
    }

    #[test]
    fn test_health_status_default() {
        let status = HealthStatus::default();
        assert_eq!(status.state, ProcessorState::Watching);
        assert!(status.error_message.is_none());
    }

    #[test]
    fn test_health_status_constructors() {
        let watching = HealthStatus::watching();
        assert_eq!(watching.state, ProcessorState::Watching);
        assert!(watching.error_message.is_none());

        let processing = HealthStatus::processing();
        assert_eq!(processing.state, ProcessorState::Processing);
        assert!(processing.error_message.is_none());

        let idle = HealthStatus::idle();
        assert_eq!(idle.state, ProcessorState::Idle);
        assert!(idle.error_message.is_none());

        let error = HealthStatus::error("Test error");
        assert_eq!(error.state, ProcessorState::Error);
        assert_eq!(error.error_message, Some("Test error".to_string()));
    }

    #[test]
    fn test_health_status_serialization() {
        let status = HealthStatus::watching();
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"state\":\"watching\""));

        let error = HealthStatus::error("Test error");
        let json = serde_json::to_string(&error).unwrap();
        assert!(json.contains("\"state\":\"error\""));
        assert!(json.contains("\"error_message\":\"Test error\""));
    }
}
