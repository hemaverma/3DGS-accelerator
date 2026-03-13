//! Mock 3DGS backend for unit tests
//!
//! Provides a test double that implements GaussianSplatBackend without
//! requiring COLMAP or actual training.

use async_trait::async_trait;
use std::path::Path;
use three_dgs_processor::backends::{
    BackendOutput, GaussianSplatBackend, TrainingConfig,
};

/// Mock backend that simulates training without external dependencies
#[derive(Debug, Clone)]
pub struct MockBackend {
    /// Whether to simulate training failure
    pub should_fail: bool,
    /// Simulated training duration in milliseconds
    pub simulated_duration_ms: u64,
}

impl MockBackend {
    /// Create a new mock backend that succeeds
    pub fn new() -> Self {
        Self {
            should_fail: false,
            simulated_duration_ms: 10,
        }
    }

    /// Create a mock backend that fails during training
    pub fn failing() -> Self {
        Self {
            should_fail: true,
            simulated_duration_ms: 0,
        }
    }

    /// Create a mock backend with custom duration
    #[allow(dead_code)]
    pub fn with_duration(duration_ms: u64) -> Self {
        Self {
            should_fail: false,
            simulated_duration_ms: duration_ms,
        }
    }

    /// Creates a mock BackendOutput for tests
    fn mock_output(&self, _frames_count: usize) -> BackendOutput {
        let mut output = BackendOutput::new(
            std::path::PathBuf::from("/tmp/mock_checkpoint.pth"),
            1000,
            "mock".to_string(),
        );
        output.final_loss = Some(0.001);
        output.gaussian_count = Some(100_000);
        output.metadata.training_time_secs = Some((self.simulated_duration_ms as f64) / 1000.0);
        output.metadata.backend_version = Some("1.0.0-mock".to_string());
        output
    }
}

impl Default for MockBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GaussianSplatBackend for MockBackend {
    async fn train(
        &self,
        frames: &[&Path],
        _config: &TrainingConfig,
    ) -> anyhow::Result<BackendOutput> {
        if self.should_fail {
            anyhow::bail!("Mock backend configured to fail");
        }

        if frames.is_empty() {
            anyhow::bail!("No frames provided");
        }

        // Simulate processing time
        if self.simulated_duration_ms > 0 {
            tokio::time::sleep(tokio::time::Duration::from_millis(
                self.simulated_duration_ms,
            ))
            .await;
        }

        Ok(self.mock_output(frames.len()))
    }

    async fn export_ply(&self, output: &BackendOutput, path: &Path) -> anyhow::Result<()> {
        if self.should_fail {
            anyhow::bail!("Mock backend configured to fail");
        }

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write minimal PLY file
        let gaussian_count = output.gaussian_count.unwrap_or(100_000);
        let ply_content = format!(
            "ply\nformat ascii 1.0\ncomment Mock PLY export\nelement vertex {}\nproperty float x\nproperty float y\nproperty float z\nend_header\n0.0 0.0 0.0\n",
            gaussian_count
        );
        std::fs::write(path, ply_content)?;

        Ok(())
    }

    async fn export_splat(&self, output: &BackendOutput, path: &Path) -> anyhow::Result<()> {
        if self.should_fail {
            anyhow::bail!("Mock backend configured to fail");
        }

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Write valid .splat file: 32 bytes per gaussian, no header
        let gaussian_count = output.gaussian_count.unwrap_or(100_000).min(1000);
        let mut data = Vec::with_capacity(gaussian_count * 32);
        for i in 0..gaussian_count {
            // Position 3×f32 + Scale 3×f32 + Color RGBA 4×u8 + Rotation 4×u8 = 32 bytes
            let v = i as f32 * 0.1;
            data.extend_from_slice(&v.to_le_bytes());
            data.extend_from_slice(&v.to_le_bytes());
            data.extend_from_slice(&v.to_le_bytes());
            data.extend_from_slice(&0.01f32.to_le_bytes());
            data.extend_from_slice(&0.01f32.to_le_bytes());
            data.extend_from_slice(&0.01f32.to_le_bytes());
            data.extend_from_slice(&[255u8, 255, 255, 255]); // RGBA
            data.extend_from_slice(&[255u8, 128, 128, 128]); // rotation
        }
        std::fs::write(path, data)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_mock_backend_success() {
        let backend = MockBackend::new();
        let temp = TempDir::new().unwrap();

        // Create actual frame files
        let frame1 = temp.path().join("frame1.jpg");
        let frame2 = temp.path().join("frame2.jpg");
        std::fs::write(&frame1, b"").unwrap();
        std::fs::write(&frame2, b"").unwrap();

        let frames: Vec<&Path> = vec![frame1.as_path(), frame2.as_path()];
        let config = TrainingConfig::default();

        let result = backend.train(&frames, &config).await;
        assert!(result.is_ok());

        let output = result.unwrap();
        assert_eq!(output.metadata.backend_name, "mock");
        assert_eq!(output.iterations_completed, 1000);
    }

    #[tokio::test]
    async fn test_mock_backend_failure() {
        let backend = MockBackend::failing();
        let temp = TempDir::new().unwrap();
        let frame1 = temp.path().join("frame1.jpg");
        std::fs::write(&frame1, b"").unwrap();

        let frames: Vec<&Path> = vec![frame1.as_path()];
        let config = TrainingConfig::default();

        let result = backend.train(&frames, &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_backend_no_frames() {
        let backend = MockBackend::new();
        let frames: Vec<&Path> = vec![];
        let config = TrainingConfig::default();

        let result = backend.train(&frames, &config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_export_ply() {
        let backend = MockBackend::new();
        let temp_frames = TempDir::new().unwrap();
        let frame1 = temp_frames.path().join("frame1.jpg");
        std::fs::write(&frame1, b"").unwrap();

        let frames: Vec<&Path> = vec![frame1.as_path()];
        let config = TrainingConfig::default();

        let output = backend.train(&frames, &config).await.unwrap();

        let temp = TempDir::new().unwrap();
        let ply_path = temp.path().join("test.ply");

        backend.export_ply(&output, &ply_path).await.unwrap();
        assert!(ply_path.exists());

        let content = std::fs::read_to_string(&ply_path).unwrap();
        assert!(content.contains("ply"));
        assert!(content.contains("format ascii 1.0"));
    }

    #[tokio::test]
    async fn test_export_splat() {
        let backend = MockBackend::new();
        let temp_frames = TempDir::new().unwrap();
        let frame1 = temp_frames.path().join("frame1.jpg");
        std::fs::write(&frame1, b"").unwrap();

        let frames: Vec<&Path> = vec![frame1.as_path()];
        let config = TrainingConfig::default();

        let output = backend.train(&frames, &config).await.unwrap();

        let temp = TempDir::new().unwrap();
        let splat_path = temp.path().join("test.splat");

        backend.export_splat(&output, &splat_path).await.unwrap();
        assert!(splat_path.exists());

        let content = std::fs::read(&splat_path).unwrap();
        assert_eq!(content.len() % 32, 0, "File size must be a multiple of 32 bytes");
    }
}
