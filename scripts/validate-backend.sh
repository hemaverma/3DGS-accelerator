#!/usr/bin/env bash
# Backend Validation Script
#
# This script validates 3DGS backend implementations with real COLMAP test data.
# It provides a simple way to test backends without running the full integration test suite.
#
# Usage:
#   ./scripts/validate-backend.sh [backend_name] [iterations]
#
# Examples:
#   ./scripts/validate-backend.sh mock 100          # Test mock backend (always works)
#   ./scripts/validate-backend.sh gsplat 1000       # Test gsplat backend with 1000 iterations
#   ./scripts/validate-backend.sh all 100           # Test all available backends
#
# Requirements:
#   - Test COLMAP data: testdata/sample_scene/test_run/
#   - Backend binaries/scripts installed (for real backends)
#   - GPU for CUDA-based backends

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Configuration
TEST_DATA_DIR="$PROJECT_ROOT/testdata/sample_scene/test_run"
COLMAP_DIR="$TEST_DATA_DIR/colmap/sparse/0"
IMAGES_DIR="$TEST_DATA_DIR/images"
OUTPUT_DIR="$TEST_DATA_DIR/output"

# Default values
BACKEND="${1:-mock}"
ITERATIONS="${2:-100}"

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Helper functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if test data exists
check_test_data() {
    log_info "Checking test data..."
    
    if [ ! -d "$TEST_DATA_DIR" ]; then
        log_error "Test data directory not found: $TEST_DATA_DIR"
        log_info "Generating test data..."
        python3 "$PROJECT_ROOT/scripts/create_test_colmap_data.py"
    fi
    
    if [ ! -f "$COLMAP_DIR/cameras.bin" ]; then
        log_error "COLMAP test data not found"
        log_info "Run: python3 scripts/create_test_colmap_data.py"
        exit 1
    fi
    
    log_info "✓ Test data exists"
}

# Validate mock backend
validate_mock() {
    log_info "Validating mock backend..."
    
    # Create output directory
    local output="$OUTPUT_DIR/mock_$(date +%Y%m%d_%H%M%S)"
    mkdir -p "$output"
    
    # Run cargo test for mock backend
    cd "$PROJECT_ROOT"
    if cargo test --test integration backend_validation::test_mock_backend_with_real_data -- --nocapture; then
        log_info "✓ Mock backend validation PASSED"
        return 0
    else
        log_error "✗ Mock backend validation FAILED"
        return 1
    fi
}

# Validate gsplat backend
validate_gsplat() {
    log_info "Validating gsplat backend (${ITERATIONS} iterations)..."
    
    # Check if gsplat is available
    if ! command -v python3 &> /dev/null; then
        log_error "python3 not found"
        return 1
    fi
    
    # Check if gsplat training script exists
    local script="$PROJECT_ROOT/scripts/gsplat_train.py"
    if [ ! -f "$script" ]; then
        log_error "gsplat training script not found: $script"
        return 1
    fi
    
    # Check if gsplat package is installed
    if ! python3 -c "import gsplat" 2>/dev/null; then
        log_warn "gsplat Python package not installed"
        log_info "Install with: pip install gsplat torch"
        log_warn "Skipping gsplat validation"
        return 1
    fi
    
    # Create output directory
    local output="$OUTPUT_DIR/gsplat_$(date +%Y%m%d_%H%M%S)"
    mkdir -p "$output"
    
    log_info "Running gsplat training..."
    log_info "  Data: $IMAGES_DIR"
    log_info "  COLMAP: $COLMAP_DIR"
    log_info "  Output: $output"
    log_info "  Iterations: $ITERATIONS"
    
    # Run gsplat training
    export GSPLAT_BIN="$script"
    
    if python3 "$script" \
        --data "$IMAGES_DIR" \
        --colmap-dir "$COLMAP_DIR" \
        --model-dir "$output" \
        --iterations "$ITERATIONS" \
        --save-ply; then
        
        log_info "✓ gsplat training completed"
        
        # Check output files
        if [ -f "$output/point_cloud.ply" ]; then
            local size=$(du -h "$output/point_cloud.ply" | cut -f1)
            log_info "  PLY output: $size"
        fi
        
        if [ -f "$output/checkpoint.pth" ]; then
            local size=$(du -h "$output/checkpoint.pth" | cut -f1)
            log_info "  Checkpoint: $size"
        fi
        
        log_info "✓ gsplat backend validation PASSED"
        return 0
    else
        log_error "✗ gsplat backend validation FAILED"
        return 1
    fi
}

# Validate gaussian-splatting backend
validate_gaussian_splatting() {
    log_info "Validating gaussian-splatting backend..."
    log_warn "gaussian-splatting backend validation not yet implemented"
    log_info "This requires the original Gaussian Splatting C++/CUDA implementation"
    return 1
}

# Validate 3DGS.cpp backend
validate_3dgs_cpp() {
    log_info "Validating 3DGS.cpp backend..."
    log_warn "3DGS.cpp backend validation not yet implemented"
    log_info "This requires the 3DGS.cpp binary to be installed"
    return 1
}

# Validate all backends
validate_all() {
    log_info "Validating all backends..."
    
    local passed=0
    local failed=0
    
    # Mock backend (should always pass)
    if validate_mock; then
        ((passed++))
    else
        ((failed++))
    fi
    
    echo ""
    
    # gsplat backend (optional)
    if validate_gsplat; then
        ((passed++))
    else
        ((failed++))
    fi
    
    echo ""
    log_info "Validation summary:"
    log_info "  Passed: $passed"
    log_info "  Failed: $failed"
    
    return $failed
}

# Main validation logic
main() {
    log_info "Backend Validation Script"
    log_info "=========================="
    log_info "Backend: $BACKEND"
    log_info "Iterations: $ITERATIONS"
    echo ""
    
    # Check prerequisites
    check_test_data
    echo ""
    
    # Run validation based on backend
    case "$BACKEND" in
        mock)
            validate_mock
            ;;
        gsplat)
            validate_gsplat
            ;;
        gaussian-splatting)
            validate_gaussian_splatting
            ;;
        3dgs-cpp)
            validate_3dgs_cpp
            ;;
        all)
            validate_all
            ;;
        *)
            log_error "Unknown backend: $BACKEND"
            log_info "Available backends: mock, gsplat, gaussian-splatting, 3dgs-cpp, all"
            exit 1
            ;;
    esac
    
    local result=$?
    echo ""
    
    if [ $result -eq 0 ]; then
        log_info "✓ Validation complete - ALL TESTS PASSED"
    else
        log_error "✗ Validation complete - SOME TESTS FAILED"
    fi
    
    exit $result
}

# Run main function
main "$@"
