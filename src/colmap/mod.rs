//! COLMAP Structure-from-Motion integration
//!
//! This module provides integration with COLMAP for sparse 3D reconstruction.
//! It extracts camera poses and sparse point clouds from multi-view images.

pub mod models;
pub mod parser;
pub mod runner;

use anyhow::{Context, Result};
use std::path::Path;
use tracing::{info, instrument};

pub use models::{ColmapOutput, ColmapStats, MatcherType};
pub use runner::ColmapRunner;

/// Run complete COLMAP pipeline: feature extraction, matching, and sparse reconstruction
///
/// # Arguments
/// * `image_dir` - Directory containing extracted frames
/// * `database_path` - Path where COLMAP database will be created
/// * `sparse_output_dir` - Directory for sparse reconstruction output
/// * `matcher` - Matching strategy (exhaustive, sequential, vocab_tree)
/// * `camera_model` - Camera model (OPENCV, SIMPLE_PINHOLE, FULL_OPENCV)
///
/// # Returns
/// * `ColmapOutput` - Contains paths to output and statistics (point count, etc.)
#[instrument(skip_all, fields(
    image_dir = %image_dir.display(),
    matcher = ?matcher,
    camera_model = %camera_model
))]
pub async fn run_colmap(
    image_dir: &Path,
    database_path: &Path,
    sparse_output_dir: &Path,
    matcher: MatcherType,
    camera_model: &str,
) -> Result<ColmapOutput> {
    info!("Starting COLMAP sparse reconstruction pipeline");

    let runner = ColmapRunner::new();

    // Step 1: Feature extraction
    info!("Running COLMAP feature extraction");
    runner
        .extract_features(image_dir, database_path, camera_model)
        .await
        .context("COLMAP feature extraction failed")?;

    // Step 2: Feature matching
    info!("Running COLMAP feature matching with {:?} matcher", matcher);
    runner
        .match_features(database_path, matcher)
        .await
        .context("COLMAP feature matching failed")?;

    // Step 3: Sparse reconstruction
    info!("Running COLMAP sparse reconstruction");
    runner
        .sparse_reconstruction(database_path, image_dir, sparse_output_dir)
        .await
        .context("COLMAP sparse reconstruction failed")?;

    // Step 4: Parse output and validate
    let stats = parser::parse_sparse_output(sparse_output_dir)
        .await
        .context("Failed to parse COLMAP sparse reconstruction output")?;

    info!(
        point_count = stats.total_points,
        camera_count = stats.camera_count,
        "COLMAP reconstruction complete"
    );

    // Validate minimum point count requirement
    if stats.total_points < 1000 {
        anyhow::bail!(
            "COLMAP reconstruction quality too low: only {} points (minimum 1000 required)",
            stats.total_points
        );
    }

    Ok(ColmapOutput {
        sparse_dir: sparse_output_dir.to_path_buf(),
        database_path: database_path.to_path_buf(),
        stats,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matcher_type_from_str() {
        assert_eq!(
            "exhaustive".parse::<MatcherType>().unwrap(),
            MatcherType::Exhaustive
        );
        assert_eq!(
            "sequential".parse::<MatcherType>().unwrap(),
            MatcherType::Sequential
        );
        assert_eq!(
            "vocab_tree".parse::<MatcherType>().unwrap(),
            MatcherType::VocabTree
        );
    }
}
