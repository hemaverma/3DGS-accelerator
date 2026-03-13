pub mod ffmpeg;
pub mod frames;
pub mod metadata;
pub mod models;

// Re-export common types for convenience
pub use ffmpeg::{ExtractionOptions, FrameSet, VideoInput};
pub use frames::{extract_frames_from_video, extract_frames_from_videos};
pub use metadata::{extract_metadata, extract_metadata_sync};
pub use models::{CameraInfo, GpsCoordinates, Orientation, Resolution, VideoMetadata};
