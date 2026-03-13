//! Reference backend implementation using gaussian-splatting
//!
//! This module provides integration with the original gaussian-splatting
//! implementation from [3D Gaussian Splatting for Real-Time Radiance Field Rendering]
//! (<https://github.com/graphdeco-inria/gaussian-splatting>).
//!
//! # Requirements
//!
//! - gaussian-splatting tool must be available via `GAUSSIAN_SPLATTING_BIN` env var or in PATH
//! - COLMAP sparse reconstruction must be completed before training
//! - CUDA-capable GPU recommended (CPU training is extremely slow)
//!
//! # Training Workflow
//!
//! 1. Receives COLMAP sparse reconstruction directory and training config
//! 2. Invokes gaussian-splatting training script with appropriate parameters
//! 3. Monitors training progress via log parsing
//! 4. Returns trained model checkpoint and metrics
//!
//! # Export Formats
//!
//! - PLY: Standard format exported directly by gaussian-splatting
//! - SPLAT: Converted from PLY using conversion utility

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::process::Command;
use std::time::Instant;
use tracing::{debug, info, instrument, warn};

use super::{BackendMetadata, BackendOutput, GaussianSplatBackend, TrainingConfig};

/// Reference backend using original gaussian-splatting implementation
pub struct GaussianSplattingBackend {
    /// Path to gaussian-splatting executable/script
    gs_bin: String,
}

impl GaussianSplattingBackend {
    /// Create a new gaussian-splatting backend
    ///
    /// # Environment Variables
    ///
    /// - `GAUSSIAN_SPLATTING_BIN`: Path to training script (default: "gaussian-splatting")
    pub fn new() -> Self {
        let gs_bin = std::env::var("GAUSSIAN_SPLATTING_BIN")
            .unwrap_or_else(|_| "gaussian-splatting".to_string());

        GaussianSplattingBackend { gs_bin }
    }

    /// Parse training metrics from gaussian-splatting output
    ///
    /// Extracts final loss and iteration count from training logs
    fn parse_training_output(output: &str) -> (Option<f64>, u32) {
        let mut final_loss = None;
        let mut iterations = 0;

        for line in output.lines() {
            // Example log line: "Iteration 30000 - Loss: 0.0042"
            if line.contains("Iteration") && line.contains("Loss") {
                if let Some(iter_str) = line.split("Iteration").nth(1) {
                    if let Some(num_str) = iter_str.split_whitespace().next() {
                        if let Ok(iter) = num_str.parse::<u32>() {
                            iterations = iterations.max(iter);
                        }
                    }
                }

                if let Some(loss_str) = line.split("Loss:").nth(1) {
                    if let Ok(loss) = loss_str.trim().parse::<f64>() {
                        final_loss = Some(loss);
                    }
                }
            }
        }

        (final_loss, iterations)
    }

    /// Count Gaussians from output PLY file
    ///
    /// Parses the PLY header to determine number of vertices (Gaussians)
    fn count_gaussians(ply_path: &Path) -> Result<usize> {
        use std::io::{BufRead, BufReader};

        let file = std::fs::File::open(ply_path)
            .with_context(|| format!("Failed to open PLY file: {}", ply_path.display()))?;

        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            if line.starts_with("element vertex ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 3 {
                    return parts[2]
                        .parse::<usize>()
                        .context("Failed to parse vertex count from PLY header");
                }
            }
            // PLY header ends at "end_header"
            if line.starts_with("end_header") {
                break;
            }
        }

        anyhow::bail!("Could not find vertex count in PLY header");
    }
}

impl Default for GaussianSplattingBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GaussianSplatBackend for GaussianSplattingBackend {
    /// Train a 3D Gaussian Splatting model
    ///
    /// Expects frames to already have COLMAP reconstruction completed.
    /// The COLMAP sparse reconstruction should be in a standard location
    /// relative to the frames directory.
    ///
    /// # Arguments
    ///
    /// * `frames` - Paths to input frames (used to locate COLMAP data)
    /// * `config` - Training hyperparameters from YAML config
    ///
    /// # Implementation Notes
    ///
    /// This implementation assumes the following directory structure:
    /// ```text
    /// workspace/
    ///   images/           <- frames directory
    ///   colmap/
    ///     sparse/0/       <- COLMAP reconstruction
    ///   output/           <- training output
    /// ```
    #[instrument(skip(self, frames), fields(
        gs_bin = %self.gs_bin,
        frame_count = frames.len(),
        iterations = config.iterations
    ))]
    async fn train(&self, frames: &[&Path], config: &TrainingConfig) -> Result<BackendOutput> {
        if frames.is_empty() {
            anyhow::bail!("Cannot train with zero frames");
        }

        // Determine workspace directory from first frame path
        // Assumes frames are in workspace/images/
        let first_frame = frames[0];
        let workspace_dir = first_frame
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| anyhow::anyhow!("Invalid frame path structure"))?;

        debug!(
            workspace = %workspace_dir.display(),
            "Determined workspace directory from frame paths"
        );

        // Expected paths
        let image_dir = workspace_dir.join("images");
        let colmap_sparse_dir = workspace_dir.join("colmap").join("sparse").join("0");
        let output_dir = workspace_dir.join("output");

        // Validate COLMAP reconstruction exists
        if !colmap_sparse_dir.exists() {
            anyhow::bail!(
                "COLMAP sparse reconstruction not found at: {}",
                colmap_sparse_dir.display()
            );
        }

        // Create output directory
        std::fs::create_dir_all(&output_dir).context("Failed to create output directory")?;

        info!(
            images = %image_dir.display(),
            colmap = %colmap_sparse_dir.display(),
            output = %output_dir.display(),
            "Starting gaussian-splatting training"
        );

        let gs_bin = self.gs_bin.clone();
        let image_dir_clone = image_dir.clone();
        let output_dir_clone = output_dir.clone();
        let config_clone = config.clone();

        let start_time = Instant::now();

        // Run training in blocking task (long-running process)
        let (stdout, stderr) = tokio::task::spawn_blocking(move || {
            debug!("Invoking gaussian-splatting training process");

            // Build command with training parameters
            let mut cmd = Command::new(&gs_bin);
            cmd.arg("--source_path")
                .arg(&image_dir_clone)
                .arg("--model_path")
                .arg(&output_dir_clone)
                .arg("--iterations")
                .arg(config_clone.iterations.to_string())
                .arg("--learning_rate")
                .arg(config_clone.learning_rate.to_string())
                .arg("--densification_interval")
                .arg(config_clone.densification_interval.to_string())
                .arg("--densify_from_iter")
                .arg(config_clone.densification_start.to_string())
                .arg("--densify_until_iter")
                .arg(config_clone.densification_stop.to_string())
                .arg("--opacity_reset_interval")
                .arg(config_clone.opacity_reset_interval.to_string());

            let output = cmd
                .output()
                .context("Failed to execute gaussian-splatting training")?;

            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();

            if !output.status.success() {
                anyhow::bail!(
                    "Gaussian-splatting training failed with status {}: {}",
                    output.status,
                    stderr
                );
            }

            Ok::<_, anyhow::Error>((stdout, stderr))
        })
        .await
        .context("Training task panicked")??;

        let training_time_secs = start_time.elapsed().as_secs_f64();

        info!(
            duration_secs = training_time_secs,
            "Gaussian-splatting training completed successfully"
        );

        // Parse training metrics from output
        let combined_output = format!("{}\n{}", stdout, stderr);
        let (final_loss, iterations_completed) = Self::parse_training_output(&combined_output);

        // Locate trained model checkpoint
        // Gaussian-splatting typically saves point_cloud.ply in output directory
        let model_path = output_dir
            .join("point_cloud")
            .join("iteration_30000")
            .join("point_cloud.ply");

        if !model_path.exists() {
            warn!(
                expected_path = %model_path.display(),
                "Default model path not found, searching for alternatives"
            );

            // Try alternative locations
            let alt_path = output_dir.join("point_cloud.ply");
            if alt_path.exists() {
                debug!("Found model at alternative path: {}", alt_path.display());
            } else {
                anyhow::bail!(
                    "Trained model not found at expected locations. Check training output."
                );
            }
        }

        // Count Gaussians from PLY file
        let gaussian_count = Self::count_gaussians(&model_path).ok();

        let metadata = BackendMetadata {
            backend_name: "gaussian-splatting".to_string(),
            backend_version: None,
            training_time_secs: Some(training_time_secs),
            peak_memory_bytes: None,
            gpu_device: None,
            extra: std::collections::HashMap::new(),
        };

        let output = BackendOutput {
            model_path,
            iterations_completed: iterations_completed.max(config.iterations),
            final_loss,
            gaussian_count,
            metadata,
        };

        debug!(
            model_path = %output.model_path.display(),
            gaussians = ?output.gaussian_count,
            final_loss = ?output.final_loss,
            "Training output prepared"
        );

        Ok(output)
    }

    /// Export trained model to PLY format
    ///
    /// Since gaussian-splatting already produces PLY output, this method
    /// simply copies the model checkpoint to the desired output path.
    #[instrument(skip(self, model), fields(
        model_path = %model.model_path.display(),
        output_path = %output_path.display()
    ))]
    async fn export_ply(&self, model: &BackendOutput, output_path: &Path) -> Result<()> {
        debug!("Exporting model to PLY format");

        // Verify model exists
        model.verify()?;

        // Create parent directory if needed
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create output directory: {}", parent.display())
            })?;
        }

        // Clone paths before moving into spawn_blocking
        let model_path = model.model_path.clone();
        let output_path_clone = output_path.to_path_buf();

        // Copy PLY file to output path
        tokio::task::spawn_blocking(move || {
            std::fs::copy(&model_path, &output_path_clone).with_context(|| {
                format!(
                    "Failed to copy PLY from {} to {}",
                    model_path.display(),
                    output_path_clone.display()
                )
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("PLY export task panicked")??;

        info!(
            output = %output_path.display(),
            "PLY export completed successfully"
        );

        Ok(())
    }

    /// Export trained model to SPLAT format
    ///
    /// Converts the PLY model to SPLAT format for web-based rendering.
    /// This typically requires a conversion tool or Python script.
    ///
    /// # Implementation Note
    ///
    /// This is a placeholder implementation. In production, you would:
    /// 1. Use a PLY-to-SPLAT conversion tool
    /// 2. Or invoke a Python script that performs the conversion
    /// 3. Or implement native Rust conversion
    #[instrument(skip(self, model), fields(
        model_path = %model.model_path.display(),
        output_path = %output_path.display()
    ))]
    async fn export_splat(&self, model: &BackendOutput, output_path: &Path) -> Result<()> {
        debug!("Exporting model to SPLAT format");

        // Verify model exists
        model.verify()?;

        // Create parent directory if needed
        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create output directory: {}", parent.display())
            })?;
        }

        // Check for conversion tool
        let converter_bin =
            std::env::var("PLY_TO_SPLAT_BIN").unwrap_or_else(|_| "ply-to-splat".to_string());

        let model_path = model.model_path.clone();
        let output_path_clone = output_path.to_path_buf();

        tokio::task::spawn_blocking(move || {
            debug!(
                converter = %converter_bin,
                "Invoking PLY-to-SPLAT converter"
            );

            let output = Command::new(&converter_bin)
                .arg("--input")
                .arg(&model_path)
                .arg("--output")
                .arg(&output_path_clone)
                .output()
                .context("Failed to execute PLY-to-SPLAT converter")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!(
                    "PLY-to-SPLAT conversion failed with status {}: {}",
                    output.status,
                    stderr
                );
            }

            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("SPLAT export task panicked")??;

        info!(
            output = %output_path.display(),
            "SPLAT export completed successfully"
        );

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let backend = GaussianSplattingBackend::new();
        assert!(!backend.gs_bin.is_empty());
    }

    #[test]
    fn test_backend_respects_env_var() {
        std::env::set_var("GAUSSIAN_SPLATTING_BIN", "/custom/path/gs");
        let backend = GaussianSplattingBackend::new();
        assert_eq!(backend.gs_bin, "/custom/path/gs");
        std::env::remove_var("GAUSSIAN_SPLATTING_BIN");
    }

    #[test]
    fn test_parse_training_output() {
        let output = r#"
Iteration 1000 - Loss: 0.1542
Iteration 5000 - Loss: 0.0823
Iteration 10000 - Loss: 0.0456
Iteration 30000 - Loss: 0.0042
Training complete
        "#;

        let (loss, iterations) = GaussianSplattingBackend::parse_training_output(output);

        assert_eq!(iterations, 30000);
        assert_eq!(loss, Some(0.0042));
    }

    #[test]
    fn test_parse_training_output_empty() {
        let output = "";
        let (loss, iterations) = GaussianSplattingBackend::parse_training_output(output);

        assert_eq!(iterations, 0);
        assert_eq!(loss, None);
    }
}
