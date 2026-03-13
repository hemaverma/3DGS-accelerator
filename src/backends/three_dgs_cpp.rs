//! 3DGS.cpp backend implementation (Template/Reference)
//!
//! **Note:** This is a template/reference implementation demonstrating how to integrate
//! a C++ backend for 3D Gaussian Splatting. It provides the complete backend trait
//! implementation and serves as a guide for integrating actual C++ backends.
//!
//! When a real 3DGS.cpp binary is available, minimal changes are needed to connect
//! this implementation to the actual executable.
//!
//! # About 3DGS.cpp
//!
//! This backend is designed to integrate with C++ implementations of 3D Gaussian
//! Splatting that focus on:
//! - Maximum performance on CPU and GPU
//! - Lower memory footprint
//! - Production-ready stability
//! - Cross-platform compatibility
//! - No Python runtime dependency
//!
//! # Requirements (for actual integration)
//!
//! - 3DGS.cpp binary installed and available in PATH
//! - CUDA toolkit (for GPU acceleration)
//! - Compatible with Linux, macOS, Windows
//!
//! # Environment Variables
//!
//! - `TDGS_CPP_BIN`: Path to 3dgs-cpp executable (default: "3dgs-cpp")
//! - `TDGS_CPP_DEVICE`: Device to use ("cuda" or "cpu", default: "cuda")
//!
//! # Training Workflow
//!
//! 1. Receives COLMAP sparse reconstruction and training config
//! 2. Invokes 3dgs-cpp binary with training parameters
//! 3. Monitors progress via log output
//! 4. Exports to PLY and SPLAT formats directly from C++ code
//!
//! # Export Formats
//!
//! - PLY: Native C++ implementation
//! - SPLAT: Optimized binary export for web rendering

use anyhow::{Context, Result};
use async_trait::async_trait;
use std::path::Path;
use std::process::Command;
use std::time::Instant;
use tracing::{debug, info, instrument, warn};

use super::{BackendMetadata, BackendOutput, GaussianSplatBackend, TrainingConfig};

/// 3DGS.cpp backend implementation (Template)
///
/// This is a reference implementation showing how to integrate a C++ backend.
/// When the binary is not available, it creates placeholder outputs for testing.
/// Replace the command execution logic with actual binary calls when integrating
/// a real C++ 3DGS implementation.
pub struct ThreeDGSCppBackend {
    /// Path to 3dgs-cpp executable
    cpp_bin: String,

    /// Device to use for training (cuda/cpu)
    device: String,
}

impl ThreeDGSCppBackend {
    /// Create a new 3DGS.cpp backend
    ///
    /// # Environment Variables
    ///
    /// - `TDGS_CPP_BIN`: Custom path to executable
    /// - `TDGS_CPP_DEVICE`: Training device (cuda/cpu)
    pub fn new() -> Self {
        let cpp_bin = std::env::var("TDGS_CPP_BIN").unwrap_or_else(|_| "3dgs-cpp".to_string());

        let device = std::env::var("TDGS_CPP_DEVICE").unwrap_or_else(|_| "cuda".to_string());

        ThreeDGSCppBackend { cpp_bin, device }
    }

    /// Parse training metrics from 3dgs-cpp output
    fn parse_training_output(output: &str) -> (Option<f64>, u32) {
        let mut final_loss = None;
        let mut iterations = 0;

        for line in output.lines() {
            // 3dgs-cpp log format: "[Iteration 30000] Loss: 0.0042"
            if line.contains("[Iteration") && line.contains("Loss") {
                if let Some(iter_str) = line.split("[Iteration").nth(1) {
                    if let Some(num_str) = iter_str.split(']').next() {
                        if let Ok(iter) = num_str.trim().parse::<u32>() {
                            iterations = iterations.max(iter);
                        }
                    }
                }

                if let Some(loss_str) = line.split("Loss:").nth(1) {
                    let loss_val = loss_str.split_whitespace().next().unwrap_or("");
                    if let Ok(loss) = loss_val.parse::<f64>() {
                        final_loss = Some(loss);
                    }
                }
            }
        }

        (final_loss, iterations)
    }

    /// Count Gaussians from model checkpoint
    fn count_gaussians(model_path: &Path) -> Result<usize> {
        if !model_path.exists() {
            anyhow::bail!("Model file not found: {}", model_path.display());
        }

        // If it's a PLY file, parse the header
        if model_path.extension().and_then(|e| e.to_str()) == Some("ply") {
            return Self::count_from_ply(model_path);
        }

        // For binary formats, we'd need to parse the header
        // Stub: return 0 for unknown formats
        warn!(
            path = %model_path.display(),
            "Cannot determine Gaussian count for non-PLY format"
        );
        Ok(0)
    }

    fn count_from_ply(ply_path: &Path) -> Result<usize> {
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
            if line.starts_with("end_header") {
                break;
            }
        }

        anyhow::bail!("Could not find vertex count in PLY header");
    }

    /// Validate CUDA availability (if using GPU)
    fn check_cuda_available(&self) -> Result<()> {
        if self.device == "cpu" {
            debug!("Using CPU device, skipping CUDA check");
            return Ok(());
        }

        // In a real implementation, this would check:
        // - nvidia-smi availability
        // - CUDA driver version
        // - GPU memory availability

        debug!("CUDA availability check (stub - assuming available)");
        Ok(())
    }
}

impl Default for ThreeDGSCppBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl GaussianSplatBackend for ThreeDGSCppBackend {
    #[instrument(skip(self, frames), fields(
        backend = "3dgs-cpp",
        device = %self.device,
        frame_count = frames.len(),
        iterations = config.iterations
    ))]
    async fn train(&self, frames: &[&Path], config: &TrainingConfig) -> Result<BackendOutput> {
        if frames.is_empty() {
            anyhow::bail!("Cannot train with zero frames");
        }

        // Check CUDA availability if needed
        self.check_cuda_available()?;

        // Determine workspace from first frame
        let first_frame = frames[0];
        let workspace_dir = first_frame
            .parent()
            .and_then(|p| p.parent())
            .ok_or_else(|| anyhow::anyhow!("Invalid frame path structure"))?;

        debug!(
            workspace = %workspace_dir.display(),
            "Determined workspace directory"
        );

        // Expected paths
        let colmap_sparse_dir = workspace_dir.join("colmap").join("sparse").join("0");
        let output_dir = workspace_dir.join("output");

        // Validate COLMAP data exists
        if !colmap_sparse_dir.exists() {
            anyhow::bail!(
                "COLMAP sparse reconstruction not found at: {}",
                colmap_sparse_dir.display()
            );
        }

        // Create output directory
        std::fs::create_dir_all(&output_dir).context("Failed to create output directory")?;

        info!(
            colmap = %colmap_sparse_dir.display(),
            output = %output_dir.display(),
            device = %self.device,
            "Starting 3dgs-cpp training"
        );

        let cpp_bin = self.cpp_bin.clone();
        let device = self.device.clone();
        let device_for_metadata = device.clone(); // Clone for use after async block
        let colmap_dir_clone = colmap_sparse_dir.clone();
        let output_dir_clone = output_dir.clone();
        let config_clone = config.clone();

        let start_time = Instant::now();

        // Run training in blocking task
        let (stdout, stderr) = tokio::task::spawn_blocking(move || {
            debug!("Invoking 3dgs-cpp training process");

            // Build command
            // NOTE: This is a stub implementation. Real 3dgs-cpp integration would
            // require actual binary with proper CLI interface.
            let mut cmd = Command::new(&cpp_bin);
            cmd.arg("train");
            cmd.arg("--source").arg(&colmap_dir_clone);
            cmd.arg("--output").arg(&output_dir_clone);
            cmd.arg("--iterations")
                .arg(config_clone.iterations.to_string());
            cmd.arg("--device").arg(&device);
            cmd.arg("--learning-rate")
                .arg(config_clone.learning_rate.to_string());
            cmd.arg("--densify-interval")
                .arg(config_clone.densification_interval.to_string());

            debug!(command = ?cmd, "Executing 3dgs-cpp training");

            // Execute command
            let output = cmd
                .output()
                .context("Failed to execute 3dgs-cpp training command")?;

            Ok::<_, anyhow::Error>((
                String::from_utf8_lossy(&output.stdout).to_string(),
                String::from_utf8_lossy(&output.stderr).to_string(),
            ))
        })
        .await
        .context("Training task panicked")??;

        let training_time = start_time.elapsed().as_secs_f64();

        // Parse training output
        let (final_loss, iterations_completed) = Self::parse_training_output(&stdout);

        info!(
            iterations = iterations_completed,
            loss = ?final_loss,
            time_secs = training_time,
            "3dgs-cpp training completed"
        );

        // Determine model path
        // 3dgs-cpp typically outputs to output_dir/model.ply
        let model_path = output_dir.join("model.ply");

        // For stub: create a placeholder if it doesn't exist
        if !model_path.exists() {
            warn!(
                path = %model_path.display(),
                "Model file not found (stub implementation) - creating placeholder"
            );
            std::fs::write(&model_path, "# Stub PLY file from 3dgs-cpp\n")?;
        }

        let gaussian_count = Self::count_gaussians(&model_path).ok();

        if !stderr.is_empty() {
            debug!(stderr = %stderr, "3dgs-cpp stderr output");
        }

        Ok(BackendOutput {
            model_path,
            iterations_completed,
            final_loss,
            gaussian_count,
            metadata: BackendMetadata {
                backend_name: "3dgs-cpp".to_string(),
                backend_version: None,
                training_time_secs: Some(training_time),
                peak_memory_bytes: None,
                gpu_device: Some(device_for_metadata),
                extra: std::collections::HashMap::new(),
            },
        })
    }

    #[instrument(skip(self, model), fields(output = %output_path.display()))]
    async fn export_ply(&self, model: &BackendOutput, output_path: &Path) -> Result<()> {
        info!("Exporting model to PLY format");

        if !model.model_path.exists() {
            anyhow::bail!("Model file not found: {}", model.model_path.display());
        }

        // 3dgs-cpp typically already outputs PLY - just copy it
        tokio::fs::copy(&model.model_path, output_path)
            .await
            .with_context(|| {
                format!(
                    "Failed to copy PLY from {} to {}",
                    model.model_path.display(),
                    output_path.display()
                )
            })?;

        info!(path = %output_path.display(), "PLY export complete");
        Ok(())
    }

    #[instrument(skip(self, model), fields(output = %output_path.display()))]
    async fn export_splat(&self, model: &BackendOutput, output_path: &Path) -> Result<()> {
        info!("Exporting model to SPLAT format");

        let model_path = &model.model_path;
        if !model_path.exists() {
            anyhow::bail!("Model file not found: {}", model_path.display());
        }

        // 3dgs-cpp can export SPLAT natively via CLI
        let cpp_bin = self.cpp_bin.clone();
        let model_path_clone = model_path.to_path_buf();
        let output_path_clone = output_path.to_path_buf();

        tokio::task::spawn_blocking(move || {
            debug!("Invoking 3dgs-cpp SPLAT export");

            // NOTE: This is a stub. Real implementation would call:
            // 3dgs-cpp export --format splat --input model.ply --output model.splat
            let mut cmd = Command::new(&cpp_bin);
            cmd.arg("export");
            cmd.arg("--format").arg("splat");
            cmd.arg("--input").arg(&model_path_clone);
            cmd.arg("--output").arg(&output_path_clone);

            debug!(command = ?cmd, "Executing 3dgs-cpp SPLAT export");

            let output = cmd
                .output()
                .context("Failed to execute 3dgs-cpp export command")?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                anyhow::bail!("3dgs-cpp export failed: {}", stderr);
            }

            Ok::<_, anyhow::Error>(())
        })
        .await
        .context("Export task panicked")??;

        // Stub: if export didn't create file, create placeholder
        if !output_path.exists() {
            warn!(
                path = %output_path.display(),
                "SPLAT file not created (stub) - creating placeholder"
            );
            tokio::fs::write(output_path, b"SPLAT_CPP_STUB").await?;
        }

        info!(path = %output_path.display(), "SPLAT export complete");
        Ok(())
    }
}

/// Plugin entry point for dynamic loading
///
/// This function is called when the plugin is loaded via libloading.
/// It must be exported with C linkage and the name `create_backend`.
///
/// Note: In production, each backend would be compiled as a separate plugin.
/// This is conditionally compiled to avoid symbol conflicts when testing.
#[cfg(feature = "3dgs-cpp-plugin")]
#[no_mangle]
#[allow(improper_ctypes_definitions)]
pub extern "C" fn create_backend() -> Box<dyn GaussianSplatBackend> {
    Box::new(ThreeDGSCppBackend::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test]
    fn test_backend_creation() {
        let backend = ThreeDGSCppBackend::new();
        assert!(!backend.cpp_bin.is_empty());
        assert!(!backend.device.is_empty());
    }

    #[test]
    fn test_parse_training_output() {
        let output =
            "[Iteration 1000] Loss: 0.5\n[Iteration 2000] Loss: 0.3\n[Iteration 3000] Loss: 0.1";
        let (loss, iterations) = ThreeDGSCppBackend::parse_training_output(output);

        assert_eq!(loss, Some(0.1));
        assert_eq!(iterations, 3000);
    }

    #[test]
    fn test_parse_empty_output() {
        let output = "";
        let (loss, iterations) = ThreeDGSCppBackend::parse_training_output(output);

        assert_eq!(loss, None);
        assert_eq!(iterations, 0);
    }

    #[test]
    #[serial]
    fn test_default_device() {
        std::env::remove_var("TDGS_CPP_DEVICE");
        let backend = ThreeDGSCppBackend::new();
        assert_eq!(backend.device, "cuda");
    }

    #[test]
    #[serial]
    fn test_custom_device() {
        std::env::set_var("TDGS_CPP_DEVICE", "cpu");
        let backend = ThreeDGSCppBackend::new();
        assert_eq!(backend.device, "cpu");
        std::env::remove_var("TDGS_CPP_DEVICE");
    }
}
