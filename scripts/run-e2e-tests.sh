#!/usr/bin/env bash
# Run E2E tests for 3DGS Video Processor
#
# This script automates E2E test execution with proper setup and cleanup.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
GENERATE_VIDEOS=${GENERATE_VIDEOS:-"auto"}
BUILD_IMAGE=${BUILD_IMAGE:-"auto"}
CLEANUP=${CLEANUP:-"true"}
VERBOSE=${VERBOSE:-"false"}

print_header() {
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}  $1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
}

print_step() {
    echo -e "${GREEN}▶${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC}  $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

# Check prerequisites
check_prerequisites() {
    print_header "Checking Prerequisites"
    
    local all_good=true
    
    # Check Rust
    if command -v cargo &> /dev/null; then
        print_success "Rust/Cargo: $(cargo --version | head -n1)"
    else
        print_error "Rust/Cargo not found"
        all_good=false
    fi
    
    # Check Docker
    if command -v docker &> /dev/null; then
        if docker ps &> /dev/null; then
            print_success "Docker: $(docker --version | head -n1)"
        else
            print_error "Docker daemon not running"
            all_good=false
        fi
    else
        print_error "Docker not found"
        all_good=false
    fi
    
    # Check FFmpeg (for test video generation)
    if command -v ffmpeg &> /dev/null; then
        print_success "FFmpeg: $(ffmpeg -version | head -n1 | cut -d' ' -f3)"
    else
        print_warning "FFmpeg not found (needed for test video generation)"
        if [[ "$GENERATE_VIDEOS" == "true" ]]; then
            all_good=false
        fi
    fi
    
    echo ""
    
    if [[ "$all_good" == "false" ]]; then
        print_error "Prerequisites not met. Please install missing tools."
        exit 1
    fi
}

# Generate test videos if needed
generate_test_videos() {
    print_header "Test Video Setup"
    
    local testdata_dir="$PROJECT_ROOT/testdata/sample_scene"
    
    # Check if test videos exist
    if [[ -f "$testdata_dir/view1.mp4" ]] && \
       [[ -f "$testdata_dir/view2.mp4" ]] && \
       [[ -f "$testdata_dir/view3.mp4" ]]; then
        print_success "Test videos already exist"
        
        if [[ "$GENERATE_VIDEOS" == "force" ]]; then
            print_step "Regenerating test videos (forced)..."
            "$PROJECT_ROOT/scripts/generate-test-videos.sh"
        fi
    else
        print_step "Generating test videos..."
        if ! "$PROJECT_ROOT/scripts/generate-test-videos.sh"; then
            print_error "Failed to generate test videos"
            exit 1
        fi
        print_success "Test videos generated"
    fi
    
    echo ""
}

# Build Docker image if needed
build_docker_image() {
    print_header "Docker Image Setup"
    
    # Check if image exists
    if docker images -q 3dgs-processor:test &> /dev/null | grep -q .; then
        print_success "Docker image '3dgs-processor:test' exists"
        
        if [[ "$BUILD_IMAGE" == "force" ]]; then
            print_step "Rebuilding Docker image (forced)..."
            cd "$PROJECT_ROOT"
            if ! docker build -t 3dgs-processor:test .; then
                print_error "Failed to build Docker image"
                exit 1
            fi
            print_success "Docker image rebuilt"
        fi
    else
        print_step "Building Docker image (first time)..."
        cd "$PROJECT_ROOT"
        if ! docker build -t 3dgs-processor:test .; then
            print_error "Failed to build Docker image"
            exit 1
        fi
        print_success "Docker image built"
    fi
    
    echo ""
}

# Run E2E tests
run_tests() {
    print_header "Running E2E Tests"
    
    cd "$PROJECT_ROOT"
    
    local cargo_args=(
        "test"
        "--test" "e2e"
        "--"
        "--test-threads=1"
    )
    
    if [[ "$VERBOSE" == "true" ]]; then
        cargo_args+=("--nocapture")
    fi
    
    # Set environment variables
    export RUST_LOG=${RUST_LOG:-"debug"}
    
    print_step "Test command: cargo ${cargo_args[*]}"
    echo ""
    
    # Run tests
    if cargo "${cargo_args[@]}"; then
        echo ""
        print_success "All E2E tests passed!"
        return 0
    else
        echo ""
        print_error "Some E2E tests failed"
        return 1
    fi
}

# Cleanup leftover containers
cleanup_containers() {
    print_header "Cleanup"
    
    # Find any leftover test containers
    local containers=$(docker ps -aq --filter "name=3dgs-e2e-test" --filter "name=azurite-test" 2>/dev/null || true)
    
    if [[ -n "$containers" ]]; then
        print_step "Stopping leftover test containers..."
        echo "$containers" | xargs docker stop 2>/dev/null || true
        echo "$containers" | xargs docker rm 2>/dev/null || true
        print_success "Containers cleaned up"
    else
        print_success "No leftover containers found"
    fi
    
    echo ""
}

# Show usage
usage() {
    cat << EOF
Usage: $0 [OPTIONS]

Run E2E tests for 3DGS Video Processor with automatic setup.

OPTIONS:
    -h, --help              Show this help message
    -v, --verbose           Run tests with verbose output (--nocapture)
    -g, --generate          Force regenerate test videos
    -b, --build             Force rebuild Docker image
    -c, --no-cleanup        Don't cleanup containers after tests
    -t, --test PATTERN      Run specific test matching PATTERN

ENVIRONMENT VARIABLES:
    GENERATE_VIDEOS=force   Force video regeneration
    BUILD_IMAGE=force       Force image rebuild
    CLEANUP=false           Skip cleanup
    VERBOSE=true            Verbose output
    RUST_LOG=level          Set log level (default: debug)

EXAMPLES:
    # Run all E2E tests
    $0

    # Run with verbose output
    $0 --verbose

    # Force rebuild everything
    $0 --generate --build

    # Run specific test
    $0 --test test_e2e_single_job_processing

    # Run without automatic cleanup (for debugging)
    $0 --no-cleanup

EOF
}

# Parse arguments
TEST_PATTERN=""
while [[ $# -gt 0 ]]; do
    case $1 in
        -h|--help)
            usage
            exit 0
            ;;
        -v|--verbose)
            VERBOSE="true"
            shift
            ;;
        -g|--generate)
            GENERATE_VIDEOS="force"
            shift
            ;;
        -b|--build)
            BUILD_IMAGE="force"
            shift
            ;;
        -c|--no-cleanup)
            CLEANUP="false"
            shift
            ;;
        -t|--test)
            TEST_PATTERN="$2"
            shift 2
            ;;
        *)
            print_error "Unknown option: $1"
            usage
            exit 1
            ;;
    esac
done

# Main execution
main() {
    print_header "3DGS Video Processor - E2E Test Runner"
    echo ""
    
    check_prerequisites
    
    if [[ "$CLEANUP" == "true" ]]; then
        cleanup_containers
    fi
    
    generate_test_videos
    build_docker_image
    
    # Run tests
    local test_result=0
    if [[ -n "$TEST_PATTERN" ]]; then
        print_step "Running tests matching: $TEST_PATTERN"
        echo ""
        cd "$PROJECT_ROOT"
        if [[ "$VERBOSE" == "true" ]]; then
            cargo test --test e2e "$TEST_PATTERN" -- --test-threads=1 --nocapture || test_result=$?
        else
            cargo test --test e2e "$TEST_PATTERN" -- --test-threads=1 || test_result=$?
        fi
    else
        run_tests || test_result=$?
    fi
    
    if [[ "$CLEANUP" == "true" ]]; then
        cleanup_containers
    fi
    
    # Summary
    print_header "Summary"
    if [[ $test_result -eq 0 ]]; then
        print_success "E2E tests completed successfully!"
        echo ""
        exit 0
    else
        print_error "E2E tests failed!"
        echo ""
        print_step "Debugging tips:"
        echo "  - Check container logs: docker logs <container-id>"
        echo "  - Run with verbose: $0 --verbose"
        echo "  - Keep containers alive: $0 --no-cleanup"
        echo "  - See tests/E2E_TESTING.md for more info"
        echo ""
        exit 1
    fi
}

main
