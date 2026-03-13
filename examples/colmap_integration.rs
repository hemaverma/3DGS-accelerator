//! COLMAP Integration Example
//!
//! This example demonstrates how to use the COLMAP module for
//! Structure-from-Motion reconstruction.

use anyhow::Result;
use std::path::Path;
use three_dgs_processor::colmap::{run_colmap, MatcherType};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Paths for COLMAP pipeline
    let image_dir = Path::new("data/frames");
    let database_path = Path::new("data/colmap/database.db");
    let sparse_output_dir = Path::new("data/colmap/sparse");

    // Run COLMAP reconstruction
    let output = run_colmap(
        image_dir,
        database_path,
        sparse_output_dir,
        MatcherType::Exhaustive,
        "OPENCV",
    )
    .await?;

    println!("COLMAP Reconstruction Complete!");
    println!("  Total Points: {}", output.stats.total_points);
    println!("  Cameras: {}", output.stats.camera_count);
    println!("  Sparse Dir: {}", output.sparse_dir.display());

    Ok(())
}
