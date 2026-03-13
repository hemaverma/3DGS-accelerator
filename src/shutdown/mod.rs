//! Graceful shutdown coordination
//!
//! Provides graceful shutdown handling with:
//! - SIGTERM/SIGINT signal handling
//! - Current stage completion
//! - 30-second maximum shutdown timeout
//! - Cleanup coordination

pub mod signal_handler;

pub use signal_handler::{wait_for_shutdown_signal, ShutdownCoordinator};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Maximum time to wait for graceful shutdown before forceful exit
pub const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(30);

/// Global shutdown flag that can be checked by processing stages.
///
/// Includes both a synchronous `AtomicBool` for cheap polling and a
/// `CancellationToken` that async code can `.cancelled().await` on.
#[derive(Clone)]
pub struct ShutdownFlag {
    inner: Arc<AtomicBool>,
    token: CancellationToken,
}

impl ShutdownFlag {
    /// Create a new shutdown flag
    pub fn new() -> Self {
        Self {
            inner: Arc::new(AtomicBool::new(false)),
            token: CancellationToken::new(),
        }
    }

    /// Check if shutdown has been requested (non-blocking)
    pub fn is_shutdown_requested(&self) -> bool {
        self.inner.load(Ordering::Relaxed)
    }

    /// Request shutdown — sets the flag and cancels the token
    pub fn request_shutdown(&self) {
        self.inner.store(true, Ordering::Relaxed);
        self.token.cancel();
    }

    /// Returns a future that completes when shutdown is requested.
    /// Use with `tokio::select!` to make blocking operations cancellable.
    pub async fn cancelled(&self) {
        self.token.cancelled().await;
    }

    /// Returns a clone of the inner `CancellationToken` for passing
    /// to functions that accept one directly.
    pub fn cancellation_token(&self) -> CancellationToken {
        self.token.clone()
    }
}

impl Default for ShutdownFlag {
    fn default() -> Self {
        Self::new()
    }
}
