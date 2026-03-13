#!/usr/bin/env bash
# Set up Docker buildx for multi-platform builds
#
# This script initializes a Docker buildx builder for cross-platform image builds.
# Supports linux/amd64 and linux/arm64 architectures.

set -euo pipefail

BUILDER_NAME="3dgs-multiarch"

echo "🏗️  Setting up Docker buildx for multi-arch builds..."

# Check Docker is installed
if ! command -v docker &> /dev/null; then
    echo "❌ Error: Docker is required but not installed"
    exit 1
fi

# Check if buildx is available
if ! docker buildx version &> /dev/null; then
    echo "❌ Error: Docker buildx is not available"
    echo "Update Docker to a recent version that includes buildx"
    exit 1
fi

# Check if builder already exists
if docker buildx ls | grep -q "$BUILDER_NAME"; then
    echo "ℹ️  Builder '$BUILDER_NAME' already exists"
    read -p "Remove and recreate? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        echo "🗑️  Removing existing builder..."
        docker buildx rm "$BUILDER_NAME" || true
    else
        echo "✅ Using existing builder"
        docker buildx use "$BUILDER_NAME"
        docker buildx inspect --bootstrap
        exit 0
    fi
fi

# Create new buildx builder with docker-container driver
echo "📦 Creating buildx builder '$BUILDER_NAME'..."
docker buildx create \
    --name "$BUILDER_NAME" \
    --driver docker-container \
    --bootstrap \
    --use

# Verify builder supports required platforms
echo "🔍 Verifying platform support..."
docker buildx inspect --bootstrap

echo ""
echo "✅ Buildx builder configured successfully!"
echo ""
echo "Supported platforms:"
docker buildx inspect | grep "Platforms:" | sed 's/Platforms:/  -/'
echo ""
echo "To use this builder, run:"
echo "  docker buildx use $BUILDER_NAME"
echo ""
echo "To build multi-arch images, run:"
echo "  docker buildx build --platform linux/amd64,linux/arm64 -t IMAGE_NAME ."
