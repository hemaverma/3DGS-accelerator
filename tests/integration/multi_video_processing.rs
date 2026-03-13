//! Integration tests for multi-video processing
//!
//! Tests concurrent frame extraction and processing of multiple videos
//! from different capture angles/times.

use std::path::PathBuf;
use tempfile::TempDir;

use super::common::fixtures::create_test_frames;
use super::common::mocks::MockBackend;
use three_dgs_processor::backends::{GaussianSplatBackend, TrainingConfig};
use three_dgs_processor::extractors::models::{Resolution, VideoMetadata};
use three_dgs_processor::extractors::ffmpeg::VideoInput;

#[tokio::test]
async fn test_process_multiple_videos_sequential() {
    // Simulate processing 3 videos with different resolutions
    let temp = TempDir::new().unwrap();
    let backend = MockBackend::new();

    // Create frame sets simulating extraction from 3 different videos
    let video1_frames = create_test_frames(&temp.path().join("video1"), 30).unwrap();
    let video2_frames = create_test_frames(&temp.path().join("video2"), 25).unwrap();
    let video3_frames = create_test_frames(&temp.path().join("video3"), 35).unwrap();

    // Combine all frames
    let mut all_frames: Vec<PathBuf> = Vec::new();
    all_frames.extend(video1_frames);
    all_frames.extend(video2_frames);
    all_frames.extend(video3_frames);

    assert_eq!(all_frames.len(), 90);

    // Test training with all frames
    let config = TrainingConfig::default();
    let frame_refs: Vec<&std::path::Path> = all_frames.iter().map(|p| p.as_path()).collect();
    
    let result = backend.train(&frame_refs, &config).await.unwrap();
    assert_eq!(result.iterations_completed, 1000);
    // Mock backend doesn't create actual file, just returns path
    assert!(!result.model_path.as_os_str().is_empty());
}

#[tokio::test]
async fn test_concurrent_frame_extraction_simulation() {
    // Test concurrent processing pattern (real implementation uses buffer_unordered)
    use tokio::task;
    
    let temp = TempDir::new().unwrap();
    
    // Simulate concurrent frame extraction from 5 videos
    let mut handles = vec![];
    for i in 0..5 {
        let base_path = temp.path().to_path_buf();
        let handle = task::spawn(async move {
            let video_dir = base_path.join(format!("video_{}", i));
            create_test_frames(&video_dir, 20)
        });
        handles.push(handle);
    }
    
    // Wait for all extractions to complete
    let mut total_frames = 0;
    for handle in handles {
        let frames = handle.await.unwrap().unwrap();
        total_frames += frames.len();
    }
    
    assert_eq!(total_frames, 100); // 5 videos × 20 frames each
}

#[tokio::test]
async fn test_video_metadata_aggregation() {
    // Test creating metadata for multiple videos with different properties
    let videos = vec![
        VideoMetadata {
            filename: "front.mp4".to_string(),
            path: PathBuf::from("/input/front.mp4"),
            frame_count: 150,
            resolution: Resolution::new(1920, 1080),
            fps: 30.0,
            duration_secs: 5.0,
            camera_info: None,
        },
        VideoMetadata {
            filename: "left.mp4".to_string(),
            path: PathBuf::from("/input/left.mp4"),
            frame_count: 120,
            resolution: Resolution::new(1920, 1080),
            fps: 24.0,
            duration_secs: 5.0,
            camera_info: None,
        },
        VideoMetadata {
            filename: "right.mp4".to_string(),
            path: PathBuf::from("/input/right.mp4"),
            frame_count: 180,
            resolution: Resolution::new(3840, 2160),
            fps: 30.0,
            duration_secs: 6.0,
            camera_info: None,
        },
    ];
    
    // Verify aggregation
    let total_frames: u32 = videos.iter().map(|v| v.frame_count).sum();
    assert_eq!(total_frames, 450);
    
    let unique_resolutions: std::collections::HashSet<_> = 
        videos.iter().map(|v| (v.resolution.width, v.resolution.height)).collect();
    assert_eq!(unique_resolutions.len(), 2); // 1080p and 4K
}

#[test]
fn test_video_input_creation() {
    // Test VideoInput struct creation for multiple videos
    let video1 = VideoInput::new(PathBuf::from("/input/scene/video1.mp4")).unwrap();
    let video2 = VideoInput::new(PathBuf::from("/input/scene/video2.mp4")).unwrap();
    
    assert_eq!(video1.path.file_name().unwrap(), "video1.mp4");
    assert_eq!(video2.path.file_name().unwrap(), "video2.mp4");
    assert_eq!(video1.filename, "video1.mp4");
    assert_eq!(video2.filename, "video2.mp4");
}

#[tokio::test]
async fn test_empty_video_list() {
    // Ensure system handles empty video list gracefully
    let _temp = TempDir::new().unwrap();
    let backend = MockBackend::new();
    
    let empty_frames: Vec<PathBuf> = vec![];
    let config = TrainingConfig::default();
    let frame_refs: Vec<&std::path::Path> = empty_frames.iter().map(|p| p.as_path()).collect();
    
    // Training with no frames should fail
    let result = backend.train(&frame_refs, &config).await;
    assert!(result.is_err() || result.unwrap().gaussian_count.unwrap_or(0) == 0);
}

#[tokio::test]
async fn test_single_video_processing() {
    // Verify single video works (edge case of multi-video pipeline)
    let temp = TempDir::new().unwrap();
    let backend = MockBackend::new();
    
    let frames = create_test_frames(&temp.path().join("single_video"), 50).unwrap();
    let config = TrainingConfig::default();
    let frame_refs: Vec<&std::path::Path> = frames.iter().map(|p| p.as_path()).collect();
    
    let result = backend.train(&frame_refs, &config).await.unwrap();
    assert_eq!(result.iterations_completed, 1000);
    
    // Export both formats
    let ply_path = temp.path().join("output.ply");
    let splat_path = temp.path().join("output.splat");
    
    backend.export_ply(&result, &ply_path).await.unwrap();
    backend.export_splat(&result, &splat_path).await.unwrap();
    
    assert!(ply_path.exists());
    assert!(splat_path.exists());
}

#[tokio::test]
async fn test_mixed_frame_count_videos() {
    // Test processing videos with very different frame counts
    let temp = TempDir::new().unwrap();
    let backend = MockBackend::new();
    
    // Simulate: short clip (10 frames), medium (50 frames), long (200 frames)
    let short_frames = create_test_frames(&temp.path().join("short"), 10).unwrap();
    let medium_frames = create_test_frames(&temp.path().join("medium"), 50).unwrap();
    let long_frames = create_test_frames(&temp.path().join("long"), 200).unwrap();
    
    let mut all_frames = Vec::new();
    all_frames.extend(short_frames);
    all_frames.extend(medium_frames);
    all_frames.extend(long_frames);
    
    assert_eq!(all_frames.len(), 260);
    
    let config = TrainingConfig::default();
    let frame_refs: Vec<&std::path::Path> = all_frames.iter().map(|p| p.as_path()).collect();
    
    let result = backend.train(&frame_refs, &config).await.unwrap();
    assert!(result.iterations_completed > 0);
}
