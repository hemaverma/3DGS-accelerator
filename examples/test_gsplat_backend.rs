#!/usr/bin/env cargo
//! Test gsplat backend with real COLMAP data
//!
//! This example demonstrates training with the gsplat backend using
//! the test COLMAP reconstruction data.
//!
//! Usage:
//!   GSPLAT_BIN=scripts/gsplat_train.py cargo run --example test_gsplat_backend

use anyhow::Result;
use std::path::PathBuf;
use three_dgs_processor::backends::{GaussianSplatBackend, GsplatBackend, TrainingConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("debug")
        .init();

    println!("=== Testing gsplat Backend with Real COLMAP Data ===\n");

    // Locate test data
    let test_data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("testdata")
        .join("sample_scene")
        .join("test_run");

    if !test_data_dir.exists() {
        eprintln!("Error: Test data not found at: {}", test_data_dir.display());
        eprintln!("Run: python3 scripts/create_test_colmap_data.py");
        std::process::exit(1);
    }

    // Verify COLMAP data exists
    let colmap_dir = test_data_dir.join("colmap").join("sparse").join("0");
    if !colmap_dir.exists() {
        eprintln!("Error: COLMAP data not found at: {}", colmap_dir.display());
        std::process::exit(1);
    }

    println!("Test data location: {}", test_data_dir.display());
    println!("COLMAP sparse dir: {}", colmap_dir.display());

    // Collect frame paths
    let images_dir = test_data_dir.join("images");
    let mut frames: Vec<PathBuf> = std::fs::read_dir(&images_dir)?
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()?.to_str()? == "jpg" {
                Some(path)
            } else {
                None
            }
        })
        .collect();

    frames.sort();

    println!("\nFound {} frame images", frames.len());

    // Create gsplat backend
    println!("\n--- Creating gsplat backend ---");
    let backend = GsplatBackend::new();

    // Configure training (use minimal iterations for testing)
    let config = TrainingConfig {
        iterations: 100, // Quick test
        learning_rate: 0.0016,
        densification_start: 50,
        densification_stop: 80,
        densification_interval: 10,
        opacity_reset_interval: 30,
    };

    println!("\nTraining configuration:");
    println!("  Iterations: {}", config.iterations);
    println!("  Learning rate: {}", config.learning_rate);
    println!("  Densification: start={}, stop={}, interval={}", 
        config.densification_start,
        config.densification_stop,
        config.densification_interval
    );

    // Train model
    println!("\n--- Starting training ---");
    let frame_refs: Vec<&std::path::Path> = frames.iter().map(|p| p.as_path()).collect();
    
    match backend.train(&frame_refs, &config).await {
        Ok(output) => {
            println!("\n✓ Training completed successfully!");
            println!("\nOutput details:");
            println!("  Model path: {}", output.model_path.display());
            
            if let Some(count) = output.gaussian_count {
                println!("  Gaussian count: {}", count);
            }
            
            if let Some(loss) = output.final_loss {
                println!("  Final loss: {:.6}", loss);
            }
            
            if let Some(time) = output.metadata.training_time_secs {
                println!("  Training time: {:.2}s", time);
            }
            
            if let Some(device) = &output.metadata.gpu_device {
                println!("  Device: {}", device);
            }

            // Verify output file exists
            if output.model_path.exists() {
                let file_size = std::fs::metadata(&output.model_path)?.len();
                println!("\n✓ Model file exists ({} bytes)", file_size);
            } else {
                eprintln!("\n✗ Warning: Model file not found at expected path");
            }
        }
        Err(e) => {
            eprintln!("\n✗ Training failed: {}", e);
            eprintln!("\nNote: If CUDA is not available, gsplat may fail to render.");
            eprintln!("This is expected on macOS or systems without GPU support.");
            std::process::exit(1);
        }
    }

    println!("\n=== Test completed ===");
    Ok(())
}
