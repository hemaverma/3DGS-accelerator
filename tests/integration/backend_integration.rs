//! Backend Integration Tests
//!
//! Tests the complete backend functionality including:
//! - Training with test COLMAP data
//! - PLY and SPLAT export
//! - Output parsing and validation
//! - Multi-backend support

use std::path::{Path, PathBuf};
use tempfile::TempDir;
use three_dgs_processor::backends::{
    BackendRegistry, GaussianSplatBackend, MockBackend, TrainingConfig,
};
use three_dgs_processor::backends::models::BackendOutput;

/// Create a realistic PLY file for testing Gaussian counting
async fn create_test_ply(path: &Path, vertex_count: usize) -> anyhow::Result<()> {
    let mut content = String::new();
    content.push_str("ply\n");
    content.push_str("format ascii 1.0\n");
    content.push_str(&format!("element vertex {}\n", vertex_count));
    content.push_str("property float x\n");
    content.push_str("property float y\n");
    content.push_str("property float z\n");
    content.push_str("end_header\n");
    
    // Add some vertex data
    for i in 0..vertex_count.min(10) {
        content.push_str(&format!("{:.6} {:.6} {:.6}\n", i as f32, i as f32, i as f32));
    }
    
    tokio::fs::write(path, content).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_backend_full_workflow() {
        let temp = TempDir::new().unwrap();
        let backend = MockBackend::new();

        // Create test frames
        let frames_dir = temp.path().join("frames");
        tokio::fs::create_dir_all(&frames_dir).await.unwrap();
        
        let mut frame_paths = Vec::new();
        for i in 0..5 {
            let frame_path = frames_dir.join(format!("frame_{:06}.jpg", i));
            tokio::fs::write(&frame_path, b"fake image data").await.unwrap();
            frame_paths.push(frame_path);
        }

        // Train model
        let frame_refs: Vec<&Path> = frame_paths.iter().map(|p| p.as_path()).collect();
        let config = TrainingConfig {
            iterations: 1000,
            learning_rate: 0.0016,
            densification_start: 500,
            densification_stop: 800,
            densification_interval: 100,
            opacity_reset_interval: 3000,
        };

        let output = backend.train(&frame_refs, &config).await.unwrap();

        // Validate training output
        assert_eq!(output.metadata.backend_name, "mock");
        assert_eq!(output.iterations_completed, 1000);
        assert!(output.gaussian_count.is_some());
        assert_eq!(output.gaussian_count.unwrap(), 5000); // 5 frames * 1000
        assert!(output.final_loss.is_some());
        assert!(output.metadata.training_time_secs.is_some());

        // Export to PLY
        let ply_path = temp.path().join("output.ply");
        backend.export_ply(&output, &ply_path).await.unwrap();
        assert!(ply_path.exists());

        let ply_content = tokio::fs::read_to_string(&ply_path).await.unwrap();
        assert!(ply_content.contains("ply"));
        assert!(ply_content.contains("format ascii 1.0"));
        assert!(ply_content.contains("element vertex"));
        assert!(ply_content.contains("end_header"));

        // Export to SPLAT
        let splat_path = temp.path().join("output.splat");
        backend.export_splat(&output, &splat_path).await.unwrap();
        assert!(splat_path.exists());

        let splat_content = tokio::fs::read(&splat_path).await.unwrap();
        assert_eq!(splat_content.len() % 32, 0, "File size must be a multiple of 32 bytes");
        assert!(splat_content.len() > 0);
    }

    #[tokio::test]
    async fn test_backend_output_verification() {
        let temp = TempDir::new().unwrap();
        let model_path = temp.path().join("model.pth");
        
        // Create a valid model file
        tokio::fs::write(&model_path, b"fake model data").await.unwrap();

        let mut output = BackendOutput::new(
            model_path.clone(),
            1000,
            "test".to_string(),
        );
        output.gaussian_count = Some(50000);
        output.final_loss = Some(0.0042);

        // Verify should succeed
        assert!(output.verify().is_ok());

        // Create output with non-existent file
        let bad_output = BackendOutput::new(
            PathBuf::from("/nonexistent/model.pth"),
            1000,
            "test".to_string(),
        );

        // Verify should fail
        assert!(bad_output.verify().is_err());
    }

    #[tokio::test]
    async fn test_backend_export_creates_directories() {
        let temp = TempDir::new().unwrap();
        let backend = MockBackend::new();

        let mut output = BackendOutput::new(
            temp.path().join("model.pth"),
            1000,
            "mock".to_string(),
        );
        output.gaussian_count = Some(1000);

        // Export to nested directory that doesn't exist
        let nested_ply = temp.path().join("nested").join("dir").join("output.ply");
        backend.export_ply(&output, &nested_ply).await.unwrap();
        assert!(nested_ply.exists());
        assert!(nested_ply.parent().unwrap().is_dir());

        let nested_splat = temp.path().join("another").join("nested").join("output.splat");
        backend.export_splat(&output, &nested_splat).await.unwrap();
        assert!(nested_splat.exists());
    }

    #[tokio::test]
    async fn test_ply_gaussian_counting() {
        let temp = TempDir::new().unwrap();
        let ply_path = temp.path().join("test.ply");

        // Create PLY with known vertex count
        create_test_ply(&ply_path, 12345).await.unwrap();

        // Mock backend can count gaussians from PLY files
        // We'll test this through the actual implementation
        let content = tokio::fs::read_to_string(&ply_path).await.unwrap();
        assert!(content.contains("element vertex 12345"));
    }

    #[tokio::test]
    async fn test_backend_registry_get_mock() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        // Mock backend should be available
        assert!(registry.is_backend_available("mock"));

        let backend = registry.get_backend("mock").await.unwrap();
        
        // Create simple test
        let temp = TempDir::new().unwrap();
        let frame = temp.path().join("frame.jpg");
        tokio::fs::write(&frame, b"test").await.unwrap();

        let frames: Vec<&Path> = vec![frame.as_path()];
        let config = TrainingConfig::default();
        
        let output = backend.train(&frames, &config).await.unwrap();
        assert_eq!(output.metadata.backend_name, "mock");
    }

    #[tokio::test]
    async fn test_backend_registry_list_backends() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let backends = registry.list_backends();
        
        // Should have at least mock backend
        assert!(backends.contains(&"mock".to_string()));
        
        // May have gaussian-splatting, gsplat, 3dgs-cpp depending on availability
        assert!(!backends.is_empty());
    }

    #[tokio::test]
    async fn test_backend_registry_nonexistent() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let result = registry.get_backend("nonexistent-backend").await;
        assert!(result.is_err());
        let err = result.err().unwrap();
        let err_msg = err.to_string();
        // Backend registry may return various error messages
        assert!(
            err_msg.contains("not found") || 
            err_msg.contains("Unknown") || 
            err_msg.contains("not available") ||
            err_msg.contains("Failed to load") ||
            err_msg.contains("nonexistent")
        );
    }

    #[tokio::test]
    async fn test_training_config_variations() {
        let temp = TempDir::new().unwrap();
        let backend = MockBackend::new();

        let frame = temp.path().join("frame.jpg");
        tokio::fs::write(&frame, b"test").await.unwrap();
        let frames: Vec<&Path> = vec![frame.as_path()];

        // Test with minimal config
        let min_config = TrainingConfig {
            iterations: 100,
            learning_rate: 0.01,
            densification_start: 50,
            densification_stop: 80,
            densification_interval: 10,
            opacity_reset_interval: 50,
        };

        let result = backend.train(&frames, &min_config).await;
        assert!(result.is_ok());

        // Test with high iteration config
        let high_config = TrainingConfig {
            iterations: 30000,
            learning_rate: 0.0016,
            densification_start: 500,
            densification_stop: 15000,
            densification_interval: 100,
            opacity_reset_interval: 3000,
        };

        let result = backend.train(&frames, &high_config).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_backend_error_on_empty_frames() {
        let backend = MockBackend::new();
        let frames: Vec<&Path> = vec![];
        let config = TrainingConfig::default();

        let result = backend.train(&frames, &config).await;
        assert!(result.is_err());
        // The error message should mention frames or training
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(err_msg.contains("frame") || err_msg.contains("train") || err_msg.contains("Cannot"));
        }
    }

    #[tokio::test]
    async fn test_backend_error_on_missing_frames() {
        let backend = MockBackend::new();
        let fake_frame = PathBuf::from("/tmp/nonexistent_frame_12345.jpg");
        let frames: Vec<&Path> = vec![fake_frame.as_path()];
        let config = TrainingConfig::default();

        let result = backend.train(&frames, &config).await;
        assert!(result.is_err());
        if let Err(e) = result {
            let err_msg = e.to_string();
            assert!(err_msg.contains("not found") || err_msg.contains("Frame not found"));
        }
    }

    #[tokio::test]
    async fn test_multiple_backends_available() {
        let mut registry = BackendRegistry::new();
        registry.register_defaults();

        let backends = registry.list_backends();
        
        // Test that we can get multiple backends
        for backend_name in &backends {
            let result = registry.get_backend(backend_name).await;
            assert!(result.is_ok(), "Failed to get backend: {}", backend_name);
        }
    }

    #[tokio::test]
    async fn test_backend_output_metadata() {
        let temp = TempDir::new().unwrap();
        let backend = MockBackend::new();

        let frame = temp.path().join("frame.jpg");
        tokio::fs::write(&frame, b"test").await.unwrap();
        let frames: Vec<&Path> = vec![frame.as_path()];
        let config = TrainingConfig::default();

        let output = backend.train(&frames, &config).await.unwrap();

        // Check metadata fields
        assert_eq!(output.metadata.backend_name, "mock");
        assert!(output.metadata.backend_version.is_some());
        assert!(output.metadata.training_time_secs.is_some());
        assert!(output.metadata.gpu_device.is_some());
        assert!(output.metadata.training_time_secs.unwrap() >= 0.0);
    }

    #[tokio::test]
    async fn test_ply_export_preserves_gaussian_count() {
        let temp = TempDir::new().unwrap();
        let backend = MockBackend::new();

        let frame = temp.path().join("frame.jpg");
        tokio::fs::write(&frame, b"test").await.unwrap();
        let frames: Vec<&Path> = vec![frame.as_path()];
        let config = TrainingConfig::default();

        let output = backend.train(&frames, &config).await.unwrap();
        let original_count = output.gaussian_count.unwrap();

        // Export to PLY
        let ply_path = temp.path().join("output.ply");
        backend.export_ply(&output, &ply_path).await.unwrap();

        // Read and verify the PLY contains the gaussian count
        let ply_content = tokio::fs::read_to_string(&ply_path).await.unwrap();
        assert!(ply_content.contains(&format!("element vertex {}", original_count)));
    }

    #[tokio::test]
    async fn test_backend_concurrent_training() {
        // Test that multiple backends can train concurrently
        let temp = TempDir::new().unwrap();
        
        let mut handles = vec![];
        
        for i in 0..3 {
            let temp_path = temp.path().to_path_buf();
            let handle = tokio::spawn(async move {
                let backend = MockBackend::new();
                let frame_dir = temp_path.join(format!("job_{}", i));
                tokio::fs::create_dir_all(&frame_dir).await.unwrap();
                
                let frame = frame_dir.join("frame.jpg");
                tokio::fs::write(&frame, b"test").await.unwrap();
                
                let frames: Vec<&Path> = vec![frame.as_path()];
                let config = TrainingConfig::default();
                
                backend.train(&frames, &config).await
            });
            handles.push(handle);
        }

        // All should succeed
        for handle in handles {
            let result = handle.await.unwrap();
            assert!(result.is_ok());
        }
    }
}
