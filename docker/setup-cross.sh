#!/usr/bin/env bash
# Set up cross-compilation toolchain for ARM64
#
# This script installs cross-compilation tools needed for building
# Rust binaries for aarch64-unknown-linux-gnu (ARM64 Linux).

set -euo pipefail

echo "🔧 Setting up cross-compilation toolchain for ARM64..."

# Detect platform
if [[ "$OSTYPE" == "darwin"* ]]; then
    PLATFORM="macOS"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    PLATFORM="Linux"
else
    echo "❌ Unsupported platform: $OSTYPE"
    exit 1
fi

# Install cross-compilation target
echo "📦 Installing Rust ARM64 target..."
rustup target add aarch64-unknown-linux-gnu

# Platform-specific toolchain setup
if [[ "$PLATFORM" == "macOS" ]]; then
    echo "🍎 macOS detected - Docker buildx will handle cross-compilation"
    echo "ℹ️  Native cross-compilation requires additional setup (not required for Docker builds)"
    
elif [[ "$PLATFORM" == "Linux" ]]; then
    echo "🐧 Linux detected - installing cross-compilation tools..."
    
    # Check for package manager
    if command -v apt-get &> /dev/null; then
        sudo apt-get update
        sudo apt-get install -y gcc-aarch64-linux-gnu g++-aarch64-linux-gnu
    elif command -v dnf &> /dev/null; then
        sudo dnf install -y gcc-aarch64-linux-gnu
    else
        echo "⚠️  Unknown package manager - please install gcc-aarch64-linux-gnu manually"
    fi
fi

echo ""
echo "✅ Cross-compilation setup complete!"
echo ""
echo "Installed targets:"
rustup target list --installed | grep -E "(x86_64|aarch64)"
echo ""
echo "To build for ARM64:"
echo "  cargo build --target aarch64-unknown-linux-gnu --release"
