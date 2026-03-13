//! Reconstruction backend trait definition

use anyhow::Result;
use async_trait::async_trait;
use std::path::Path;

use super::models::ReconstructionOutput;

/// Abstract reconstruction backend interface
///
/// Implementations provide Structure-from-Motion (SfM) or similar reconstruction
/// to convert multi-view images into camera poses and sparse 3D points.
///
/// # Supported Backends
/// - COLMAP: Gold-standard SfM pipeline
/// - Nerfstudio: Pre-computed transforms.json format
/// - Polycam/Luma: Mobile capture exports
/// - Precalibrated: Known camera poses (bypass SfM)
#[async_trait]
pub trait ReconstructionBackend: Send + Sync {
    /// Backend name (e.g., "colmap", "nerfstudio")
    fn name(&self) -> &str;

    /// Backend description
    fn description(&self) -> &str;

    /// Check if this backend is available (binaries installed, etc.)
    fn is_available(&self) -> Result<bool>;

    /// Run reconstruction on a directory of images
    ///
    /// # Arguments
    /// * `image_dir` - Directory containing input images (frames from videos)
    /// * `output_dir` - Directory where reconstruction outputs will be written
    /// * `config` - Backend-specific configuration (JSON)
    ///
    /// # Returns
    /// * `ReconstructionOutput` - Camera poses, point cloud, and statistics
    async fn reconstruct(
        &self,
        image_dir: &Path,
        output_dir: &Path,
        config: &serde_json::Value,
    ) -> Result<ReconstructionOutput>;

    /// Validate reconstruction output quality
    ///
    /// Checks minimum quality thresholds (point count, registered images, etc.)
    /// The minimum point count defaults to 1000 but can be overridden via
    /// `MIN_RECONSTRUCTION_POINTS` environment variable.
    fn validate_output(&self, output: &ReconstructionOutput) -> Result<()> {
        let min_points: usize = std::env::var("MIN_RECONSTRUCTION_POINTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(1000);

        if output.stats.total_points < min_points {
            anyhow::bail!(
                "Reconstruction quality too low: only {} points (minimum {} required)",
                output.stats.total_points,
                min_points
            );
        }

        if output.stats.registered_images < 2 {
            anyhow::bail!(
                "Insufficient registered images: {} (minimum 2 required)",
                output.stats.registered_images
            );
        }

        Ok(())
    }
}
