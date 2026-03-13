#!/usr/bin/env bash
# Build multi-architecture Docker images
#
# This script builds Docker images for both x86_64 (AMD64) and ARM64 (aarch64)
# platforms and creates a multi-platform manifest.

set -euo pipefail

IMAGE_NAME="${IMAGE_NAME:-3dgs-processor}"
IMAGE_TAG="${IMAGE_TAG:-latest}"
PLATFORMS="linux/amd64,linux/arm64"
BUILDER_NAME="3dgs-multiarch"

echo "🏗️  Building multi-architecture Docker images..."
echo "📦 Image: $IMAGE_NAME:$IMAGE_TAG"
echo "🎯 Platforms: $PLATFORMS"
echo ""

# Ensure buildx builder exists
if ! docker buildx ls | grep -q "$BUILDER_NAME"; then
    echo "⚠️  Buildx builder '$BUILDER_NAME' not found"
    echo "Running setup script..."
    ./docker/buildx-setup.sh
fi

# Use the multi-arch builder
docker buildx use "$BUILDER_NAME"

# Parse command line arguments
PUSH=false
LOAD=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --push)
            PUSH=true
            shift
            ;;
        --load)
            LOAD=true
            PLATFORMS="linux/amd64"  # Can only load single platform
            shift
            ;;
        *)
            echo "Unknown option: $1"
            echo "Usage: $0 [--push] [--load]"
            echo "  --push: Push to registry after build"
            echo "  --load: Load image into local Docker (single platform only)"
            exit 1
            ;;
    esac
done

# Build command
BUILD_CMD="docker buildx build --platform $PLATFORMS -t $IMAGE_NAME:$IMAGE_TAG"

if [[ "$PUSH" == "true" ]]; then
    BUILD_CMD="$BUILD_CMD --push"
    echo "🚀 Will push to registry after build"
elif [[ "$LOAD" == "true" ]]; then
    BUILD_CMD="$BUILD_CMD --load"
    echo "📥 Will load into local Docker"
else
    echo "💾 Will build without loading (use --load or --push)"
fi

BUILD_CMD="$BUILD_CMD ."

echo "🔨 Build command: $BUILD_CMD"
echo ""

# Execute build
eval "$BUILD_CMD"

echo ""
echo "✅ Multi-architecture build complete!"
echo ""

if [[ "$PUSH" == "false" ]] && [[ "$LOAD" == "false" ]]; then
    echo "ℹ️  Images built but not loaded to Docker"
    echo "To load for testing, run:"
    echo "  $0 --load"
    echo ""
    echo "To push to registry, run:"
    echo "  $0 --push"
fi
