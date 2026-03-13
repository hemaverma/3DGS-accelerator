#!/bin/bash
set -euo pipefail

# Install COLMAP for Structure-from-Motion
# Supports both x86_64 (amd64) and arm64 architectures

echo "Installing COLMAP..."

# Detect architecture
ARCH=$(uname -m)
echo "Detected architecture: ${ARCH}"

# Update package lists
apt-get update

# Install build dependencies
echo "Installing COLMAP build dependencies..."
apt-get install -y --no-install-recommends \
    git \
    cmake \
    ninja-build \
    build-essential \
    libboost-program-options-dev \
    libboost-filesystem-dev \
    libboost-graph-dev \
    libboost-system-dev \
    libeigen3-dev \
    libflann-dev \
    libfreeimage-dev \
    libmetis-dev \
    libgoogle-glog-dev \
    libgtest-dev \
    libsqlite3-dev \
    libglew-dev \
    qtbase5-dev \
    libqt5opengl5-dev \
    libcgal-dev \
    libceres-dev

# Build COLMAP from source for multi-arch compatibility
COLMAP_VERSION="3.9"
echo "Building COLMAP ${COLMAP_VERSION} from source..."

cd /tmp
git clone --branch ${COLMAP_VERSION} --depth 1 https://github.com/colmap/colmap.git
cd colmap

mkdir build
cd build

# Configure with optimizations for the target architecture
cmake .. -GNinja \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX=/usr/local \
    -DCMAKE_CXX_FLAGS="-march=native -O3"

# Build and install
ninja
ninja install

# Verify installation
echo "Verifying COLMAP installation..."
colmap -h > /dev/null 2>&1 && echo "COLMAP command available"

# Cleanup build artifacts to reduce image size
cd /
rm -rf /tmp/colmap

# Remove build-only dependencies to reduce image size
apt-get remove -y \
    git \
    cmake \
    ninja-build \
    build-essential \
    && apt-get autoremove -y

# Clean up package lists
rm -rf /var/lib/apt/lists/*

echo "COLMAP installation complete for ${ARCH}"
