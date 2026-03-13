//! 3DGS Video Processor
//!
//! A containerized Rust service that watches directories for video uploads,
//! extracts frames, processes them through COLMAP and 3D Gaussian Splatting backends,
//! and outputs .ply and .splat files.

pub mod azure;
pub mod backends;
pub mod cleanup;
pub mod colmap;
pub mod config;
pub mod error;
pub mod exporters;
pub mod extractors;
pub mod health;
pub mod logging;
pub mod manifest;
pub mod processor;
pub mod reconstruction;
pub mod shutdown;
pub mod storage;
pub mod validation;
pub mod watcher;

pub use config::Config;
pub use error::{ProcessorError, Result};
