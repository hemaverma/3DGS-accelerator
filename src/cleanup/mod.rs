//! Data retention and cleanup management
//!
//! This module implements automatic cleanup of old processed and error folders
//! based on configurable retention policies.

pub mod retention;
pub mod scheduler;

pub use retention::{cleanup_old_folders, RetentionPolicy};
pub use scheduler::{start_retention_scheduler, RetentionScheduler};
