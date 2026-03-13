# Docker Build Guide

## Overview

The 3DGS Video Processor can be containerized using Docker or Podman for portability and deployment. The multi-stage Dockerfile includes all necessary dependencies:

- **Rust binary** - Compiled from source with optimizations
- **FFmpeg** - For video frame extraction  
- **COLMAP** - For structure-from-motion reconstruction
- **Python/PyTorch/gsplat** - For 3DGS training backend
- **Blobfuse2** - For Azure Blob Storage mounting (amd64 only)

## Image Specifications

- **Base Image**: Debian Bookworm Slim
- **Python**: 3.11 with PyTorch CPU-only build
- **Backends**: gsplat (default), mock
- **Platforms**: linux/amd64, linux/arm64
- **Size**: ~2-3 GB compressed (includes PyTorch)

## Building the Image

### Using Podman (Recommended)

Podman works rootless and doesn't require a daemon:

```bash
# Build for current architecture (arm64 on macOS, amd64 on most Linux)
./docker-build-podman.sh

# Build for specific platform
PLATFORM=linux/amd64 ./docker-build-podman.sh
PLATFORM=linux/arm64 ./docker-build-podman.sh

# Build and push to registry
REGISTRY=docker.io/yourusername PUSH=true ./docker-build-podman.sh
```

### Using Docker Buildx

For Docker with Buildx support:

```bash
# Build for current architecture
./docker-build.sh

# Build for multiple architectures and push
PUSH=true ./docker-build.sh
```

## Build Time

Expect 10-30 minutes for first build depending on your machine:

- **Rust compilation**: ~2-5 minutes
- **COLMAP build**: ~5-15 minutes  
- **Python/PyTorch**: ~3-10 minutes
- Subsequent builds use layer caching and are much faster

## Running the Container

### Basic Usage (Local Filesystem)

```bash
podman run --rm \
  -v ./input:/input \
  -v ./output:/output \
  -v ./processed:/processed \
  -v ./error:/error \
  -e INPUT_PATH=/input \
  -e OUTPUT_PATH=/output \
  -e PROCESSED_PATH=/processed \
  -e ERROR_PATH=/error \
  3dgs-processor:latest
```

### With Azure Blob Storage

**Note**: Blobfuse2 is only available for linux/amd64

```bash
podman run --rm \
  --privileged \
  --device /dev/fuse \
  --cap-add SYS_ADMIN \
  -e AZURE_STORAGE_CONNECTION_STRING="DefaultEndpoints..." \
  -e INPUT_CONTAINER=input \
  -e OUTPUT_CONTAINER=output \
  -e PROCESSED_CONTAINER=processed \
  -e ERROR_CONTAINER=error \
  3dgs-processor:latest
```

### Environment Variables

**Required**:
- `INPUT_PATH` - Input video directory
- `OUTPUT_PATH` - Output model directory  
- `PROCESSED_PATH` - Successfully processed jobs
- `ERROR_PATH` - Failed jobs

**Optional**:
- `BACKEND=gsplat` - 3DGS backend (default: gsplat)
- `LOG_LEVEL=info` - Log verbosity
- `RETENTION_DAYS=30` - Cleanup old processed/error folders
- `MAX_RETRIES=3` - Retry failed jobs
- `UPLOAD_STABILITY_TIMEOUT_SECS=60` - Wait for file upload completion

**Azure** (when using Blobfuse2):
- `AZURE_STORAGE_CONNECTION_STRING` - Connection string OR
- `AZURE_STORAGE_ACCOUNT` + `AZURE_STORAGE_SAS_TOKEN` OR  
- `AZURE_STORAGE_ACCOUNT` + `AZURE_USE_MANAGED_IDENTITY=true`

## Image Contents

### Software Included

```
/usr/local/bin/3dgs-processor   - Main Rust binary
/usr/local/bin/colmap           - COLMAP binary
/usr/local/bin/mount-azure.sh   - Azure mounting helper
/opt/venv/                      - Python venv with gsplat
/app/scripts/gsplat_train.py    - Gsplat training script
```

### Verification

Check installed versions inside container:

```bash
podman run --rm 3dgs-processor:latest sh -c '
  echo "=== Installed Software ===" &&
  3dgs-processor --version &&
  colmap --version &&
  ffmpeg -version | head -1 &&
  python --version &&
  python -c "import torch; print(f\"PyTorch {torch.__version__}\")" &&
  python -c "import gsplat; print(f\"gsplat {gsplat.__version__}\")"
'
```

## Multi-Architecture Builds

### Building for Both Platforms

```bash
# Option 1: Build separately
PLATFORM=linux/amd64 IMAGE_TAG=latest-amd64 ./docker-build-podman.sh
PLATFORM=linux/arm64 IMAGE_TAG=latest-arm64 ./docker-build-podman.sh

# Option 2: Create manifest (Docker only)
docker buildx build \
  --platform linux/amd64,linux/arm64 \
  -t yourusername/3dgs-processor:latest \
  --push \
  .
```

### Platform-Specific Notes

**linux/amd64**:
- ✅ Full features including Blobfuse2
- ✅ CUDA GPU support (if nvidia-container-toolkit installed)
- ✅ Best for cloud deployments (Azure, AWS, GCP)

**linux/arm64**:
- ✅ Faster builds on Apple Silicon Macs
- ⚠️ No Blobfuse2 support (Microsoft doesn't publish arm64 builds)
- ✅ Works great for local development
- ✅ Good for AWS Graviton instances

## Troubleshooting

### Build Fails with "Out of Memory"

Increase docker/podman memory:
```bash
# Podman
podman machine set --memory 8192

# Docker Desktop
# Settings → Resources → Memory → 8 GB
```

### Build is Very Slow

Layer caching helps tremendously on subsequent builds. First build includes:
- Full COLMAP compilation from source (~10 mins)
- PyTorch download and installation (~500MB+)

### Blobfuse2 Not Available

This is expected on arm64. Use local filesystem or:
- Build amd64 image on x86 machine
- Use alternative storage (NFS, S3 FUSE, etc.)

### Python Import Errors

The container uses a virtual environment at `/opt/venv`. Ensure:
```bash
which python  # Should be /opt/venv/bin/python
python -c "import gsplat"  # Should not error
```

## Known Limitations

### File Watching on macOS with Docker/Podman

**Issue**: When running Linux containers on macOS with volume mounts, the inotify file watcher does not receive events for new files created on the macOS host filesystem.

**Why**: Linux's inotify mechanism relies on kernel-level filesystem events. Docker/Podman on macOS use a VM (HyperKit, VZ, or QEMU) to run the Linux container, and filesystem events from macOS don't propagate through the VM's filesystem sharing layer to the Linux kernel's inotify subsystem.

**Impact**: The processor won't automatically detect new job folders created in the input directory when running on macOS.

**Workarounds**:

1. **Deploy on Linux (Recommended)**: The container runs perfectly on native Linux hosts where inotify works as expected:
   - Azure Container Instances
   - AWS ECS/Fargate
   - Google Cloud Run
   - Kubernetes (AKS, EKS, GKE)
   - Linux VMs

2. **Use Polling Mode (Network Mounts)**: The system has a polling fallback that activates automatically for network mounts:
   ```bash
   # Explicitly enable polling (not currently exposed as env var)
   # This is handled automatically for NFS/Azure Files mounts
   ```

3. **Manual Testing on macOS**: Create job folders before starting the container:
   ```bash
   # Create test job first
   mkdir -p container-test/input/test_job_001
   cp videos/*.mp4 container-test/input/test_job_001/
   
   # Then start container
   podman run -v $PWD/container-test/input:/input ...
   ```

4. **Linux VM on macOS**: Use a Linux VM (Multipass, VirtualBox, UTM) and run the container inside:
   ```bash
   # Install Multipass
   brew install multipass
   
   # Launch Ubuntu VM
   multipass launch --name 3dgs-linux --cpus 4 --memory 8G --disk 50G
   multipass shell 3dgs-linux
   
   # Inside VM, install podman and run container
   sudo apt install podman
   podman run ...
   ```

**Verification**: The containerized application itself is fully functional on macOS. This limitation only affects file system event watching. All other functionality (frame extraction, COLMAP, training, exports) works identically on macOS and Linux.

### Blobfuse2 on arm64

Microsoft doesn't publish arm64 builds of Blobfuse2. Use:
- amd64 images for Azure Blob Storage integration
- arm64 images for local development without Azure
- Alternative FUSE mounts (goofys, s3fs) on arm64

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Build Docker Image
on: [push]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v2
      
      - name: Build and push
        run: |
          docker login -u ${{ secrets.DOCKER_USERNAME }} -p ${{ secrets.DOCKER_PASSWORD }}
          PUSH=true ./docker-build.sh
```

## Testing the Image

See [DOCKER_VALIDATION.md](DOCKER_VALIDATION.md) for comprehensive testing procedures.

Quick smoke test:

```bash
# Test help output
podman run --rm 3dgs-processor:latest --help

# Test with synthetic data
mkdir -p test-data/{input,output,processed,error}
# Add test video to test-data/input/
podman run --rm \
  -v ./test-data:/data \
  -e INPUT_PATH=/data/input \
  -e OUTPUT_PATH=/data/output \
  -e PROCESSED_PATH=/data/processed \
  -e ERROR_PATH=/data/error \
  3dgs-processor:latest
```

## Next Steps

1. **Local Testing**: Use podman locally with test videos
2. **Cloud Deployment**: Push to registry and deploy to Azure Container Instances or Kubernetes
3. **GPU Testing**: Test with nvidia-container-toolkit for GPU acceleration
4. **Monitoring**: Add logging and metrics collection

## Related Documentation

- [Architecture](ARCHITECTURE.md) - System design
- [Deployment Guide](DEPLOYMENT.md) - Cloud deployment
- [User Guide](USER_GUIDE.md) - Running the processor
- [Docker Validation](DOCKER_VALIDATION.md) - Testing procedures
