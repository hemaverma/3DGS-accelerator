#!/bin/bash
set -euo pipefail

# Install Blobfuse2 for Azure Blob Storage mounting
# Supports both x86_64 (amd64) and arm64 architectures

echo "Installing Blobfuse2..."

# Detect architecture
ARCH=$(uname -m)
echo "Detected architecture: ${ARCH}"

# Map architecture to Debian architecture names
case "${ARCH}" in
    x86_64)
        DEB_ARCH="amd64"
        ;;
    aarch64)
        DEB_ARCH="arm64"
        ;;
    *)
        echo "Unsupported architecture: ${ARCH}"
        exit 1
        ;;
esac

echo "Using Debian architecture: ${DEB_ARCH}"

# Add Microsoft GPG key
curl -fsSL https://packages.microsoft.com/keys/microsoft.asc | \
    gpg --dearmor -o /usr/share/keyrings/microsoft-prod.gpg

# Get distribution information
DISTRO=$(lsb_release -is | tr '[:upper:]' '[:lower:]')
CODENAME=$(lsb_release -cs)

echo "Configuring Microsoft repository for ${DISTRO} ${CODENAME}..."

# Add Microsoft package repository with multi-arch support
echo "deb [arch=${DEB_ARCH} signed-by=/usr/share/keyrings/microsoft-prod.gpg] https://packages.microsoft.com/repos/microsoft-${DISTRO}-${CODENAME}-prod ${CODENAME} main" | \
    tee /etc/apt/sources.list.d/microsoft.list

# Update package lists
apt-get update

# Install blobfuse2
echo "Installing blobfuse2..."
apt-get install -y --no-install-recommends blobfuse2

# Clean up package lists to reduce image size
rm -rf /var/lib/apt/lists/*

# Verify installation
echo "Verifying Blobfuse2 installation..."
blobfuse2 --version

echo "Blobfuse2 installation complete for ${ARCH}"
