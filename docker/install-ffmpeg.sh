#!/bin/bash
set -euo pipefail

# Install FFmpeg for frame extraction
# Supports both x86_64 (amd64) and arm64 architectures

echo "Installing FFmpeg..."

# Detect architecture
ARCH=$(uname -m)
echo "Detected architecture: ${ARCH}"

# Update package lists
apt-get update

# Install FFmpeg and required libraries
# Using Debian repositories which support both amd64 and arm64
apt-get install -y --no-install-recommends \
    ffmpeg \
    libavcodec-dev \
    libavformat-dev \
    libavutil-dev \
    libswscale-dev \
    libavfilter-dev

# Clean up package lists to reduce image size
rm -rf /var/lib/apt/lists/*

# Verify installation
echo "Verifying FFmpeg installation..."
ffmpeg -version | head -n 1
ffprobe -version | head -n 1

echo "FFmpeg installation complete for ${ARCH}"
