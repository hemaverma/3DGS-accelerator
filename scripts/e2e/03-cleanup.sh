#!/usr/bin/env bash
# 03-cleanup.sh — Clean up test directories before an E2E run
#
# Wipes the output/data tree and temp working directory to ensure
# a clean starting state for each test run.
#
# Paths cleaned:
#   ./output/data/input/
#   ./output/data/processed/
#   ./output/data/error/
#   ./output/data/output/
#   /tmp/3dgs-work/

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/../.."

echo "============================================"
echo "  03 — Clean Up Test Directories"
echo "============================================"
echo ""

DATA_ROOT="$PROJECT_ROOT/output/data"

echo "🗑️  Removing existing test data..."
rm -rf "$DATA_ROOT/input"
rm -rf "$DATA_ROOT/processed"
rm -rf "$DATA_ROOT/error"
rm -rf "$DATA_ROOT/output"
rm -rf /tmp/3dgs-work

echo "📁 Recreating empty directories..."
mkdir -p "$DATA_ROOT/input"
mkdir -p "$DATA_ROOT/processed"
mkdir -p "$DATA_ROOT/error"
mkdir -p "$DATA_ROOT/output"
mkdir -p /tmp/3dgs-work

echo ""
echo "✅ Clean state ready:"
find "$DATA_ROOT" -maxdepth 1 -type d | sort
echo ""
