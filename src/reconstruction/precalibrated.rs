//! Pre-calibrated camera backend
//!
//! Uses pre-calibrated camera poses from external sources (Polycam, Luma AI, etc.)
//! Bypasses Structure-from-Motion entirely by loading known camera parameters.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use tracing::{info, instrument};

use super::backend::ReconstructionBackend;
use super::models::{
    CameraIntrinsics, CameraPose, ImageRegistration, ReconstructionOutput, ReconstructionStats,
};

/// Pre-calibrated camera backend
///
/// Loads camera poses from a simple JSON format with known intrinsics and extrinsics.
/// Useful for professional camera rigs, motion capture systems, or mobile capture apps.
pub struct PrecalibratedBackend;

impl PrecalibratedBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PrecalibratedBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ReconstructionBackend for PrecalibratedBackend {
    fn name(&self) -> &str {
        "precalibrated"
    }

    fn description(&self) -> &str {
        "Pre-calibrated cameras (known poses from external calibration)"
    }

    fn is_available(&self) -> Result<bool> {
        // Always available (no external dependencies)
        Ok(true)
    }

    #[instrument(skip(self, config), fields(
        image_dir = %image_dir.display(),
        output_dir = %output_dir.display()
    ))]
    async fn reconstruct(
        &self,
        image_dir: &Path,
        output_dir: &Path,
        config: &serde_json::Value,
    ) -> Result<ReconstructionOutput> {
        info!("Loading pre-calibrated camera data");

        // Look for cameras.json in image_dir or config
        let cameras_path = if let Some(path_str) = config.get("cameras_path") {
            Path::new(path_str.as_str().unwrap()).to_path_buf()
        } else {
            image_dir.join("cameras.json")
        };

        if !cameras_path.exists() {
            anyhow::bail!(
                "cameras.json not found at {}. Provide path via config.cameras_path",
                cameras_path.display()
            );
        }

        // Parse cameras.json
        let file = File::open(&cameras_path).context("Failed to open cameras.json")?;
        let reader = BufReader::new(file);
        let precalibrated: PrecalibratedCameras = serde_json::from_reader(reader)
            .context("Failed to parse cameras.json")?;

        // Convert to common format
        let cameras: Vec<ImageRegistration> = precalibrated
            .cameras
            .into_iter()
            .map(|cam| {
                let image_path = image_dir.join(&cam.image_filename);

                ImageRegistration {
                    image_id: cam.image_filename.clone(),
                    image_path,
                    pose: CameraPose {
                        rotation: [cam.qw, cam.qx, cam.qy, cam.qz],
                        translation: [cam.tx, cam.ty, cam.tz],
                    },
                    intrinsics: CameraIntrinsics {
                        model: cam.model.unwrap_or_else(|| "OPENCV".to_string()),
                        width: cam.width,
                        height: cam.height,
                        fx: cam.fx,
                        fy: cam.fy,
                        cx: cam.cx,
                        cy: cam.cy,
                        distortion: cam.distortion.unwrap_or_default(),
                    },
                }
            })
            .collect();

        let stats = ReconstructionStats {
            total_points: 0, // No sparse reconstruction performed
            registered_images: cameras.len(),
            avg_reprojection_error: None,
            backend_name: "precalibrated".to_string(),
        };

        let metadata = json!({
            "cameras_path": cameras_path.to_string_lossy(),
            "source": precalibrated.source,
        });

        // Create output directory and copy cameras.json
        std::fs::create_dir_all(output_dir)?;
        std::fs::copy(&cameras_path, output_dir.join("cameras.json"))?;

        Ok(ReconstructionOutput {
            output_dir: output_dir.to_path_buf(),
            cameras,
            sparse_point_cloud: None,
            stats,
            metadata,
        })
    }

    fn validate_output(&self, output: &ReconstructionOutput) -> Result<()> {
        // Pre-calibrated format doesn't have sparse points, so only check images
        if output.stats.registered_images < 2 {
            anyhow::bail!(
                "Insufficient cameras: {} (minimum 2 required)",
                output.stats.registered_images
            );
        }
        Ok(())
    }
}

/// Pre-calibrated cameras JSON format
#[derive(Debug, Deserialize)]
struct PrecalibratedCameras {
    /// Source of calibration (e.g., "polycam", "luma", "custom")
    source: Option<String>,

    /// List of cameras with known poses
    cameras: Vec<PrecalibratedCamera>,
}

#[derive(Debug, Deserialize)]
struct PrecalibratedCamera {
    /// Image filename (relative to image_dir)
    image_filename: String,

    /// Camera model (e.g., "OPENCV", "SIMPLE_PINHOLE")
    model: Option<String>,

    /// Image dimensions
    width: u32,
    height: u32,

    /// Intrinsics
    fx: f64,
    fy: f64,
    cx: f64,
    cy: f64,

    /// Optional distortion coefficients
    distortion: Option<Vec<f64>>,

    /// Rotation quaternion [w, x, y, z]
    qw: f64,
    qx: f64,
    qy: f64,
    qz: f64,

    /// Translation [x, y, z]
    tx: f64,
    ty: f64,
    tz: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precalibrated_backend_creation() {
        let backend = PrecalibratedBackend::new();
        assert_eq!(backend.name(), "precalibrated");
    }

    #[test]
    fn test_precalibrated_backend_available() {
        let backend = PrecalibratedBackend::new();
        assert!(backend.is_available().unwrap());
    }
}
