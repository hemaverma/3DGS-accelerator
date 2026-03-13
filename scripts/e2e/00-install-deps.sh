#!/usr/bin/env bash
# 00-install-deps.sh — Install system dependencies for E2E testing
#
# Prerequisites:
#   - Ubuntu 24.04 (or compatible Debian-based system)
#   - sudo access
#
# Installs:
#   - ffmpeg (video frame extraction + ffprobe for metadata)
#   - colmap (Structure-from-Motion sparse reconstruction)
#   - unzip (for extracting test datasets)
#
# These are installed via apt (pre-built binaries, no compilation required):
#   - COLMAP 3.9.1 from Ubuntu universe repository
#   - FFmpeg 6.1.1 from Ubuntu universe repository

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/../.."

echo "============================================"
echo "  00 — Install System Dependencies"
echo "============================================"
echo ""

# Check for sudo
if ! command -v sudo &>/dev/null; then
    echo "❌ sudo is required but not found"
    exit 1
fi

echo "📦 Updating apt package list..."
sudo apt-get update -qq

echo ""
echo "📦 Installing ffmpeg, colmap, unzip..."
sudo apt-get install -y -qq ffmpeg colmap unzip

echo ""
echo "✅ Dependencies installed successfully!"
echo ""

# Verify installations
echo "🔍 Verifying installations:"
echo -n "  ffmpeg:  "
ffmpeg -version 2>/dev/null | head -1

echo -n "  ffprobe: "
ffprobe -version 2>/dev/null | head -1

echo -n "  colmap:  "
colmap --version 2>/dev/null || colmap help 2>&1 | head -1

echo -n "  unzip:   "
unzip -v 2>/dev/null | head -1

echo ""
echo "✅ All dependencies verified"
