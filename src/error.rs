//! Comprehensive error types for 3DGS Video Processor
//!
//! This module defines error types covering all failure scenarios:
//! - Input validation (video format, resolution, frames)
//! - COLMAP reconstruction failures
//! - 3DGS training failures
//! - Storage and disk space errors
//! - Azure authentication and network errors
//! - Configuration errors

use std::path::PathBuf;
use thiserror::Error;

/// Comprehensive error type for all processor failures
#[derive(Error, Debug)]
pub enum ProcessorError {
    /// Video validation errors
    #[error("Video validation failed: {0}")]
    Validation(#[from] ValidationError),

    /// COLMAP processing errors
    #[error("COLMAP processing failed: {0}")]
    Colmap(#[from] ColmapError),

    /// 3DGS training errors
    #[error("3DGS training failed: {0}")]
    Training(#[from] TrainingError),

    /// Storage and disk space errors
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),

    /// Azure authentication and mount errors
    #[error("Azure error: {0}")]
    Azure(#[from] AzureError),

    /// Network and connectivity errors
    #[error("Network error: {0}")]
    Network(#[from] NetworkError),

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    /// FFmpeg/FFprobe execution errors
    #[error("FFmpeg error: {0}")]
    Ffmpeg(#[from] FfmpegError),

    /// File watcher errors
    #[error("File watcher error: {0}")]
    Watcher(#[from] WatcherError),

    /// Job processing errors
    #[error("Job processing error: {0}")]
    Job(String),

    /// Generic I/O errors
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// Other errors
    #[error("{0}")]
    Other(String),
}

impl ProcessorError {
    /// Determine if this error is retryable
    ///
    /// Returns true if the error is transient and retrying might succeed.
    /// Returns false if the error is permanent and retrying won't help.
    ///
    /// # Retryable Errors (Transient)
    ///
    /// - Network errors (connection lost, timeout, DNS failures)
    /// - Azure errors (authentication, mount failures - may be temporary)
    /// - Storage errors (disk space issues - may be resolved)
    /// - FFmpeg errors (process failures - may be temporary)
    /// - I/O errors (may be temporary)
    /// - Training errors (except NoGpu)
    /// - COLMAP errors (except InsufficientPoints)
    ///
    /// # Non-Retryable Errors (Permanent)
    ///
    /// - Validation errors (bad input data won't improve with retry)
    /// - Configuration errors (misconfiguration won't fix itself)
    /// - Training errors with NoGpu (hardware limitation)
    /// - COLMAP errors with InsufficientPoints (input quality issue)
    pub fn is_retryable(&self) -> bool {
        match self {
            // Never retry validation errors - input data is bad
            ProcessorError::Validation(_) => false,

            // Never retry configuration errors - config won't fix itself
            ProcessorError::Config(_) => false,

            // COLMAP errors: retry except for insufficient points (bad data)
            ProcessorError::Colmap(e) => !matches!(e, ColmapError::InsufficientPoints { .. }),

            // Training errors: retry except for NoGpu (hardware limitation)
            ProcessorError::Training(e) => !matches!(e, TrainingError::NoGpu),

            // All other errors are potentially transient
            ProcessorError::Storage(_) => true,
            ProcessorError::Azure(_) => true,
            ProcessorError::Network(_) => true,
            ProcessorError::Ffmpeg(_) => true,
            ProcessorError::Watcher(_) => true,
            ProcessorError::Job(_) => true,
            ProcessorError::Io(_) => true,
            ProcessorError::Other(_) => true,
        }
    }
}


/// Video validation errors
#[derive(Error, Debug)]
pub enum ValidationError {
    #[error("Video file does not exist: {path}")]
    FileNotFound { path: PathBuf },

    #[error("Unsupported video format: {format}. Supported formats: MP4, MOV, AVI, MKV, WebM")]
    UnsupportedFormat { format: String },

    #[error("Video resolution {width}x{height} is below minimum {min_width}x{min_height}")]
    ResolutionTooLow {
        width: u32,
        height: u32,
        min_width: u32,
        min_height: u32,
    },

    #[error("Video has only {frame_count} frames, minimum required: {min_frames}")]
    InsufficientFrames { frame_count: u32, min_frames: u32 },

    #[error("Video file is corrupted or unreadable: {path}")]
    CorruptedFile {
        path: PathBuf,
        #[source]
        source: Option<anyhow::Error>,
    },

    #[error("Invalid video codec: {codec}")]
    InvalidCodec { codec: String },

    #[error("Cannot determine video duration: {path}")]
    NoDuration { path: PathBuf },

    #[error("Video duration {duration_secs}s is too short (minimum: {min_duration_secs}s)")]
    DurationTooShort {
        duration_secs: f64,
        min_duration_secs: f64,
    },
}

/// COLMAP reconstruction errors
#[derive(Error, Debug)]
pub enum ColmapError {
    #[error("COLMAP feature extraction failed: {message}")]
    FeatureExtractionFailed { message: String },

    #[error("COLMAP feature matching failed: {message}")]
    FeatureMatchingFailed { message: String },

    #[error("COLMAP sparse reconstruction failed: {message}")]
    SparseReconstructionFailed { message: String },

    #[error("COLMAP produced insufficient points: {point_count} (minimum: {min_points})")]
    InsufficientPoints {
        point_count: usize,
        min_points: usize,
    },

    #[error("COLMAP executable not found: {path}")]
    ExecutableNotFound { path: String },

    #[error("COLMAP output directory missing: {path}")]
    OutputDirMissing { path: PathBuf },

    #[error("COLMAP process timed out after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },
}

/// 3DGS training errors
#[derive(Error, Debug)]
pub enum TrainingError {
    #[error("Backend plugin not found: {backend_name}")]
    BackendNotFound { backend_name: String },

    #[error("Backend plugin failed to load: {path}")]
    PluginLoadFailed {
        path: PathBuf,
        #[source]
        source: anyhow::Error,
    },

    #[error("Training process failed: {message}")]
    TrainingFailed { message: String },

    #[error("Training produced no output: {expected_path}")]
    NoOutput { expected_path: PathBuf },

    #[error("Training process timed out after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    #[error("Invalid training configuration: {message}")]
    InvalidConfig { message: String },

    #[error("GPU not available for training")]
    NoGpu,
}

/// Storage and disk space errors
#[derive(Error, Debug)]
pub enum StorageError {
    #[error("Insufficient disk space: {available_gb}GB available, {required_gb}GB required")]
    InsufficientSpace { available_gb: u64, required_gb: u64 },

    #[error("Output directory does not exist: {path}")]
    OutputDirMissing { path: PathBuf },

    #[error("Cannot create directory: {path}")]
    CannotCreateDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Cannot write to directory: {path}")]
    CannotWrite {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Disk full: cannot write to {path}")]
    DiskFull { path: PathBuf },

    #[error("File already exists: {path}")]
    FileExists { path: PathBuf },
}

/// Azure authentication and mount errors
#[derive(Error, Debug)]
pub enum AzureError {
    #[error("Azure authentication failed: {message}")]
    AuthenticationFailed { message: String },

    #[error("Blobfuse2 mount failed: {message}")]
    MountFailed { message: String },

    #[error("Blobfuse2 executable not found")]
    BlobfuseNotFound,

    #[error("Azure connection string invalid or missing")]
    InvalidConnectionString,

    #[error("Azure storage account not found: {account_name}")]
    StorageAccountNotFound { account_name: String },

    #[error("Azure container not found: {container_name}")]
    ContainerNotFound { container_name: String },

    #[error("Mount point already in use: {path}")]
    MountPointInUse { path: PathBuf },

    #[error("Unmount failed: {path}")]
    UnmountFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// Network and connectivity errors
#[derive(Error, Debug)]
pub enum NetworkError {
    #[error("Network connection lost")]
    ConnectionLost,

    #[error("Network timeout after {timeout_secs}s")]
    Timeout { timeout_secs: u64 },

    #[error("Azure Blob Storage network error: {message}")]
    AzureBlobError { message: String },

    #[error("DNS resolution failed: {host}")]
    DnsResolutionFailed { host: String },

    #[error("Connection refused: {host}:{port}")]
    ConnectionRefused { host: String, port: u16 },
}

/// Configuration errors
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Missing required environment variable: {var_name}")]
    MissingEnvVar { var_name: String },

    #[error("Invalid environment variable value: {var_name}={value}")]
    InvalidEnvVar { var_name: String, value: String },

    #[error("Configuration file not found: {path}")]
    ConfigFileNotFound { path: PathBuf },

    #[error("Configuration file parse error: {path}")]
    ConfigParseFailed {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("Invalid configuration: {message}")]
    InvalidConfig { message: String },
}

/// FFmpeg/FFprobe errors
#[derive(Error, Debug)]
pub enum FfmpegError {
    #[error("FFmpeg executable not found")]
    FfmpegNotFound,

    #[error("FFprobe executable not found")]
    FfprobeNotFound,

    #[error("FFmpeg command failed: {command}")]
    CommandFailed {
        command: String,
        #[source]
        source: std::io::Error,
    },

    #[error("FFmpeg process failed with exit code {exit_code}: {stderr}")]
    ProcessFailed { exit_code: i32, stderr: String },

    #[error("Cannot parse FFprobe output: {message}")]
    ParseFailed { message: String },
}

/// File watcher errors
#[derive(Error, Debug)]
pub enum WatcherError {
    #[error("Failed to initialize file watcher: {message}")]
    InitFailed { message: String },

    #[error("File watcher event error: {message}")]
    EventError { message: String },

    #[error("Watch path does not exist: {path}")]
    PathNotFound { path: PathBuf },
}

/// Result type alias for processor operations
pub type Result<T> = std::result::Result<T, ProcessorError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_errors_not_retryable() {
        let err = ProcessorError::Validation(ValidationError::FileNotFound {
            path: PathBuf::from("/test.mp4"),
        });
        assert!(!err.is_retryable(), "Validation errors should not be retryable");
    }

    #[test]
    fn test_config_errors_not_retryable() {
        let err = ProcessorError::Config(ConfigError::MissingEnvVar {
            var_name: "TEST_VAR".to_string(),
        });
        assert!(!err.is_retryable(), "Config errors should not be retryable");
    }

    #[test]
    fn test_network_errors_retryable() {
        let err = ProcessorError::Network(NetworkError::ConnectionLost);
        assert!(err.is_retryable(), "Network errors should be retryable");
    }

    #[test]
    fn test_azure_errors_retryable() {
        let err = ProcessorError::Azure(AzureError::MountFailed {
            message: "Failed to mount".to_string(),
        });
        assert!(err.is_retryable(), "Azure errors should be retryable");
    }

    #[test]
    fn test_storage_errors_retryable() {
        let err = ProcessorError::Storage(StorageError::InsufficientSpace {
            available_gb: 5,
            required_gb: 10,
        });
        assert!(err.is_retryable(), "Storage errors should be retryable");
    }

    #[test]
    fn test_ffmpeg_errors_retryable() {
        let err = ProcessorError::Ffmpeg(FfmpegError::FfmpegNotFound);
        assert!(err.is_retryable(), "FFmpeg errors should be retryable");
    }

    #[test]
    fn test_colmap_insufficient_points_not_retryable() {
        let err = ProcessorError::Colmap(ColmapError::InsufficientPoints {
            point_count: 10,
            min_points: 100,
        });
        assert!(
            !err.is_retryable(),
            "COLMAP insufficient points error should not be retryable"
        );
    }

    #[test]
    fn test_colmap_other_errors_retryable() {
        let err = ProcessorError::Colmap(ColmapError::Timeout { timeout_secs: 60 });
        assert!(
            err.is_retryable(),
            "COLMAP timeout error should be retryable"
        );
    }

    #[test]
    fn test_training_no_gpu_not_retryable() {
        let err = ProcessorError::Training(TrainingError::NoGpu);
        assert!(
            !err.is_retryable(),
            "Training NoGpu error should not be retryable"
        );
    }

    #[test]
    fn test_training_other_errors_retryable() {
        let err = ProcessorError::Training(TrainingError::TrainingFailed {
            message: "Failed".to_string(),
        });
        assert!(
            err.is_retryable(),
            "Training failures should be retryable"
        );
    }

    #[test]
    fn test_io_errors_retryable() {
        let err = ProcessorError::Io(std::io::Error::other("Test error"));
        assert!(err.is_retryable(), "IO errors should be retryable");
    }
}
