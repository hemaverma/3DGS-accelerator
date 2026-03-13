#!/usr/bin/env bash
# 02-build.sh — Build the 3DGS processor Rust application
#
# Builds the binary in release mode for optimal performance.
# The binary is output to: target/release/3dgs-processor

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$SCRIPT_DIR/../.."

echo "============================================"
echo "  02 — Build Rust Application"
echo "============================================"
echo ""

cd "$PROJECT_ROOT"

# Check for Rust toolchain
if ! command -v cargo &>/dev/null; then
    echo "❌ Rust/Cargo not found. Install from: https://rustup.rs/"
    exit 1
fi

echo "🔧 Rust toolchain:"
echo "  cargo: $(cargo --version)"
echo "  rustc: $(rustc --version)"
echo ""

echo "🏗️  Building 3dgs-processor (release mode)..."
cargo build --release 2>&1

BINARY="$PROJECT_ROOT/target/release/3dgs-processor"
if [ -f "$BINARY" ]; then
    echo ""
    echo "✅ Build successful!"
    echo "  Binary: $BINARY"
    echo "  Size:   $(du -h "$BINARY" | cut -f1)"
else
    echo "❌ Build failed — binary not found at $BINARY"
    exit 1
fi
