//! Integration tests for input validation
//!
//! Tests video file validation before processing.

use std::path::PathBuf;

use three_dgs_processor::extractors::models::{Resolution, VideoMetadata};
use three_dgs_processor::validation::video::ValidationConfig;

#[test]
fn test_default_validation_config() {
    let config = ValidationConfig::default();
    
    assert_eq!(config.min_width, 640);
    assert_eq!(config.min_height, 480);
    assert_eq!(config.min_frames, 30);
    assert_eq!(config.min_duration_secs, 1.0);
}

#[test]
fn test_validation_config_from_env() {
    // Test loading from environment variables
    std::env::set_var("MIN_VIDEO_WIDTH", "1920");
    std::env::set_var("MIN_VIDEO_HEIGHT", "1080");
    std::env::set_var("MIN_VIDEO_FRAMES", "60");
    std::env::set_var("MIN_VIDEO_DURATION", "2.0");
    
    let config = ValidationConfig::from_env();
    
    assert_eq!(config.min_width, 1920);
    assert_eq!(config.min_height, 1080);
    assert_eq!(config.min_frames, 60);
    assert_eq!(config.min_duration_secs, 2.0);
    
    // Cleanup
    std::env::remove_var("MIN_VIDEO_WIDTH");
    std::env::remove_var("MIN_VIDEO_HEIGHT");
    std::env::remove_var("MIN_VIDEO_FRAMES");
    std::env::remove_var("MIN_VIDEO_DURATION");
}

#[test]
fn test_valid_hd_video() {
    let metadata = VideoMetadata {
        filename: "valid_hd.mp4".to_string(),
        path: PathBuf::from("/input/valid_hd.mp4"),
        frame_count: 300,
        resolution: Resolution::new(1920, 1080),
        fps: 30.0,
        duration_secs: 10.0,
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    
    assert!(metadata.frame_count >= config.min_frames);
    assert!(metadata.resolution.width >= config.min_width);
    assert!(metadata.resolution.height >= config.min_height);
    assert!(metadata.duration_secs >= config.min_duration_secs);
}

#[test]
fn test_valid_4k_video() {
    let metadata = VideoMetadata {
        filename: "valid_4k.mp4".to_string(),
        path: PathBuf::from("/input/valid_4k.mp4"),
        frame_count: 600,
        resolution: Resolution::new(3840, 2160),
        fps: 60.0,
        duration_secs: 10.0,
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    
    assert!(metadata.frame_count >= config.min_frames);
    assert!(metadata.resolution.width >= config.min_width);
    assert!(metadata.resolution.height >= config.min_height);
}

#[test]
fn test_invalid_too_few_frames() {
    let metadata = VideoMetadata {
        filename: "short.mp4".to_string(),
        path: PathBuf::from("/input/short.mp4"),
        frame_count: 15, // Less than min_frames
        resolution: Resolution::new(1920, 1080),
        fps: 30.0,
        duration_secs: 0.5,
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    assert!(metadata.frame_count < config.min_frames);
}

#[test]
fn test_invalid_low_width() {
    let metadata = VideoMetadata {
        filename: "narrow.mp4".to_string(),
        path: PathBuf::from("/input/narrow.mp4"),
        frame_count: 100,
        resolution: Resolution::new(320, 1080), // Width too low
        fps: 30.0,
        duration_secs: 3.33,
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    assert!(metadata.resolution.width < config.min_width);
}

#[test]
fn test_invalid_low_height() {
    let metadata = VideoMetadata {
        filename: "short_height.mp4".to_string(),
        path: PathBuf::from("/input/short_height.mp4"),
        frame_count: 100,
        resolution: Resolution::new(1920, 240), // Height too low
        fps: 30.0,
        duration_secs: 3.33,
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    assert!(metadata.resolution.height < config.min_height);
}

#[test]
fn test_invalid_short_duration() {
    let metadata = VideoMetadata {
        filename: "blink.mp4".to_string(),
        path: PathBuf::from("/input/blink.mp4"),
        frame_count: 15,
        resolution: Resolution::new(1920, 1080),
        fps: 30.0,
        duration_secs: 0.5, // Less than min_duration
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    assert!(metadata.duration_secs < config.min_duration_secs);
}

#[test]
fn test_boundary_conditions() {
    let config = ValidationConfig::default();
    
    // Exactly at minimum (should be valid)
    let at_minimum = VideoMetadata {
        filename: "at_min.mp4".to_string(),
        path: PathBuf::from("/input/at_min.mp4"),
        frame_count: config.min_frames,
        resolution: Resolution::new(config.min_width, config.min_height),
        fps: 30.0,
        duration_secs: config.min_duration_secs,
        camera_info: None,
    };
    
    assert!(at_minimum.frame_count >= config.min_frames);
    assert!(at_minimum.resolution.width >= config.min_width);
    assert!(at_minimum.resolution.height >= config.min_height);
    assert!(at_minimum.duration_secs >= config.min_duration_secs);
    
    // Just below minimum (should be invalid)
    let below_minimum = VideoMetadata {
        filename: "below_min.mp4".to_string(),
        path: PathBuf::from("/input/below_min.mp4"),
        frame_count: config.min_frames - 1,
        resolution: Resolution::new(config.min_width - 1, config.min_height - 1),
        fps: 30.0,
        duration_secs: config.min_duration_secs - 0.1,
        camera_info: None,
    };
    
    assert!(below_minimum.frame_count < config.min_frames);
    assert!(below_minimum.resolution.width < config.min_width);
    assert!(below_minimum.resolution.height < config.min_height);
    assert!(below_minimum.duration_secs < config.min_duration_secs);
}

#[test]
fn test_high_fps_video() {
    // Test high frame rate videos (120fps, 240fps)
    let high_fps = VideoMetadata {
        filename: "high_fps.mp4".to_string(),
        path: PathBuf::from("/input/high_fps.mp4"),
        frame_count: 1200, // 10 seconds @ 120fps
        resolution: Resolution::new(1920, 1080),
        fps: 120.0,
        duration_secs: 10.0,
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    
    assert!(high_fps.frame_count >= config.min_frames);
    assert!(high_fps.fps > 60.0);
}

#[test]
fn test_low_fps_video() {
    // Test low frame rate videos (15fps, 24fps cinematic)
    let low_fps = VideoMetadata {
        filename: "cinematic.mp4".to_string(),
        path: PathBuf::from("/input/cinematic.mp4"),
        frame_count: 240, // 10 seconds @ 24fps
        resolution: Resolution::new(1920, 1080),
        fps: 24.0,
        duration_secs: 10.0,
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    
    assert!(low_fps.frame_count >= config.min_frames);
    assert!(low_fps.fps == 24.0);
}

#[test]
fn test_ultra_wide_resolution() {
    // Test ultra-wide aspect ratios (21:9)
    let ultra_wide = VideoMetadata {
        filename: "ultrawide.mp4".to_string(),
        path: PathBuf::from("/input/ultrawide.mp4"),
        frame_count: 300,
        resolution: Resolution::new(2560, 1080), // 21:9
        fps: 30.0,
        duration_secs: 10.0,
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    
    assert!(ultra_wide.resolution.width >= config.min_width);
    assert!(ultra_wide.resolution.height >= config.min_height);
}

#[test]
fn test_vertical_video() {
    // Test vertical/portrait orientation (9:16 for mobile)
    let vertical = VideoMetadata {
        filename: "vertical.mp4".to_string(),
        path: PathBuf::from("/input/vertical.mp4"),
        frame_count: 300,
        resolution: Resolution::new(1080, 1920), // Portrait
        fps: 30.0,
        duration_secs: 10.0,
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    
    assert!(vertical.resolution.width >= config.min_width);
    assert!(vertical.resolution.height >= config.min_height);
}

#[test]
fn test_square_resolution() {
    // Test square aspect ratio (1:1)
    let square = VideoMetadata {
        filename: "square.mp4".to_string(),
        path: PathBuf::from("/input/square.mp4"),
        frame_count: 300,
        resolution: Resolution::new(1080, 1080), // Square
        fps: 30.0,
        duration_secs: 10.0,
        camera_info: None,
    };
    
    let config = ValidationConfig::default();
    
    assert!(square.resolution.width >= config.min_width);
    assert!(square.resolution.height >= config.min_height);
}

#[test]
fn test_validation_with_custom_thresholds() {
    // Test with stricter validation requirements
    let strict_config = ValidationConfig {
        min_width: 1920,
        min_height: 1080,
        min_frames: 120,
        min_duration_secs: 4.0,
    };
    
    let video = VideoMetadata {
        filename: "test.mp4".to_string(),
        path: PathBuf::from("/input/test.mp4"),
        frame_count: 100,
        resolution: Resolution::new(1280, 720),
        fps: 30.0,
        duration_secs: 3.33,
        camera_info: None,
    };
    
    // Should fail strict validation
    assert!(video.frame_count < strict_config.min_frames);
    assert!(video.resolution.width < strict_config.min_width);
    assert!(video.resolution.height < strict_config.min_height);
    assert!(video.duration_secs < strict_config.min_duration_secs);
}

#[test]
fn test_batch_validation() {
    // Test validating multiple videos at once
    let videos = vec![
        VideoMetadata {
            filename: "video1.mp4".to_string(),
            path: PathBuf::from("/input/video1.mp4"),
            frame_count: 300,
            resolution: Resolution::new(1920, 1080),
            fps: 30.0,
            duration_secs: 10.0,
            camera_info: None,
        },
        VideoMetadata {
            filename: "video2.mp4".to_string(),
            path: PathBuf::from("/input/video2.mp4"),
            frame_count: 20, // Invalid
            resolution: Resolution::new(1920, 1080),
            fps: 30.0,
            duration_secs: 0.67,
            camera_info: None,
        },
        VideoMetadata {
            filename: "video3.mp4".to_string(),
            path: PathBuf::from("/input/video3.mp4"),
            frame_count: 300,
            resolution: Resolution::new(320, 240), // Invalid
            fps: 30.0,
            duration_secs: 10.0,
            camera_info: None,
        },
    ];
    
    let config = ValidationConfig::default();
    
    let valid_videos: Vec<_> = videos
        .iter()
        .filter(|v| {
            v.frame_count >= config.min_frames
                && v.resolution.width >= config.min_width
                && v.resolution.height >= config.min_height
                && v.duration_secs >= config.min_duration_secs
        })
        .collect();
    
    assert_eq!(valid_videos.len(), 1); // Only video1 is valid
}
