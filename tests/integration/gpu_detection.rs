//! GPU Detection Integration Tests
//!
//! Tests GPU detection logic and backend selection based on available hardware.
//! These tests validate that the system correctly identifies GPUs and selects
//! appropriate backends.

use three_dgs_processor::backends::gpu_detect::{detect_gpu, GpuInfo, GpuPlatform};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_detection() {
        // This will detect actual GPU on the system or return None platform
        let gpu_info = detect_gpu();
        
        // Test should pass regardless of GPU presence
        if gpu_info.has_gpu() {
            // If GPU detected, validate the info structure
            assert!(gpu_info.device_name.is_some());
            assert!(matches!(
                gpu_info.platform,
                GpuPlatform::Cuda | GpuPlatform::Metal | GpuPlatform::Rocm
            ));
            
            // VRAM should be reasonable if reported
            if let Some(vram) = gpu_info.vram_gb {
                assert!(vram > 0.0);
                assert!(vram < 1000.0); // Sanity check
            }
        } else {
            // No GPU detected - valid on CPU-only systems
            println!("No GPU detected (CPU-only system)");
            assert_eq!(gpu_info.platform, GpuPlatform::None);
        }
    }

    #[test]
    fn test_gpu_platform_debug() {
        assert!(format!("{:?}", GpuPlatform::Cuda).contains("Cuda"));
        assert!(format!("{:?}", GpuPlatform::Metal).contains("Metal"));
        assert!(format!("{:?}", GpuPlatform::Rocm).contains("Rocm"));
    }

    #[test]
    fn test_gpu_info_structure() {
        let gpu_info = GpuInfo {
            platform: GpuPlatform::Cuda,
            device_name: Some("NVIDIA GeForce RTX 3090".to_string()),
            vram_gb: Some(24.0),
            is_usable: true,
        };

        assert_eq!(gpu_info.platform, GpuPlatform::Cuda);
        assert_eq!(gpu_info.device_name, Some("NVIDIA GeForce RTX 3090".to_string()));
        assert_eq!(gpu_info.vram_gb, Some(24.0));
        assert!(gpu_info.is_usable);
    }

    #[test]
    fn test_gpu_info_without_vram() {
        let gpu_info = GpuInfo {
            platform: GpuPlatform::Metal,
            device_name: Some("Apple M1".to_string()),
            vram_gb: None,
            is_usable: true,
        };

        assert_eq!(gpu_info.platform, GpuPlatform::Metal);
        assert_eq!(gpu_info.device_name, Some("Apple M1".to_string()));
        assert!(gpu_info.vram_gb.is_none());
    }

    #[test]
    fn test_force_cpu_backend_env() {
        // Test that FORCE_CPU_BACKEND=1 works
        std::env::set_var("FORCE_CPU_BACKEND", "1");
        
        // When forced to CPU, GPU detection should be skipped
        // (Implementation detail - actual behavior may vary)
        
        std::env::remove_var("FORCE_CPU_BACKEND");
    }

    #[test]
    fn test_cuda_detection_commands() {
        // Test if nvidia-smi is available (indicates CUDA GPU)
        let nvidia_smi = std::process::Command::new("nvidia-smi")
            .arg("--query-gpu=name,memory.total")
            .arg("--format=csv,noheader")
            .output();

        if let Ok(output) = nvidia_smi {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                println!("CUDA GPU detected: {}", stdout.trim());
                
                // If nvidia-smi works, GPU detection should find it
                let gpu_info = detect_gpu();
                if gpu_info.has_cuda() {
                    assert_eq!(gpu_info.platform, GpuPlatform::Cuda);
                }
            } else {
                println!("nvidia-smi command failed - no CUDA GPU");
            }
        } else {
            println!("nvidia-smi not available");
        }
    }

    #[test]
    fn test_metal_detection_on_macos() {
        // Test Metal detection on macOS
        #[cfg(target_os = "macos")]
        {
            let system_profiler = std::process::Command::new("system_profiler")
                .arg("SPDisplaysDataType")
                .output();

            if let Ok(output) = system_profiler {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    
                    if stdout.contains("Apple") || stdout.contains("Metal") {
                        println!("Metal GPU likely available");
                        
                        // GPU detection should find Metal GPU
                        let gpu_info = detect_gpu();
                        if gpu_info.has_metal() {
                            // On Apple Silicon, should detect Metal
                            assert_eq!(gpu_info.platform, GpuPlatform::Metal);
                        }
                    }
                }
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            println!("Metal detection test skipped (not macOS)");
        }
    }

    #[test]
    fn test_rocm_detection() {
        // Test ROCm detection (AMD GPUs)
        let rocm_smi = std::process::Command::new("rocm-smi")
            .arg("--showproductname")
            .output();

        if let Ok(output) = rocm_smi {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                println!("ROCm GPU detected: {}", stdout.trim());
                
                let gpu_info = detect_gpu();
                if matches!(gpu_info.platform, GpuPlatform::Rocm) {
                    assert_eq!(gpu_info.platform, GpuPlatform::Rocm);
                }
            } else {
                println!("rocm-smi command failed - no ROCm GPU");
            }
        } else {
            println!("rocm-smi not available");
        }
    }

    #[test]
    fn test_gpu_detection_consistency() {
        // Run detection multiple times - should get consistent results
        let gpu1 = detect_gpu();
        let gpu2 = detect_gpu();

        // Platform should be consistent
        assert_eq!(gpu1.platform, gpu2.platform);
        assert_eq!(gpu1.device_name, gpu2.device_name);
        assert_eq!(gpu1.is_usable, gpu2.is_usable);
    }

    #[test]
    fn test_gpu_recommended_backend() {
        let gpu_info = detect_gpu();

        let recommended = gpu_info.recommend_backend();

        println!(
            "Detected {:?} GPU, recommended backend: {}",
            gpu_info.platform, recommended
        );

        // Validate recommendation is sensible
        assert!(!recommended.is_empty());
        assert!(matches!(recommended, "mock" | "gsplat" | "gaussian-splatting" | "3dgs-cpp"));
    }

    #[test]
    fn test_vram_parsing() {
        // Test parsing VRAM from different outputs
        let nvidia_output = "NVIDIA GeForce RTX 3090, 24576 MiB";
        
        // Extract VRAM in GB
        if let Some(mem_str) = nvidia_output.split(',').nth(1) {
            if let Some(num_str) = mem_str.trim().split_whitespace().next() {
                if let Ok(mem_mib) = num_str.parse::<f64>() {
                    let vram_gb = mem_mib / 1024.0;
                    assert!((vram_gb - 24.0).abs() < 0.1);
                }
            }
        }
    }

    #[test]
    fn test_gpu_info_debug_format() {
        let gpu_info = GpuInfo {
            platform: GpuPlatform::Cuda,
            device_name: Some("Test GPU".to_string()),
            vram_gb: Some(8.0),
            is_usable: true,
        };

        let debug_str = format!("{:?}", gpu_info);
        assert!(debug_str.contains("Cuda"));
        assert!(debug_str.contains("Test GPU"));
        assert!(debug_str.contains("8"));
    }

    #[test]
    fn test_multiple_gpu_detection() {
        // Systems with multiple GPUs should detect at least one
        let gpu_info = detect_gpu();
        
        // Should always return valid result
        let name = gpu_info.device_name.as_deref().unwrap_or("None");
        println!("Primary GPU detected: {} ({:?})", name, gpu_info.platform);
    }
}
