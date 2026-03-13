//! Abstract reconstruction backend system
//!
//! This module provides a pluggable reconstruction backend architecture,
//! allowing support for COLMAP, Nerfstudio, Polycam, and other SfM/reconstruction formats.

pub mod backend;
pub mod colmap_backend;
pub mod models;
pub mod nerfstudio;
pub mod precalibrated;
pub mod registry;

pub use backend::ReconstructionBackend;
pub use colmap_backend::ColmapBackend;
pub use models::{
    CameraIntrinsics, CameraPose, ImageRegistration, MatcherType, ReconstructionOutput,
    ReconstructionStats,
};
pub use registry::{get_reconstruction_backend, list_reconstruction_backends, BackendInfo};
