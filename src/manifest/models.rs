//! Manifest JSON structure models
//!
//! This module defines the JSON structure for manifest files that describe
//! video processing jobs for COLMAP and 3DGS backends.

use serde::{Deserialize, Serialize};

/// Complete manifest file for a processing job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    /// Unique job identifier
    pub job_id: String,

    /// Timestamp when processing started (ISO 8601 UTC)
    pub processing_timestamp: String,

    /// List of videos in this job with their extracted frames
    pub videos: Vec<VideoEntry>,

    /// Camera intrinsics for this job (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_intrinsics: Option<CameraIntrinsics>,
}

impl Manifest {
    /// Create a new manifest with current timestamp
    pub fn new(job_id: String) -> Self {
        let processing_timestamp = chrono::Utc::now().to_rfc3339();

        Manifest {
            job_id,
            processing_timestamp,
            videos: Vec::new(),
            camera_intrinsics: None,
        }
    }

    /// Add a video entry to the manifest
    pub fn add_video(&mut self, video: VideoEntry) {
        self.videos.push(video);
    }

    /// Set camera intrinsics for the manifest
    pub fn set_camera_intrinsics(&mut self, intrinsics: CameraIntrinsics) {
        self.camera_intrinsics = Some(intrinsics);
    }

    /// Write manifest to JSON file
    pub fn write_to_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// Video entry in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoEntry {
    /// Original video filename
    pub filename: String,

    /// Total number of frames extracted
    pub frame_count: u32,

    /// Video resolution (width x height)
    pub resolution: String,

    /// Original frame rate (frames per second)
    pub fps: f64,

    /// Video duration in seconds
    pub duration_secs: f64,

    /// Recording timestamp (ISO 8601 UTC) if available
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording_timestamp: Option<String>,

    /// Camera/device model identifier
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera_model: Option<String>,

    /// List of extracted frame files
    pub frames: Vec<FrameEntry>,
}

/// Frame entry in the manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameEntry {
    /// Frame number (zero-indexed)
    pub frame_number: u32,

    /// Timestamp offset from start of video (seconds)
    pub timestamp_offset: f64,

    /// Frame filename
    pub filename: String,
}

impl FrameEntry {
    /// Create a new frame entry
    pub fn new(frame_number: u32, timestamp_offset: f64, filename: String) -> Self {
        FrameEntry {
            frame_number,
            timestamp_offset,
            filename,
        }
    }
}

/// Camera intrinsics parameters
///
/// These parameters describe the camera's internal characteristics needed for
/// 3D reconstruction. If not provided, COLMAP will auto-calibrate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraIntrinsics {
    /// Camera model type (OPENCV, SIMPLE_PINHOLE, RADIAL, etc.)
    pub model: String,

    /// Focal length in X direction (pixels)
    pub fx: f64,

    /// Focal length in Y direction (pixels)
    pub fy: f64,

    /// Principal point X coordinate (pixels)
    pub cx: f64,

    /// Principal point Y coordinate (pixels)
    pub cy: f64,

    /// Radial distortion coefficient 1
    #[serde(default)]
    pub k1: f64,

    /// Radial distortion coefficient 2
    #[serde(default)]
    pub k2: f64,

    /// Tangential distortion coefficient 1
    #[serde(default)]
    pub p1: f64,

    /// Tangential distortion coefficient 2
    #[serde(default)]
    pub p2: f64,
}

impl CameraIntrinsics {
    /// Create OPENCV camera model with distortion parameters
    #[allow(clippy::too_many_arguments)]
    pub fn opencv(fx: f64, fy: f64, cx: f64, cy: f64, k1: f64, k2: f64, p1: f64, p2: f64) -> Self {
        CameraIntrinsics {
            model: "OPENCV".to_string(),
            fx,
            fy,
            cx,
            cy,
            k1,
            k2,
            p1,
            p2,
        }
    }

    /// Create simple pinhole camera model (no distortion)
    pub fn simple_pinhole(fx: f64, fy: f64, cx: f64, cy: f64) -> Self {
        CameraIntrinsics {
            model: "OPENCV".to_string(),
            fx,
            fy,
            cx,
            cy,
            k1: 0.0,
            k2: 0.0,
            p1: 0.0,
            p2: 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_creation() {
        let manifest = Manifest::new("job-001".to_string());
        assert_eq!(manifest.job_id, "job-001");
        assert_eq!(manifest.videos.len(), 0);
        assert!(manifest.camera_intrinsics.is_none());
    }

    #[test]
    fn test_manifest_serialization() {
        let mut manifest = Manifest::new("job-001".to_string());

        let video = VideoEntry {
            filename: "test.mp4".to_string(),
            frame_count: 120,
            resolution: "1920x1080".to_string(),
            fps: 30.0,
            duration_secs: 4.0,
            recording_timestamp: Some("2026-02-11T18:15:23Z".to_string()),
            camera_model: Some("iPhone 15 Pro".to_string()),
            frames: vec![
                FrameEntry::new(0, 0.0, "frame_000001.jpg".to_string()),
                FrameEntry::new(1, 0.033, "frame_000002.jpg".to_string()),
            ],
        };

        manifest.add_video(video);
        manifest.set_camera_intrinsics(CameraIntrinsics::simple_pinhole(
            1000.0, 1000.0, 960.0, 540.0,
        ));

        let json = serde_json::to_string_pretty(&manifest).unwrap();
        assert!(json.contains("job-001"));
        assert!(json.contains("test.mp4"));
        assert!(json.contains("OPENCV"));
    }

    #[test]
    fn test_frame_entry() {
        let frame = FrameEntry::new(5, 0.167, "frame_000006.jpg".to_string());
        assert_eq!(frame.frame_number, 5);
        assert!((frame.timestamp_offset - 0.167).abs() < 0.001);
        assert_eq!(frame.filename, "frame_000006.jpg");
    }

    #[test]
    fn test_camera_intrinsics_simple_pinhole() {
        let intrinsics = CameraIntrinsics::simple_pinhole(1000.0, 1000.0, 960.0, 540.0);
        assert_eq!(intrinsics.model, "OPENCV");
        assert_eq!(intrinsics.fx, 1000.0);
        assert_eq!(intrinsics.fy, 1000.0);
        assert_eq!(intrinsics.cx, 960.0);
        assert_eq!(intrinsics.cy, 540.0);
        assert_eq!(intrinsics.k1, 0.0);
        assert_eq!(intrinsics.k2, 0.0);
    }

    #[test]
    fn test_camera_intrinsics_opencv() {
        let intrinsics =
            CameraIntrinsics::opencv(1000.0, 1000.0, 960.0, 540.0, -0.1, 0.05, 0.001, 0.002);
        assert_eq!(intrinsics.model, "OPENCV");
        assert_eq!(intrinsics.k1, -0.1);
        assert_eq!(intrinsics.k2, 0.05);
        assert_eq!(intrinsics.p1, 0.001);
        assert_eq!(intrinsics.p2, 0.002);
    }
}
