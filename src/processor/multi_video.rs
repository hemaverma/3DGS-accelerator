//! Concurrent multi-video processing orchestration
//!
//! This module provides concurrent frame extraction from multiple videos
//! using futures streams with buffer_unordered for optimal parallelism.

use anyhow::{Context, Result};
use futures::stream::{self, StreamExt};
use tracing::{debug, info, instrument};

use crate::extractors::ffmpeg::{ExtractionOptions, FrameSet, VideoInput};
use crate::extractors::frames::extract_frames_from_video;

/// Process multiple videos concurrently with frame extraction
///
/// This function orchestrates concurrent processing of multiple videos using
/// futures::stream with buffer_unordered to control parallelism. It extracts
/// frames from all videos concurrently (limited by CPU count) and aggregates
/// the results.
///
/// # Arguments
///
/// * `videos` - Vector of video inputs to process
/// * `options` - Frame extraction options to apply to all videos
///
/// # Returns
///
/// A vector of FrameSets, one for each successfully processed video.
/// If any video fails, the entire job fails and returns an error.
///
/// # Errors
///
/// Returns an error if:
/// - Any video fails to process
/// - No videos provided
/// - Frame extraction fails for any video
#[instrument(skip(videos, options), fields(video_count = videos.len()))]
pub async fn process_videos(
    videos: Vec<VideoInput>,
    options: ExtractionOptions,
) -> Result<Vec<FrameSet>> {
    if videos.is_empty() {
        anyhow::bail!("No videos provided for processing");
    }

    let video_count = videos.len();
    info!(
        video_count = video_count,
        "Starting concurrent multi-video processing"
    );

    // Determine concurrency limit based on CPU count
    let concurrency_limit = num_cpus::get();
    debug!(
        concurrency_limit = concurrency_limit,
        "Using CPU-based concurrency limit"
    );

    // Process videos concurrently using futures::stream
    // buffer_unordered maintains up to N futures in flight at once
    let frame_sets: Vec<FrameSet> = stream::iter(videos)
        .map(|video| {
            let options_clone = options.clone();
            async move {
                debug!(
                    video = %video.filename,
                    "Starting frame extraction for video"
                );
                extract_frames_from_video(video, options_clone).await
            }
        })
        .buffer_unordered(concurrency_limit)
        .collect::<Vec<Result<FrameSet>>>()
        .await
        .into_iter()
        .collect::<Result<Vec<FrameSet>>>()
        .context("Failed to extract frames from one or more videos")?;

    info!(
        video_count = video_count,
        total_frames = frame_sets.iter().map(|fs| fs.frame_count).sum::<usize>(),
        "Concurrent multi-video processing completed successfully"
    );

    Ok(frame_sets)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[tokio::test]
    async fn test_process_videos_empty_list() {
        let videos = Vec::new();
        let options = ExtractionOptions::from_env_config(None, None);

        let result = process_videos(videos, options).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No videos provided"));
    }

    #[tokio::test]
    async fn test_process_videos_single_video_missing_file() {
        // Create a video input for a non-existent file
        let video = VideoInput {
            path: PathBuf::from("/nonexistent/video.mp4"),
            filename: "video.mp4".to_string(),
        };
        let videos = vec![video];
        let options = ExtractionOptions::from_env_config(None, None);

        let result = process_videos(videos, options).await;
        assert!(result.is_err());
    }

    // Note: Full integration tests with actual video files will be in tests/integration
    // Unit tests here focus on error handling and validation logic
}
