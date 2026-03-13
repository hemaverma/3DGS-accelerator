//! Storage and disk space management
//!
//! This module provides utilities for monitoring disk space and ensuring
//! sufficient storage is available before and during processing operations.

pub mod space;

pub use space::{check_disk_space, DiskSpaceChecker, DiskSpaceStatus, SpaceRequirements};
