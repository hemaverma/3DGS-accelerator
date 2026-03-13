//! Backend input/output types for 3DGS processing

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// Re-export TrainingConfig from config module for convenience
pub use crate::config::TrainingConfig;

/// Output from a successful 3DGS training run
///
/// Contains the trained model representation and metadata.
/// Backends may store the model in different formats:
/// - As a checkpoint file on disk
/// - As an in-memory representation
/// - As a reference to a cloud-stored model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackendOutput {
    /// Path to the trained model checkpoint/data
    /// This could be a .pth file, .ply file, or backend-specific format
    pub model_path: PathBuf,

    /// Number of training iterations completed
    pub iterations_completed: u32,

    /// Final training loss value
    pub final_loss: Option<f64>,

    /// Number of Gaussian primitives in the trained model
    pub gaussian_count: Option<usize>,

    /// Backend-specific metadata (e.g., timing, memory usage)
    pub metadata: BackendMetadata,
}

/// Backend-specific metadata about the training process
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BackendMetadata {
    /// Backend name/identifier (e.g., "gaussian-splatting", "gsplat")
    pub backend_name: String,

    /// Backend version string
    pub backend_version: Option<String>,

    /// Total training time in seconds
    pub training_time_secs: Option<f64>,

    /// Peak memory usage in bytes
    pub peak_memory_bytes: Option<usize>,

    /// GPU device used (if applicable)
    pub gpu_device: Option<String>,

    /// Additional backend-specific key-value pairs
    #[serde(flatten)]
    pub extra: std::collections::HashMap<String, serde_json::Value>,
}

impl BackendOutput {
    /// Create a new BackendOutput with required fields
    pub fn new(model_path: PathBuf, iterations_completed: u32, backend_name: String) -> Self {
        Self {
            model_path,
            iterations_completed,
            final_loss: None,
            gaussian_count: None,
            metadata: BackendMetadata {
                backend_name,
                backend_version: None,
                training_time_secs: None,
                peak_memory_bytes: None,
                gpu_device: None,
                extra: std::collections::HashMap::new(),
            },
        }
    }

    /// Verify that the model file exists on disk
    pub fn verify(&self) -> Result<()> {
        if !self.model_path.exists() {
            anyhow::bail!("Model file does not exist: {}", self.model_path.display());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_output_creation() {
        let output = BackendOutput::new(
            PathBuf::from("/tmp/model.pth"),
            30000,
            "gaussian-splatting".to_string(),
        );

        assert_eq!(output.model_path, PathBuf::from("/tmp/model.pth"));
        assert_eq!(output.iterations_completed, 30000);
        assert_eq!(output.metadata.backend_name, "gaussian-splatting");
    }

    #[test]
    fn test_backend_output_verify_fails_for_missing_file() {
        let output = BackendOutput::new(
            PathBuf::from("/nonexistent/model.pth"),
            1000,
            "test".to_string(),
        );

        assert!(output.verify().is_err());
    }

    #[test]
    fn test_backend_metadata_serialization() {
        let mut metadata = BackendMetadata {
            backend_name: "gsplat".to_string(),
            backend_version: Some("1.0.0".to_string()),
            training_time_secs: Some(120.5),
            ..Default::default()
        };
        metadata.extra.insert(
            "custom_field".to_string(),
            serde_json::Value::String("custom_value".to_string()),
        );

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: BackendMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.backend_name, "gsplat");
        assert_eq!(deserialized.backend_version, Some("1.0.0".to_string()));
        assert_eq!(deserialized.training_time_secs, Some(120.5));
        assert!(deserialized.extra.contains_key("custom_field"));
    }
}
