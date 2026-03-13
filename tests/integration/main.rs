//! Integration tests for 3DGS Video Processor
//!
//! Comprehensive test suite covering:
//! - Multi-video processing with concurrent frame extraction
//! - Duplicate job detection and avoidance
//! - Retry logic with exponential backoff
//! - Error scenarios and edge cases
//! - Job queue management (FIFO)
//! - Input validation
//! - File stability detection
//!
//! Tests use real FFmpeg for frame extraction and COLMAP for reconstruction
//! when available, falling back to mock backends for unit-style testing.

// Include shared test utilities
#[path = "../common/mod.rs"]
mod common;

// Include test modules
mod backend_integration;
mod backend_registry;
mod backend_validation;
mod deduplication;
mod error_scenarios;
mod gpu_detection;
mod multi_video_processing;
mod ply_parsing;
mod queue_management;
mod retry_logic;
mod stability_edge_cases;
mod validation;

use common::fixtures::{create_test_frames, create_test_workspace};
use common::mocks::MockBackend;
use std::path::PathBuf;
use tempfile::TempDir;
use three_dgs_processor::backends::{GaussianSplatBackend, TrainingConfig};
use three_dgs_processor::extractors::models::{CameraInfo, Resolution, VideoMetadata};
use three_dgs_processor::manifest::models::{CameraIntrinsics, Manifest};

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_backend_integration() {
        let temp = TempDir::new().unwrap();
        let frames_paths = create_test_frames(temp.path(), 10).unwrap();

        // Convert to &[&Path] for backend trait
        let frame_refs: Vec<&std::path::Path> = frames_paths.iter().map(|p| p.as_path()).collect();

        let backend = MockBackend::new();
        let config = TrainingConfig::default();

        let output = backend.train(&frame_refs, &config).await.unwrap();
        assert_eq!(output.iterations_completed, 1000);
        assert_eq!(output.metadata.backend_name, "mock");

        // Test export
        let ply_path = temp.path().join("output.ply");
        backend.export_ply(&output, &ply_path).await.unwrap();
        assert!(ply_path.exists());

        let splat_path = temp.path().join("output.splat");
        backend.export_splat(&output, &splat_path).await.unwrap();
        assert!(splat_path.exists());
    }

    #[test]
    fn test_workspace_structure() {
        let workspace = create_test_workspace().unwrap();
        assert!(workspace.path().join("input").is_dir());
        assert!(workspace.path().join("output").is_dir());
        assert!(workspace.path().join("processed").is_dir());
        assert!(workspace.path().join("error").is_dir());
    }

    #[test]
    fn test_frame_generation() {
        let temp = TempDir::new().unwrap();
        let frames = create_test_frames(temp.path(), 25).unwrap();

        assert_eq!(frames.len(), 25);
        for frame in frames {
            assert!(frame.exists());
            assert!(frame.to_string_lossy().contains("frame_"));
            assert!(frame.extension().unwrap() == "jpg");
        }
    }

    #[test]
    fn test_manifest_structure() {
        // Test manifest creation with mock data
        let manifest = Manifest {
            job_id: "test-job-001".to_string(),
            processing_timestamp: "2024-01-01T00:00:00Z".to_string(),
            videos: vec![],
            camera_intrinsics: Some(CameraIntrinsics::simple_pinhole(
                1000.0, 1000.0, 960.0, 540.0,
            )),
        };

        // Verify manifest can be serialized
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(json.contains("job_id"));
        assert!(json.contains("camera_intrinsics"));
    }

    #[test]
    fn test_video_metadata_structure() {
        let metadata = VideoMetadata {
            filename: "test.mp4".to_string(),
            path: PathBuf::from("/tmp/test.mp4"),
            frame_count: 300,
            resolution: Resolution::new(1920, 1080),
            fps: 30.0,
            duration_secs: 10.0,
            camera_info: Some(CameraInfo {
                recording_timestamp: Some("2024-01-01T00:00:00Z".to_string()),
                gps_coordinates: None,
                camera_model: Some("TestCam TC-1000".to_string()),
                orientation: None,
                focal_length_mm: None,
                focal_length_35mm: None,
                sensor_width_mm: None,
                sensor_height_mm: None,
            }),
        };

        assert_eq!(metadata.frame_count, 300);
        assert_eq!(metadata.fps, 30.0);
        assert_eq!(metadata.filename, "test.mp4");
    }

    #[tokio::test]
    async fn test_concurrent_frame_creation() {
        use tokio::task;

        let temp = TempDir::new().unwrap();
        let base_path = temp.path().to_path_buf();

        // Create frames concurrently
        let mut handles = vec![];
        for i in 0..3 {
            let path = base_path.clone();
            let handle = task::spawn(async move {
                let dir = path.join(format!("batch_{}", i));
                create_test_frames(&dir, 5)
            });
            handles.push(handle);
        }

        // Wait for all to complete
        let mut total_frames = 0;
        for handle in handles {
            let frames = handle.await.unwrap().unwrap();
            total_frames += frames.len();
        }

        assert_eq!(total_frames, 15);
    }
}
