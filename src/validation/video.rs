//! Video file validation
//!
//! Validates video files before processing:
//! - File integrity (readable, valid codec)
//! - Minimum resolution
//! - Sufficient frame count
//! - Supported formats

use crate::error::{ProcessorError, ValidationError};
use crate::extractors::ffmpeg::VideoInput;
use crate::extractors::metadata::extract_metadata;
use crate::extractors::models::VideoMetadata;
use std::path::Path;
use tracing::{info, warn};

/// Video validation configuration
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Minimum video width in pixels
    pub min_width: u32,
    /// Minimum video height in pixels
    pub min_height: u32,
    /// Minimum number of frames required
    pub min_frames: u32,
    /// Minimum video duration in seconds
    pub min_duration_secs: f64,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            min_width: 640,
            min_height: 480,
            min_frames: 30,
            min_duration_secs: 1.0,
        }
    }
}

impl ValidationConfig {
    /// Create validation config from environment variables
    pub fn from_env() -> Self {
        Self {
            min_width: std::env::var("MIN_VIDEO_WIDTH")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(640),
            min_height: std::env::var("MIN_VIDEO_HEIGHT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(480),
            min_frames: std::env::var("MIN_VIDEO_FRAMES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(30),
            min_duration_secs: std::env::var("MIN_VIDEO_DURATION")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1.0),
        }
    }
}

/// Video file validator
pub struct VideoValidator {
    config: ValidationConfig,
}

impl VideoValidator {
    /// Create a new validator with the given configuration
    pub fn new(config: ValidationConfig) -> Self {
        Self { config }
    }

    /// Create a validator with default configuration
    pub fn with_defaults() -> Self {
        Self::new(ValidationConfig::default())
    }

    /// Create a validator from environment variables
    pub fn from_env() -> Self {
        Self::new(ValidationConfig::from_env())
    }

    /// Validate a video file
    ///
    /// Checks:
    /// 1. File exists
    /// 2. File is readable
    /// 3. Supported format (MP4, MOV, AVI, MKV, WebM)
    /// 4. Valid codec
    /// 5. Minimum resolution
    /// 6. Minimum frame count
    /// 7. Minimum duration
    pub async fn validate(&self, path: &Path) -> Result<VideoMetadata, ProcessorError> {
        info!(path = %path.display(), "Validating video file");

        // 1-3. Create VideoInput and validate format/existence
        let video_input =
            VideoInput::new(path.to_path_buf()).map_err(|e| ValidationError::CorruptedFile {
                path: path.to_path_buf(),
                source: Some(e),
            })?;

        video_input.validate().map_err(|e| {
            if e.to_string().contains("does not exist") {
                ValidationError::FileNotFound {
                    path: path.to_path_buf(),
                }
            } else if e.to_string().contains("Unsupported video format") {
                let ext = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                ValidationError::UnsupportedFormat { format: ext }
            } else {
                ValidationError::CorruptedFile {
                    path: path.to_path_buf(),
                    source: Some(e),
                }
            }
        })?;

        // 4. Extract metadata using FFprobe (also validates codec)
        let metadata =
            extract_metadata(&video_input)
                .await
                .map_err(|e| ValidationError::CorruptedFile {
                    path: path.to_path_buf(),
                    source: Some(e),
                })?;

        // 5. Validate resolution
        self.validate_resolution(&metadata)?;

        // 6. Validate frame count
        self.validate_frame_count(&metadata)?;

        // 7. Validate duration
        self.validate_duration(&metadata)?;

        info!(
            path = %path.display(),
            width = metadata.resolution.width,
            height = metadata.resolution.height,
            frames = metadata.frame_count,
            duration = metadata.duration_secs,
            "Video validation passed"
        );

        Ok(metadata)
    }

    /// Validate video resolution meets minimum requirements
    fn validate_resolution(&self, metadata: &VideoMetadata) -> Result<(), ValidationError> {
        if metadata.resolution.width < self.config.min_width
            || metadata.resolution.height < self.config.min_height
        {
            warn!(
                width = metadata.resolution.width,
                height = metadata.resolution.height,
                min_width = self.config.min_width,
                min_height = self.config.min_height,
                "Video resolution below minimum"
            );
            return Err(ValidationError::ResolutionTooLow {
                width: metadata.resolution.width,
                height: metadata.resolution.height,
                min_width: self.config.min_width,
                min_height: self.config.min_height,
            });
        }
        Ok(())
    }

    /// Validate video has sufficient frames for 3DGS
    fn validate_frame_count(&self, metadata: &VideoMetadata) -> Result<(), ValidationError> {
        if metadata.frame_count < self.config.min_frames {
            warn!(
                frame_count = metadata.frame_count,
                min_frames = self.config.min_frames,
                "Video has insufficient frames"
            );
            return Err(ValidationError::InsufficientFrames {
                frame_count: metadata.frame_count,
                min_frames: self.config.min_frames,
            });
        }
        Ok(())
    }

    /// Validate video duration
    fn validate_duration(&self, metadata: &VideoMetadata) -> Result<(), ValidationError> {
        if metadata.duration_secs < self.config.min_duration_secs {
            warn!(
                duration = metadata.duration_secs,
                min_duration = self.config.min_duration_secs,
                "Video duration too short"
            );
            return Err(ValidationError::DurationTooShort {
                duration_secs: metadata.duration_secs,
                min_duration_secs: self.config.min_duration_secs,
            });
        }
        Ok(())
    }

    /// Quick format check without full validation (synchronous)
    pub fn check_format_sync(path: &Path) -> Result<(), ValidationError> {
        if !path.exists() {
            return Err(ValidationError::FileNotFound {
                path: path.to_path_buf(),
            });
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        match ext.as_str() {
            "mp4" | "mov" | "avi" | "mkv" | "webm" => Ok(()),
            _ => Err(ValidationError::UnsupportedFormat { format: ext }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::TempDir;

    #[test]
    fn test_default_config() {
        let config = ValidationConfig::default();
        assert_eq!(config.min_width, 640);
        assert_eq!(config.min_height, 480);
        assert_eq!(config.min_frames, 30);
        assert_eq!(config.min_duration_secs, 1.0);
    }

    #[test]
    fn test_check_format_sync() {
        // Test file doesn't exist
        assert!(VideoValidator::check_format_sync(Path::new("test.mp4")).is_err());
        assert!(VideoValidator::check_format_sync(Path::new("test.mov")).is_err());

        // Test unsupported format with existing file
        let temp_dir = TempDir::new().unwrap();
        let txt_file = temp_dir.path().join("test.txt");
        File::create(&txt_file).unwrap();

        let result = VideoValidator::check_format_sync(&txt_file);
        assert!(matches!(
            result,
            Err(ValidationError::UnsupportedFormat { .. })
        ));
    }

    #[test]
    fn test_validation_config_from_env() {
        // Test with no env vars set (should use defaults)
        let config = ValidationConfig::from_env();
        assert_eq!(config.min_width, 640);
        assert_eq!(config.min_height, 480);
    }
}
