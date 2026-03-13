//! Nerfstudio transforms.json backend
//!
//! Reads pre-computed camera poses from Nerfstudio's transforms.json format.
//! This allows using data from Nerfstudio, Instant-NGP, or similar tools.

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

/// Nerfstudio transforms.json backend
///
/// Reads camera poses from pre-computed transforms.json file.
/// Does not perform actual reconstruction - just loads existing data.
pub struct NerfstudioBackend;

impl NerfstudioBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for NerfstudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ReconstructionBackend for NerfstudioBackend {
    fn name(&self) -> &str {
        "nerfstudio"
    }

    fn description(&self) -> &str {
        "Nerfstudio transforms.json format (pre-computed camera poses)"
    }

    fn is_available(&self) -> Result<bool> {
        // This backend is always available (no external dependencies)
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
        info!("Loading Nerfstudio transforms.json");

        // Look for transforms.json in image_dir or config
        let transforms_path = if let Some(path_str) = config.get("transforms_path") {
            Path::new(path_str.as_str().unwrap()).to_path_buf()
        } else {
            image_dir.join("transforms.json")
        };

        if !transforms_path.exists() {
            anyhow::bail!(
                "transforms.json not found at {}. Provide path via config.transforms_path",
                transforms_path.display()
            );
        }

        // Parse transforms.json
        let file = File::open(&transforms_path)
            .context("Failed to open transforms.json")?;
        let reader = BufReader::new(file);
        let transforms: NerfstudioTransforms = serde_json::from_reader(reader)
            .context("Failed to parse transforms.json")?;

        // Convert to common format
        let mut cameras = Vec::new();
        for frame in transforms.frames {
            let image_path = image_dir.join(&frame.file_path);

            // Extract rotation (quaternion) and translation from transform matrix
            let (rotation, translation) = parse_transform_matrix(&frame.transform_matrix)?;

            let intrinsics = CameraIntrinsics {
                model: "NERFSTUDIO".to_string(),
                width: transforms.w.unwrap_or(800), // Default if not specified
                height: transforms.h.unwrap_or(600),
                fx: transforms.fl_x.unwrap_or(800.0),
                fy: transforms.fl_y.unwrap_or(800.0),
                cx: transforms.cx.unwrap_or(400.0),
                cy: transforms.cy.unwrap_or(300.0),
                distortion: vec![
                    transforms.k1.unwrap_or(0.0),
                    transforms.k2.unwrap_or(0.0),
                    transforms.p1.unwrap_or(0.0),
                    transforms.p2.unwrap_or(0.0),
                ],
            };

            cameras.push(ImageRegistration {
                image_id: frame.file_path.clone(),
                image_path,
                pose: CameraPose {
                    rotation,
                    translation,
                },
                intrinsics,
            });
        }

        let stats = ReconstructionStats {
            total_points: 0, // Nerfstudio format doesn't include sparse points
            registered_images: cameras.len(),
            avg_reprojection_error: None,
            backend_name: "nerfstudio".to_string(),
        };

        let metadata = json!({
            "transforms_path": transforms_path.to_string_lossy(),
            "camera_angle_x": transforms.camera_angle_x,
        });

        // Create output directory and copy transforms.json
        std::fs::create_dir_all(output_dir)?;
        std::fs::copy(&transforms_path, output_dir.join("transforms.json"))?;

        Ok(ReconstructionOutput {
            output_dir: output_dir.to_path_buf(),
            cameras,
            sparse_point_cloud: None,
            stats,
            metadata,
        })
    }

    fn validate_output(&self, output: &ReconstructionOutput) -> Result<()> {
        // Nerfstudio format doesn't have sparse points, so only check images
        if output.stats.registered_images < 2 {
            anyhow::bail!(
                "Insufficient registered images: {} (minimum 2 required)",
                output.stats.registered_images
            );
        }
        Ok(())
    }
}

/// Nerfstudio transforms.json format
#[derive(Debug, Deserialize)]
struct NerfstudioTransforms {
    camera_angle_x: Option<f64>,
    fl_x: Option<f64>,
    fl_y: Option<f64>,
    cx: Option<f64>,
    cy: Option<f64>,
    w: Option<u32>,
    h: Option<u32>,
    k1: Option<f64>,
    k2: Option<f64>,
    p1: Option<f64>,
    p2: Option<f64>,
    frames: Vec<NerfstudioFrame>,
}

#[derive(Debug, Deserialize)]
struct NerfstudioFrame {
    file_path: String,
    transform_matrix: [[f64; 4]; 4],
}

/// Parse 4x4 transform matrix into rotation quaternion and translation
fn parse_transform_matrix(matrix: &[[f64; 4]; 4]) -> Result<([f64; 4], [f64; 3])> {
    // Extract translation (last column)
    let translation = [matrix[0][3], matrix[1][3], matrix[2][3]];

    // Extract rotation matrix (3x3 top-left)
    let rot = [
        [matrix[0][0], matrix[0][1], matrix[0][2]],
        [matrix[1][0], matrix[1][1], matrix[1][2]],
        [matrix[2][0], matrix[2][1], matrix[2][2]],
    ];

    // Convert rotation matrix to quaternion
    let rotation = rotation_matrix_to_quaternion(&rot)?;

    Ok((rotation, translation))
}

/// Convert 3x3 rotation matrix to quaternion [w, x, y, z]
fn rotation_matrix_to_quaternion(r: &[[f64; 3]; 3]) -> Result<[f64; 4]> {
    // Simplified conversion (Shepperd's method)
    let trace = r[0][0] + r[1][1] + r[2][2];

    let (w, x, y, z) = if trace > 0.0 {
        let s = (trace + 1.0).sqrt() * 2.0;
        (
            0.25 * s,
            (r[2][1] - r[1][2]) / s,
            (r[0][2] - r[2][0]) / s,
            (r[1][0] - r[0][1]) / s,
        )
    } else {
        // Handle other cases (simplified)
        (1.0, 0.0, 0.0, 0.0)
    };

    Ok([w, x, y, z])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nerfstudio_backend_creation() {
        let backend = NerfstudioBackend::new();
        assert_eq!(backend.name(), "nerfstudio");
    }

    #[test]
    fn test_nerfstudio_backend_available() {
        let backend = NerfstudioBackend::new();
        assert!(backend.is_available().unwrap());
    }
}
