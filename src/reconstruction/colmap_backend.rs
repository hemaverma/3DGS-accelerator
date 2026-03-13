//! COLMAP reconstruction backend
//!
//! Implements the ReconstructionBackend trait using COLMAP for
//! Structure-from-Motion sparse reconstruction.

use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::json;
use std::path::Path;
use tracing::{info, instrument};

use super::backend::ReconstructionBackend;
use super::models::{MatcherType, ReconstructionOutput, ReconstructionStats};
use crate::colmap::{self, ColmapRunner};

/// COLMAP reconstruction backend
pub struct ColmapBackend {
    runner: ColmapRunner,
}

impl ColmapBackend {
    /// Create a new COLMAP backend
    pub fn new() -> Self {
        Self {
            runner: ColmapRunner::new(),
        }
    }

    /// Parse COLMAP-specific configuration from JSON
    fn parse_config(&self, config: &serde_json::Value) -> ColmapConfig {
        ColmapConfig {
            matcher: config
                .get("matcher")
                .and_then(|v| v.as_str())
                .and_then(|s| s.parse().ok())
                .unwrap_or(MatcherType::Exhaustive),
            camera_model: config
                .get("camera_model")
                .and_then(|v| v.as_str())
                .unwrap_or("OPENCV")
                .to_string(),
        }
    }
}

impl Default for ColmapBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ReconstructionBackend for ColmapBackend {
    fn name(&self) -> &str {
        "colmap"
    }

    fn description(&self) -> &str {
        "COLMAP Structure-from-Motion (gold standard SfM pipeline)"
    }

    fn is_available(&self) -> Result<bool> {
        // Check if COLMAP binary is available
        let colmap_bin = std::env::var("COLMAP_BIN").unwrap_or_else(|_| "colmap".to_string());

        let output = std::process::Command::new(&colmap_bin)
            .arg("--version")
            .output();

        Ok(output.is_ok())
    }

    #[instrument(skip(self, config), fields(
        image_dir = %image_dir.display(),
        output_dir = %output_dir.display()
    ))]
    async fn reconstruct(
        &self,
        image_dir: &Path,
        output_dir: &Path,
        config: &serde_json::Value,
    ) -> Result<ReconstructionOutput> {
        info!("Starting COLMAP reconstruction");

        let colmap_config = self.parse_config(config);

        // Create output directories
        std::fs::create_dir_all(output_dir).context("Failed to create output directory")?;

        let database_path = output_dir.join("database.db");
        let sparse_dir = output_dir.join("sparse");

        // Step 1: Feature extraction
        info!("Running COLMAP feature extraction");
        self.runner
            .extract_features(image_dir, &database_path, &colmap_config.camera_model)
            .await
            .context("COLMAP feature extraction failed")?;

        // Step 2: Feature matching
        info!(
            "Running COLMAP feature matching with {:?} matcher",
            colmap_config.matcher
        );
        self.runner
            .match_features(&database_path, colmap_config.matcher)
            .await
            .context("COLMAP feature matching failed")?;

        // Step 3: Sparse reconstruction
        info!("Running COLMAP sparse reconstruction");
        self.runner
            .sparse_reconstruction(&database_path, image_dir, &sparse_dir)
            .await
            .context("COLMAP sparse reconstruction failed")?;

        // Step 4: Parse output
        let colmap_stats = colmap::parser::parse_sparse_output(&sparse_dir)
            .await
            .context("Failed to parse COLMAP sparse reconstruction output")?;

        info!(
            point_count = colmap_stats.total_points,
            camera_count = colmap_stats.camera_count,
            "COLMAP reconstruction complete"
        );

        // Convert to common format
        let stats = ReconstructionStats {
            total_points: colmap_stats.total_points,
            registered_images: colmap_stats.camera_count,
            avg_reprojection_error: colmap_stats.avg_reprojection_error,
            backend_name: "colmap".to_string(),
        };

        // Build metadata
        let metadata = json!({
            "database_path": database_path.to_string_lossy(),
            "sparse_dir": sparse_dir.to_string_lossy(),
            "matcher": colmap_config.matcher.to_string(),
            "camera_model": colmap_config.camera_model,
        });

        // Note: For full camera pose parsing, we'd need to parse cameras.txt and images.txt
        // For now, we'll leave cameras empty and let the 3DGS backend read COLMAP format directly
        let output = ReconstructionOutput {
            output_dir: output_dir.to_path_buf(),
            cameras: Vec::new(), // TODO: Parse cameras.txt and images.txt
            sparse_point_cloud: Some(sparse_dir.join("0").join("points3D.bin")),
            stats,
            metadata,
        };

        Ok(output)
    }
}

/// COLMAP-specific configuration
struct ColmapConfig {
    matcher: MatcherType,
    camera_model: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_colmap_backend_creation() {
        let backend = ColmapBackend::new();
        assert_eq!(backend.name(), "colmap");
    }

    #[test]
    fn test_colmap_backend_description() {
        let backend = ColmapBackend::new();
        assert!(!backend.description().is_empty());
    }

    #[test]
    fn test_parse_config_defaults() {
        let backend = ColmapBackend::new();
        let config = json!({});
        let parsed = backend.parse_config(&config);
        assert_eq!(parsed.matcher, MatcherType::Exhaustive);
        assert_eq!(parsed.camera_model, "OPENCV");
    }

    #[test]
    fn test_parse_config_custom() {
        let backend = ColmapBackend::new();
        let config = json!({
            "matcher": "sequential",
            "camera_model": "SIMPLE_PINHOLE"
        });
        let parsed = backend.parse_config(&config);
        assert_eq!(parsed.matcher, MatcherType::Sequential);
        assert_eq!(parsed.camera_model, "SIMPLE_PINHOLE");
    }
}
