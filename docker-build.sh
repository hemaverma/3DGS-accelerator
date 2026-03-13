#!/bin/bash
set -euo pipefail

set =x

# Build script for 3DGS Video Processor Docker image
# Supports multi-arch builds using Docker Buildx

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${SCRIPT_DIR}"

IMAGE_NAME="${IMAGE_NAME:-3dgs-processor}"
IMAGE_TAG="${IMAGE_TAG:-latest}"
PLATFORMS="${PLATFORMS:-linux/amd64,linux/arm64}"
PUSH="${PUSH:-false}"

echo "========================================="
echo "Building 3DGS Video Processor"
echo "========================================="
echo "Image: ${IMAGE_NAME}:${IMAGE_TAG}"
echo "Platforms: ${PLATFORMS}"
echo "Push: ${PUSH}"
echo "========================================="

# Check if buildx is available
if ! docker buildx version &> /dev/null; then
    echo "ERROR: Docker Buildx is not available"
    echo "Please install Docker Buildx or use Docker Desktop"
    exit 1
fi

# Create builder instance if it doesn't exist
BUILDER_NAME="c3dgs-multiarch-builder"
if ! docker buildx inspect "${BUILDER_NAME}" &> /dev/null; then
    echo "Creating buildx builder: ${BUILDER_NAME}"
    docker buildx create --name "${BUILDER_NAME}" --use
else
    echo "Using existing buildx builder: ${BUILDER_NAME}"
    docker buildx use "${BUILDER_NAME}"
fi

remove_builder() {
    echo "Cleaning up buildx builder: ${BUILDER_NAME}"
    docker buildx rm "${BUILDER_NAME}" || true
}

# Build arguments
BUILD_ARGS=(
    --platform "${PLATFORMS}"
    -t "${IMAGE_NAME}:${IMAGE_TAG}"
    --build-arg RUST_VERSION=1.75
    --build-arg DEBIAN_VERSION=bookworm
)

# Add push flag if enabled
if [ "${PUSH}" = "true" ]; then
    BUILD_ARGS+=(--push)
else
    BUILD_ARGS+=(--load)
    echo ""
    echo "NOTE: Building for single platform (--load) since multi-arch requires --push"
    echo "To build multi-arch, set PUSH=true and ensure you have a registry configured"
    BUILD_ARGS=(
        --platform linux/amd64
        -t "${IMAGE_NAME}:${IMAGE_TAG}"
        --build-arg RUST_VERSION=1.75
        --build-arg DEBIAN_VERSION=bookworm
        --load
    )
fi

# Build the image
echo ""
echo "Building Docker image..."
docker buildx build "${BUILD_ARGS[@]}" .

echo ""
echo "========================================="
echo "Build complete!"
echo "========================================="
if [ "${PUSH}" = "false" ]; then
    echo ""
    echo "To run the container:"
    echo "  docker run --rm ${IMAGE_NAME}:${IMAGE_TAG}"
    echo ""
    echo "To build for multiple architectures and push:"
    echo "  PUSH=true ./docker-build.sh"
fi

# make this conditional on arg for "remove-builder"
if [ "${REMOVE_BUILDER:-false}" = "true" ]; then
    remove_builder
fi
