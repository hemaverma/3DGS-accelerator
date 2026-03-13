//! Export trained 3DGS models to various formats
//!
//! This module provides high-level export functions that handle:
//! - UTC timestamp-based filename generation
//! - Output directory management
//! - Coordination with backend export implementations
//!
//! # Supported Formats
//!
//! - `.ply` - Standard 3D point cloud format
//! - `.splat` - Web-optimized format for browser-based viewers

pub mod ply;
pub mod splat;

pub use ply::export_ply;
pub use splat::export_to_splat;
