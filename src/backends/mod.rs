//! Backend trait and plugin system for 3DGS processing
//!
//! This module defines the backend abstraction layer for swappable 3D Gaussian Splatting
//! implementations. Backends can be loaded as dynamic libraries (plugins) at runtime.
//!
//! # Architecture
//!
//! - [`GaussianSplatBackend`] - Core trait that all backends must implement
//! - [`BackendOutput`] - Standardized output from training operations
//! - [`TrainingConfig`] - Configuration parameters for training
//!
//! # Supported Backends
//!
//! - `gaussian-splatting` - Original implementation (reference)
//! - `gsplat` - Optimized Python/CUDA implementation
//! - `3dgs-cpp` - C++ implementation for production deployments
//!
//! # Plugin Loading
//!
//! Backends are loaded dynamically based on the `BACKEND` environment variable or
//! `backend` field in the YAML config. Each backend is compiled as a shared library
//! (`.so` on Linux, `.dylib` on macOS) and loaded at runtime.
//!
//! # Example
//!
//! ```no_run
//! use three_dgs_processor::backends::{GaussianSplatBackend, TrainingConfig};
//! use std::path::PathBuf;
//!
//! async fn train_model(backend: &dyn GaussianSplatBackend) -> anyhow::Result<()> {
//!     let frames = vec![
//!         PathBuf::from("frames/frame_000000.jpg"),
//!         PathBuf::from("frames/frame_000001.jpg"),
//!     ];
//!     let config = TrainingConfig::default();
//!     
//!     let output = backend.train(&frames, &config).await?;
//!     backend.export_ply(&output, &PathBuf::from("output/model.ply")).await?;
//!     backend.export_splat(&output, &PathBuf::from("output/model.splat")).await?;
//!     
//!     Ok(())
//! }
//! ```

pub mod gaussian_splatting;
pub mod gpu_detect;
pub mod gsplat;
pub mod mock;
pub mod models;
pub mod plugin_loader;
pub mod registry;
pub mod three_dgs_cpp;

pub use gaussian_splatting::GaussianSplattingBackend;
pub use gpu_detect::{detect_gpu, gpu_status_string, GpuInfo, GpuPlatform};
pub use gsplat::GsplatBackend;
pub use mock::MockBackend;
pub use models::{BackendMetadata, BackendOutput, TrainingConfig};
pub use plugin_loader::PluginLoader;
pub use registry::BackendRegistry;
pub use three_dgs_cpp::ThreeDGSCppBackend;

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

/// Core trait for 3D Gaussian Splatting backend implementations
///
/// This trait defines the interface that all 3DGS backends must implement.
/// Backends are responsible for:
/// 1. Training a 3D Gaussian Splatting model from input frames
/// 2. Exporting the trained model to standard formats (.ply, .splat)
///
/// # Thread Safety
///
/// Implementations must be `Send + Sync` to support concurrent processing
/// in the async runtime.
///
/// # Error Handling
///
/// All methods return `Result<T>` to propagate errors. Backends should provide
/// detailed error context using `anyhow::Context`.
///
/// # Training Process
///
/// The typical training workflow:
/// 1. Call `train()` with extracted frames and configuration
/// 2. Backend performs camera pose estimation (e.g., via COLMAP)
/// 3. Backend trains Gaussian primitives via iterative optimization
/// 4. Returns `BackendOutput` containing model checkpoint and metadata
///
/// # Export Formats
///
/// - **PLY**: Standard 3D point cloud format, widely supported by viewers
/// - **SPLAT**: Web-optimized format for real-time browser rendering
#[async_trait]
pub trait GaussianSplatBackend: Send + Sync {
    /// Train a 3D Gaussian Splatting model from input frames
    ///
    /// # Arguments
    ///
    /// * `frames` - Slice of paths to extracted frame images (JPEG/PNG)
    ///              Frames should be ordered temporally if from video
    /// * `config` - Training hyperparameters (iterations, learning rate, etc.)
    ///
    /// # Returns
    ///
    /// A `BackendOutput` containing the trained model checkpoint path and metadata
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Frame files don't exist or are unreadable
    /// - Insufficient frames for reconstruction (minimum ~20 recommended)
    /// - Camera pose estimation fails
    /// - Training diverges or fails to converge
    /// - Insufficient GPU memory (if backend requires GPU)
    ///
    /// # Implementation Notes
    ///
    /// - This method may be long-running (minutes to hours depending on config)
    /// - Should be wrapped in `tokio::task::spawn_blocking` if backend is synchronous
    /// - Progress updates should be logged using `tracing` macros
    /// - Checkpoints should be saved to a temporary directory, not directly to output
    async fn train(&self, frames: &[&Path], config: &TrainingConfig) -> Result<BackendOutput>;

    /// Export a trained model to PLY format
    ///
    /// PLY (Polygon File Format) is a standard format for storing 3D point clouds.
    /// Each Gaussian primitive is typically represented as a point with additional
    /// attributes (covariance, spherical harmonics coefficients, opacity).
    ///
    /// # Arguments
    ///
    /// * `model` - The trained model output from `train()`
    /// * `output_path` - Destination path for the .ply file
    ///                   Parent directory must exist
    ///
    /// # Returns
    ///
    /// Success if export completes, error otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Model checkpoint file is missing or corrupted
    /// - Output path is not writable
    /// - Model format is incompatible with PLY export
    ///
    /// # File Format
    ///
    /// Output PLY files should include:
    /// - Position (x, y, z) as float32/float64
    /// - Normal vectors (if available)
    /// - Color (RGB or RGBA)
    /// - Additional custom properties for Gaussian-specific attributes
    async fn export_ply(&self, model: &BackendOutput, output_path: &Path) -> Result<()>;

    /// Export a trained model to SPLAT format
    ///
    /// SPLAT is a compact binary format optimized for real-time rendering in
    /// web browsers. It typically stores Gaussian primitives in a tightly packed
    /// format suitable for GPU upload.
    ///
    /// # Arguments
    ///
    /// * `model` - The trained model output from `train()`
    /// * `output_path` - Destination path for the .splat file
    ///                   Parent directory must exist
    ///
    /// # Returns
    ///
    /// Success if export completes, error otherwise
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Model checkpoint file is missing or corrupted
    /// - Output path is not writable
    /// - Model format is incompatible with SPLAT export
    ///
    /// # File Format
    ///
    /// SPLAT format specification:
    /// - Header with magic number and version
    /// - Number of Gaussians (uint32)
    /// - Per-Gaussian data: position, scale, rotation, color, opacity
    /// - Platform-independent binary encoding (little-endian)
    async fn export_splat(&self, model: &BackendOutput, output_path: &Path) -> Result<()>;
}
