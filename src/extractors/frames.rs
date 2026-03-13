//! Frame extraction module using FFmpeg
//!
//! This module provides async frame extraction from video files using FFmpeg.
//! FFmpeg operations run in spawn_blocking to avoid blocking the async runtime.

use anyhow::{Context, Result};
use tempfile::TempDir;
use tracing::{debug, instrument};

use super::ffmpeg::{extract_frames_sync, ExtractionOptions, FrameSet, VideoInput};

/// Extract frames from a single video file asynchronously
///
/// This function wraps the synchronous FFmpeg extraction in spawn_blocking
/// to prevent blocking the async runtime during I/O operations.
///
/// # Arguments
///
/// * `video` - The input video file
/// * `options` - Frame extraction options (frame rate, frame count, etc.)
///
/// # Returns
///
/// A FrameSet containing paths to all extracted frames
#[instrument(skip(video, options), fields(video = %video.filename))]
pub async fn extract_frames_from_video(
    video: VideoInput,
    options: ExtractionOptions,
) -> Result<FrameSet> {
    debug!(
        video = %video.path.display(),
        "Starting async frame extraction"
    );

    // Validate video before processing
    video.validate()?;

    // Create temporary directory for extracted frames
    // Using a unique subdirectory per video to avoid conflicts
    let temp_dir = TempDir::new().context("Failed to create temporary directory for frames")?;
    let output_dir = temp_dir.path().to_path_buf();

    debug!(
        video = %video.path.display(),
        output_dir = %output_dir.display(),
        "Created temporary directory for frames"
    );

    // Clone for move into spawn_blocking
    let video_clone = video.clone();
    let output_dir_clone = output_dir.clone();
    let options_clone = options.clone();

    // Run FFmpeg extraction in spawn_blocking to avoid blocking async runtime
    let frame_paths = tokio::task::spawn_blocking(move || {
        extract_frames_sync(&video_clone, &output_dir_clone, &options_clone)
    })
    .await
    .context("Failed to spawn FFmpeg extraction task")??;

    if frame_paths.is_empty() {
        anyhow::bail!("No frames extracted from video: {}", video.path.display());
    }

    // Create FrameSet with extracted frames
    let frame_set = FrameSet::new(frame_paths, output_dir, video);

    debug!(
        frame_count = frame_set.frame_count,
        "Frame extraction completed successfully"
    );

    // Note: TempDir will be dropped and cleaned up when FrameSet is dropped
    // unless the caller explicitly persists the frames to another location
    std::mem::forget(temp_dir); // Prevent automatic cleanup

    Ok(frame_set)
}

/// Extract frames from multiple videos concurrently
///
/// This function delegates to the processor::multi_video module for
/// concurrent processing of multiple videos using futures::stream.
/// All videos are processed in parallel (limited by CPU count) and
/// results are aggregated. If any video fails, the entire operation fails.
pub async fn extract_frames_from_videos(
    videos: Vec<VideoInput>,
    options: ExtractionOptions,
) -> Result<Vec<FrameSet>> {
    crate::processor::process_videos(videos, options).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extraction_options_creation() {
        let opts = ExtractionOptions::from_env_config(Some(1.0), None);
        assert_eq!(opts.frame_rate, Some(1.0));
        assert_eq!(opts.frame_count, None);

        let opts = ExtractionOptions::from_env_config(None, Some(10));
        assert_eq!(opts.frame_rate, None);
        assert_eq!(opts.frame_count, Some(10));
    }

    #[tokio::test]
    async fn test_extract_nonexistent_video_fails() {
        let video = VideoInput {
            path: PathBuf::from("/nonexistent/video.mp4"),
            filename: "video.mp4".to_string(),
        };
        let options = ExtractionOptions::from_env_config(None, None);

        let result = extract_frames_from_video(video, options).await;
        assert!(result.is_err());
    }
}
