//! Test fixture utilities for 3DGS processor testing
//!
//! Provides helpers for creating test data without requiring external tools.

#[allow(unused_imports)]
pub mod frames;

#[allow(unused_imports)]
pub use frames::create_test_frames;

use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Creates a temporary directory with test structure
///
/// Returns a `TempDir` which automatically cleans up when dropped
pub fn create_test_workspace() -> anyhow::Result<TempDir> {
    let temp_dir = TempDir::new()?;

    // Create standard directory structure
    std::fs::create_dir(temp_dir.path().join("input"))?;
    std::fs::create_dir(temp_dir.path().join("output"))?;
    std::fs::create_dir(temp_dir.path().join("processed"))?;
    std::fs::create_dir(temp_dir.path().join("error"))?;

    Ok(temp_dir)
}

/// Creates a test video input directory with mock videos
pub fn create_test_video_dir(base: &Path, name: &str) -> anyhow::Result<PathBuf> {
    let video_dir = base.join(name);
    std::fs::create_dir_all(&video_dir)?;

    // Create placeholder video files (empty for unit tests)
    std::fs::write(video_dir.join("view1.mp4"), b"")?;
    std::fs::write(video_dir.join("view2.mp4"), b"")?;

    Ok(video_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_workspace() {
        let workspace = create_test_workspace().unwrap();

        assert!(workspace.path().join("input").exists());
        assert!(workspace.path().join("output").exists());
        assert!(workspace.path().join("processed").exists());
        assert!(workspace.path().join("error").exists());
    }

    #[test]
    fn test_create_video_dir() {
        let workspace = create_test_workspace().unwrap();
        let video_dir = create_test_video_dir(workspace.path(), "test_scene").unwrap();

        assert!(video_dir.join("view1.mp4").exists());
        assert!(video_dir.join("view2.mp4").exists());
    }
}
