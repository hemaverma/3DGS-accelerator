//! Common data structures for reconstruction backends

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Re-export MatcherType from colmap module for convenience
pub use crate::colmap::models::MatcherType;

/// Output from any reconstruction backend
#[derive(Debug, Clone)]
pub struct ReconstructionOutput {
    /// Path to reconstruction output directory (COLMAP sparse, transforms.json, etc.)
    pub output_dir: PathBuf,

    /// Registered camera poses and intrinsics
    pub cameras: Vec<ImageRegistration>,

    /// Path to sparse point cloud (if available)
    pub sparse_point_cloud: Option<PathBuf>,

    /// Reconstruction statistics
    pub stats: ReconstructionStats,

    /// Backend-specific metadata (database paths, etc.)
    pub metadata: serde_json::Value,
}

/// Statistics from reconstruction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconstructionStats {
    /// Total number of 3D points reconstructed
    pub total_points: usize,

    /// Number of cameras/images successfully registered
    pub registered_images: usize,

    /// Average reprojection error (if available)
    pub avg_reprojection_error: Option<f64>,

    /// Backend that produced this reconstruction
    pub backend_name: String,
}

/// Camera pose and registration info for a single image
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageRegistration {
    /// Image identifier (filename or ID)
    pub image_id: String,

    /// Path to the image file
    pub image_path: PathBuf,

    /// Camera pose (position and orientation)
    pub pose: CameraPose,

    /// Camera intrinsics
    pub intrinsics: CameraIntrinsics,
}

/// Camera pose in 3D space
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraPose {
    /// Rotation quaternion [w, x, y, z]
    pub rotation: [f64; 4],

    /// Translation vector [x, y, z]
    pub translation: [f64; 3],
}

/// Camera intrinsic parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraIntrinsics {
    /// Camera model (e.g., "OPENCV", "SIMPLE_PINHOLE", "FULL_OPENCV")
    pub model: String,

    /// Image width in pixels
    pub width: u32,

    /// Image height in pixels
    pub height: u32,

    /// Focal length X (pixels)
    pub fx: f64,

    /// Focal length Y (pixels)
    pub fy: f64,

    /// Principal point X (pixels)
    pub cx: f64,

    /// Principal point Y (pixels)
    pub cy: f64,

    /// Distortion coefficients (model-specific)
    pub distortion: Vec<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matcher_type_roundtrip() {
        let types = vec![
            MatcherType::Exhaustive,
            MatcherType::Sequential,
            MatcherType::VocabTree,
        ];

        for matcher in types {
            let s = matcher.to_string();
            let parsed = s.parse::<MatcherType>().unwrap();
            assert_eq!(matcher, parsed);
        }
    }

    #[test]
    fn test_matcher_type_invalid() {
        assert!("invalid".parse::<MatcherType>().is_err());
    }
}
