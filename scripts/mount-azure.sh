#!/bin/bash
# mount-azure.sh - Helper script for mounting Azure Blob containers via Blobfuse2
# This script is called by the Rust application to mount blob containers

set -euo pipefail

# Script usage
usage() {
    echo "Usage: $0 <config-file> <mount-path>"
    echo ""
    echo "Arguments:"
    echo "  config-file  Path to blobfuse2 YAML configuration file"
    echo "  mount-path   Local mount point directory"
    exit 1
}

# Check arguments
if [ $# -ne 2 ]; then
    usage
fi

CONFIG_FILE="$1"
MOUNT_PATH="$2"

# Validate config file exists
if [ ! -f "$CONFIG_FILE" ]; then
    echo "ERROR: Config file not found: $CONFIG_FILE"
    exit 1
fi

# Create mount point if it doesn't exist
mkdir -p "$MOUNT_PATH"

# Check if already mounted
if mountpoint -q "$MOUNT_PATH" 2>/dev/null; then
    echo "WARNING: $MOUNT_PATH is already mounted, skipping"
    exit 0
fi

# Mount using blobfuse2
echo "Mounting Azure Blob container to $MOUNT_PATH"
blobfuse2 mount "$MOUNT_PATH" --config-file="$CONFIG_FILE" --foreground=false

# Wait for mount to be ready
sleep 2

# Verify mount succeeded
if ! mountpoint -q "$MOUNT_PATH"; then
    echo "ERROR: Mount verification failed for $MOUNT_PATH"
    exit 1
fi

echo "Successfully mounted $MOUNT_PATH"
