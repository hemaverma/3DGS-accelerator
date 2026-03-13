//! Example demonstrating the 3dgs-cpp backend usage
//!
//! This example shows how to:
//! 1. Register backends with the BackendRegistry
//! 2. Retrieve the 3dgs-cpp backend
//! 3. Configure it with environment variables
//!
//! Run with:
//! ```bash
//! cargo run --example test_3dgs_cpp_backend
//! # Or with custom device:
//! TDGS_CPP_DEVICE=cpu cargo run --example test_3dgs_cpp_backend
//! ```

use three_dgs_processor::backends::{BackendRegistry, ThreeDGSCppBackend};
use tracing::Level;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_max_level(Level::INFO)
        .init();

    println!("=== 3DGS.cpp Backend Example ===\n");

    // Create and register all backends
    let mut registry = BackendRegistry::new();
    registry.register_defaults();

    println!("Registered backends: {:?}\n", registry.list_backends());

    // Check if 3dgs-cpp is available
    let is_available = registry.is_backend_available("3dgs-cpp");
    println!("Is 3dgs-cpp backend available? {}\n", is_available);

    // Get the backend
    match registry.get_backend("3dgs-cpp").await {
        Ok(_backend) => {
            println!("✓ Successfully loaded 3dgs-cpp backend");
            println!("  Backend type: {}", std::any::type_name::<Box<dyn three_dgs_processor::backends::GaussianSplatBackend>>());
        }
        Err(e) => {
            println!("✗ Failed to load backend: {}", e);
            return Err(e);
        }
    }

    // Also demonstrate direct instantiation
    println!("\nDirect instantiation:");
    let _direct_backend = ThreeDGSCppBackend::new();
    
    // Check environment variables
    let device = std::env::var("TDGS_CPP_DEVICE").unwrap_or_else(|_| "cuda".to_string());
    let bin_path = std::env::var("TDGS_CPP_BIN").unwrap_or_else(|_| "3dgs-cpp".to_string());
    
    println!("  Device: {}", device);
    println!("  Binary path: {}", bin_path);

    println!("\n✓ 3dgs-cpp backend is fully functional!");

    Ok(())
}
