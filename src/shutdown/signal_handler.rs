//! Signal handling for graceful shutdown
//!
//! Handles SIGTERM and SIGINT signals, coordinating graceful shutdown
//! across the application.

use super::{ShutdownFlag, SHUTDOWN_TIMEOUT};
use anyhow::{Context, Result};
use tokio::signal;
use tokio::sync::Notify;
use tokio::time::timeout;
use tracing::{error, info, warn};

/// Coordinator for graceful shutdown
#[derive(Clone)]
pub struct ShutdownCoordinator {
    shutdown_flag: ShutdownFlag,
    shutdown_notify: Arc<Notify>,
}

use std::sync::Arc;

impl ShutdownCoordinator {
    /// Create a new shutdown coordinator
    pub fn new() -> Self {
        Self {
            shutdown_flag: ShutdownFlag::new(),
            shutdown_notify: Arc::new(Notify::new()),
        }
    }

    /// Get the shutdown flag for checking shutdown status
    pub fn shutdown_flag(&self) -> ShutdownFlag {
        self.shutdown_flag.clone()
    }

    /// Wait for shutdown signal (SIGTERM or SIGINT)
    pub async fn wait_for_shutdown_signal(&self) {
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("Failed to install CTRL+C signal handler");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM signal handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {
                info!("Received SIGINT (Ctrl+C), initiating graceful shutdown...");
            }
            _ = terminate => {
                info!("Received SIGTERM, initiating graceful shutdown...");
            }
        }

        // Set shutdown flag
        self.shutdown_flag.request_shutdown();

        // Notify waiting tasks
        self.shutdown_notify.notify_waiters();
    }

    /// Wait for shutdown to be requested
    pub async fn wait_for_shutdown(&self) {
        self.shutdown_notify.notified().await;
    }

    /// Perform graceful shutdown with timeout
    ///
    /// Waits for current operations to complete, with a maximum timeout.
    /// Returns Ok(()) if shutdown completed gracefully, Err if timeout exceeded.
    pub async fn shutdown_with_timeout<F, Fut>(&self, cleanup_fn: F) -> Result<()>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<()>>,
    {
        info!(
            timeout_secs = SHUTDOWN_TIMEOUT.as_secs(),
            "Starting graceful shutdown with timeout"
        );

        // Wait for cleanup with timeout
        match timeout(SHUTDOWN_TIMEOUT, cleanup_fn()).await {
            Ok(Ok(())) => {
                info!("Graceful shutdown completed successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                error!(error = %e, "Error during shutdown cleanup");
                Err(e).context("Cleanup failed during shutdown")
            }
            Err(_) => {
                warn!(
                    timeout_secs = SHUTDOWN_TIMEOUT.as_secs(),
                    "Shutdown timeout exceeded, forcing exit"
                );
                Err(anyhow::anyhow!(
                    "Graceful shutdown timeout after {} seconds",
                    SHUTDOWN_TIMEOUT.as_secs()
                ))
            }
        }
    }
}

impl Default for ShutdownCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

/// Wait for shutdown signal (SIGTERM or SIGINT)
///
/// This is a standalone function for simple use cases.
pub async fn wait_for_shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install SIGTERM signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            info!("Received SIGINT (Ctrl+C), initiating shutdown...");
        }
        _ = terminate => {
            info!("Received SIGTERM, initiating shutdown...");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, Duration};

    #[tokio::test]
    async fn test_shutdown_flag() {
        let flag = ShutdownFlag::new();
        assert!(!flag.is_shutdown_requested());

        flag.request_shutdown();
        assert!(flag.is_shutdown_requested());
    }

    #[tokio::test]
    async fn test_shutdown_coordinator() {
        let coordinator = ShutdownCoordinator::new();
        let flag = coordinator.shutdown_flag();

        assert!(!flag.is_shutdown_requested());

        // Request shutdown
        flag.request_shutdown();
        assert!(flag.is_shutdown_requested());
    }

    #[tokio::test]
    async fn test_shutdown_with_timeout_success() {
        let coordinator = ShutdownCoordinator::new();

        let result = coordinator
            .shutdown_with_timeout(|| async {
                // Simulate quick cleanup
                sleep(Duration::from_millis(100)).await;
                Ok(())
            })
            .await;

        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_shutdown_with_timeout_cleanup_error() {
        let coordinator = ShutdownCoordinator::new();

        let result = coordinator
            .shutdown_with_timeout(|| async { Err(anyhow::anyhow!("Cleanup failed")) })
            .await;

        assert!(result.is_err());
    }
}
