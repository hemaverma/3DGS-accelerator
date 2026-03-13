//! Mock frame generation for tests
//!
//! Creates minimal test images without FFmpeg dependency

use std::path::{Path, PathBuf};

/// Creates a single test frame (1x1 RGB image)
///
/// This minimal image is sufficient for unit tests that don't require
/// actual image processing, avoiding FFmpeg dependency.
pub fn create_test_frame(path: &Path) -> anyhow::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Path must have parent directory"))?;
    std::fs::create_dir_all(parent)?;

    // Create minimal RGB image using image crate if available,
    // otherwise just create empty file for unit tests
    #[cfg(feature = "test-images")]
    {
        use image::RgbImage;
        let img = RgbImage::from_pixel(1, 1, image::Rgb([128, 128, 128]));
        img.save(path)?;
    }

    #[cfg(not(feature = "test-images"))]
    {
        // For unit tests, just create empty file
        std::fs::write(path, b"")?;
    }

    Ok(())
}

/// Creates multiple test frames with sequential naming
///
/// # Arguments
///
/// * `dir` - Directory to create frames in
/// * `count` - Number of frames to create
///
/// # Returns
///
/// Vector of paths to created frames
pub fn create_test_frames(dir: &Path, count: usize) -> anyhow::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(dir)?;

    let mut frames = Vec::with_capacity(count);
    for i in 0..count {
        let frame_path = dir.join(format!("frame_{:06}.jpg", i));
        create_test_frame(&frame_path)?;
        frames.push(frame_path);
    }

    Ok(frames)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_single_frame() {
        let temp = TempDir::new().unwrap();
        let frame_path = temp.path().join("test_frame.jpg");

        create_test_frame(&frame_path).unwrap();
        assert!(frame_path.exists());
    }

    #[test]
    fn test_create_multiple_frames() {
        let temp = TempDir::new().unwrap();
        let frames = create_test_frames(temp.path(), 5).unwrap();

        assert_eq!(frames.len(), 5);
        for (i, frame) in frames.iter().enumerate() {
            assert!(frame.exists());
            let expected_name = format!("frame_{:06}.jpg", i);
            assert_eq!(frame.file_name().unwrap().to_str().unwrap(), expected_name);
        }
    }

    #[test]
    fn test_frame_creates_parent_dirs() {
        let temp = TempDir::new().unwrap();
        let nested_path = temp.path().join("subdir/nested/frame.jpg");

        create_test_frame(&nested_path).unwrap();
        assert!(nested_path.exists());
    }
}
