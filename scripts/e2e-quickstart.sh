#!/usr/bin/env bash
# Quick start script to set up and run E2E tests

set -euo pipefail

echo "🚀 3DGS Video Processor - E2E Test Quick Start"
echo ""

# Check prerequisites
echo "📋 Checking prerequisites..."
if ! command -v cargo &> /dev/null; then
    echo "❌ Rust/Cargo not found. Install from: https://rustup.rs/"
    exit 1
fi

if ! command -v docker &> /dev/null; then
    echo "❌ Docker not found. Install from: https://www.docker.com/get-started"
    exit 1
fi

if ! docker ps &> /dev/null; then
    echo "❌ Docker daemon not running. Please start Docker."
    exit 1
fi

if ! command -v ffmpeg &> /dev/null; then
    echo "⚠️  FFmpeg not found (needed for test video generation)"
    echo "   macOS: brew install ffmpeg"
    echo "   Ubuntu: sudo apt install ffmpeg"
    exit 1
fi

echo "✅ All prerequisites met"
echo ""

# Generate test videos
echo "🎬 Generating test videos..."
if [ ! -f "testdata/sample_scene/view1.mp4" ]; then
    ./scripts/generate-test-videos.sh
else
    echo "✅ Test videos already exist"
fi
echo ""

# Build Docker image
echo "🐳 Checking Docker image..."
if ! docker images -q 3dgs-processor:test | grep -q .; then
    echo "📦 Building Docker image (this may take a few minutes)..."
    docker build -t 3dgs-processor:test .
else
    echo "✅ Docker image exists"
fi
echo ""

# Run tests
echo "🧪 Running E2E tests..."
echo ""
cargo test --test e2e -- --test-threads=1

echo ""
echo "🎉 E2E tests complete!"
echo ""
echo "Next steps:"
echo "  - Run specific test: ./scripts/run-e2e-tests.sh --test <test_name>"
echo "  - Verbose output: ./scripts/run-e2e-tests.sh --verbose"
echo "  - See documentation: tests/E2E_TESTING.md"
