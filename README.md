# 3DGS Video Processor

Containerized Rust service for processing multi-view video captures into 3D Gaussian Splatting models.

[![Docker](https://img.shields.io/badge/docker-multi--arch-blue)](https://hub.docker.com)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange)](https://www.rust-lang.org)
[![License](https://img.shields.io/badge/license-MIT-green)](LICENSE)

## Overview

3DGS Video Processor is a long-running service that:

* **Watches** directories for new video uploads (multiple videos per scene)
* **Extracts** frames from all videos concurrently using FFmpeg
* **Reconstructs** camera poses using pluggable reconstruction backends (COLMAP, Nerfstudio, etc.)
* **Trains** 3D Gaussian Splatting models using swappable backends
* **Exports** outputs in standard formats (.ply and .splat)
* **Manages** processed data with configurable retention policies

Perfect for automated 3DGS processing pipelines in local or cloud environments (Azure Blob Storage via Blobfuse2).

## Quick Start

### Local Mode

```bash
# Create directory structure
mkdir -p input output processed error

# Run container
docker run -d \
  --name 3dgs-processor \
  -v $(pwd)/input:/data/input \
  -v $(pwd)/output:/data/output \
  -v $(pwd)/processed:/data/processed \
  -v $(pwd)/error:/data/error \
  -e INPUT_PATH=/data/input \
  -e OUTPUT_PATH=/data/output \
  -e PROCESSED_PATH=/data/processed \
  -e ERROR_PATH=/data/error \
  -e BACKEND=gaussian-splatting \
  3dgs-processor:latest

# Copy multi-view videos to input directory
mkdir -p input/scene_001
cp view1.mp4 view2.mp4 view3.mp4 input/scene_001/

# Wait for processing (monitor logs)
docker logs -f 3dgs-processor

# Check output
ls output/scene_001/
# -> manifest.json, model.ply, model.splat
```

### Azure Blob Storage Mode

```bash
docker run -d --privileged \
  --name 3dgs-processor \
  -e AZURE_STORAGE_CONNECTION_STRING="DefaultEndpointsProtocol=..." \
  -e AZURE_CONTAINER_NAME=3dgs-input \
  -e BACKEND=gsplat \
  3dgs-processor:latest
```

## Features

* **Multi-Video Processing**: Handles multiple videos per scene for better reconstruction
* **Pluggable Reconstruction**: Swap SfM backends (COLMAP, Nerfstudio, Precalibrated)
* **Pluggable 3DGS Backends**: Swap training implementations (gaussian-splatting, gsplat, 3DGS.cpp)
* **Watch Mode**: Automatically processes new uploads with stability detection
* **Progress Tracking**: Real-time progress monitoring with checkpoint-based resumption
* **Error Recovery**: Retry logic with exponential backoff, error folder isolation
* **Resource Management**: Temporary file cleanup, configurable retention policies
* **Cloud Ready**: Native Azure Blob Storage support via Blobfuse2
* **Multi-Architecture**: Runs on x86_64 and ARM64 (Apple Silicon, AWS Graviton)

## Hardware Requirements

### Minimum Requirements (Testing/Development)

| Component | Specification |
|-----------|---------------|
| **CPU** | 4 cores, 2.0 GHz+ |
| **RAM** | 8 GB |
| **Storage** | 50 GB free (for processing temporary files) |
| **GPU** | None (CPU-only mode using mock backend) |

Suitable for: Pipeline testing, development, small test scenes

### Recommended Requirements (Small-Medium Scenes)

| Component | Specification |
|-----------|---------------|
| **CPU** | 8 cores, 3.0 GHz+ (Intel i7/Ryzen 7 or better) |
| **RAM** | 16 GB |
| **Storage** | 100 GB free SSD |
| **GPU** | 6-8 GB VRAM (NVIDIA RTX 3060, Apple M1/M2, AMD RX 6700 XT) |

Suitable for: 2-5 videos, 30-60 seconds each, 1080p resolution

**Processing Time**: 5-15 minutes per scene (30,000 training iterations)

### Production Requirements (Large Scenes)

| Component | Specification |
|-----------|---------------|
| **CPU** | 16+ cores, 3.5 GHz+ (Intel Xeon, AMD EPYC, Ryzen 9) |
| **RAM** | 32 GB+ |
| **Storage** | 500 GB+ NVMe SSD |
| **GPU** | 12-24 GB VRAM (NVIDIA RTX 3090/4090, A100, Apple M2 Ultra) |

Suitable for: 5-10+ videos, 2-5 minutes each, 4K resolution

**Processing Time**: 10-30 minutes per scene (30,000+ iterations)

### GPU Recommendations

**NVIDIA CUDA** (Best Performance):
- ✅ RTX 3060/3070/3080/3090 (8-24 GB VRAM)
- ✅ RTX 4060/4070/4080/4090 (8-24 GB VRAM)  
- ✅ A100/A6000 (40-80 GB VRAM) for production
- Backend: **gsplat** (optimized for CUDA)

**Apple Metal**:
- ✅ M1/M2/M3 (8-24 GB unified memory)
- ✅ M1/M2/M3 Pro/Max/Ultra (16-192 GB)
- Backend: **gaussian-splatting** (Metal support)

**AMD ROCm**:
- ✅ RX 6700 XT, 6800 XT, 6900 XT (12-16 GB VRAM)
- ✅ Radeon VII, Instinct MI100/MI200 series
- Backend: **gaussian-splatting** (ROCm support)

**CPU-Only** (Testing):
- ⚠️ Mock backend only (no real training)
- Use for: Pipeline testing, development, CI/CD

### Storage Requirements Per Job

Estimate disk space needed based on input videos:

| Input Size | Extracted Frames | COLMAP | Training | Output | **Total** |
|------------|------------------|--------|----------|--------|-----------|
| 3×30s videos @ 1080p (500 MB) | 2-4 GB | 500 MB | 1-2 GB | 100-500 MB | **~5-8 GB** |
| 5×60s videos @ 1080p (1.5 GB) | 8-12 GB | 2 GB | 3-5 GB | 200 MB-1 GB | **~15-20 GB** |
| 10×120s videos @ 4K (10 GB) | 40-60 GB | 10 GB | 10-15 GB | 500 MB-2 GB | **~60-90 GB** |

**Recommendation**: Allocate 15-20x input video size for temporary processing files.

### Memory Requirements

RAM usage scales with:
- Number of concurrent videos during frame extraction
- COLMAP reconstruction complexity (number of images, features)
- 3DGS training batch size

**Guidelines**:
- **8 GB RAM**: 2-3 videos, 1080p
- **16 GB RAM**: 5-7 videos, 1080p or 2-3 videos @ 4K
- **32 GB RAM**: 10+ videos, 4K or production workloads

### Network Requirements (Azure Mode)

For Azure Blob Storage deployments:

- **Bandwidth**: 100+ Mbps recommended (1 Gbps for production)
- **Latency**: <50ms to Azure region (for blobfuse2 FUSE mounts)
- **Egress**: Consider Azure egress costs for large output files

### Platform Support

| Platform | Architecture | Status | Notes |
|----------|--------------|--------|-------|
| **Linux** | x86_64 (amd64) | ✅ Full support | Recommended for production |
| **Linux** | ARM64 (aarch64) | ✅ Full support | AWS Graviton, cloud ARM |
| **macOS** | Apple Silicon (ARM64) | ⚠️ Development only | No blobfuse2, limited inotify |
| **macOS** | x86_64 | ⚠️ Development only | Intel Macs, same limitations |
| **Windows** | x86_64 | 🚧 Via WSL2 | Docker Desktop + WSL2 required |

**Production Deployment**: Use Linux (Ubuntu 22.04+, Debian 11+, RHEL 8+)

## Installation

### Pull from Registry

```bash
docker pull 3dgs-processor:latest
```

### Build from Source

```bash
# Clone repository
git clone https://github.com/example/3dgs-processor.git
cd 3dgs-processor

# Build for current architecture
cargo build --release

# Or build multi-arch Docker image
./scripts/build-multiarch.sh --load
```

## Configuration

Configuration via environment variables (required) and YAML file (optional training parameters).

**Required Environment Variables:**

```bash
INPUT_PATH=/data/input                # Watch directory for new videos
OUTPUT_PATH=/data/output              # Output directory for .ply/.splat files
PROCESSED_PATH=/data/processed        # Archive for completed jobs
ERROR_PATH=/data/error                # Quarantine for failed jobs
BACKEND=gaussian-splatting            # 3DGS Backend: gaussian-splatting|gsplat|3dgs-cpp
RECONSTRUCTION_BACKEND=colmap         # Reconstruction: colmap|nerfstudio|precalibrated (default: colmap)
```

**Reconstruction Backends:**

The system supports pluggable reconstruction backends for camera pose estimation:

* **`colmap`** (default) - Gold-standard Structure-from-Motion pipeline
* **`nerfstudio`** - Load pre-computed camera poses from `transforms.json`
* **`precalibrated`** - Use known camera poses from external calibration
* Support for Polycam, Luma AI, and other mobile capture formats

See [docs/CONFIGURATION.md](docs/CONFIGURATION.md) for the complete configuration reference (all YAML settings and environment variables).

## Documentation

* [Configuration Reference](docs/CONFIGURATION.md) - Complete reference for all YAML and environment variable settings
* [User Guide](docs/USER_GUIDE.md) - Complete configuration and usage documentation
* [Architecture](docs/ARCHITECTURE.md) - System design and component descriptions
* [Progress Tracking](docs/PROGRESS_TRACKING.md) - Real-time monitoring and checkpointing
* [Deployment](docs/DEPLOYMENT.md) - Production deployment patterns and best practices
* [Troubleshooting](docs/TROUBLESHOOTING.md) - Common issues and solutions
* [PRD](docs/3dgs-video-processor-prd.md) - Product requirements specification

## Requirements

**Runtime:**

* Docker 20.10+ (with buildx for multi-arch builds)
* 8GB+ RAM (16GB+ recommended for large scenes)
* 50GB+ disk space for processing

**Bundled in Container:**

* FFmpeg 4.4+
* COLMAP 3.8+
* 3DGS backends (pluggable)

## Development

```bash
# Run tests (Note: Do NOT use --all-features due to plugin symbol conflicts)
cargo test

# Run integration tests
cargo test --test integration

# Run clippy
cargo clippy --all-targets -- -D warnings

# Format code
cargo fmt

# Generate test videos
./scripts/generate-test-videos.sh
```

## Testing

### Synthetic Test Data (Fast)

For unit and integration testing during development:

```bash
# Generate minimal COLMAP test data
python3 scripts/create_test_colmap_data.py

# Run integration tests
cargo test --test integration

# Test with mock backend (no GPU required)
BACKEND=mock cargo test
```

### Real Dataset Testing (Quality Validation)

For realistic quality validation, use the **Tanks and Temples** benchmark:

```bash
# Download a scene (~2-20GB)
./scripts/download-tanks-and-temples.sh barn

# Run benchmark with automatic metrics collection
./scripts/benchmark-tanks-and-temples.sh barn

# Or test directly with Python training script
python3 scripts/gsplat_train.py \
  --data testdata/tanks-and-temples/barn/images \
  --colmap-dir testdata/tanks-and-temples/barn/colmap/sparse/0 \
  --model-dir outputs/barn \
  --iterations 7000 \
  --save-ply --save-splat
```

**Note**: Tanks and Temples provides pre-extracted images and COLMAP reconstructions, so we test the training backend directly (the Python script that the Rust processor calls internally). For testing the **complete pipeline** with videos, use the synthetic test videos or your own footage:

```bash
# Generate test videos
./scripts/generate-test-videos.sh

# Run full processor (watch → extract → COLMAP → train)
export INPUT_PATH=$PWD/testdata/sample_scene
export OUTPUT_PATH=$PWD/outputs  
export BACKEND=auto
cargo run --release

# Trigger processing
mkdir -p testdata/sample_scene/my_scene
cp testdata/sample_scene/*.mp4 testdata/sample_scene/my_scene/
```

**Available scenes**: barn, truck, church, caterpillar, courthouse, ignatius, meetingroom

See [docs/TANKS_AND_TEMPLES_TESTING.md](docs/TANKS_AND_TEMPLES_TESTING.md) for complete testing guide.

## License

MIT License - See [LICENSE](LICENSE) for details.

## Support

* **Issues**: [GitHub Issues](https://github.com/azure-samples/3DGS-accelerator/issues)
* **Discussions**: [GitHub Discussions](https://github.com/azure-samples/3DGS-accelerator/discussions)

## Acknowledgments

Built with:

* [COLMAP](https://colmap.github.io/) - Structure-from-Motion
* [3D Gaussian Splatting](https://github.com/graphdeco-inria/gaussian-splatting) - Original implementation
* [gsplat](https://github.com/nerfstudio-project/gsplat) - Optimized 3DGS library

## Trademarks

Trademarks This project may contain trademarks or logos for projects, products, or services. Authorized use of Microsoft trademarks or logos is subject to and must follow Microsoft’s Trademark & Brand Guidelines. Use of Microsoft trademarks or logos in modified versions of this project must not cause confusion or imply Microsoft sponsorship. Any use of third-party trademarks or logos are subject to those third-party’s policies.