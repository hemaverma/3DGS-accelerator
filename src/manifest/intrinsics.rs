//! Camera intrinsics computation
//!
//! This module computes camera intrinsics from EXIF metadata or loads
//! user-provided overrides from camera_intrinsics.yaml files.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info, warn};

use super::models::CameraIntrinsics;
use crate::extractors::models::{Resolution, VideoMetadata};

/// User-provided camera intrinsics override from YAML file
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CameraIntrinsicsOverride {
    model: String,
    fx: f64,
    fy: f64,
    cx: f64,
    cy: f64,
    #[serde(default)]
    k1: f64,
    #[serde(default)]
    k2: f64,
    #[serde(default)]
    p1: f64,
    #[serde(default)]
    p2: f64,
}

impl From<CameraIntrinsicsOverride> for CameraIntrinsics {
    fn from(override_data: CameraIntrinsicsOverride) -> Self {
        CameraIntrinsics {
            model: override_data.model,
            fx: override_data.fx,
            fy: override_data.fy,
            cx: override_data.cx,
            cy: override_data.cy,
            k1: override_data.k1,
            k2: override_data.k2,
            p1: override_data.p1,
            p2: override_data.p2,
        }
    }
}

/// Compute or load camera intrinsics for a set of videos
///
/// Strategy:
/// 1. Check for camera_intrinsics.yaml override file in input directory
/// 2. If not present, try to compute from EXIF focal length
/// 3. If unavailable, return None to signal COLMAP auto-calibration
///
/// # Arguments
///
/// * `videos` - Video metadata to compute intrinsics from
/// * `input_dir` - Directory to check for camera_intrinsics.yaml override
///
/// # Returns
///
/// Some(CameraIntrinsics) if computed or loaded from override, None if COLMAP should auto-calibrate
pub fn compute_camera_intrinsics(
    videos: &[VideoMetadata],
    input_dir: &Path,
) -> Result<Option<CameraIntrinsics>> {
    // Strategy 1: Check for user-provided override file
    let override_path = input_dir.join("camera_intrinsics.yaml");
    if override_path.exists() {
        info!(
            path = %override_path.display(),
            "Loading camera intrinsics from override file"
        );
        return load_intrinsics_override(&override_path).map(Some);
    }

    // Strategy 2: Try to compute from EXIF metadata
    // Check if we have consistent resolution across all videos
    if videos.is_empty() {
        warn!("No videos provided for intrinsics computation");
        return Ok(None);
    }

    let first_resolution = videos[0].resolution;
    let consistent_resolution = videos.iter().all(|v| v.resolution == first_resolution);

    if !consistent_resolution {
        warn!("Videos have inconsistent resolutions, cannot compute shared intrinsics");
        info!("COLMAP will auto-calibrate camera parameters");
        return Ok(None);
    }

    // Try to extract focal length from first video's EXIF data
    if let Some(camera_info) = &videos[0].camera_info {
        if let Some(focal_mm) = camera_info.focal_length_mm {
            // We have focal length, try to compute intrinsics
            if let (Some(sensor_w), Some(sensor_h)) =
                (camera_info.sensor_width_mm, camera_info.sensor_height_mm)
            {
                // We have complete data, compute intrinsics
                info!(
                    focal_length_mm = focal_mm,
                    sensor_width_mm = sensor_w,
                    sensor_height_mm = sensor_h,
                    "Computing camera intrinsics from EXIF metadata"
                );
                return Ok(Some(compute_from_exif(
                    focal_mm,
                    sensor_w,
                    sensor_h,
                    first_resolution,
                )));
            } else {
                // Have focal length but no sensor size, try to estimate from 35mm equivalent
                if let Some(focal_35mm) = camera_info.focal_length_35mm {
                    let crop_factor = focal_35mm / focal_mm;
                    let sensor_w = 36.0 / crop_factor;
                    let sensor_h = 24.0 / crop_factor;
                    info!(
                        focal_length_mm = focal_mm,
                        focal_length_35mm = focal_35mm,
                        estimated_sensor_width_mm = sensor_w,
                        estimated_sensor_height_mm = sensor_h,
                        "Computing camera intrinsics from EXIF with estimated sensor size"
                    );
                    return Ok(Some(compute_from_exif(
                        focal_mm,
                        sensor_w,
                        sensor_h,
                        first_resolution,
                    )));
                } else {
                    // Have focal length but no sensor size info
                    // Use a conservative default for smartphone sensors (1/2.55")
                    let sensor_w = 5.76; // mm
                    let sensor_h = 4.29; // mm
                    warn!(
                        focal_length_mm = focal_mm,
                        "Using default smartphone sensor size (1/2.55\") for intrinsics computation"
                    );
                    return Ok(Some(compute_from_exif(
                        focal_mm,
                        sensor_w,
                        sensor_h,
                        first_resolution,
                    )));
                }
            }
        }
    }

    // No EXIF focal length available, fall back to COLMAP auto-calibration
    debug!(
        resolution = %first_resolution,
        "No camera intrinsics available, COLMAP will auto-calibrate"
    );

    Ok(None)
}

/// Compute intrinsics from EXIF focal length and sensor dimensions
///
/// Formula:
/// - fx = (focal_length_mm / sensor_width_mm) * image_width_px
/// - fy = (focal_length_mm / sensor_height_mm) * image_height_px
/// - cx = image_width_px / 2.0
/// - cy = image_height_px / 2.0
fn compute_from_exif(
    focal_length_mm: f64,
    sensor_width_mm: f64,
    sensor_height_mm: f64,
    resolution: Resolution,
) -> CameraIntrinsics {
    let fx = (focal_length_mm / sensor_width_mm) * resolution.width as f64;
    let fy = (focal_length_mm / sensor_height_mm) * resolution.height as f64;
    let cx = resolution.width as f64 / 2.0;
    let cy = resolution.height as f64 / 2.0;

    debug!(
        focal_length_mm = focal_length_mm,
        sensor_width_mm = sensor_width_mm,
        sensor_height_mm = sensor_height_mm,
        resolution = %resolution,
        fx = fx,
        fy = fy,
        "Computed camera intrinsics from EXIF"
    );

    CameraIntrinsics::simple_pinhole(fx, fy, cx, cy)
}

/// Load camera intrinsics from user-provided YAML file
fn load_intrinsics_override(path: &Path) -> Result<CameraIntrinsics> {
    let yaml_content = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read camera intrinsics file: {}", path.display()))?;

    let override_data: CameraIntrinsicsOverride = serde_yaml::from_str(&yaml_content)
        .with_context(|| format!("Failed to parse camera intrinsics YAML: {}", path.display()))?;

    info!(
        model = %override_data.model,
        fx = override_data.fx,
        fy = override_data.fy,
        "Loaded camera intrinsics override"
    );

    Ok(override_data.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_compute_from_exif() {
        // Typical smartphone camera: 26mm equivalent, 1/2.55" sensor
        let focal_length = 4.25; // mm
        let sensor_width = 5.76; // mm (1/2.55" sensor)
        let sensor_height = 4.29; // mm
        let resolution = Resolution::new(1920, 1080);

        let intrinsics = compute_from_exif(focal_length, sensor_width, sensor_height, resolution);

        // Expected: fx ≈ 1416, fy ≈ 1069
        assert!((intrinsics.fx - 1416.0).abs() < 10.0);
        assert!((intrinsics.fy - 1069.0).abs() < 10.0);
        assert_eq!(intrinsics.cx, 960.0);
        assert_eq!(intrinsics.cy, 540.0);
    }

    #[test]
    fn test_compute_camera_intrinsics_no_videos() {
        let result = compute_camera_intrinsics(&[], Path::new("/tmp")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_compute_camera_intrinsics_consistent_resolution() {
        let videos = vec![
            VideoMetadata {
                filename: "video1.mp4".to_string(),
                path: PathBuf::from("/tmp/video1.mp4"),
                frame_count: 120,
                resolution: Resolution::new(1920, 1080),
                fps: 30.0,
                duration_secs: 4.0,
                camera_info: None,
            },
            VideoMetadata {
                filename: "video2.mp4".to_string(),
                path: PathBuf::from("/tmp/video2.mp4"),
                frame_count: 120,
                resolution: Resolution::new(1920, 1080),
                fps: 30.0,
                duration_secs: 4.0,
                camera_info: None,
            },
        ];

        let result = compute_camera_intrinsics(&videos, Path::new("/tmp")).unwrap();
        // Should be None because we don't have EXIF focal length yet
        assert!(result.is_none());
    }

    #[test]
    fn test_compute_camera_intrinsics_inconsistent_resolution() {
        let videos = vec![
            VideoMetadata {
                filename: "video1.mp4".to_string(),
                path: PathBuf::from("/tmp/video1.mp4"),
                frame_count: 120,
                resolution: Resolution::new(1920, 1080),
                fps: 30.0,
                duration_secs: 4.0,
                camera_info: None,
            },
            VideoMetadata {
                filename: "video2.mp4".to_string(),
                path: PathBuf::from("/tmp/video2.mp4"),
                frame_count: 120,
                resolution: Resolution::new(3840, 2160),
                fps: 30.0,
                duration_secs: 4.0,
                camera_info: None,
            },
        ];

        let result = compute_camera_intrinsics(&videos, Path::new("/tmp")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_load_intrinsics_override() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let yaml_content = r#"
model: OPENCV
fx: 1000.0
fy: 1000.0
cx: 960.0
cy: 540.0
k1: -0.1
k2: 0.05
p1: 0.001
p2: 0.002
"#;

        let mut temp_file = NamedTempFile::new().unwrap();
        temp_file.write_all(yaml_content.as_bytes()).unwrap();
        temp_file.flush().unwrap();

        let intrinsics = load_intrinsics_override(temp_file.path()).unwrap();
        assert_eq!(intrinsics.model, "OPENCV");
        assert_eq!(intrinsics.fx, 1000.0);
        assert_eq!(intrinsics.fy, 1000.0);
        assert_eq!(intrinsics.cx, 960.0);
        assert_eq!(intrinsics.cy, 540.0);
        assert_eq!(intrinsics.k1, -0.1);
        assert_eq!(intrinsics.k2, 0.05);
    }

    #[test]
    fn test_compute_camera_intrinsics_with_exif() {
        use crate::extractors::models::CameraInfo;

        // Test with complete EXIF data
        let camera_info = CameraInfo {
            focal_length_mm: Some(4.25),
            sensor_width_mm: Some(5.76),
            sensor_height_mm: Some(4.29),
            ..CameraInfo::empty()
        };

        let videos = vec![VideoMetadata {
            filename: "video1.mp4".to_string(),
            path: PathBuf::from("/tmp/video1.mp4"),
            frame_count: 120,
            resolution: Resolution::new(1920, 1080),
            fps: 30.0,
            duration_secs: 4.0,
            camera_info: Some(camera_info),
        }];

        let result = compute_camera_intrinsics(&videos, Path::new("/tmp")).unwrap();
        assert!(result.is_some());

        let intrinsics = result.unwrap();
        // fx = (4.25 / 5.76) * 1920 ≈ 1416
        assert!((intrinsics.fx - 1416.0).abs() < 10.0);
        // fy = (4.25 / 4.29) * 1080 ≈ 1069
        assert!((intrinsics.fy - 1069.0).abs() < 10.0);
        assert_eq!(intrinsics.cx, 960.0);
        assert_eq!(intrinsics.cy, 540.0);
    }

    #[test]
    fn test_compute_camera_intrinsics_with_35mm_equivalent() {
        use crate::extractors::models::CameraInfo;

        // Test with focal length and 35mm equivalent (no sensor size)
        let camera_info = CameraInfo {
            focal_length_mm: Some(4.25),
            focal_length_35mm: Some(26.0),
            ..CameraInfo::empty()
        };

        let videos = vec![VideoMetadata {
            filename: "video1.mp4".to_string(),
            path: PathBuf::from("/tmp/video1.mp4"),
            frame_count: 120,
            resolution: Resolution::new(1920, 1080),
            fps: 30.0,
            duration_secs: 4.0,
            camera_info: Some(camera_info),
        }];

        let result = compute_camera_intrinsics(&videos, Path::new("/tmp")).unwrap();
        assert!(result.is_some());

        let intrinsics = result.unwrap();
        // Should compute sensor size from crop factor and then intrinsics
        assert!(intrinsics.fx > 0.0);
        assert!(intrinsics.fy > 0.0);
        assert_eq!(intrinsics.cx, 960.0);
        assert_eq!(intrinsics.cy, 540.0);
    }

    #[test]
    fn test_compute_camera_intrinsics_default_sensor() {
        use crate::extractors::models::CameraInfo;

        // Test with only focal length (use default sensor size)
        let camera_info = CameraInfo {
            focal_length_mm: Some(4.25),
            ..CameraInfo::empty()
        };

        let videos = vec![VideoMetadata {
            filename: "video1.mp4".to_string(),
            path: PathBuf::from("/tmp/video1.mp4"),
            frame_count: 120,
            resolution: Resolution::new(1920, 1080),
            fps: 30.0,
            duration_secs: 4.0,
            camera_info: Some(camera_info),
        }];

        let result = compute_camera_intrinsics(&videos, Path::new("/tmp")).unwrap();
        assert!(result.is_some());

        let intrinsics = result.unwrap();
        // Should use default smartphone sensor (1/2.55")
        assert!(intrinsics.fx > 0.0);
        assert!(intrinsics.fy > 0.0);
    }
}
