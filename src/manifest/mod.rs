//! Manifest generation for video processing jobs
//!
//! This module generates JSON manifest files that describe video processing jobs
//! for COLMAP and 3DGS backends. Manifests include video metadata, frame lists,
//! and camera intrinsics.

use anyhow::{Context, Result};
use std::path::Path;
use tracing::{debug, info, instrument};

pub mod intrinsics;
pub mod models;

pub use models::{CameraIntrinsics, FrameEntry, Manifest, VideoEntry};

use crate::extractors::ffmpeg::FrameSet;
use crate::extractors::models::VideoMetadata;

/// Generate manifest from video metadata and extracted frames
///
/// # Arguments
///
/// * `job_id` - Unique identifier for this processing job
/// * `metadata` - Video metadata for all videos in the job
/// * `frame_sets` - Extracted frame sets for all videos
/// * `input_dir` - Input directory (checked for camera_intrinsics.yaml override)
///
/// # Returns
///
/// Complete manifest ready to be written to disk
#[instrument(skip(metadata, frame_sets), fields(job_id = %job_id, num_videos = metadata.len()))]
pub fn generate_manifest(
    job_id: String,
    metadata: &[VideoMetadata],
    frame_sets: &[FrameSet],
    input_dir: &Path,
) -> Result<Manifest> {
    info!(
        job_id = %job_id,
        num_videos = metadata.len(),
        "Generating manifest"
    );

    // Validate inputs
    if metadata.len() != frame_sets.len() {
        anyhow::bail!(
            "Metadata and frame sets count mismatch: {} vs {}",
            metadata.len(),
            frame_sets.len()
        );
    }

    if metadata.is_empty() {
        anyhow::bail!("Cannot generate manifest with no videos");
    }

    // Create manifest with job ID and current timestamp
    let mut manifest = Manifest::new(job_id);

    // Add video entries
    for (meta, frames) in metadata.iter().zip(frame_sets.iter()) {
        let video_entry = create_video_entry(meta, frames)?;
        manifest.add_video(video_entry);
    }

    // Compute or load camera intrinsics
    let intrinsics = intrinsics::compute_camera_intrinsics(metadata, input_dir)?;
    if let Some(intrinsics) = intrinsics {
        info!(
            model = %intrinsics.model,
            fx = intrinsics.fx,
            fy = intrinsics.fy,
            "Using camera intrinsics"
        );
        manifest.set_camera_intrinsics(intrinsics);
    } else {
        info!("No camera intrinsics available, COLMAP will auto-calibrate");
    }

    debug!("Manifest generation complete");
    Ok(manifest)
}

/// Create a video entry from metadata and frame set
fn create_video_entry(metadata: &VideoMetadata, frame_set: &FrameSet) -> Result<VideoEntry> {
    debug!(
        video = %metadata.filename,
        frame_count = frame_set.frame_count,
        "Creating video entry"
    );

    // Validate frame count matches
    if frame_set.frame_count != metadata.frame_count as usize {
        // This is a warning, not an error - extracted frame count may differ from video frame count
        // when using frame rate sampling or max frame count limits
        debug!(
            video = %metadata.filename,
            extracted = frame_set.frame_count,
            total = metadata.frame_count,
            "Extracted frame count differs from total video frames"
        );
    }

    // Create frame entries
    let frames = create_frame_entries(frame_set, metadata.fps)?;

    // Extract optional fields from camera info
    let (recording_timestamp, camera_model) = if let Some(ref camera_info) = metadata.camera_info {
        (
            camera_info.recording_timestamp.clone(),
            camera_info.camera_model.clone(),
        )
    } else {
        (None, None)
    };

    let video_entry = VideoEntry {
        filename: metadata.filename.clone(),
        frame_count: frame_set.frame_count as u32,
        resolution: metadata.resolution.to_string(),
        fps: metadata.fps,
        duration_secs: metadata.duration_secs,
        recording_timestamp,
        camera_model,
        frames,
    };

    Ok(video_entry)
}

/// Create frame entries from a frame set
fn create_frame_entries(frame_set: &FrameSet, fps: f64) -> Result<Vec<FrameEntry>> {
    let mut frames = Vec::with_capacity(frame_set.frame_count);

    for (idx, frame_path) in frame_set.frame_paths.iter().enumerate() {
        let filename = frame_path
            .file_name()
            .and_then(|n| n.to_str())
            .context("Invalid frame filename")?
            .to_string();

        // Calculate timestamp offset based on frame index and FPS
        let timestamp_offset = if fps > 0.0 { idx as f64 / fps } else { 0.0 };

        frames.push(FrameEntry::new(idx as u32, timestamp_offset, filename));
    }

    Ok(frames)
}

/// Write manifest to disk as JSON file
///
/// # Arguments
///
/// * `manifest` - The manifest to write
/// * `output_path` - Path where manifest.json should be written
#[instrument(skip(manifest), fields(output_path = %output_path.display()))]
pub fn write_manifest(manifest: &Manifest, output_path: &Path) -> Result<()> {
    info!(
        output_path = %output_path.display(),
        num_videos = manifest.videos.len(),
        "Writing manifest to disk"
    );

    manifest.write_to_file(output_path)?;

    info!(
        output_path = %output_path.display(),
        "Manifest written successfully"
    );

    Ok(())
}

/// Generate and write manifest in one operation
///
/// Convenience function that combines generate_manifest and write_manifest
pub fn generate_and_write_manifest(
    job_id: String,
    metadata: &[VideoMetadata],
    frame_sets: &[FrameSet],
    input_dir: &Path,
    output_path: &Path,
) -> Result<()> {
    let manifest = generate_manifest(job_id, metadata, frame_sets, input_dir)?;
    write_manifest(&manifest, output_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extractors::ffmpeg::VideoInput;
    use crate::extractors::models::{CameraInfo, Resolution};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_metadata() -> VideoMetadata {
        VideoMetadata {
            filename: "test.mp4".to_string(),
            path: PathBuf::from("/tmp/test.mp4"),
            frame_count: 120,
            resolution: Resolution::new(1920, 1080),
            fps: 30.0,
            duration_secs: 4.0,
            camera_info: Some(CameraInfo {
                recording_timestamp: Some("2026-02-11T18:15:23Z".to_string()),
                camera_model: Some("iPhone 15 Pro".to_string()),
                gps_coordinates: None,
                orientation: None,
                focal_length_mm: None,
                focal_length_35mm: None,
                sensor_width_mm: None,
                sensor_height_mm: None,
            }),
        }
    }

    fn create_test_frame_set() -> FrameSet {
        let frame_paths = vec![
            PathBuf::from("/tmp/frames/frame_000001.jpg"),
            PathBuf::from("/tmp/frames/frame_000002.jpg"),
            PathBuf::from("/tmp/frames/frame_000003.jpg"),
        ];

        FrameSet::new(
            frame_paths,
            PathBuf::from("/tmp/frames"),
            VideoInput::new(PathBuf::from("/tmp/test.mp4")).unwrap(),
        )
    }

    #[test]
    fn test_create_frame_entries() {
        let frame_set = create_test_frame_set();
        let frames = create_frame_entries(&frame_set, 30.0).unwrap();

        assert_eq!(frames.len(), 3);
        assert_eq!(frames[0].frame_number, 0);
        assert_eq!(frames[0].filename, "frame_000001.jpg");
        assert!((frames[0].timestamp_offset - 0.0).abs() < 0.001);

        assert_eq!(frames[1].frame_number, 1);
        assert!((frames[1].timestamp_offset - 0.0333).abs() < 0.001);

        assert_eq!(frames[2].frame_number, 2);
        assert!((frames[2].timestamp_offset - 0.0666).abs() < 0.001);
    }

    #[test]
    fn test_create_video_entry() {
        let metadata = create_test_metadata();
        let frame_set = create_test_frame_set();

        let video_entry = create_video_entry(&metadata, &frame_set).unwrap();

        assert_eq!(video_entry.filename, "test.mp4");
        assert_eq!(video_entry.frame_count, 3);
        assert_eq!(video_entry.resolution, "1920x1080");
        assert_eq!(video_entry.fps, 30.0);
        assert_eq!(video_entry.duration_secs, 4.0);
        assert_eq!(
            video_entry.recording_timestamp,
            Some("2026-02-11T18:15:23Z".to_string())
        );
        assert_eq!(video_entry.camera_model, Some("iPhone 15 Pro".to_string()));
        assert_eq!(video_entry.frames.len(), 3);
    }

    #[test]
    fn test_generate_manifest() {
        let metadata = vec![create_test_metadata()];
        let frame_sets = vec![create_test_frame_set()];

        let manifest = generate_manifest(
            "job-001".to_string(),
            &metadata,
            &frame_sets,
            Path::new("/tmp"),
        )
        .unwrap();

        assert_eq!(manifest.job_id, "job-001");
        assert_eq!(manifest.videos.len(), 1);
        assert_eq!(manifest.videos[0].filename, "test.mp4");
    }

    #[test]
    fn test_generate_manifest_validation() {
        let metadata = vec![create_test_metadata()];
        let frame_sets = vec![];

        let result = generate_manifest(
            "job-001".to_string(),
            &metadata,
            &frame_sets,
            Path::new("/tmp"),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_write_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("manifest.json");

        let metadata = vec![create_test_metadata()];
        let frame_sets = vec![create_test_frame_set()];

        let manifest = generate_manifest(
            "job-001".to_string(),
            &metadata,
            &frame_sets,
            Path::new("/tmp"),
        )
        .unwrap();

        write_manifest(&manifest, &manifest_path).unwrap();

        assert!(manifest_path.exists());

        let content = std::fs::read_to_string(&manifest_path).unwrap();
        assert!(content.contains("job-001"));
        assert!(content.contains("test.mp4"));
    }

    #[test]
    fn test_generate_and_write_manifest() {
        let temp_dir = TempDir::new().unwrap();
        let manifest_path = temp_dir.path().join("manifest.json");

        let metadata = vec![create_test_metadata()];
        let frame_sets = vec![create_test_frame_set()];

        generate_and_write_manifest(
            "job-001".to_string(),
            &metadata,
            &frame_sets,
            Path::new("/tmp"),
            &manifest_path,
        )
        .unwrap();

        assert!(manifest_path.exists());
    }
}
