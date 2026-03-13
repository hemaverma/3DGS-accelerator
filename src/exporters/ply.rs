//! PLY export functionality for 3DGS models
//!
//! Exports trained Gaussian Splatting models to the standard PLY format
//! with UTC timestamp naming convention: YYYY-MM-DD_HH-MM-SS.ply

use anyhow::{Context, Result};
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{info, instrument};

use crate::backends::models::BackendOutput;
use crate::config::ExportConfig;

/// Export a trained 3DGS model to PLY format
///
/// # Arguments
///
/// * `model` - The trained model output from the backend
/// * `output_dir` - Directory where the PLY file will be saved
/// * `config` - Export configuration (precision, etc.)
///
/// # Returns
///
/// Path to the exported PLY file
///
/// # Errors
///
/// Returns error if:
/// - Model file doesn't exist
/// - Output directory doesn't exist or isn't writable
/// - File copy operation fails
///
/// # Example
///
/// ```no_run
/// use three_dgs_processor::exporters::ply::export_ply;
/// use three_dgs_processor::backends::models::BackendOutput;
/// use three_dgs_processor::config::ExportConfig;
/// use std::path::PathBuf;
///
/// # fn main() -> anyhow::Result<()> {
/// let model = BackendOutput::new(
///     PathBuf::from("/tmp/model.ply"),
///     30000,
///     "gaussian-splatting".to_string()
/// );
/// let output_dir = PathBuf::from("/output");
/// let config = ExportConfig::default();
///
/// let ply_path = export_ply(&model, &output_dir, &config)?;
/// println!("Exported to: {}", ply_path.display());
/// # Ok(())
/// # }
/// ```
#[instrument(skip(model, config), fields(
    model_path = %model.model_path.display(),
    output_dir = %output_dir.display(),
    precision = %config.ply_precision
))]
pub fn export_ply(
    model: &BackendOutput,
    output_dir: &Path,
    config: &ExportConfig,
) -> Result<PathBuf> {
    // Verify model file exists
    model.verify().context("Failed to verify model file")?;

    // Ensure output directory exists
    if !output_dir.exists() {
        fs::create_dir_all(output_dir).with_context(|| {
            format!(
                "Failed to create output directory: {}",
                output_dir.display()
            )
        })?;
    }

    // Generate UTC timestamp filename
    let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S");
    let filename = format!("{}.ply", timestamp);
    let output_path = output_dir.join(&filename);

    // Log export details
    info!(
        model_path = %model.model_path.display(),
        output_path = %output_path.display(),
        precision = %config.ply_precision,
        gaussian_count = ?model.gaussian_count,
        "Exporting model to PLY format"
    );

    // Copy model file to output location with timestamp name
    // Note: If the backend already outputs in PLY format, this is a simple copy.
    // If conversion is needed (e.g., from .pth to .ply), the backend should
    // handle that before calling this function.
    fs::copy(&model.model_path, &output_path).with_context(|| {
        format!(
            "Failed to copy model from {} to {}",
            model.model_path.display(),
            output_path.display()
        )
    })?;

    // Get file size for logging
    let metadata = fs::metadata(&output_path)
        .with_context(|| format!("Failed to get metadata for {}", output_path.display()))?;
    let size_mb = metadata.len() as f64 / 1_048_576.0;

    info!(
        output_path = %output_path.display(),
        size_mb = %format!("{:.2}", size_mb),
        "PLY export complete"
    );

    Ok(output_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// Create a minimal valid PLY file for testing
    fn create_test_ply_file(path: &Path) -> Result<()> {
        let ply_content = r#"ply
format ascii 1.0
element vertex 1
property float x
property float y
property float z
end_header
0.0 0.0 0.0
"#;
        let mut file = fs::File::create(path)?;
        file.write_all(ply_content.as_bytes())?;
        Ok(())
    }

    #[test]
    fn test_export_ply_success() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let model_dir = TempDir::new()?;
        let output_dir = temp_dir.path();
        let model_path = model_dir.path().join("model.ply");

        // Create test model file
        create_test_ply_file(&model_path)?;

        let model = BackendOutput::new(model_path.clone(), 30000, "gaussian-splatting".to_string());

        let config = ExportConfig {
            formats: vec!["ply".to_string()],
            ply_precision: "float32".to_string(),
        };

        let result = export_ply(&model, output_dir, &config)?;

        // Verify file was created
        assert!(result.exists());
        assert!(result.extension().unwrap() == "ply");

        // Verify filename format (YYYY-MM-DD_HH-MM-SS.ply)
        let filename = result.file_name().unwrap().to_string_lossy();
        assert!(filename.ends_with(".ply"));
        assert!(filename.len() == "2026-02-11_20-30-45.ply".len());

        // Verify file content matches source
        let source_content = fs::read(&model_path)?;
        let exported_content = fs::read(&result)?;
        assert_eq!(source_content, exported_content);

        Ok(())
    }

    #[test]
    fn test_export_ply_creates_output_directory() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let model_dir = TempDir::new()?;
        let output_dir = temp_dir.path().join("nested").join("output");
        let model_path = model_dir.path().join("model.ply");

        // Create test model file
        create_test_ply_file(&model_path)?;

        let model = BackendOutput::new(model_path, 30000, "gaussian-splatting".to_string());

        let config = ExportConfig::default();

        // Output directory doesn't exist yet
        assert!(!output_dir.exists());

        let result = export_ply(&model, &output_dir, &config)?;

        // Verify directory was created and file exists
        assert!(output_dir.exists());
        assert!(result.exists());

        Ok(())
    }

    #[test]
    fn test_export_ply_missing_model_file() {
        let temp_dir = TempDir::new().unwrap();
        let output_dir = temp_dir.path();
        let model_path = PathBuf::from("/nonexistent/model.ply");

        let model = BackendOutput::new(model_path, 30000, "gaussian-splatting".to_string());

        let config = ExportConfig::default();

        let result = export_ply(&model, output_dir, &config);
        assert!(result.is_err());
        let error_msg = result.unwrap_err().to_string();
        // Error message should indicate verification failure
        assert!(error_msg.contains("verify") || error_msg.contains("does not exist"));
    }

    #[test]
    fn test_export_ply_with_float16_precision() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let model_dir = TempDir::new()?;
        let output_dir = temp_dir.path();
        let model_path = model_dir.path().join("model.ply");

        // Create test model file
        create_test_ply_file(&model_path)?;

        let model = BackendOutput::new(model_path, 30000, "gaussian-splatting".to_string());

        let config = ExportConfig {
            formats: vec!["ply".to_string()],
            ply_precision: "float16".to_string(),
        };

        let result = export_ply(&model, output_dir, &config)?;

        // Verify file was created with float16 config
        // (Actual precision conversion would be handled by the backend)
        assert!(result.exists());

        Ok(())
    }

    #[test]
    fn test_export_ply_timestamp_format() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let model_dir = TempDir::new()?;
        let output_dir = temp_dir.path();
        let model_path = model_dir.path().join("model.ply");

        // Create test model file
        create_test_ply_file(&model_path)?;

        let model = BackendOutput::new(model_path, 30000, "gaussian-splatting".to_string());

        let config = ExportConfig::default();

        let result = export_ply(&model, output_dir, &config)?;
        let filename = result.file_name().unwrap().to_string_lossy();

        // Verify filename matches pattern: YYYY-MM-DD_HH-MM-SS.ply
        let parts: Vec<&str> = filename.split('.').collect();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[1], "ply");

        let datetime_part = parts[0];
        let components: Vec<&str> = datetime_part.split('_').collect();
        assert_eq!(components.len(), 2);

        // Check date format: YYYY-MM-DD
        let date_parts: Vec<&str> = components[0].split('-').collect();
        assert_eq!(date_parts.len(), 3);
        assert_eq!(date_parts[0].len(), 4); // Year
        assert_eq!(date_parts[1].len(), 2); // Month
        assert_eq!(date_parts[2].len(), 2); // Day

        // Check time format: HH-MM-SS
        let time_parts: Vec<&str> = components[1].split('-').collect();
        assert_eq!(time_parts.len(), 3);
        assert_eq!(time_parts[0].len(), 2); // Hour
        assert_eq!(time_parts[1].len(), 2); // Minute
        assert_eq!(time_parts[2].len(), 2); // Second

        Ok(())
    }
}
