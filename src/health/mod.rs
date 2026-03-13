//! Health check HTTP endpoint
//!
//! Provides an optional HTTP /health endpoint for container orchestrators.
//! Disabled by default, enabled via HEALTH_CHECK_ENABLED environment variable.
//! Includes job progress tracking when processing.

pub mod status;

pub use status::{HealthStatus, JobProgress, ProcessorState};

use crate::config::Config;
use anyhow::{Context, Result};
use axum::{
    extract::State as AxumState, http::StatusCode, response::IntoResponse, routing::get, Json,
    Router,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

/// Health check server state
#[derive(Clone)]
pub struct HealthCheckState {
    status: Arc<RwLock<HealthStatus>>,
}

impl HealthCheckState {
    /// Create a new health check state
    pub fn new() -> Self {
        Self {
            status: Arc::new(RwLock::new(HealthStatus::default())),
        }
    }

    /// Update the processor state
    pub async fn update_state(&self, state: ProcessorState) {
        let mut status = self.status.write().await;
        status.state = state;
        status.last_update = chrono::Utc::now();
        // Clear job progress when not processing
        if !matches!(state, ProcessorState::Processing) {
            status.current_job = None;
        }
    }

    /// Update job progress
    pub async fn update_job_progress(&self, job_progress: JobProgress) {
        let mut status = self.status.write().await;
        status.state = ProcessorState::Processing;
        status.current_job = Some(job_progress);
        status.last_update = chrono::Utc::now();
    }

    /// Set error message
    pub async fn set_error(&self, message: String) {
        let mut status = self.status.write().await;
        status.state = ProcessorState::Error;
        status.error_message = Some(message);
        status.last_update = chrono::Utc::now();
    }

    /// Clear error message
    pub async fn clear_error(&self) {
        let mut status = self.status.write().await;
        status.error_message = None;
    }

    /// Get current health status
    pub async fn get_status(&self) -> HealthStatus {
        self.status.read().await.clone()
    }
}

impl Default for HealthCheckState {
    fn default() -> Self {
        Self::new()
    }
}

/// Health check endpoint handler
async fn health_handler(AxumState(state): AxumState<HealthCheckState>) -> impl IntoResponse {
    let status = state.get_status().await;

    // Return 200 OK unless in error state
    let status_code = if matches!(status.state, ProcessorState::Error) {
        StatusCode::SERVICE_UNAVAILABLE
    } else {
        StatusCode::OK
    };

    (status_code, Json(status))
}

/// Start the health check HTTP server
///
/// Returns the server handle and the shared state for updating health status.
pub async fn start_health_server(
    _config: &Config,
) -> Result<(tokio::task::JoinHandle<Result<()>>, HealthCheckState)> {
    // Check if health check is enabled
    let enabled = std::env::var("HEALTH_CHECK_ENABLED")
        .unwrap_or_else(|_| "false".to_string())
        .parse::<bool>()
        .unwrap_or(false);

    if !enabled {
        info!("Health check endpoint is disabled (set HEALTH_CHECK_ENABLED=true to enable)");
        // Return a dummy handle and state
        let state = HealthCheckState::new();
        let handle = tokio::spawn(async { Ok(()) });
        return Ok((handle, state));
    }

    // Get port from environment or use default
    let port = std::env::var("HEALTH_CHECK_PORT")
        .unwrap_or_else(|_| "8080".to_string())
        .parse::<u16>()
        .context("Invalid HEALTH_CHECK_PORT")?;

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    let state = HealthCheckState::new();
    let app = Router::new()
        .route("/health", get(health_handler))
        .with_state(state.clone());

    info!(port = port, "Starting health check HTTP server");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("Failed to bind health check server")?;

    let handle = tokio::spawn(async move {
        axum::serve(listener, app)
            .await
            .context("Health check server failed")?;
        Ok(())
    });

    Ok((handle, state))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_health_check_state() {
        let state = HealthCheckState::new();

        // Default state should be watching
        let status = state.get_status().await;
        assert!(matches!(status.state, ProcessorState::Watching));
        assert!(status.error_message.is_none());

        // Update to processing
        state.update_state(ProcessorState::Processing).await;
        let status = state.get_status().await;
        assert!(matches!(status.state, ProcessorState::Processing));

        // Set error
        state.set_error("Test error".to_string()).await;
        let status = state.get_status().await;
        assert!(matches!(status.state, ProcessorState::Error));
        assert_eq!(status.error_message, Some("Test error".to_string()));

        // Clear error
        state.clear_error().await;
        let status = state.get_status().await;
        assert!(status.error_message.is_none());
    }
}
