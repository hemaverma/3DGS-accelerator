//! COLMAP output parsing
//!
//! This module parses COLMAP sparse reconstruction output to extract
//! statistics such as point count, camera count, and reprojection errors.

use anyhow::{Context, Result};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use tracing::{debug, instrument};

use super::models::ColmapStats;

/// Parse COLMAP sparse reconstruction output
///
/// COLMAP stores sparse reconstruction in binary format by default.
/// This function reads the points3D.bin file to extract statistics.
#[instrument(skip_all, fields(sparse_dir = %sparse_dir.display()))]
pub async fn parse_sparse_output(sparse_dir: &Path) -> Result<ColmapStats> {
    let sparse_dir = sparse_dir.to_path_buf();

    tokio::task::spawn_blocking(move || {
        // COLMAP mapper creates subdirectories for each reconstruction (0, 1, 2, ...)
        // We'll use the first one (0) which is typically the main reconstruction
        let reconstruction_dir = sparse_dir.join("0");

        if !reconstruction_dir.exists() {
            anyhow::bail!(
                "COLMAP reconstruction directory not found: {}",
                reconstruction_dir.display()
            );
        }

        let points3d_path = reconstruction_dir.join("points3D.txt");
        let _cameras_path = reconstruction_dir.join("cameras.txt");
        let images_path = reconstruction_dir.join("images.txt");

        // Check if text format exists, otherwise we have binary format
        let total_points = if points3d_path.exists() {
            count_points_from_text(&points3d_path)?
        } else {
            // For binary format, we'll need to convert first or estimate
            // For now, we'll use a simpler approach: convert binary to text
            convert_binary_to_text(&sparse_dir, &reconstruction_dir)?;
            count_points_from_text(&points3d_path)?
        };

        let camera_count = if images_path.exists() {
            count_cameras_from_text(&images_path)?
        } else {
            0
        };

        debug!(
            total_points,
            camera_count, "Parsed COLMAP sparse reconstruction statistics"
        );

        Ok(ColmapStats {
            total_points,
            camera_count,
            avg_reprojection_error: None, // Can be computed from points3D.txt if needed
        })
    })
    .await
    .context("Sparse output parsing task panicked")?
}

/// Convert COLMAP binary format to text format using colmap model_converter
fn convert_binary_to_text(_sparse_dir: &Path, reconstruction_dir: &Path) -> Result<()> {
    debug!("Converting COLMAP binary output to text format");

    let colmap_bin = std::env::var("COLMAP_BIN").unwrap_or_else(|_| "colmap".to_string());

    let output = std::process::Command::new(&colmap_bin)
        .arg("model_converter")
        .arg("--input_path")
        .arg(reconstruction_dir)
        .arg("--output_path")
        .arg(reconstruction_dir)
        .arg("--output_type")
        .arg("TXT")
        .output()
        .context("Failed to execute COLMAP model_converter")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "COLMAP model conversion failed with status {}: {}",
            output.status,
            stderr
        );
    }

    debug!("COLMAP model conversion completed");
    Ok(())
}

/// Count 3D points from points3D.txt
///
/// Format: POINT3D_ID X Y Z R G B ERROR TRACK_LENGTH TRACK[...]
/// Lines starting with # are comments
fn count_points_from_text(points3d_path: &Path) -> Result<usize> {
    let file = File::open(points3d_path)
        .with_context(|| format!("Failed to open points3D.txt at {}", points3d_path.display()))?;

    let reader = BufReader::new(file);
    let mut count = 0;

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        count += 1;
    }

    Ok(count)
}

/// Count cameras (registered images) from images.txt
///
/// Format: IMAGE_ID QW QX QY QZ TX TY TZ CAMERA_ID NAME
/// Lines starting with # are comments
/// Each image has 2 lines: metadata line and points line
fn count_cameras_from_text(images_path: &Path) -> Result<usize> {
    let file = File::open(images_path)
        .with_context(|| format!("Failed to open images.txt at {}", images_path.display()))?;

    let reader = BufReader::new(file);
    let mut count = 0;
    let mut is_metadata_line = true;

    for line in reader.lines() {
        let line = line?;
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        // Count only metadata lines (every other non-comment line)
        if is_metadata_line {
            count += 1;
        }
        is_metadata_line = !is_metadata_line;
    }

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_count_points_from_text() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let points_path = temp_dir.path().join("points3D.txt");

        let mut file = File::create(&points_path)?;
        writeln!(file, "# 3D point list with one line of data per point:")?;
        writeln!(
            file,
            "#   POINT3D_ID, X, Y, Z, R, G, B, ERROR, TRACK[] as (IMAGE_ID, POINT2D_IDX)"
        )?;
        writeln!(file, "1 0.5 0.3 1.2 255 128 64 0.5 3 1 100 2 200 3 300")?;
        writeln!(file, "2 1.0 0.6 2.4 128 255 32 0.3 2 1 101 2 201")?;
        writeln!(
            file,
            "3 -0.5 -0.3 0.8 64 128 255 0.7 4 1 102 2 202 3 302 4 402"
        )?;

        let count = count_points_from_text(&points_path)?;
        assert_eq!(count, 3);

        Ok(())
    }

    #[test]
    fn test_count_cameras_from_text() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let images_path = temp_dir.path().join("images.txt");

        let mut file = File::create(&images_path)?;
        writeln!(file, "# Image list with two lines of data per image:")?;
        writeln!(
            file,
            "#   IMAGE_ID, QW, QX, QY, QZ, TX, TY, TZ, CAMERA_ID, NAME"
        )?;
        writeln!(file, "#   POINTS2D[] as (X, Y, POINT3D_ID)")?;
        writeln!(file, "1 0.5 0.3 1.2 0.8 0.1 0.2 0.3 1 frame_000001.jpg")?;
        writeln!(file, "100.5 200.3 1 150.2 250.4 2")?;
        writeln!(file, "2 0.6 0.4 1.1 0.9 0.2 0.3 0.4 1 frame_000002.jpg")?;
        writeln!(file, "110.5 210.3 1 160.2 260.4 2")?;

        let count = count_cameras_from_text(&images_path)?;
        assert_eq!(count, 2);

        Ok(())
    }
}
