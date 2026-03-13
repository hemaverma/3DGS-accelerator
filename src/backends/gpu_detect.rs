//! GPU detection and capability checking
//!
//! This module provides utilities to detect GPU availability (CUDA, Metal, ROCm)
//! and recommend appropriate backends based on hardware capabilities.

use anyhow::Result;
use std::process::Command;
use tracing::{debug, info, warn};

/// GPU vendor/platform types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GpuPlatform {
    /// NVIDIA CUDA
    Cuda,
    /// Apple Metal
    Metal,
    /// AMD ROCm
    Rocm,
    /// CPU-only (no GPU)
    None,
}

/// GPU detection result
#[derive(Debug, Clone)]
pub struct GpuInfo {
    /// GPU platform detected
    pub platform: GpuPlatform,
    /// GPU device name (if available)
    pub device_name: Option<String>,
    /// VRAM in GB (if available)
    pub vram_gb: Option<f32>,
    /// Whether GPU is usable for training
    pub is_usable: bool,
}

impl GpuInfo {
    /// Check if GPU is CUDA-capable
    pub fn has_cuda(&self) -> bool {
        self.platform == GpuPlatform::Cuda && self.is_usable
    }

    /// Check if GPU is Metal-capable
    pub fn has_metal(&self) -> bool {
        self.platform == GpuPlatform::Metal && self.is_usable
    }

    /// Check if any GPU is available
    pub fn has_gpu(&self) -> bool {
        self.platform != GpuPlatform::None && self.is_usable
    }

    /// Recommend backend based on GPU capabilities
    pub fn recommend_backend(&self) -> &str {
        if !self.is_usable {
            return "mock"; // No usable GPU: use mock for testing
        }
        
        match self.platform {
            GpuPlatform::Cuda => "gsplat", // gsplat is fastest with CUDA
            GpuPlatform::Metal => "gaussian-splatting", // Fallback for Metal
            GpuPlatform::Rocm => "gaussian-splatting", // ROCm support varies
            GpuPlatform::None => "mock", // CPU-only: use mock for testing
        }
    }
}

/// Detect GPU capabilities
///
/// Checks for CUDA, Metal, and ROCm in that order.
/// Returns information about the best available GPU.
pub fn detect_gpu() -> GpuInfo {
    // Try CUDA first (most common for 3DGS)
    if let Ok(info) = detect_cuda() {
        if info.is_usable {
            info!(?info, "CUDA GPU detected");
            return info;
        }
    }

    // Try Metal (Apple Silicon)
    if let Ok(info) = detect_metal() {
        if info.is_usable {
            info!(?info, "Metal GPU detected");
            return info;
        }
    }

    // Try ROCm (AMD)
    if let Ok(info) = detect_rocm() {
        if info.is_usable {
            info!(?info, "ROCm GPU detected");
            return info;
        }
    }

    // No GPU found
    warn!("No GPU detected - will use CPU fallback (very slow)");
    GpuInfo {
        platform: GpuPlatform::None,
        device_name: None,
        vram_gb: None,
        is_usable: false,
    }
}

/// Detect CUDA GPU
fn detect_cuda() -> Result<GpuInfo> {
    // Try nvidia-smi first (most reliable)
    if let Ok(output) = Command::new("nvidia-smi")
        .arg("--query-gpu=name,memory.total")
        .arg("--format=csv,noheader")
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let parts: Vec<&str> = stdout.trim().split(',').collect();
            
            if parts.len() >= 2 {
                let device_name = parts[0].trim().to_string();
                let vram_str = parts[1].trim().replace(" MiB", "");
                let vram_gb = vram_str.parse::<f32>().ok().map(|mb| mb / 1024.0);
                
                debug!(device = %device_name, vram_gb = ?vram_gb, "CUDA GPU found via nvidia-smi");
                
                return Ok(GpuInfo {
                    platform: GpuPlatform::Cuda,
                    device_name: Some(device_name),
                    vram_gb,
                    is_usable: true,
                });
            }
        }
    }

    // Try Python PyTorch as fallback
    if let Ok(output) = Command::new("python3")
        .arg("-c")
        .arg("import torch; print(torch.cuda.is_available()); print(torch.cuda.get_device_name(0) if torch.cuda.is_available() else '')")
        .output()
    {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let lines: Vec<&str> = stdout.lines().collect();
            
            if lines.len() >= 2 && lines[0].trim() == "True" {
                let device_name = if !lines[1].is_empty() {
                    Some(lines[1].trim().to_string())
                } else {
                    None
                };
                
                debug!(device = ?device_name, "CUDA GPU found via PyTorch");
                
                return Ok(GpuInfo {
                    platform: GpuPlatform::Cuda,
                    device_name,
                    vram_gb: None,
                    is_usable: true,
                });
            }
        }
    }

    anyhow::bail!("CUDA not detected")
}

/// Detect Metal GPU (Apple Silicon)
fn detect_metal() -> Result<GpuInfo> {
    #[cfg(target_os = "macos")]
    {
        // Check if we're on Apple Silicon
        if let Ok(output) = Command::new("sysctl")
            .arg("-n")
            .arg("machdep.cpu.brand_string")
            .output()
        {
            if output.status.success() {
                let cpu_info = String::from_utf8_lossy(&output.stdout);
                if cpu_info.contains("Apple") {
                    debug!("Apple Silicon detected");
                    
                    // Metal is available on all Apple Silicon Macs
                    return Ok(GpuInfo {
                        platform: GpuPlatform::Metal,
                        device_name: Some("Apple GPU".to_string()),
                        vram_gb: None, // Unified memory
                        is_usable: true,
                    });
                }
            }
        }
    }

    anyhow::bail!("Metal not detected")
}

/// Detect ROCm GPU (AMD)
fn detect_rocm() -> Result<GpuInfo> {
    // Try rocm-smi
    if let Ok(output) = Command::new("rocm-smi").arg("--showproductname").output() {
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Some(line) = stdout.lines().nth(2) {
                let device_name = line.trim().to_string();
                
                debug!(device = %device_name, "ROCm GPU found");
                
                return Ok(GpuInfo {
                    platform: GpuPlatform::Rocm,
                    device_name: Some(device_name),
                    vram_gb: None,
                    is_usable: true,
                });
            }
        }
    }

    anyhow::bail!("ROCm not detected")
}

/// Get GPU status as human-readable string
pub fn gpu_status_string(info: &GpuInfo) -> String {
    match info.platform {
        GpuPlatform::Cuda if info.is_usable => {
            let device = info.device_name.as_deref().unwrap_or("Unknown CUDA GPU");
            let vram = info.vram_gb.map(|gb| format!(" ({:.1}GB VRAM)", gb)).unwrap_or_default();
            format!("✓ CUDA GPU: {}{}", device, vram)
        }
        GpuPlatform::Metal if info.is_usable => {
            "✓ Metal GPU: Apple Silicon".to_string()
        }
        GpuPlatform::Rocm if info.is_usable => {
            let device = info.device_name.as_deref().unwrap_or("Unknown AMD GPU");
            format!("✓ ROCm GPU: {}", device)
        }
        _ => "⚠ No GPU detected - CPU fallback mode (training will be very slow)".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gpu_detection() {
        let info = detect_gpu();
        // Just verify it doesn't panic
        println!("Detected GPU: {:?}", info);
    }

    #[test]
    fn test_backend_recommendation() {
        let cuda_info = GpuInfo {
            platform: GpuPlatform::Cuda,
            device_name: Some("RTX 4090".to_string()),
            vram_gb: Some(24.0),
            is_usable: true,
        };
        assert_eq!(cuda_info.recommend_backend(), "gsplat");

        let cpu_info = GpuInfo {
            platform: GpuPlatform::None,
            device_name: None,
            vram_gb: None,
            is_usable: false,
        };
        assert_eq!(cpu_info.recommend_backend(), "mock");
    }

    #[test]
    fn test_gpu_status_string() {
        let info = GpuInfo {
            platform: GpuPlatform::Cuda,
            device_name: Some("RTX 4090".to_string()),
            vram_gb: Some(24.0),
            is_usable: true,
        };
        let status = gpu_status_string(&info);
        assert!(status.contains("CUDA"));
        assert!(status.contains("RTX 4090"));
    }
}
