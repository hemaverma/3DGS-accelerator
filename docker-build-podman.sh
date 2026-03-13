#!/bin/bash
set -euo pipefail

# Build script for 3DGS Video Processor Docker image using Podman
# Supports multi-arch builds using Podman's native --platform flag

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

IMAGE_NAME="${IMAGE_NAME:-3dgs-processor}"
IMAGE_TAG="${IMAGE_TAG:-latest}"
PLATFORM="${PLATFORM:-linux/amd64}"
PUSH="${PUSH:-false}"
REGISTRY="${REGISTRY:-}"

echo "========================================="
echo "Building 3DGS Video Processor (Podman)"
echo "========================================="
echo "Image: ${IMAGE_NAME}:${IMAGE_TAG}"
echo "Platform: ${PLATFORM}"
echo "Push: ${PUSH}"
if [ -n "${REGISTRY}" ]; then
    echo "Registry: ${REGISTRY}"
fi
echo "========================================="

# Check if podman is available
if ! command -v podman &> /dev/null; then
    echo "ERROR: Podman is not available"
    echo "Please install Podman: https://podman.io/getting-started/installation"
    exit 1
fi

echo "Podman version: $(podman --version)"

# Build arguments
BUILD_ARGS=(
    --platform "${PLATFORM}"
    -t "${IMAGE_NAME}:${IMAGE_TAG}"
    --format docker
)

# Add registry tag if specified
if [ -n "${REGISTRY}" ]; then
    BUILD_ARGS+=(-t "${REGISTRY}/${IMAGE_NAME}:${IMAGE_TAG}")
fi

# Build the image
echo ""
echo "Building Docker image for ${PLATFORM}..."
podman build "${BUILD_ARGS[@]}" .

BUILD_STATUS=$?

if [ ${BUILD_STATUS} -ne 0 ]; then
    echo ""
    echo "========================================="
    echo "Build FAILED!"
    echo "========================================="
    exit ${BUILD_STATUS}
fi

echo ""
echo "========================================="
echo "Build complete!"
echo "========================================="
echo ""
echo "Image: ${IMAGE_NAME}:${IMAGE_TAG}"
echo "Platform: ${PLATFORM}"

# Push if requested
if [ "${PUSH}" = "true" ]; then
    if [ -z "${REGISTRY}" ]; then
        echo ""
        echo "ERROR: REGISTRY must be set when PUSH=true"
        echo "Example: REGISTRY=docker.io/yourusername PUSH=true ./docker-build-podman.sh"
        exit 1
    fi
    
    echo ""
    echo "Pushing to registry..."
    podman push "${REGISTRY}/${IMAGE_NAME}:${IMAGE_TAG}"
    
    echo ""
    echo "Pushed to: ${REGISTRY}/${IMAGE_NAME}:${IMAGE_TAG}"
else
    echo ""
    echo "To run the container:"
    echo "  podman run --rm ${IMAGE_NAME}:${IMAGE_TAG} --help"
    echo ""
    echo "To test with local directories:"
    echo "  podman run --rm \\"
    echo "    -v ./input:/input \\"
    echo "    -v ./output:/output \\"
    echo "    -v ./processed:/processed \\"
    echo "    -v ./error:/error \\"
    echo "    -e INPUT_PATH=/input \\"
    echo "    -e OUTPUT_PATH=/output \\"
    echo "    -e PROCESSED_PATH=/processed \\"
    echo "    -e ERROR_PATH=/error \\"
    echo "    ${IMAGE_NAME}:${IMAGE_TAG}"
    echo ""
    echo "To build for multiple architectures:"
    echo "  PLATFORM=linux/amd64 ./docker-build-podman.sh"
    echo "  PLATFORM=linux/arm64 ./docker-build-podman.sh"
    echo ""
    echo "To build and push to a registry:"
    echo "  REGISTRY=docker.io/yourusername PUSH=true ./docker-build-podman.sh"
fi

echo ""
echo "========================================="
