//! Input validation module
//!
//! This module provides validation for video files and other inputs
//! before processing begins.

pub mod video;

pub use video::{ValidationConfig, VideoValidator};
