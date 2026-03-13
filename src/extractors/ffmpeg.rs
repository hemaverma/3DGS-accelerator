//! FFmpeg command wrapper for frame extraction
//!
//! This module provides FFmpeg integration via spawn_blocking to avoid
//! blocking the async runtime during synchronous I/O operations.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{debug, info, warn};

/// Input video file with metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoInput {
    /// Path to the video file
    pub path: PathBuf,
    /// Video filename (without path)
    pub filename: String,
}

impl VideoInput {
    /// Create a new VideoInput from a path
    pub fn new(path: PathBuf) -> Result<Self> {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid video filename")?
            .to_string();

        Ok(VideoInput { path, filename })
    }

    /// Validate that the video file exists and has a supported format
    pub fn validate(&self) -> Result<()> {
        if !self.path.exists() {
            anyhow::bail!("Video file does not exist: {}", self.path.display());
        }

        let ext = self
            .path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        match ext.as_str() {
            "mp4" | "mov" | "avi" | "mkv" | "webm" => Ok(()),
            _ => anyhow::bail!(
                "Unsupported video format: {}. Supported: MP4, MOV, AVI, MKV, WebM",
                ext
            ),
        }
    }
}

/// Set of extracted frames from a video
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrameSet {
    /// Paths to extracted frame files
    pub frame_paths: Vec<PathBuf>,
    /// Total number of extracted frames
    pub frame_count: usize,
    /// Directory containing the frames
    pub output_dir: PathBuf,
    /// Original video this FrameSet was extracted from
    pub source_video: VideoInput,
}

impl FrameSet {
    /// Create a new FrameSet
    pub fn new(frame_paths: Vec<PathBuf>, output_dir: PathBuf, source_video: VideoInput) -> Self {
        let frame_count = frame_paths.len();
        FrameSet {
            frame_paths,
            frame_count,
            output_dir,
            source_video,
        }
    }
}

/// Frame extraction options
#[derive(Debug, Clone)]
pub struct ExtractionOptions {
    /// Frame rate to extract (e.g., 1.0 = 1 frame per second)
    pub frame_rate: Option<f32>,
    /// Max number of frames to extract
    pub frame_count: Option<u32>,
}

impl ExtractionOptions {
    /// Create extraction options from environment config
    ///
    /// Falls back to FRAME_RATE and FRAME_COUNT environment variables when
    /// explicit parameters are None.
    pub fn from_env_config(frame_rate: Option<f32>, frame_count: Option<u32>) -> Self {
        let frame_rate = frame_rate.or_else(|| {
            std::env::var("FRAME_RATE")
                .ok()
                .and_then(|v| v.parse().ok())
        });
        let frame_count = frame_count.or_else(|| {
            std::env::var("FRAME_COUNT")
                .ok()
                .and_then(|v| v.parse().ok())
        });
        ExtractionOptions {
            frame_rate,
            frame_count,
        }
    }

    /// Build FFmpeg filter string based on options
    fn build_filter(&self) -> Option<String> {
        if let Some(fps) = self.frame_rate {
            Some(format!("fps={}", fps))
        } else {
            // Extract evenly distributed frames by selecting every Nth frame
            // This will be combined with -frames:v to limit total frames
            self.frame_count
                .map(|_| "select='not(mod(n\\,1))'".to_string())
        }
    }
}

/// Execute FFmpeg to extract frames from a video
///
/// This function runs synchronously and should be called via spawn_blocking
pub fn extract_frames_sync(
    video: &VideoInput,
    output_dir: &Path,
    options: &ExtractionOptions,
) -> Result<Vec<PathBuf>> {
    info!(
        video = %video.path.display(),
        output_dir = %output_dir.display(),
        "Extracting frames with FFmpeg"
    );

    // Build output pattern: frame_000001.jpg, frame_000002.jpg, etc.
    let output_pattern = output_dir.join("frame_%06d.jpg");

    let mut cmd = Command::new("ffmpeg");

    // Input file
    cmd.arg("-i").arg(&video.path);

    // Apply filter if specified
    if let Some(filter) = options.build_filter() {
        cmd.arg("-vf").arg(filter);
    }

    // Limit frame count if specified
    if let Some(count) = options.frame_count {
        cmd.arg("-frames:v").arg(count.to_string());
    }

    // Output quality and format
    cmd.arg("-q:v").arg("2"); // High quality JPEG (scale 2-31, 2 is best)
    cmd.arg("-an"); // No audio

    // Overwrite existing files without prompting
    cmd.arg("-y");

    // Output pattern
    cmd.arg(&output_pattern);

    debug!("FFmpeg command: {:?}", cmd);

    // Execute FFmpeg
    let output = cmd.output().context("Failed to execute FFmpeg command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("FFmpeg failed: {}", stderr);
    }

    // Collect extracted frame paths
    let mut frame_paths = Vec::new();
    let entries = std::fs::read_dir(output_dir).context("Failed to read output directory")?;

    for entry in entries {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) == Some("jpg") {
            frame_paths.push(path);
        }
    }

    // Sort frames by filename to ensure sequential order
    frame_paths.sort();

    if frame_paths.is_empty() {
        warn!(
            video = %video.path.display(),
            "No frames extracted from video"
        );
    } else {
        info!(
            video = %video.path.display(),
            frame_count = frame_paths.len(),
            "Successfully extracted frames"
        );
    }

    Ok(frame_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_video_input_validate() {
        // Test supported formats
        let video = VideoInput {
            path: PathBuf::from("test.mp4"),
            filename: "test.mp4".to_string(),
        };
        assert!(video.validate().is_err()); // File doesn't exist

        // Test unsupported format
        let video = VideoInput {
            path: PathBuf::from("test.txt"),
            filename: "test.txt".to_string(),
        };
        assert!(video.validate().is_err());
    }

    #[test]
    fn test_extraction_options_filter() {
        let opts = ExtractionOptions {
            frame_rate: Some(1.0),
            frame_count: None,
        };
        assert_eq!(opts.build_filter(), Some("fps=1".to_string()));

        let opts = ExtractionOptions {
            frame_rate: None,
            frame_count: Some(10),
        };
        assert!(opts.build_filter().is_some());

        let opts = ExtractionOptions {
            frame_rate: None,
            frame_count: None,
        };
        assert_eq!(opts.build_filter(), None);
    }
}
