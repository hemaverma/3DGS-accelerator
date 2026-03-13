//! SPLAT format export functionality
//!
//! Exports trained 3DGS models to .splat format optimized for web-based viewers.
//! The .splat format is a compact binary representation of Gaussian primitives
//! suitable for real-time rendering in browsers.
//!
//! # Filename Convention
//!
//! Output files follow the pattern: `YYYY-MM-DD_HH-MM-SS.splat` (UTC timestamp)
//!
//! # Example
//!
//! ```no_run
//! use three_dgs_processor::exporters::export_to_splat;
//! use three_dgs_processor::backends::{BackendOutput, GaussianSplatBackend, GaussianSplattingBackend};
//! use std::path::PathBuf;
//!
//! # async fn example() -> anyhow::Result<()> {
//! let backend = GaussianSplattingBackend::new();
//! let model = BackendOutput::new(
//!     PathBuf::from("model.pth"),
//!     30000,
//!     "gaussian-splatting".to_string()
//! );
//! let output_dir = PathBuf::from("/output");
//!
//! let splat_path = export_to_splat(&backend, &model, &output_dir).await?;
//! println!("Exported to: {}", splat_path.display());
//! # Ok(())
//! # }
//! ```

use crate::backends::{BackendOutput, GaussianSplatBackend};
use anyhow::{Context, Result};
use chrono::Utc;
use std::path::{Path, PathBuf};
use tracing::{debug, info, instrument};

/// Export a trained 3DGS model to SPLAT format with UTC timestamp naming
///
/// # Arguments
///
/// * `backend` - The 3DGS backend that trained the model
/// * `model` - The trained model output from backend.train()
/// * `output_dir` - Directory where the .splat file will be saved
///
/// # Returns
///
/// Full path to the exported .splat file
///
/// # Errors
///
/// Returns an error if:
/// - Model verification fails (model file doesn't exist)
/// - Output directory cannot be created
/// - Backend export operation fails
/// - File system I/O errors occur
///
/// # Filename Format
///
/// Files are named with UTC timestamps: `YYYY-MM-DD_HH-MM-SS.splat`
/// Example: `2026-02-11_15-30-45.splat`
#[instrument(skip(backend, model), fields(
    output_dir = %output_dir.display(),
    model_path = %model.model_path.display()
))]
pub async fn export_to_splat(
    backend: &dyn GaussianSplatBackend,
    model: &BackendOutput,
    output_dir: &Path,
) -> Result<PathBuf> {
    debug!("Starting SPLAT export with UTC timestamp naming");

    // Verify model exists
    model
        .verify()
        .context("Model verification failed before SPLAT export")?;

    // Create output directory if it doesn't exist
    std::fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    // Generate UTC timestamp filename
    let timestamp = Utc::now();
    let filename = timestamp.format("%Y-%m-%d_%H-%M-%S.splat").to_string();
    let output_path = output_dir.join(&filename);

    debug!(
        filename = %filename,
        output_path = %output_path.display(),
        "Generated SPLAT filename with UTC timestamp"
    );

    // Call backend's export_splat implementation
    backend
        .export_splat(model, &output_path)
        .await
        .context("Backend SPLAT export failed")?;

    info!(
        output_path = %output_path.display(),
        file_size = ?std::fs::metadata(&output_path).ok().map(|m| m.len()),
        "SPLAT export completed successfully"
    );

    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::models::BackendMetadata;
    use async_trait::async_trait;
    use std::sync::Arc;
    use std::sync::Mutex;
    use tempfile::TempDir;

    /// Mock backend for testing SPLAT export
    struct MockBackend {
        export_called: Arc<Mutex<bool>>,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                export_called: Arc::new(Mutex::new(false)),
            }
        }

        fn was_export_called(&self) -> bool {
            *self.export_called.lock().unwrap()
        }
    }

    #[async_trait]
    impl GaussianSplatBackend for MockBackend {
        async fn train(
            &self,
            _frames: &[&Path],
            _config: &crate::config::TrainingConfig,
        ) -> Result<BackendOutput> {
            unimplemented!("Not needed for export tests")
        }

        async fn export_ply(&self, _model: &BackendOutput, _output_path: &Path) -> Result<()> {
            unimplemented!("Not needed for SPLAT export tests")
        }

        async fn export_splat(&self, _model: &BackendOutput, output_path: &Path) -> Result<()> {
            // Mark that export was called
            *self.export_called.lock().unwrap() = true;

            // Create a dummy SPLAT file
            std::fs::write(output_path, b"MOCK_SPLAT_DATA")
                .context("Failed to write mock SPLAT file")?;

            Ok(())
        }
    }

    #[tokio::test]
    async fn test_export_to_splat_success() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path().join("model");
        std::fs::create_dir_all(&model_dir).unwrap();

        // Create a mock model file
        let model_path = model_dir.join("model.pth");
        std::fs::write(&model_path, b"mock_model").unwrap();

        let model = BackendOutput {
            model_path,
            iterations_completed: 30000,
            final_loss: Some(0.0042),
            gaussian_count: Some(100000),
            metadata: BackendMetadata {
                backend_name: "mock".to_string(),
                ..Default::default()
            },
        };

        let output_dir = temp_dir.path().join("output");
        let backend = MockBackend::new();

        let result = export_to_splat(&backend, &model, &output_dir).await;
        assert!(result.is_ok(), "Export should succeed");

        let splat_path = result.unwrap();

        // Verify file was created
        assert!(splat_path.exists(), "SPLAT file should exist");
        assert!(
            backend.was_export_called(),
            "Backend export should be called"
        );

        // Verify filename format (YYYY-MM-DD_HH-MM-SS.splat)
        let filename = splat_path.file_name().unwrap().to_str().unwrap();
        assert!(
            filename.ends_with(".splat"),
            "Filename should end with .splat"
        );
        assert_eq!(
            filename.len(),
            25, // YYYY-MM-DD_HH-MM-SS.splat = 25 chars (10 + 1 + 8 + 6)
            "Filename should be 25 characters long"
        );

        // Verify file contains mock data
        let content = std::fs::read(&splat_path).unwrap();
        assert_eq!(content, b"MOCK_SPLAT_DATA");
    }

    #[tokio::test]
    async fn test_export_to_splat_nonexistent_model() {
        let temp_dir = TempDir::new().unwrap();

        let model = BackendOutput {
            model_path: PathBuf::from("/nonexistent/model.pth"),
            iterations_completed: 30000,
            final_loss: None,
            gaussian_count: None,
            metadata: BackendMetadata {
                backend_name: "mock".to_string(),
                ..Default::default()
            },
        };

        let output_dir = temp_dir.path().join("output");
        let backend = MockBackend::new();

        let result = export_to_splat(&backend, &model, &output_dir).await;
        assert!(result.is_err(), "Export should fail for nonexistent model");

        let error_msg = result.unwrap_err().to_string();
        assert!(
            error_msg.contains("Model file does not exist")
                || error_msg.contains("verification failed"),
            "Error should mention model verification failure"
        );
    }

    #[tokio::test]
    async fn test_export_creates_output_directory() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path().join("model");
        std::fs::create_dir_all(&model_dir).unwrap();

        let model_path = model_dir.join("model.pth");
        std::fs::write(&model_path, b"mock_model").unwrap();

        let model = BackendOutput {
            model_path,
            iterations_completed: 30000,
            final_loss: None,
            gaussian_count: None,
            metadata: BackendMetadata {
                backend_name: "mock".to_string(),
                ..Default::default()
            },
        };

        // Output directory doesn't exist yet
        let output_dir = temp_dir.path().join("nested/output/dir");
        assert!(!output_dir.exists());

        let backend = MockBackend::new();
        let result = export_to_splat(&backend, &model, &output_dir).await;

        assert!(result.is_ok(), "Export should create output directory");
        assert!(output_dir.exists(), "Output directory should be created");
    }

    #[tokio::test]
    async fn test_utc_timestamp_format() {
        let temp_dir = TempDir::new().unwrap();
        let model_dir = temp_dir.path().join("model");
        std::fs::create_dir_all(&model_dir).unwrap();

        let model_path = model_dir.join("model.pth");
        std::fs::write(&model_path, b"mock_model").unwrap();

        let model = BackendOutput {
            model_path,
            iterations_completed: 30000,
            final_loss: None,
            gaussian_count: None,
            metadata: BackendMetadata {
                backend_name: "mock".to_string(),
                ..Default::default()
            },
        };

        let output_dir = temp_dir.path().join("output");
        let backend = MockBackend::new();

        let before = Utc::now();
        let splat_path = export_to_splat(&backend, &model, &output_dir)
            .await
            .unwrap();
        let after = Utc::now();

        let filename = splat_path.file_name().unwrap().to_str().unwrap();

        // Parse timestamp from filename
        let timestamp_str = filename.strip_suffix(".splat").unwrap();
        let parsed = chrono::NaiveDateTime::parse_from_str(timestamp_str, "%Y-%m-%d_%H-%M-%S");

        assert!(
            parsed.is_ok(),
            "Timestamp should be parseable: {}",
            timestamp_str
        );

        // Verify timestamp is within reasonable bounds (UTC)
        // Allow 5 second slack for test execution time
        let parsed_dt = parsed.unwrap().and_utc();
        let before_minus_slack = before - chrono::Duration::seconds(5);
        let after_plus_slack = after + chrono::Duration::seconds(5);

        assert!(
            parsed_dt >= before_minus_slack && parsed_dt <= after_plus_slack,
            "Timestamp {:?} should be between {:?} and {:?}",
            parsed_dt,
            before_minus_slack,
            after_plus_slack
        );
    }
}
